use crate::{Db, DbError, EventRepository, JobRepository};
use pixivarchive_domain::{
    event::{EventPayload, EventResource},
    job::JobLease,
    pixiv::PixivBookmarkVisibility,
};
use sqlx::{Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PixivBookmarkSyncEntry {
    pub pixiv_work_id: i64,
    pub visibility: PixivBookmarkVisibility,
}

#[derive(Clone)]
pub struct BookmarkRepository {
    db: Db,
}

impl BookmarkRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn mark_added(
        &self,
        account_id: Uuid,
        pixiv_work_id: i64,
        bookmark_id: Option<i64>,
        visibility: PixivBookmarkVisibility,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        let changed_work_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            WITH target AS MATERIALIZED (
                SELECT id AS work_id
                FROM work
                WHERE pixiv_work_id = $2
            ),
            previous AS MATERIALIZED (
                SELECT bookmark.pixiv_bookmark_id,
                       bookmark.visibility,
                       bookmark.active
                FROM pixiv_work_bookmark AS bookmark
                JOIN target USING (work_id)
                WHERE bookmark.pixiv_account_id = $1
            ),
            upserted AS (
                INSERT INTO pixiv_work_bookmark (
                    pixiv_account_id, work_id, pixiv_bookmark_id, visibility, active
                )
                SELECT $1, target.work_id, $3, $4, true
                FROM target
                ON CONFLICT (pixiv_account_id, work_id)
                DO UPDATE SET pixiv_bookmark_id = excluded.pixiv_bookmark_id,
                              visibility = excluded.visibility,
                              active = true,
                              last_seen_at = now(),
                              updated_at = now()
                RETURNING work_id
            )
            SELECT upserted.work_id
            FROM upserted
            WHERE NOT EXISTS (SELECT 1 FROM previous)
               OR EXISTS (
                    SELECT 1
                    FROM previous
                    WHERE pixiv_bookmark_id IS DISTINCT FROM $3
                       OR visibility IS DISTINCT FROM $4
                       OR active = false
               )
            "#,
        )
        .bind(account_id)
        .bind(pixiv_work_id)
        .bind(bookmark_id)
        .bind(visibility_value(visibility))
        .fetch_optional(&mut *tx)
        .await?;
        append_bookmark_event(&self.db, &mut tx, account_id, changed_work_id.is_some()).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn active_bookmark_id(
        &self,
        account_id: Uuid,
        pixiv_work_id: i64,
    ) -> Result<Option<i64>, DbError> {
        Ok(sqlx::query_scalar(
            r#"
            SELECT bookmark.pixiv_bookmark_id
            FROM pixiv_work_bookmark bookmark
            JOIN work ON work.id = bookmark.work_id
            WHERE bookmark.pixiv_account_id = $1
              AND work.pixiv_work_id = $2
              AND bookmark.active = true
            "#,
        )
        .bind(account_id)
        .bind(pixiv_work_id)
        .fetch_optional(self.db.pool())
        .await?
        .flatten())
    }

    pub async fn mark_removed_by_work(
        &self,
        account_id: Uuid,
        pixiv_work_id: i64,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        let changed_work_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            UPDATE pixiv_work_bookmark AS bookmark
            SET active = false,
                last_seen_at = now(),
                updated_at = now()
            FROM work
            WHERE bookmark.pixiv_account_id = $1
              AND bookmark.work_id = work.id
              AND work.pixiv_work_id = $2
              AND bookmark.active = true
            RETURNING bookmark.work_id
            "#,
        )
        .bind(account_id)
        .bind(pixiv_work_id)
        .fetch_optional(&mut *tx)
        .await?;
        append_bookmark_event(&self.db, &mut tx, account_id, changed_work_id.is_some()).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn last_full_reconciled_at(
        &self,
        account_id: Uuid,
    ) -> Result<Option<OffsetDateTime>, DbError> {
        Ok(sqlx::query_scalar(
            "SELECT last_full_reconciled_at FROM pixiv_bookmark_sync_state WHERE pixiv_account_id = $1",
        )
        .bind(account_id)
        .fetch_optional(self.db.pool())
        .await?
        .flatten())
    }

    pub async fn reconcile_full(
        &self,
        account_id: Uuid,
        entries: &[PixivBookmarkSyncEntry],
        reconciled_at: OffsetDateTime,
    ) -> Result<(), DbError> {
        self.reconcile_full_with_lease(None, account_id, entries, reconciled_at)
            .await
    }

    pub async fn reconcile_full_for_job(
        &self,
        lease: JobLease,
        account_id: Uuid,
        entries: &[PixivBookmarkSyncEntry],
        reconciled_at: OffsetDateTime,
    ) -> Result<(), DbError> {
        self.reconcile_full_with_lease(Some(lease), account_id, entries, reconciled_at)
            .await
    }

    async fn reconcile_full_with_lease(
        &self,
        lease: Option<JobLease>,
        account_id: Uuid,
        entries: &[PixivBookmarkSyncEntry],
        reconciled_at: OffsetDateTime,
    ) -> Result<(), DbError> {
        let pixiv_work_ids = entries
            .iter()
            .map(|entry| entry.pixiv_work_id)
            .collect::<Vec<_>>();
        let visibilities = entries
            .iter()
            .map(|entry| visibility_value(entry.visibility))
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut tx = self.db.begin().await?;
        if let Some(lease) = lease {
            JobRepository::new(self.db.clone())
                .lock_active_lease_in_tx(&mut tx, lease)
                .await?;
        }
        let mut changed_work_ids = sqlx::query_scalar::<_, Uuid>(
            r#"
            WITH seen AS (
                SELECT DISTINCT ON (pixiv_work_id)
                       pixiv_work_id,
                       visibility
                FROM unnest($2::bigint[], $3::text[]) AS seen(pixiv_work_id, visibility)
                ORDER BY pixiv_work_id,
                         CASE visibility WHEN 'private' THEN 0 ELSE 1 END
            ),
            matched AS (
                SELECT work.id AS work_id,
                       seen.visibility
                FROM seen
                JOIN work ON work.pixiv_work_id = seen.pixiv_work_id
            ),
            changed AS MATERIALIZED (
                SELECT matched.work_id
                FROM matched
                LEFT JOIN pixiv_work_bookmark AS bookmark
                  ON bookmark.pixiv_account_id = $1
                 AND bookmark.work_id = matched.work_id
                WHERE bookmark.work_id IS NULL
                   OR bookmark.visibility IS DISTINCT FROM matched.visibility
                   OR bookmark.active = false
            ),
            upserted AS (
                INSERT INTO pixiv_work_bookmark (
                    pixiv_account_id,
                    work_id,
                    pixiv_bookmark_id,
                    visibility,
                    active,
                    first_seen_at,
                    last_seen_at,
                    updated_at
                )
                SELECT $1,
                       matched.work_id,
                       NULL,
                       matched.visibility,
                       true,
                       $4,
                       $4,
                       $4
                FROM matched
                ON CONFLICT (pixiv_account_id, work_id)
                DO UPDATE SET visibility = excluded.visibility,
                              active = true,
                              last_seen_at = excluded.last_seen_at,
                              updated_at = excluded.updated_at
                RETURNING work_id
            )
            SELECT changed.work_id
            FROM changed
            JOIN upserted USING (work_id)
            "#,
        )
        .bind(account_id)
        .bind(&pixiv_work_ids)
        .bind(&visibilities)
        .bind(reconciled_at)
        .fetch_all(&mut *tx)
        .await?;
        let deactivated_work_ids = sqlx::query_scalar::<_, Uuid>(
            r#"
            UPDATE pixiv_work_bookmark AS bookmark
            SET active = false,
                last_seen_at = $3,
                updated_at = $3
            FROM work
            WHERE bookmark.pixiv_account_id = $1
              AND bookmark.work_id = work.id
              AND bookmark.active = true
              AND NOT (work.pixiv_work_id = ANY($2::bigint[]))
            RETURNING bookmark.work_id
            "#,
        )
        .bind(account_id)
        .bind(&pixiv_work_ids)
        .bind(reconciled_at)
        .fetch_all(&mut *tx)
        .await?;
        changed_work_ids.extend(deactivated_work_ids);
        sqlx::query(
            r#"
            INSERT INTO pixiv_bookmark_sync_state (
                pixiv_account_id, last_full_reconciled_at, updated_at
            )
            VALUES ($1, $2, $2)
            ON CONFLICT (pixiv_account_id)
            DO UPDATE SET last_full_reconciled_at = excluded.last_full_reconciled_at,
                          updated_at = excluded.updated_at
            "#,
        )
        .bind(account_id)
        .bind(reconciled_at)
        .execute(&mut *tx)
        .await?;
        append_bookmark_event(&self.db, &mut tx, account_id, !changed_work_ids.is_empty()).await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn append_bookmark_event(
    db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    account_id: Uuid,
    changed: bool,
) -> Result<(), DbError> {
    if !changed {
        return Ok(());
    }

    let revision = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO pixiv_bookmark_sync_state (
            pixiv_account_id, resource_revision, updated_at
        )
        VALUES ($1, 1, now())
        ON CONFLICT (pixiv_account_id)
        DO UPDATE SET resource_revision = pixiv_bookmark_sync_state.resource_revision + 1,
                      updated_at = now()
        RETURNING resource_revision
        "#,
    )
    .bind(account_id)
    .fetch_one(&mut **tx)
    .await?;
    EventRepository::new(db.clone())
        .append_in_tx(
            tx,
            EventResource::PixivBookmark,
            account_id,
            EventPayload::PixivBookmarkChanged { revision },
        )
        .await?;
    Ok(())
}

pub(crate) fn visibility_value(visibility: PixivBookmarkVisibility) -> &'static str {
    match visibility {
        PixivBookmarkVisibility::Public => "public",
        PixivBookmarkVisibility::Private => "private",
    }
}
