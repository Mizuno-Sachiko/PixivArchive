use super::{JobRepository, transitions::close_running_attempt};
use crate::{
    ActivatePixivAccount, Db, DbError, EventRepository, PixivAccountRecord, PixivAccountRepository,
    SubscriptionRepository,
};
use pixivarchive_domain::{
    event::{EventPayload, EventResource},
    job::{JobKind, JobLease, JobPriorityPolicy},
    subscription::{PixivAccountState, SubscriptionKind},
};
use sqlx::{Postgres, Row, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

impl JobRepository {
    pub async fn activate_validated_account(
        &self,
        input: ActivatePixivAccount,
        priorities: Option<&JobPriorityPolicy>,
    ) -> Result<PixivAccountRecord, DbError> {
        let mut tx = self.db.begin().await?;
        let account = PixivAccountRepository::new(self.db.clone())
            .activate_validated_in_tx(&mut tx, input)
            .await?;
        if account.state == PixivAccountState::Normal {
            SubscriptionRepository::new(self.db.clone())
                .ensure_fixed_subscriptions_in_tx(&mut tx, account.id, OffsetDateTime::now_utc())
                .await?;
        }
        if let Some(priorities) = priorities {
            resume_account_in_tx(&self.db, &mut tx, account.id, priorities).await?;
        }
        tx.commit().await?;
        Ok(account)
    }

    pub async fn clear_account_credential(
        &self,
        pixiv_account_id: Uuid,
        expected_revision: i64,
    ) -> Result<PixivAccountRecord, DbError> {
        let mut tx = self.db.begin().await?;
        let account = PixivAccountRepository::new(self.db.clone())
            .clear_credential_in_tx(&mut tx, pixiv_account_id, expected_revision)
            .await?;
        let waiting_jobs = move_jobs_to_account_wait(&mut tx, pixiv_account_id, None).await?;
        append_waiting_account_events(&self.db, &mut tx, waiting_jobs).await?;
        tx.commit().await?;
        Ok(account)
    }

    pub async fn block_account_for_job(
        &self,
        lease: JobLease,
        error_class: &str,
        message: Option<&str>,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(
            r#"
            UPDATE job
            SET state = 'waiting_account',
                lease_owner = NULL,
                lease_expires_at = NULL,
                error_class = $4,
                retryable = NULL,
                next_retry_at = NULL,
                updated_at = now(),
                resource_revision = resource_revision + 1
            WHERE id = $1
              AND resource_revision = $2
              AND lease_owner = $3
              AND lease_expires_at > now()
              AND state = 'running'
              AND pixiv_account_id IS NOT NULL
            RETURNING resource_revision, pixiv_account_id
            "#,
        )
        .bind(lease.job_id)
        .bind(lease.resource_revision)
        .bind(lease.lease_owner)
        .bind(error_class)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DbError::LeaseConflict)?;
        let revision: i64 = row.get("resource_revision");
        let pixiv_account_id: Uuid = row.get("pixiv_account_id");

        close_running_attempt(
            &mut tx,
            lease.job_id,
            "failed",
            Some(error_class),
            None,
            message,
        )
        .await?;

        let account_invalidated =
            set_account_credential_invalid(&self.db, &mut tx, pixiv_account_id).await?;
        if !account_invalidated {
            sqlx::query("UPDATE job SET error_class = NULL WHERE id = $1")
                .bind(lease.job_id)
                .execute(&mut *tx)
                .await?;
        }

        let mut waiting_jobs = vec![(lease.job_id, revision)];
        waiting_jobs.extend(
            move_jobs_to_account_wait(
                &mut tx,
                pixiv_account_id,
                account_invalidated.then_some(error_class),
            )
            .await?,
        );
        append_waiting_account_events(&self.db, &mut tx, waiting_jobs).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn block_account(
        &self,
        pixiv_account_id: Uuid,
        error_class: &str,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        let account_invalidated =
            set_account_credential_invalid(&self.db, &mut tx, pixiv_account_id).await?;
        let waiting_jobs = move_jobs_to_account_wait(
            &mut tx,
            pixiv_account_id,
            account_invalidated.then_some(error_class),
        )
        .await?;
        append_waiting_account_events(&self.db, &mut tx, waiting_jobs).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn recover_account(
        &self,
        pixiv_account_id: Uuid,
        priorities: &JobPriorityPolicy,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        let recovered = sqlx::query(
            r#"
            UPDATE pixiv_account
            SET state = 'normal',
                last_validated_at = now(),
                updated_at = now(),
                revision = revision + 1
            WHERE id = $1
              AND state IN ('credential_invalid', 'validating')
            RETURNING id, revision
            "#,
        )
        .bind(pixiv_account_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(recovered) = recovered else {
            tx.commit().await?;
            return Ok(());
        };
        crate::pixiv::append_account_event(
            &self.db,
            &mut tx,
            recovered.get("id"),
            recovered.get("revision"),
        )
        .await?;

        resume_account_in_tx(&self.db, &mut tx, pixiv_account_id, priorities).await?;
        tx.commit().await?;
        Ok(())
    }
}

async fn resume_account_in_tx(
    db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    pixiv_account_id: Uuid,
    priorities: &JobPriorityPolicy,
) -> Result<(), DbError> {
    let released_jobs = sqlx::query(
        r#"
            UPDATE job
            SET state = 'queued',
                error_class = NULL,
                retryable = NULL,
                next_retry_at = NULL,
                updated_at = now(),
                resource_revision = resource_revision + 1
            WHERE pixiv_account_id = $1
              AND state = 'waiting_account'
            RETURNING id, resource_revision
            "#,
    )
    .bind(pixiv_account_id)
    .fetch_all(&mut **tx)
    .await?;
    let events = EventRepository::new(db.clone());
    for row in released_jobs {
        events
            .append_in_tx(
                tx,
                EventResource::Job,
                row.get("id"),
                EventPayload::JobReleasedFromAccountWait {
                    revision: row.get("resource_revision"),
                },
            )
            .await?;
    }

    let subscriptions = sqlx::query(
        r#"
            SELECT id, kind
            FROM subscription
            WHERE pixiv_account_id = $1
              AND enabled = true
            ORDER BY id
            FOR UPDATE
            "#,
    )
    .bind(pixiv_account_id)
    .fetch_all(&mut **tx)
    .await?;

    for subscription in subscriptions {
        let subscription_id: Uuid = subscription.get("id");
        let kind_value: String = subscription.get("kind");
        let kind = SubscriptionKind::from_db_value(&kind_value).ok_or_else(|| {
            DbError::InvalidValue(format!("unknown subscription kind {kind_value}"))
        })?;
        let active_run: Option<Uuid> = sqlx::query_scalar(
            r#"
                SELECT id
                FROM subscription_run
                WHERE subscription_id = $1
                  AND state IN ('queued', 'running')
                LIMIT 1
                "#,
        )
        .bind(subscription_id)
        .fetch_optional(&mut **tx)
        .await?;

        let mut subscription_changed = false;
        if active_run.is_some() {
            let updated = sqlx::query(
                r#"
                    UPDATE subscription
                    SET pending_run = true,
                        pending_cursor_kind = 'backfill',
                        updated_at = now(),
                        revision = revision + 1
                    WHERE id = $1
                      AND (
                        pending_run = false
                        OR pending_cursor_kind <> 'backfill'
                      )
                    "#,
            )
            .bind(subscription_id)
            .execute(&mut **tx)
            .await?;
            subscription_changed = updated.rows_affected() > 0;
        } else {
            let exists: Option<Uuid> = sqlx::query_scalar(
                r#"
                    SELECT id
                    FROM subscription_run
                    WHERE subscription_id = $1
                      AND trigger_kind = 'merged_pending'
                      AND state = 'queued'
                    LIMIT 1
                    "#,
            )
            .bind(subscription_id)
            .fetch_optional(&mut **tx)
            .await?;
            if exists.is_none() {
                crate::subscriptions::SubscriptionRepository::new(db.clone())
                    .create_recovery_run_job_in_tx(
                        tx,
                        subscription_id,
                        priorities.priority_for(JobKind::for_subscription(kind)),
                    )
                    .await?;
                subscription_changed = true;
            }
        }
        if subscription_changed {
            crate::subscriptions::append_subscription_event(db, tx, subscription_id).await?;
        }
    }

    Ok(())
}

async fn set_account_credential_invalid(
    db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    pixiv_account_id: Uuid,
) -> Result<bool, DbError> {
    let updated = sqlx::query(
        r#"
        UPDATE pixiv_account
        SET state = 'credential_invalid',
            updated_at = now(),
            revision = revision + 1
        WHERE id = $1
          AND state <> 'unconfigured'
        RETURNING revision
        "#,
    )
    .bind(pixiv_account_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(updated) = updated else {
        let state: Option<String> =
            sqlx::query_scalar("SELECT state FROM pixiv_account WHERE id = $1")
                .bind(pixiv_account_id)
                .fetch_optional(&mut **tx)
                .await?;
        return match state.as_deref() {
            Some("unconfigured") => Ok(false),
            Some(_) => Err(DbError::RevisionConflict),
            None => Err(DbError::NotFound),
        };
    };
    let revision: i64 = updated.get("revision");
    crate::pixiv::append_account_event(db, tx, pixiv_account_id, revision).await?;
    Ok(true)
}

async fn move_jobs_to_account_wait(
    tx: &mut Transaction<'_, Postgres>,
    pixiv_account_id: Uuid,
    error_class: Option<&str>,
) -> Result<Vec<(Uuid, i64)>, DbError> {
    let rows = sqlx::query(
        r#"
        UPDATE job
        SET state = 'waiting_account',
            error_class = $2,
            retryable = NULL,
            next_retry_at = NULL,
            updated_at = now(),
            resource_revision = resource_revision + 1
        WHERE pixiv_account_id = $1
          AND (
                state = 'queued'
                OR state = 'waiting_storage'
                OR (state = 'failed' AND retryable = true)
          )
        RETURNING id, resource_revision
        "#,
    )
    .bind(pixiv_account_id)
    .bind(error_class)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get("id"), row.get("resource_revision")))
        .collect())
}

async fn append_waiting_account_events(
    db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    waiting_jobs: Vec<(Uuid, i64)>,
) -> Result<(), DbError> {
    let events = EventRepository::new(db.clone());
    for (job_id, revision) in waiting_jobs {
        events
            .append_in_tx(
                tx,
                EventResource::Job,
                job_id,
                EventPayload::JobWaitingAccount { revision },
            )
            .await?;
    }
    Ok(())
}
