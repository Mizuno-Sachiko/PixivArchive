use super::{
    JobCompletion, JobRecord, JobRepository,
    model::{ImportJobCompletion, job_from_row},
};
use crate::{Db, DbError, EventRepository, subscriptions::append_subscription_event};
use pixivarchive_domain::{
    event::{EventPayload, EventResource},
    job::JobLease,
};
use sqlx::{Postgres, Row, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

impl JobRepository {
    pub async fn lock_active_lease_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        lease: JobLease,
    ) -> Result<(), DbError> {
        let active: Option<i32> = sqlx::query_scalar(
            r#"
            SELECT 1
            FROM job
            WHERE id = $1
              AND resource_revision = $2
              AND lease_owner = $3
              AND lease_expires_at > now()
              AND state = 'running'
            FOR UPDATE
            "#,
        )
        .bind(lease.job_id)
        .bind(lease.resource_revision)
        .bind(lease.lease_owner)
        .fetch_optional(&mut **tx)
        .await?;
        active.ok_or(DbError::LeaseConflict).map(|_| ())
    }

    pub async fn complete(
        &self,
        lease: JobLease,
        completion: JobCompletion,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        self.complete_in_tx(&mut tx, lease, completion).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn complete_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        lease: JobLease,
        completion: JobCompletion,
    ) -> Result<(), DbError> {
        let revision = set_terminal_state(
            tx,
            TerminalStateUpdate {
                lease,
                state: "completed",
                error_class: None,
                retryable: None,
                next_retry_at: None,
                increment_retryable_failure_count: false,
                message: None,
            },
        )
        .await?;
        match completion {
            JobCompletion::TaskOnly => {}
            JobCompletion::Import(completion) => {
                complete_linked_import_run(tx, lease.job_id, completion).await?;
            }
            JobCompletion::Subscription(completion) => {
                crate::subscriptions::SubscriptionRepository::new(self.db.clone())
                    .complete_linked_unit_in_tx(tx, lease.job_id, completion)
                    .await?;
            }
        }
        EventRepository::new(self.db.clone())
            .append_in_tx(
                tx,
                EventResource::Job,
                lease.job_id,
                EventPayload::JobCompleted { revision },
            )
            .await?;
        Ok(())
    }

    pub async fn wait_for_storage(&self, lease: JobLease) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        let revision = set_terminal_state(
            &mut tx,
            TerminalStateUpdate {
                lease,
                state: "waiting_storage",
                error_class: None,
                retryable: None,
                next_retry_at: None,
                increment_retryable_failure_count: false,
                message: None,
            },
        )
        .await?;
        EventRepository::new(self.db.clone())
            .append_in_tx(
                &mut tx,
                EventResource::Job,
                lease.job_id,
                EventPayload::JobWaitingStorage { revision },
            )
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn set_storage_write_allowed(&self, allowed: bool) -> Result<usize, DbError> {
        let mut tx = self.db.begin().await?;
        let rows = if allowed {
            sqlx::query(
                r#"
                UPDATE job
                SET state = 'queued',
                    error_class = NULL,
                    retryable = NULL,
                    next_retry_at = NULL,
                    updated_at = now(),
                    resource_revision = resource_revision + 1
                WHERE state = 'waiting_storage'
                RETURNING id, resource_revision
                "#,
            )
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(
                r#"
                UPDATE job
                SET state = 'waiting_storage',
                    error_class = NULL,
                    retryable = NULL,
                    next_retry_at = NULL,
                    updated_at = now(),
                    resource_revision = resource_revision + 1
                WHERE kind IN ('download_media', 'generate_derivative')
                  AND state = 'queued'
                RETURNING id, resource_revision
                "#,
            )
            .fetch_all(&mut *tx)
            .await?
        };
        let events = EventRepository::new(self.db.clone());
        for row in &rows {
            let payload = if allowed {
                EventPayload::JobReleasedFromStorageWait {
                    revision: row.get("resource_revision"),
                }
            } else {
                EventPayload::JobWaitingStorage {
                    revision: row.get("resource_revision"),
                }
            };
            events
                .append_in_tx(&mut tx, EventResource::Job, row.get("id"), payload)
                .await?;
        }
        tx.commit().await?;
        Ok(rows.len())
    }

    pub async fn fail(
        &self,
        lease: JobLease,
        error_class: &str,
        retryable: bool,
        next_retry_at: Option<OffsetDateTime>,
        message: Option<&str>,
    ) -> Result<(), DbError> {
        if retryable != next_retry_at.is_some() {
            return Err(DbError::InvalidValue(
                "retryable jobs require a retry time and terminal failures cannot have one"
                    .to_owned(),
            ));
        }

        let mut tx = self.db.begin().await?;
        let revision = set_terminal_state(
            &mut tx,
            TerminalStateUpdate {
                lease,
                state: "failed",
                error_class: Some(error_class),
                retryable: Some(retryable),
                next_retry_at,
                increment_retryable_failure_count: retryable && error_class != "credential_invalid",
                message,
            },
        )
        .await?;
        if !retryable {
            fail_linked_import_run(&mut tx, lease.job_id, error_class, message).await?;
            crate::subscriptions::SubscriptionRepository::new(self.db.clone())
                .fail_linked_unit_in_tx(&mut tx, lease.job_id, error_class, message)
                .await?;
        }
        EventRepository::new(self.db.clone())
            .append_in_tx(
                &mut tx,
                EventResource::Job,
                lease.job_id,
                EventPayload::JobFailed { revision },
            )
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn cancel_requested(
        &self,
        job_id: Uuid,
        expected_revision: i64,
    ) -> Result<JobRecord, DbError> {
        let mut tx = self.db.begin().await?;
        let current = sqlx::query(
            "SELECT state, resource_revision, lease_expires_at FROM job WHERE id = $1 FOR UPDATE",
        )
        .bind(job_id)
        .fetch_one(&mut *tx)
        .await?;
        let state: String = current.get("state");
        let revision: i64 = current.get("resource_revision");
        if revision != expected_revision
            || !matches!(
                state.as_str(),
                "queued" | "running" | "waiting_account" | "waiting_storage" | "failed"
            )
        {
            return Err(DbError::RevisionConflict);
        }
        let irreversible_purge_started: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM trash_entry
                WHERE purge_attempts > 0
                  AND work_id::text = (
                      SELECT payload ->> 'work_id'
                      FROM job
                      WHERE id = $1
                        AND kind = 'purge_trash'
                  )
            )
            "#,
        )
        .bind(job_id)
        .fetch_one(&mut *tx)
        .await?;
        if irreversible_purge_started {
            return Err(DbError::RevisionConflict);
        }
        sqlx::query(
            r#"
            UPDATE media_artifact_intent
            SET cleanup_after = greatest(
                cleanup_after,
                coalesce($2, now())
            )
            WHERE job_id = $1
            "#,
        )
        .bind(job_id)
        .bind(current.get::<Option<OffsetDateTime>, _>("lease_expires_at"))
        .execute(&mut *tx)
        .await?;
        let row = sqlx::query(
            r#"
            UPDATE job
            SET state = 'cancelled',
                lease_owner = NULL,
                lease_expires_at = NULL,
                error_class = NULL,
                retryable = NULL,
                next_retry_at = NULL,
                updated_at = now(),
                resource_revision = resource_revision + 1
            WHERE id = $1
            RETURNING id,
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
            "#,
        )
        .bind(job_id)
        .fetch_one(&mut *tx)
        .await?;
        if state == "running" {
            close_running_attempt(&mut tx, job_id, "cancelled", None, None, None).await?;
        }
        cancel_linked_import_run(&mut tx, job_id).await?;
        crate::subscriptions::SubscriptionRepository::new(self.db.clone())
            .cancel_linked_unit_in_tx(&mut tx, job_id)
            .await?;
        let record = job_from_row(&row)?;
        EventRepository::new(self.db.clone())
            .append_in_tx(
                &mut tx,
                EventResource::Job,
                job_id,
                EventPayload::JobCancelled {
                    revision: record.resource_revision,
                },
            )
            .await?;
        tx.commit().await?;
        Ok(record)
    }

    pub async fn retry_requested(
        &self,
        job_id: Uuid,
        expected_revision: i64,
    ) -> Result<JobRecord, DbError> {
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(
            r#"
            UPDATE job
            SET state = 'queued',
                available_at = now(),
                lease_owner = NULL,
                lease_expires_at = NULL,
                error_class = NULL,
                retryable = NULL,
                next_retry_at = NULL,
                updated_at = now(),
                resource_revision = resource_revision + 1
            WHERE id = $1
              AND resource_revision = $2
              AND state = 'failed'
            RETURNING id,
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
            "#,
        )
        .bind(job_id)
        .bind(expected_revision)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DbError::RevisionConflict)?;
        let record = job_from_row(&row)?;
        reset_linked_import_run_for_retry(&mut tx, job_id).await?;
        reset_linked_subscription_unit_for_retry(&self.db, &mut tx, job_id).await?;
        EventRepository::new(self.db.clone())
            .append_in_tx(
                &mut tx,
                EventResource::Job,
                job_id,
                EventPayload::JobQueued {
                    revision: record.resource_revision,
                },
            )
            .await?;
        tx.commit().await?;
        Ok(record)
    }

    pub async fn retryable_failure_count(&self, job_id: Uuid) -> Result<i32, DbError> {
        sqlx::query_scalar("SELECT retryable_failure_count FROM job WHERE id = $1")
            .bind(job_id)
            .fetch_optional(self.db.pool())
            .await?
            .ok_or(DbError::NotFound)
    }
}

