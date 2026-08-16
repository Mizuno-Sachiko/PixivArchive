use super::JobRepository;
use crate::works::{trash_selection_ctes, validated_trash_batch_ids};
use crate::{DbError, EventRepository};
use pixivarchive_domain::{
    event::{EventPayload, EventResource},
    job::{JobKind, JobLease, JobPriority, NewJob},
    work::{DuePurge, TrashSelectionExpression},
};
use serde_json::json;
use sqlx::{Postgres, Row, Transaction, types::Json};
use std::collections::HashMap;
use time::OffsetDateTime;
use uuid::Uuid;

impl JobRepository {
    pub async fn enqueue(&self, job: NewJob) -> Result<Uuid, DbError> {
        let mut tx = self.db.begin().await?;
        let id = self.enqueue_in_tx(&mut tx, job).await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn enqueue_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        job: NewJob,
    ) -> Result<Uuid, DbError> {
        let id = Uuid::now_v7();
        let state: String = sqlx::query_scalar(
            r#"
            INSERT INTO job (
                id,
                priority_class,
                kind,
                payload,
                pixiv_account_id,
                state,
                error_class,
                available_at
            )
            VALUES (
                $1,
                $2,
                $3,
                $4,
                $5,
                CASE
                    WHEN EXISTS (
                        SELECT 1
                        FROM pixiv_account
                        WHERE id = $5
                          AND state IN ('unconfigured', 'credential_invalid')
                    ) THEN 'waiting_account'
                    ELSE 'queued'
                END,
                CASE
                    WHEN EXISTS (
                        SELECT 1
                        FROM pixiv_account
                        WHERE id = $5
                          AND state = 'credential_invalid'
                    ) THEN 'credential_invalid'
                    ELSE NULL
                END,
                $6
            )
            RETURNING state
            "#,
        )
        .bind(id)
        .bind(job.priority.as_str())
        .bind(job.kind)
        .bind(Json(job.payload))
        .bind(job.pixiv_account_id)
        .bind(job.available_at)
        .fetch_one(&mut **tx)
        .await?;
        let event = if state == "waiting_account" {
            EventPayload::JobWaitingAccount { revision: 1 }
        } else {
            EventPayload::JobQueued { revision: 1 }
        };
        EventRepository::new(self.db.clone())
            .append_in_tx(tx, EventResource::Job, id, event)
            .await?;
        Ok(id)
    }

    pub async fn enqueue_download_if_absent(
        &self,
        pixiv_account_id: Uuid,
        work_id: Uuid,
        pixiv_work_id: i64,
        priority: JobPriority,
    ) -> Result<Uuid, DbError> {
        self.enqueue_download_if_absent_with_lease(
            None,
            pixiv_account_id,
            work_id,
            pixiv_work_id,
            priority,
        )
        .await
    }

    pub async fn enqueue_download_if_absent_for_job(
        &self,
        lease: JobLease,
        pixiv_account_id: Uuid,
        work_id: Uuid,
        pixiv_work_id: i64,
        priority: JobPriority,
    ) -> Result<Uuid, DbError> {
        self.enqueue_download_if_absent_with_lease(
            Some(lease),
            pixiv_account_id,
            work_id,
            pixiv_work_id,
            priority,
        )
        .await
    }

    async fn enqueue_download_if_absent_with_lease(
        &self,
        lease: Option<JobLease>,
        pixiv_account_id: Uuid,
        work_id: Uuid,
        pixiv_work_id: i64,
        priority: JobPriority,
    ) -> Result<Uuid, DbError> {
        let mut tx = self.db.begin().await?;
        if let Some(lease) = lease {
            self.lock_active_lease_in_tx(&mut tx, lease).await?;
        }
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(pixiv_work_id)
            .execute(&mut *tx)
            .await?;
        let existing: Option<Uuid> = sqlx::query_scalar(
            r#"
            SELECT id
            FROM job
            WHERE kind = 'download_media'
              AND payload ->> 'pixiv_work_id' = $1
              AND (
                state IN ('queued', 'running', 'waiting_account', 'waiting_storage')
                OR (state = 'failed' AND retryable = true)
              )
            ORDER BY created_at
            LIMIT 1
            "#,
        )
        .bind(pixiv_work_id.to_string())
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(job_id) = existing {
            tx.commit().await?;
            return Ok(job_id);
        }

        let mut job = NewJob::for_kind(
            priority,
            JobKind::DownloadMedia,
            json!({
                "pixiv_work_id": pixiv_work_id,
                "work_id": work_id,
            }),
        );
        job.pixiv_account_id = Some(pixiv_account_id);
        let job_id = self.enqueue_in_tx(&mut tx, job).await?;
        tx.commit().await?;
        Ok(job_id)
    }

