use super::{
    JobAttemptRecord, JobRecord, JobRepository, JobStats,
    model::{job_count, job_from_row},
};
use crate::DbError;
use sqlx::{Postgres, Row};
use uuid::Uuid;

impl JobRepository {
    pub async fn list(&self, limit: u16) -> Result<Vec<JobRecord>, DbError> {
        self.list_filtered(limit, None, false).await
    }

    pub async fn list_filtered(
        &self,
        limit: u16,
        kind: Option<&str>,
        errors_only: bool,
    ) -> Result<Vec<JobRecord>, DbError> {
        if limit == 0 || limit > 200 {
            return Err(DbError::InvalidValue(
                "job list limit must be between 1 and 200".to_owned(),
            ));
        }
        let mut query = sqlx::QueryBuilder::<Postgres>::new(
            r#"
            SELECT id,
                   priority_class,
                   kind,
                   payload,
                   state,
                   attempts,
                   available_at,
                   error_class,
                   retryable,
                   next_retry_at,
                   resource_revision,
                   created_at,
                   updated_at
            FROM job
            "#,
        );
        query.push(" WHERE true");
        if let Some(kind) = kind {
            query.push(" AND kind = ").push_bind(kind);
        }
        if errors_only {
            query.push(" AND state IN ('failed', 'waiting_account')");
        }
        query
            .push(" ORDER BY created_at DESC, id DESC LIMIT ")
            .push_bind(i64::from(limit));
        let rows = query.build().fetch_all(self.db.pool()).await?;
        rows.iter().map(job_from_row).collect()
    }

    pub async fn stats(&self) -> Result<JobStats, DbError> {
        let row = sqlx::query(
            r#"
            SELECT
                count(*) AS total,
                count(*) FILTER (WHERE state = 'running') AS running,
                count(*) FILTER (
                    WHERE state IN ('queued', 'waiting_storage')
                       OR (state = 'failed' AND retryable = true)
                ) AS waiting,
                count(*) FILTER (
                    WHERE state = 'waiting_account'
                       OR (state = 'failed' AND retryable IS DISTINCT FROM true)
                ) AS requires_attention
            FROM job
            "#,
        )
        .fetch_one(self.db.pool())
        .await?;
        Ok(JobStats {
            total: job_count(&row, "total")?,
            running: job_count(&row, "running")?,
            waiting: job_count(&row, "waiting")?,
            requires_attention: job_count(&row, "requires_attention")?,
        })
    }

    pub async fn get(&self, job_id: Uuid) -> Result<JobRecord, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id,
                   priority_class,
                   kind,
                   payload,
                   state,
                   attempts,
                   available_at,
                   error_class,
                   retryable,
                   next_retry_at,
                   resource_revision,
                   created_at,
                   updated_at
            FROM job
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .fetch_one(self.db.pool())
        .await?;
        job_from_row(&row)
    }

    pub async fn list_attempts(&self, job_id: Uuid) -> Result<Vec<JobAttemptRecord>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT attempt_number,
                   state,
                   started_at,
                   finished_at,
                   error_class,
                   retryable,
                   message,
                   trace_id
            FROM job_attempt
            WHERE job_id = $1
            ORDER BY attempt_number DESC
            "#,
        )
        .bind(job_id)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows
            .iter()
            .map(|row| JobAttemptRecord {
                attempt_number: row.get("attempt_number"),
                state: row.get("state"),
                started_at: row.get("started_at"),
                finished_at: row.get("finished_at"),
                error_class: row.get("error_class"),
                retryable: row.get("retryable"),
                message: row.get("message"),
                trace_id: row.get("trace_id"),
            })
            .collect())
    }
}