struct TerminalStateUpdate<'a> {
    lease: JobLease,
    state: &'a str,
    error_class: Option<&'a str>,
    retryable: Option<bool>,
    next_retry_at: Option<OffsetDateTime>,
    increment_retryable_failure_count: bool,
    message: Option<&'a str>,
}

async fn set_terminal_state(
    tx: &mut Transaction<'_, Postgres>,
    update: TerminalStateUpdate<'_>,
) -> Result<i64, DbError> {
    let row = sqlx::query(
        r#"
        UPDATE job
        SET state = $4,
            lease_owner = NULL,
            lease_expires_at = NULL,
            error_class = $5,
            retryable = $6,
            next_retry_at = $7,
            retryable_failure_count = retryable_failure_count + CASE WHEN $8 THEN 1 ELSE 0 END,
            updated_at = now(),
            resource_revision = resource_revision + 1
        WHERE id = $1
          AND resource_revision = $2
          AND lease_owner = $3
          AND lease_expires_at > now()
          AND state = 'running'
        RETURNING resource_revision
        "#,
    )
    .bind(update.lease.job_id)
    .bind(update.lease.resource_revision)
    .bind(update.lease.lease_owner)
    .bind(update.state)
    .bind(update.error_class)
    .bind(update.retryable)
    .bind(update.next_retry_at)
    .bind(update.increment_retryable_failure_count)
    .fetch_optional(&mut **tx)
    .await?;

    let revision = row
        .map(|row| row.get("resource_revision"))
        .ok_or(DbError::LeaseConflict)?;

    close_running_attempt(
        tx,
        update.lease.job_id,
        update.state,
        update.error_class,
        update.retryable,
        update.message,
    )
    .await?;

    Ok(revision)
}

