use crate::{Db, DbError, EventRepository, JobCompletion, JobRepository};
use pixivarchive_domain::{
    event::{EventPayload, EventResource},
    job::JobLease,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Row, Transaction};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Clone)]
pub struct TrashRepository {
    db: Db,
}

impl TrashRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn load_purge_plan(&self, work_id: Uuid) -> Result<TrashPurgePlan, DbError> {
        let pixiv_work_id: i64 = sqlx::query_scalar(
            r#"
            SELECT pixiv_work_id
            FROM work
            WHERE id = $1
              AND collection_state = 'trash'
              AND EXISTS (
                  SELECT 1
                  FROM trash_entry
                  WHERE trash_entry.work_id = work.id
              )
            "#,
        )
        .bind(work_id)
        .fetch_one(self.db.pool())
        .await?;
        let paths: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT media_revision.source_path AS path
            FROM work_page
            JOIN media_revision ON media_revision.work_page_id = work_page.id
            WHERE work_page.work_id = $1
            UNION
            SELECT derivative.path
            FROM work_page
            JOIN media_revision ON media_revision.work_page_id = work_page.id
            JOIN derivative ON derivative.media_revision_id = media_revision.id
            WHERE work_page.work_id = $1
            ORDER BY path
            "#,
        )
        .bind(work_id)
        .fetch_all(self.db.pool())
        .await?;
        Ok(TrashPurgePlan {
            work_id,
            pixiv_work_id,
            relative_paths: paths.into_iter().map(PathBuf::from).collect(),
        })
    }

    pub async fn begin_purge(&self, work_id: Uuid) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        self.begin_purge_in_tx(&mut tx, work_id).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn begin_purge_job(&self, lease: JobLease, work_id: Uuid) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        JobRepository::new(self.db.clone())
            .lock_active_lease_in_tx(&mut tx, lease)
            .await?;
        self.begin_purge_in_tx(&mut tx, work_id).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn begin_purge_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        work_id: Uuid,
    ) -> Result<(), DbError> {
        let updated = sqlx::query(
            r#"
            UPDATE trash_entry
            SET purge_state = 'running',
                purge_attempts = purge_attempts + 1,
                last_attempt_at = now(),
                failure_message = NULL,
                failure_details = '[]'::jsonb
            WHERE work_id = $1
              AND purge_state IN ('pending', 'failed')
            "#,
        )
        .bind(work_id)
        .execute(&mut **tx)
        .await?;
        if updated.rows_affected() == 0 {
            let running: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM trash_entry WHERE work_id = $1 AND purge_state = 'running')",
            )
            .bind(work_id)
            .fetch_one(&mut **tx)
            .await?;
            if !running {
                return Err(DbError::RevisionConflict);
            }
        }
        Ok(())
    }

    pub async fn purge_completed(&self, work_id: Uuid) -> Result<bool, DbError> {
        sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM app_event
                WHERE resource = 'work'
                  AND resource_id = $1
                  AND payload ->> 'type' = 'work_deleted'
            )
            "#,
        )
        .bind(work_id)
        .fetch_one(self.db.pool())
        .await
        .map_err(Into::into)
    }

    pub async fn record_failure(
        &self,
        work_id: Uuid,
        failures: &[TrashPurgeFailure],
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        self.record_failure_in_tx(&mut tx, work_id, failures)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn record_failure_job(
        &self,
        lease: JobLease,
        work_id: Uuid,
        failures: &[TrashPurgeFailure],
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        JobRepository::new(self.db.clone())
            .lock_active_lease_in_tx(&mut tx, lease)
            .await?;
        self.record_failure_in_tx(&mut tx, work_id, failures)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn record_failure_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        work_id: Uuid,
        failures: &[TrashPurgeFailure],
    ) -> Result<(), DbError> {
        if failures.is_empty() {
            return Err(DbError::InvalidValue(
                "trash purge failure list must not be empty".to_owned(),
            ));
        }
        let details = serde_json::to_value(failures)
            .map_err(|error| DbError::InvalidValue(error.to_string()))?;
        let message = format!("{} media files could not be deleted", failures.len());
        let updated = sqlx::query(
            r#"
            UPDATE trash_entry
            SET purge_state = 'failed',
                failure_message = $2,
                failure_details = $3
            WHERE work_id = $1
              AND purge_state = 'running'
            "#,
        )
        .bind(work_id)
        .bind(message)
        .bind(details)
        .execute(&mut **tx)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(DbError::RevisionConflict);
        }
        Ok(())
    }

    pub async fn complete_purge(
        &self,
        work_id: Uuid,
        deletion_method: &str,
    ) -> Result<(), DbError> {
        validate_deletion_method(deletion_method)?;
        let mut tx = self.db.begin().await?;
        self.complete_purge_in_tx(&mut tx, work_id, deletion_method)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn complete_purge_job(
        &self,
        lease: JobLease,
        work_id: Uuid,
        deletion_method: &str,
    ) -> Result<(), DbError> {
        validate_deletion_method(deletion_method)?;
        let mut tx = self.db.begin().await?;
        JobRepository::new(self.db.clone())
            .complete_in_tx(&mut tx, lease, JobCompletion::TaskOnly)
            .await?;
        match self
            .complete_purge_in_tx(&mut tx, work_id, deletion_method)
            .await
        {
            Ok(()) => {}
            Err(DbError::NotFound) if self.purge_completed_in_tx(&mut tx, work_id).await? => {}
            Err(error) => return Err(error),
        }
        tx.commit().await?;
        Ok(())
    }

    async fn complete_purge_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        work_id: Uuid,
        deletion_method: &str,
    ) -> Result<(), DbError> {
        let work = sqlx::query(
            r#"
            SELECT pixiv_work_id, resource_revision
            FROM work
            WHERE id = $1
              AND collection_state = 'trash'
              AND EXISTS (
                  SELECT 1
                  FROM trash_entry
                  WHERE trash_entry.work_id = work.id
                    AND trash_entry.purge_state = 'running'
              )
            FOR UPDATE
            "#,
        )
        .bind(work_id)
        .fetch_optional(&mut **tx)
        .await?;
        let Some(work) = work else {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM work WHERE id = $1)")
                    .bind(work_id)
                    .fetch_one(&mut **tx)
                    .await?;
            return Err(if exists {
                DbError::RevisionConflict
            } else {
                DbError::NotFound
            });
        };
        let pixiv_work_id: i64 = work.try_get("pixiv_work_id")?;
        let resource_revision: i64 = work.try_get("resource_revision")?;
        sqlx::query("DELETE FROM work WHERE id = $1")
            .bind(work_id)
            .execute(&mut **tx)
            .await?;
        let marker_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO deletion_marker (id, pixiv_work_id, deletion_method)
            VALUES ($1, $2, $3)
            ON CONFLICT (pixiv_work_id)
            DO UPDATE SET deleted_at = now(),
                          deletion_method = excluded.deletion_method
            RETURNING id
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(pixiv_work_id)
        .bind(deletion_method)
        .fetch_one(&mut **tx)
        .await?;
        EventRepository::new(self.db.clone())
            .append_in_tx(
                tx,
                EventResource::Work,
                work_id,
                EventPayload::WorkDeleted {
                    revision: resource_revision + 1,
                },
            )
            .await?;
        EventRepository::new(self.db.clone())
            .append_in_tx(
                tx,
                EventResource::DeletionMarker,
                marker_id,
                EventPayload::DeletionMarkerCreated {
                    pixiv_work_id,
                    deletion_method: deletion_method.to_owned(),
                },
            )
            .await?;
        Ok(())
    }

    async fn purge_completed_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        work_id: Uuid,
    ) -> Result<bool, DbError> {
        sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM app_event
                WHERE resource = 'work'
                  AND resource_id = $1
                  AND payload ->> 'type' = 'work_deleted'
            )
            "#,
        )
        .bind(work_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
    }
}

fn validate_deletion_method(deletion_method: &str) -> Result<(), DbError> {
    matches!(deletion_method, "manual_purge" | "retention_expired")
        .then_some(())
        .ok_or_else(|| DbError::InvalidValue(format!("unknown deletion method {deletion_method}")))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrashPurgePlan {
    pub work_id: Uuid,
    pub pixiv_work_id: i64,
    pub relative_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrashPurgeFailure {
    pub relative_path: PathBuf,
    pub error: String,
}