    pub async fn enqueue_trash_purges_if_absent(
        &self,
        work_ids: &[Uuid],
        deletion_method: &str,
        priority: JobPriority,
    ) -> Result<Vec<DuePurge>, DbError> {
        validate_deletion_method(deletion_method)?;
        let work_ids = validated_trash_batch_ids(work_ids)?;
        let mut tx = self.db.begin().await?;
        let entries = sqlx::query(
            r#"
            SELECT work_id, purge_state
            FROM trash_entry
            WHERE work_id = ANY($1)
            ORDER BY work_id
            FOR UPDATE
            "#,
        )
        .bind(&work_ids)
        .fetch_all(&mut *tx)
        .await?;
        if entries.len() != work_ids.len() {
            return Err(DbError::NotFound);
        }
        let entries = locked_trash_entries(entries)?;
        let purges = self
            .enqueue_locked_trash_purges_in_tx(&mut tx, &entries, deletion_method, priority)
            .await?;
        tx.commit().await?;
        Ok(purges)
    }

    pub async fn enqueue_all_trash_purges_if_absent(
        &self,
        deletion_method: &str,
        priority: JobPriority,
    ) -> Result<u64, DbError> {
        validate_deletion_method(deletion_method)?;
        let mut tx = self.db.begin().await?;
        let entries = sqlx::query(
            r#"
            SELECT work_id, purge_state
            FROM trash_entry
            ORDER BY work_id
            FOR UPDATE
            "#,
        )
        .fetch_all(&mut *tx)
        .await?;
        let entries = locked_trash_entries(entries)?;
        self.enqueue_locked_trash_purges_in_tx(&mut tx, &entries, deletion_method, priority)
            .await?;
        tx.commit().await?;
        u64::try_from(entries.len())
            .map_err(|_| DbError::InvalidValue("trash collection is too large".to_owned()))
    }

    pub async fn enqueue_trash_selection_purges_if_absent(
        &self,
        expression: &TrashSelectionExpression,
        deletion_method: &str,
        priority: JobPriority,
    ) -> Result<u64, DbError> {
        validate_deletion_method(deletion_method)?;
        let mut query = trash_selection_ctes(expression)?;
        query.push(
            r#"
            SELECT trash_entry.work_id, trash_entry.purge_state
            FROM trash_entry
            JOIN selected_trash ON selected_trash.work_id = trash_entry.work_id
            WHERE trash_entry.purge_state IN ('pending', 'failed')
            ORDER BY trash_entry.work_id
            FOR UPDATE OF trash_entry
            "#,
        );
        let mut tx = self.db.begin().await?;
        let entries = query.build().fetch_all(&mut *tx).await?;
        let entries = locked_trash_entries(entries)?;
        self.enqueue_locked_trash_purges_in_tx(&mut tx, &entries, deletion_method, priority)
            .await?;
        let accepted_count = u64::try_from(entries.len())
            .map_err(|_| DbError::InvalidValue("trash selection is too large".to_owned()))?;
        tx.commit().await?;
        Ok(accepted_count)
    }