pub(super) async fn close_running_attempt(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    state: &str,
    error_class: Option<&str>,
    retryable: Option<bool>,
    message: Option<&str>,
) -> Result<(), DbError> {
    let closed = sqlx::query!(
        r#"
        UPDATE job_attempt
        SET state = $2,
            finished_at = now(),
            error_class = $3,
            retryable = $4,
            message = $5
        WHERE job_id = $1
          AND state = 'running'
          AND attempt_number = (
              SELECT max(attempt_number) FROM job_attempt WHERE job_id = $1
          )
        "#,
        job_id,
        state,
        error_class,
        retryable,
        message
    )
    .execute(&mut **tx)
    .await?;
    if closed.rows_affected() != 1 {
        return Err(DbError::LeaseConflict);
    }
    Ok(())
}

async fn reset_linked_subscription_unit_for_retry(
    db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
) -> Result<(), DbError> {
    let Some(linked) = sqlx::query(
        r#"
        SELECT u.id AS unit_id,
               u.subscription_run_id AS run_id,
               sr.subscription_id
        FROM subscription_run_unit u
        JOIN subscription_run sr ON sr.id = u.subscription_run_id
        WHERE u.job_id = $1
        FOR UPDATE OF u, sr
        "#,
    )
    .bind(job_id)
    .fetch_optional(&mut **tx)
    .await?
    else {
        return Ok(());
    };
    let unit_id: Uuid = linked.get("unit_id");
    let run_id: Uuid = linked.get("run_id");
    let subscription_id: Uuid = linked.get("subscription_id");

    let reset = sqlx::query(
        r#"
        UPDATE subscription_run_unit
        SET state = 'queued',
            discovered_count = 0,
            ignored_count = 0,
            error_class = NULL,
            error_message = NULL,
            started_at = NULL,
            finished_at = NULL
        WHERE id = $1
          AND state IN ('queued', 'failed')
        "#,
    )
    .bind(unit_id)
    .execute(&mut **tx)
    .await?;
    if reset.rows_affected() != 1 {
        return Err(DbError::RevisionConflict);
    }

    sqlx::query(
        r#"
        UPDATE subscription_run
        SET state = CASE
                WHEN EXISTS (
                    SELECT 1
                    FROM subscription_run_unit
                    WHERE subscription_run_id = $1
                      AND state = 'running'
                ) THEN 'running'
                ELSE 'queued'
            END,
            started_at = CASE
                WHEN EXISTS (
                    SELECT 1
                    FROM subscription_run_unit
                    WHERE subscription_run_id = $1
                      AND state = 'running'
                ) THEN started_at
                ELSE NULL
            END,
            finished_at = NULL,
            discovered_count = 0,
            ignored_count = 0,
            error_class = NULL,
            trace_id = NULL
        WHERE id = $1
        "#,
    )
    .bind(run_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"
        UPDATE subscription
        SET recent_state = 'running',
            updated_at = now(),
            revision = revision + 1
        WHERE id = $1
        "#,
    )
    .bind(subscription_id)
    .execute(&mut **tx)
    .await?;
    append_subscription_event(db, tx, subscription_id).await
}

