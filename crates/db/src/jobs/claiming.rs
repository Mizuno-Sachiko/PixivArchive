use super::{JobHeartbeatRecord, JobRepository};
use crate::{DbError, EventRepository};
use pixivarchive_domain::{
    event::{EventPayload, EventResource},
    job::{ClaimedJob, JobPriority, JobQuotaSelection, JobState},
};
use sqlx::{Row, types::Json};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

impl JobRepository {
    pub async fn claim_next(
        &self,
        lease_owner: Uuid,
        selection: &JobQuotaSelection,
        lease_duration: Duration,
    ) -> Result<Option<ClaimedJob>, DbError> {
        if lease_duration <= Duration::ZERO {
            return Err(DbError::InvalidValue(
                "job claim lease duration must be positive".to_owned(),
            ));
        }
        if selection.is_empty() {
            return Ok(None);
        }

        let priorities = selection.priority_values();
        let restrict_kinds = selection.has_kind_restriction();
        let kinds = selection.kind_values();
        let lease_microseconds =
            i64::try_from(lease_duration.whole_microseconds()).map_err(|_| {
                DbError::InvalidValue("job claim lease duration is too large".to_owned())
            })?;
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(
            r#"
            WITH candidate AS (
                SELECT id,
                       (state = 'running' AND lease_expires_at <= now()) AS lease_expired
                FROM job
                WHERE priority_class = ANY($1)
                  AND (NOT $2 OR kind = ANY($3))
                  AND available_at <= now()
                  AND (
                    state = 'queued'
                    OR (state = 'running' AND lease_expires_at <= now())
                    OR (state = 'failed' AND retryable = true AND next_retry_at <= now())
                  )
                ORDER BY array_position($1, priority_class), COALESCE(next_retry_at, available_at), created_at
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            UPDATE job
            SET state = 'running',
                lease_owner = $4,
                lease_expires_at = now() + ($5::bigint * interval '1 microsecond'),
                attempts = attempts + 1,
                error_class = NULL,
                retryable = NULL,
                next_retry_at = NULL,
                updated_at = now(),
                resource_revision = resource_revision + 1
            FROM candidate
            WHERE job.id = candidate.id
            RETURNING job.id,
                      job.priority_class,
                      job.kind,
                      job.payload,
                      job.state,
                      job.attempts,
                      job.lease_owner,
                      job.lease_expires_at,
                      job.resource_revision,
                      candidate.lease_expired
            "#,
        )
        .bind(&priorities)
        .bind(restrict_kinds)
        .bind(&kinds)
        .bind(lease_owner)
        .bind(lease_microseconds)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };

        if row.get::<Option<bool>, _>("lease_expired").unwrap_or(false) {
            let closed = sqlx::query!(
                r#"
                UPDATE job_attempt
                SET state = 'failed',
                    finished_at = now(),
                    error_class = 'lease_expired',
                    retryable = true
                WHERE job_id = $1
                  AND state = 'running'
                  AND attempt_number = $2
                "#,
                row.get::<Uuid, _>("id"),
                row.get::<i32, _>("attempts") - 1
            )
            .execute(&mut *tx)
            .await?;
            if closed.rows_affected() != 1 {
                return Err(DbError::LeaseConflict);
            }
        }

        sqlx::query!(
            r#"
            INSERT INTO job_attempt (id, job_id, attempt_number, state)
            VALUES ($1, $2, $3, 'running')
            "#,
            Uuid::now_v7(),
            row.get::<Uuid, _>("id"),
            row.get::<i32, _>("attempts")
        )
        .execute(&mut *tx)
        .await?;
        EventRepository::new(self.db.clone())
            .append_in_tx(
                &mut tx,
                EventResource::Job,
                row.get::<Uuid, _>("id"),
                EventPayload::JobClaimed {
                    revision: row.get::<i64, _>("resource_revision"),
                },
            )
            .await?;
        tx.commit().await?;

        let priority_class = row.get::<String, _>("priority_class");
        let priority = JobPriority::from_db_value(&priority_class).ok_or_else(|| {
            DbError::InvalidValue(format!("unknown job priority {}", priority_class))
        })?;
        let state_value = row.get::<String, _>("state");
        let state = JobState::from_db_value(&state_value)
            .ok_or_else(|| DbError::InvalidValue(format!("unknown job state {}", state_value)))?;
        let lease_expires_at = row
            .get::<Option<OffsetDateTime>, _>("lease_expires_at")
            .ok_or_else(|| {
                DbError::InvalidValue("claimed job lease expiry is missing".to_owned())
            })?;
        let stored_lease_owner = row.get::<Option<Uuid>, _>("lease_owner").ok_or_else(|| {
            DbError::InvalidValue("claimed job lease owner is missing".to_owned())
        })?;
        let payload = row.get::<Json<serde_json::Value>, _>("payload");
        let claimed = ClaimedJob {
            id: row.get("id"),
            priority,
            kind: row.get("kind"),
            payload: payload.0,
            state,
            attempt_number: row.get("attempts"),
            lease_owner: stored_lease_owner,
            lease_expires_at,
            resource_revision: row.get("resource_revision"),
        };
        Ok(Some(claimed))
    }

    pub async fn heartbeat(
        &self,
        job_id: Uuid,
        expected_revision: i64,
        lease_owner: Uuid,
        extend_by: Duration,
    ) -> Result<JobHeartbeatRecord, DbError> {
        if extend_by <= Duration::ZERO {
            return Err(DbError::InvalidValue(
                "job heartbeat extension must be positive".to_owned(),
            ));
        }

        let extend_microseconds = i64::try_from(extend_by.whole_microseconds()).map_err(|_| {
            DbError::InvalidValue("job heartbeat extension is too large".to_owned())
        })?;
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(
            r#"
            UPDATE job
            SET lease_expires_at = GREATEST(
                    lease_expires_at,
                    now() + ($4::bigint * interval '1 microsecond')
                ),
                updated_at = now()
            WHERE id = $1
              AND resource_revision = $2
              AND lease_owner = $3
              AND state = 'running'
              AND lease_expires_at > now()
            RETURNING resource_revision, lease_expires_at
            "#,
        )
        .bind(job_id)
        .bind(expected_revision)
        .bind(lease_owner)
        .bind(extend_microseconds)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(row) = row {
            let record = JobHeartbeatRecord {
                resource_revision: row.get("resource_revision"),
                lease_expires_at: row.get("lease_expires_at"),
            };
            tx.commit().await?;
            return Ok(record);
        }

        let current =
            sqlx::query("SELECT resource_revision, lease_owner, state FROM job WHERE id = $1")
                .bind(job_id)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;

        let Some(current) = current else {
            return Err(DbError::RevisionConflict);
        };
        if current.get::<String, _>("state") != "running"
            || current.get::<i64, _>("resource_revision") != expected_revision
        {
            return Err(DbError::RevisionConflict);
        }
        if current.get::<Option<Uuid>, _>("lease_owner") != Some(lease_owner) {
            return Err(DbError::LeaseConflict);
        }

        Err(DbError::LeaseConflict)
    }
}