    pub async fn enqueue_due_trash_purges_if_absent(
        &self,
        now: OffsetDateTime,
        limit: u32,
        deletion_method: &str,
        priority: JobPriority,
    ) -> Result<Vec<DuePurge>, DbError> {
        validate_deletion_method(deletion_method)?;
        let mut tx = self.db.begin().await?;
        let entries = sqlx::query(
            r#"
            WITH due AS (
                SELECT work_id
                FROM trash_entry
                WHERE scheduled_purge_at <= $1
                  AND purge_state IN ('pending', 'failed')
                ORDER BY scheduled_purge_at, work_id
                LIMIT $2
            )
            SELECT trash_entry.work_id, trash_entry.purge_state
            FROM trash_entry
            JOIN due ON due.work_id = trash_entry.work_id
            ORDER BY trash_entry.work_id
            FOR UPDATE OF trash_entry
            "#,
        )
        .bind(now)
        .bind(i64::from(limit))
        .fetch_all(&mut *tx)
        .await?;
        let entries = locked_trash_entries(entries)?;
        let purges = self
            .enqueue_locked_trash_purges_in_tx(&mut tx, &entries, deletion_method, priority)
            .await?;
        tx.commit().await?;
        Ok(purges)
    }

    async fn enqueue_locked_trash_purges_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        entries: &[LockedTrashEntry],
        deletion_method: &str,
        priority: JobPriority,
    ) -> Result<Vec<DuePurge>, DbError> {
        let work_ids = entries
            .iter()
            .map(|entry| entry.work_id)
            .collect::<Vec<_>>();
        if priority == JobPriority::Immediate && !work_ids.is_empty() {
            // A manual purge may reuse a scheduler-created job that has not started yet.
            sqlx::query(
                r#"
                UPDATE job
                SET priority_class = $2,
                    updated_at = now()
                WHERE kind = 'purge_trash'
                  AND (payload ->> 'work_id')::uuid = ANY($1)
                  AND priority_class <> $2
                  AND (
                    state IN ('queued', 'waiting_account', 'waiting_storage')
                    OR (state = 'failed' AND retryable = true)
                  )
                "#,
            )
            .bind(&work_ids)
            .bind(priority.as_str())
            .execute(&mut **tx)
            .await?;
        }
        let existing_rows = if work_ids.is_empty() {
            Vec::new()
        } else {
            sqlx::query(
                r#"
                SELECT id, (payload ->> 'work_id')::uuid AS work_id
                FROM job
                WHERE kind = 'purge_trash'
                  AND (payload ->> 'work_id')::uuid = ANY($1)
                  AND (
                    state IN ('queued', 'running', 'waiting_account', 'waiting_storage')
                    OR (state = 'failed' AND retryable = true)
                  )
                ORDER BY created_at, id
                "#,
            )
            .bind(&work_ids)
            .fetch_all(&mut **tx)
            .await?
        };
        let mut existing_jobs = HashMap::with_capacity(existing_rows.len());
        for row in existing_rows {
            existing_jobs
                .entry(row.try_get::<Uuid, _>("work_id")?)
                .or_insert(row.try_get::<Uuid, _>("id")?);
        }

        let mut purges = Vec::with_capacity(entries.len());
        for entry in entries {
            if let Some(job_id) = existing_jobs.get(&entry.work_id).copied() {
                purges.push(DuePurge {
                    work_id: entry.work_id,
                    job_id,
                });
                continue;
            }
            if !matches!(entry.purge_state.as_str(), "pending" | "failed") {
                return Err(DbError::RevisionConflict);
            }
            let job_id = self
                .enqueue_in_tx(
                    tx,
                    NewJob::for_kind(
                        priority,
                        JobKind::PurgeTrash,
                        json!({
                            "work_id": entry.work_id,
                            "deletion_method": deletion_method,
                        }),
                    ),
                )
                .await?;
            purges.push(DuePurge {
                work_id: entry.work_id,
                job_id,
            });
        }
        Ok(purges)
    }
}

struct LockedTrashEntry {
    work_id: Uuid,
    purge_state: String,
}

fn locked_trash_entries(
    rows: Vec<sqlx::postgres::PgRow>,
) -> Result<Vec<LockedTrashEntry>, DbError> {
    rows.into_iter()
        .map(|row| {
            Ok(LockedTrashEntry {
                work_id: row.try_get("work_id")?,
                purge_state: row.try_get("purge_state")?,
            })
        })
        .collect()
}

fn validate_deletion_method(deletion_method: &str) -> Result<(), DbError> {
    if matches!(deletion_method, "manual_purge" | "retention_expired") {
        Ok(())
    } else {
        Err(DbError::InvalidValue(format!(
            "unknown deletion method {deletion_method}"
        )))
    }
}