async fn reset_linked_import_run_for_retry(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
) -> Result<(), DbError> {
    let updated = sqlx::query(
        r#"
        UPDATE import_run
        SET status = 'queued',
            discovered_count = 0,
            saved_count = 0,
            error_class = NULL,
            error_message = NULL,
            started_at = NULL,
            finished_at = NULL
        WHERE job_id = $1
          AND status IN ('queued', 'failed')
        "#,
    )
    .bind(job_id)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() == 1 {
        return Ok(());
    }

    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM import_run WHERE job_id = $1")
            .bind(job_id)
            .fetch_optional(&mut **tx)
            .await?;
    match status {
        None => Ok(()),
        Some(_) => Err(DbError::RevisionConflict),
    }
}

async fn cancel_linked_import_run(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE import_run
        SET status = 'cancelled',
            error_class = NULL,
            error_message = NULL,
            finished_at = now()
        WHERE job_id = $1
          AND status IN ('queued', 'running')
        "#,
    )
    .bind(job_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn fail_linked_import_run(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    error_class: &str,
    message: Option<&str>,
) -> Result<(), DbError> {
    let updated = sqlx::query(
        r#"
        UPDATE import_run
        SET status = 'failed',
            error_class = $2,
            error_message = COALESCE($3, $2),
            finished_at = now()
        WHERE job_id = $1
          AND status IN ('queued', 'running')
        "#,
    )
    .bind(job_id)
    .bind(error_class)
    .bind(message)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() == 1 {
        return Ok(());
    }

    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM import_run WHERE job_id = $1")
            .bind(job_id)
            .fetch_optional(&mut **tx)
            .await?;
    match status.as_deref() {
        None | Some("failed") => Ok(()),
        Some(_) => Err(DbError::RevisionConflict),
    }
}

async fn complete_linked_import_run(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    completion: ImportJobCompletion,
) -> Result<(), DbError> {
    if !completion.status.is_successful_terminal()
        || completion.discovered_count < 0
        || completion.saved_count < 0
    {
        return Err(DbError::InvalidValue(
            "import job completion is not a successful result".to_owned(),
        ));
    }
    let updated = sqlx::query(
        r#"
        UPDATE import_run
        SET status = $2,
            discovered_count = $3,
            saved_count = $4,
            error_class = NULL,
            error_message = NULL,
            finished_at = now()
        WHERE job_id = $1
          AND status IN ('queued', 'running')
        "#,
    )
    .bind(job_id)
    .bind(completion.status.as_str())
    .bind(completion.discovered_count)
    .bind(completion.saved_count)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(DbError::RevisionConflict);
    }
    Ok(())
}
