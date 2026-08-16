use crate::{Db, DbError, JobRepository};
use pixivarchive_domain::{job::JobLease, pixiv::PixivFollowingVisibility};
use sqlx::Row;
use std::collections::HashSet;
use time::OffsetDateTime;
use uuid::Uuid;

pub const MODULE_NAME: &str = "following";

#[derive(Clone)]
pub struct FollowingRepository {
    db: Db,
}

impl FollowingRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn sync_authors(&self, input: SyncFollowingAuthors) -> Result<(), DbError> {
        self.sync_authors_with_lease(None, input).await
    }

    pub async fn sync_authors_for_job(
        &self,
        lease: JobLease,
        input: SyncFollowingAuthors,
    ) -> Result<(), DbError> {
        self.sync_authors_with_lease(Some(lease), input).await
    }

    async fn sync_authors_with_lease(
        &self,
        lease: Option<JobLease>,
        input: SyncFollowingAuthors,
    ) -> Result<(), DbError> {
        validate_authors(&input.authors)?;
        let artist_ids: Vec<_> = input
            .authors
            .iter()
            .map(|author| author.pixiv_artist_id)
            .collect();
        let mut tx = self.db.begin().await?;
        if let Some(lease) = lease {
            JobRepository::new(self.db.clone())
                .lock_active_lease_in_tx(&mut tx, lease)
                .await?;
        }

        for author in input.authors {
            sqlx::query(
                r#"
                INSERT INTO pixiv_following_author (
                    pixiv_account_id,
                    pixiv_artist_id,
                    display_name,
                    avatar_url,
                    visibility,
                    refreshed_at
                )
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (pixiv_account_id, pixiv_artist_id)
                DO UPDATE SET display_name = excluded.display_name,
                              avatar_url = excluded.avatar_url,
                              visibility = excluded.visibility,
                              refreshed_at = excluded.refreshed_at
                "#,
            )
            .bind(input.account_id)
            .bind(author.pixiv_artist_id)
            .bind(author.display_name.trim())
            .bind(author.avatar_url)
            .bind(visibility_value(author.visibility))
            .bind(input.refreshed_at)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            r#"
            DELETE FROM pixiv_following_author
            WHERE pixiv_account_id = $1
              AND NOT (pixiv_artist_id = ANY($2))
            "#,
        )
        .bind(input.account_id)
        .bind(artist_ids)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn upsert_author(
        &self,
        account_id: Uuid,
        refreshed_at: OffsetDateTime,
        author: FollowingAuthorSnapshot,
    ) -> Result<FollowingAuthorRecord, DbError> {
        validate_authors(std::slice::from_ref(&author))?;
        sqlx::query(
            r#"
            INSERT INTO pixiv_following_author (
                pixiv_account_id,
                pixiv_artist_id,
                display_name,
                avatar_url,
                visibility,
                refreshed_at
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (pixiv_account_id, pixiv_artist_id)
            DO UPDATE SET display_name = excluded.display_name,
                          avatar_url = excluded.avatar_url,
                          visibility = excluded.visibility,
                          refreshed_at = excluded.refreshed_at
            "#,
        )
        .bind(account_id)
        .bind(author.pixiv_artist_id)
        .bind(author.display_name.trim())
        .bind(author.avatar_url)
        .bind(visibility_value(author.visibility))
        .bind(refreshed_at)
        .execute(self.db.pool())
        .await?;
        self.get(account_id, author.pixiv_artist_id).await
    }

    pub async fn remove_author(
        &self,
        account_id: Uuid,
        pixiv_artist_id: i64,
    ) -> Result<(), DbError> {
        if pixiv_artist_id <= 0 {
            return Err(DbError::InvalidValue(
                "following author identity is invalid".to_owned(),
            ));
        }
        sqlx::query(
            r#"
            DELETE FROM pixiv_following_author
            WHERE pixiv_account_id = $1
              AND pixiv_artist_id = $2
            "#,
        )
        .bind(account_id)
        .bind(pixiv_artist_id)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    pub async fn list(&self, account_id: Uuid) -> Result<Vec<FollowingAuthorRecord>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT author.pixiv_account_id,
                   author.pixiv_artist_id,
                   author.display_name,
                   author.avatar_url,
                   author.visibility,
                   exclusion.pixiv_artist_id IS NULL AS enabled,
                   author.refreshed_at,
                   author.last_collected_at
            FROM pixiv_following_author AS author
            LEFT JOIN pixiv_following_author_exclusion AS exclusion
              ON exclusion.pixiv_account_id = author.pixiv_account_id
             AND exclusion.pixiv_artist_id = author.pixiv_artist_id
            WHERE author.pixiv_account_id = $1
            ORDER BY author.pixiv_artist_id
            "#,
        )
        .bind(account_id)
        .fetch_all(self.db.pool())
        .await?;
        rows.iter().map(author_from_row).collect()
    }

    pub async fn get(
        &self,
        account_id: Uuid,
        pixiv_artist_id: i64,
    ) -> Result<FollowingAuthorRecord, DbError> {
        let row = sqlx::query(
            r#"
            SELECT author.pixiv_account_id,
                   author.pixiv_artist_id,
                   author.display_name,
                   author.avatar_url,
                   author.visibility,
                   exclusion.pixiv_artist_id IS NULL AS enabled,
                   author.refreshed_at,
                   author.last_collected_at
            FROM pixiv_following_author AS author
            LEFT JOIN pixiv_following_author_exclusion AS exclusion
              ON exclusion.pixiv_account_id = author.pixiv_account_id
             AND exclusion.pixiv_artist_id = author.pixiv_artist_id
            WHERE author.pixiv_account_id = $1
              AND author.pixiv_artist_id = $2
            "#,
        )
        .bind(account_id)
        .bind(pixiv_artist_id)
        .fetch_one(self.db.pool())
        .await?;
        author_from_row(&row)
    }

    pub async fn set_enabled(
        &self,
        account_id: Uuid,
        pixiv_artist_id: i64,
        enabled: bool,
    ) -> Result<(), DbError> {
        self.set_enabled_many(account_id, &[pixiv_artist_id], enabled)
            .await?;
        Ok(())
    }

    pub async fn set_enabled_many(
        &self,
        account_id: Uuid,
        pixiv_artist_ids: &[i64],
        enabled: bool,
    ) -> Result<u64, DbError> {
        validate_artist_ids(pixiv_artist_ids)?;
        let mut tx = self.db.begin().await?;
        let existing_ids: Vec<i64> = sqlx::query_scalar(
            r#"
            SELECT pixiv_artist_id
            FROM pixiv_following_author
            WHERE pixiv_account_id = $1
              AND pixiv_artist_id = ANY($2)
            FOR UPDATE
            "#,
        )
        .bind(account_id)
        .bind(pixiv_artist_ids)
        .fetch_all(&mut *tx)
        .await?;
        if existing_ids.len() != pixiv_artist_ids.len() {
            return Err(DbError::NotFound);
        }

        if enabled {
            sqlx::query(
                r#"
                DELETE FROM pixiv_following_author_exclusion
                WHERE pixiv_account_id = $1
                  AND pixiv_artist_id = ANY($2)
                "#,
            )
            .bind(account_id)
            .bind(pixiv_artist_ids)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query(
                r#"
                INSERT INTO pixiv_following_author_exclusion (
                    pixiv_account_id,
                    pixiv_artist_id
                )
                SELECT $1, author.pixiv_artist_id
                FROM pixiv_following_author AS author
                WHERE author.pixiv_account_id = $1
                  AND author.pixiv_artist_id = ANY($2)
                ON CONFLICT (pixiv_account_id, pixiv_artist_id) DO NOTHING
                "#,
            )
            .bind(account_id)
            .bind(pixiv_artist_ids)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(pixiv_artist_ids.len() as u64)
    }

    pub async fn enabled_artist_ids(&self, account_id: Uuid) -> Result<Vec<i64>, DbError> {
        sqlx::query_scalar(
            r#"
            SELECT author.pixiv_artist_id
            FROM pixiv_following_author AS author
            WHERE author.pixiv_account_id = $1
              AND NOT EXISTS (
                  SELECT 1
                  FROM pixiv_following_author_exclusion AS exclusion
                  WHERE exclusion.pixiv_account_id = author.pixiv_account_id
                    AND exclusion.pixiv_artist_id = author.pixiv_artist_id
              )
            ORDER BY author.pixiv_artist_id
            "#,
        )
        .bind(account_id)
        .fetch_all(self.db.pool())
        .await
        .map_err(DbError::from)
    }

    pub async fn mark_enabled_collected(
        &self,
        account_id: Uuid,
        collected_at: OffsetDateTime,
    ) -> Result<u64, DbError> {
        self.mark_enabled_collected_with_lease(None, account_id, collected_at)
            .await
    }

    pub async fn mark_enabled_collected_for_job(
        &self,
        lease: JobLease,
        account_id: Uuid,
        collected_at: OffsetDateTime,
    ) -> Result<u64, DbError> {
        self.mark_enabled_collected_with_lease(Some(lease), account_id, collected_at)
            .await
    }

    async fn mark_enabled_collected_with_lease(
        &self,
        lease: Option<JobLease>,
        account_id: Uuid,
        collected_at: OffsetDateTime,
    ) -> Result<u64, DbError> {
        let mut tx = self.db.begin().await?;
        if let Some(lease) = lease {
            JobRepository::new(self.db.clone())
                .lock_active_lease_in_tx(&mut tx, lease)
                .await?;
        }
        let result = sqlx::query(
            r#"
            UPDATE pixiv_following_author AS author
            SET last_collected_at = $2
            WHERE author.pixiv_account_id = $1
              AND NOT EXISTS (
                  SELECT 1
                  FROM pixiv_following_author_exclusion AS exclusion
                  WHERE exclusion.pixiv_account_id = author.pixiv_account_id
                    AND exclusion.pixiv_artist_id = author.pixiv_artist_id
              )
            "#,
        )
        .bind(account_id)
        .bind(collected_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(result.rows_affected())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FollowingAuthorSnapshot {
    pub pixiv_artist_id: i64,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub visibility: PixivFollowingVisibility,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyncFollowingAuthors {
    pub account_id: Uuid,
    pub refreshed_at: OffsetDateTime,
    pub authors: Vec<FollowingAuthorSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FollowingAuthorRecord {
    pub account_id: Uuid,
    pub pixiv_artist_id: i64,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub visibility: PixivFollowingVisibility,
    pub enabled: bool,
    pub refreshed_at: OffsetDateTime,
    pub last_collected_at: Option<OffsetDateTime>,
}

fn validate_authors(authors: &[FollowingAuthorSnapshot]) -> Result<(), DbError> {
    let mut ids = HashSet::with_capacity(authors.len());
    for author in authors {
        if author.pixiv_artist_id <= 0 || author.display_name.trim().is_empty() {
            return Err(DbError::InvalidValue(
                "following author identity is invalid".to_owned(),
            ));
        }
        if !ids.insert(author.pixiv_artist_id) {
            return Err(DbError::InvalidValue(
                "following author ids must be unique".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_artist_ids(pixiv_artist_ids: &[i64]) -> Result<(), DbError> {
    if pixiv_artist_ids.is_empty() {
        return Err(DbError::InvalidValue(
            "following author ids must not be empty".to_owned(),
        ));
    }
    let mut ids = HashSet::with_capacity(pixiv_artist_ids.len());
    for &pixiv_artist_id in pixiv_artist_ids {
        if pixiv_artist_id <= 0 {
            return Err(DbError::InvalidValue(
                "following author identity is invalid".to_owned(),
            ));
        }
        if !ids.insert(pixiv_artist_id) {
            return Err(DbError::InvalidValue(
                "following author ids must be unique".to_owned(),
            ));
        }
    }
    Ok(())
}

fn author_from_row(row: &sqlx::postgres::PgRow) -> Result<FollowingAuthorRecord, DbError> {
    let visibility_value: String = row.get("visibility");
    let visibility = match visibility_value.as_str() {
        "public" => PixivFollowingVisibility::Public,
        "private" => PixivFollowingVisibility::Private,
        _ => {
            return Err(DbError::InvalidValue(format!(
                "unknown following visibility {visibility_value}"
            )));
        }
    };
    Ok(FollowingAuthorRecord {
        account_id: row.get("pixiv_account_id"),
        pixiv_artist_id: row.get("pixiv_artist_id"),
        display_name: row.get("display_name"),
        avatar_url: row.get("avatar_url"),
        visibility,
        enabled: row.get("enabled"),
        refreshed_at: row.get("refreshed_at"),
        last_collected_at: row.get("last_collected_at"),
    })
}

fn visibility_value(visibility: PixivFollowingVisibility) -> &'static str {
    match visibility {
        PixivFollowingVisibility::Public => "public",
        PixivFollowingVisibility::Private => "private",
    }
}
