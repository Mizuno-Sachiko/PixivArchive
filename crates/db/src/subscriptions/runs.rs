use super::*;

impl SubscriptionRepository {
    pub async fn last_successful_backfill_at(
        &self,
        subscription_id: Uuid,
    ) -> Result<Option<OffsetDateTime>, DbError> {
        Ok(sqlx::query_scalar(
            r#"
            SELECT max(finished_at)
            FROM subscription_run
            WHERE subscription_id = $1
              AND cursor_kind = 'backfill'
              AND state = 'succeeded'
            "#,
        )
        .bind(subscription_id)
        .fetch_one(self.db.pool())
        .await?)
    }

    pub async fn list_runs(
        &self,
        subscription_id: Uuid,
        limit: u16,
    ) -> Result<Vec<SubscriptionRunSummaryRecord>, DbError> {
        if limit == 0 || limit > 500 {
            return Err(DbError::InvalidValue(
                "subscription run limit must be between 1 and 500".to_owned(),
            ));
        }
        let rows = sqlx::query(
            r#"
            SELECT id,
                   subscription_id,
                   trigger_kind,
                   state,
                   cursor_kind,
                   discovered_count,
                   ignored_count,
                   error_class,
                   trace_id,
                   started_at,
                   finished_at,
                   created_at
            FROM subscription_run
            WHERE subscription_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT $2
            "#,
        )
        .bind(subscription_id)
        .bind(i64::from(limit))
        .fetch_all(self.db.pool())
        .await?;
        rows.iter().map(run_summary_from_row).collect()
    }

    pub async fn load_run(&self, run_id: Uuid) -> Result<SubscriptionRunRecord, DbError> {
        let row = sqlx::query(
            r#"
            SELECT sr.id,
                   sr.subscription_id,
                   sr.job_id,
                   sr.trigger_kind,
                   sr.cursor_kind,
                   sr.state,
                   sr.params_snapshot,
                   sr.rule_version_id,
                   sr.rule_document,
                   s.kind,
                   s.params,
                   s.rule_id,
                   s.pixiv_account_id
            FROM subscription_run sr
            JOIN subscription s ON s.id = sr.subscription_id
            WHERE sr.id = $1
            "#,
        )
        .bind(run_id)
        .fetch_one(self.db.pool())
        .await?;
        run_from_row(&row)
    }

    pub async fn finish_run(
        &self,
        finished: FinishSubscriptionRun,
    ) -> Result<FinishSubscriptionRunResult, DbError> {
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(
            r#"
            SELECT sr.subscription_id,
                   sr.state,
                   s.pending_run,
                   s.pending_cursor_kind
            FROM subscription_run sr
            JOIN subscription s ON s.id = sr.subscription_id
            WHERE sr.id = $1
            FOR UPDATE OF sr, s
            "#,
        )
        .bind(finished.run_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DbError::NotFound)?;

        let current_state: String = row.get("state");
        if current_state != "running" {
            return Err(DbError::RevisionConflict);
        }
        let subscription_id: Uuid = row.get("subscription_id");
        let pending_run: bool = row.get("pending_run");
        let pending_cursor_kind: String = row.get("pending_cursor_kind");
        let state = finished.state.as_str();
        let recent_state = finished.state.recent_state().ok_or_else(|| {
            DbError::InvalidValue("subscription run completion must be terminal".to_owned())
        })?;

        sqlx::query(
            r#"
            UPDATE subscription_run
            SET state = $2,
                finished_at = $3,
                discovered_count = $4,
                ignored_count = $5,
                error_class = $6,
                trace_id = $7
            WHERE id = $1
            "#,
        )
        .bind(finished.run_id)
        .bind(state)
        .bind(finished.finished_at)
        .bind(finished.discovered_count)
        .bind(finished.ignored_count)
        .bind(finished.error_class.as_deref())
        .bind(finished.trace_id)
        .execute(&mut *tx)
        .await?;

        let result = if pending_run {
            mark_subscription_continuation_running_in_tx(&mut tx, subscription_id).await?;
            let merged = self
                .create_run_job_in_tx(
                    &mut tx,
                    subscription_id,
                    "merged_pending",
                    &pending_cursor_kind,
                    finished.finished_at,
                    JobPriority::ScheduledCollection,
                )
                .await?;
            FinishSubscriptionRunResult::MergedPending(merged)
        } else {
            mark_subscription_run_finished_in_tx(&mut tx, subscription_id, recent_state).await?;
            FinishSubscriptionRunResult::Completed
        };

        append_subscription_event(&self.db, &mut tx, subscription_id).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn mark_run_running(&self, run_id: Uuid) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE subscription_run
            SET state = 'running',
                started_at = COALESCE(started_at, now())
            WHERE id = $1
              AND state = 'queued'
            "#,
        )
        .bind(run_id)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    pub async fn stop_active_run(&self, subscription_id: Uuid) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        let run_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id
            FROM subscription_run
            WHERE subscription_id = $1
              AND state IN ('queued', 'running')
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            "#,
        )
        .bind(subscription_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(run_id) = run_id else {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM subscription WHERE id = $1)")
                    .bind(subscription_id)
                    .fetch_one(&mut *tx)
                    .await?;
            return if exists {
                Err(DbError::RevisionConflict)
            } else {
                Err(DbError::NotFound)
            };
        };

        let job_ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT job_id
            FROM subscription_run_unit
            WHERE subscription_run_id = $1
              AND job_id IS NOT NULL
            ORDER BY job_id
            "#,
        )
        .bind(run_id)
        .fetch_all(&mut *tx)
        .await?;

        for job_id in job_ids {
            let state =
                sqlx::query("SELECT state, lease_expires_at FROM job WHERE id = $1 FOR UPDATE")
                    .bind(job_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            let Some(state) = state else {
                continue;
            };
            let state_value: String = state.get("state");
            if !matches!(
                state_value.as_str(),
                "queued" | "running" | "waiting_account" | "waiting_storage" | "failed"
            ) {
                continue;
            }
            sqlx::query(
                r#"
                UPDATE media_artifact_intent
                SET cleanup_after = greatest(cleanup_after, coalesce($2, now()))
                WHERE job_id = $1
                "#,
            )
            .bind(job_id)
            .bind(state.get::<Option<OffsetDateTime>, _>("lease_expires_at"))
            .execute(&mut *tx)
            .await?;
            let revision: i64 = sqlx::query_scalar(
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
                RETURNING resource_revision
                "#,
            )
            .bind(job_id)
            .fetch_one(&mut *tx)
            .await?;
            if state_value == "running" {
                sqlx::query(
                    r#"
                    UPDATE job_attempt
                    SET state = 'cancelled',
                        finished_at = now(),
                        error_class = NULL,
                        retryable = NULL,
                        message = NULL
                    WHERE job_id = $1
                      AND state = 'running'
                    "#,
                )
                .bind(job_id)
                .execute(&mut *tx)
                .await?;
            }
            EventRepository::new(self.db.clone())
                .append_in_tx(
                    &mut tx,
                    EventResource::Job,
                    job_id,
                    EventPayload::JobCancelled { revision },
                )
                .await?;
        }

        let locked = sqlx::query(
            r#"
            SELECT sr.id, sr.state
            FROM subscription_run sr
            JOIN subscription s ON s.id = sr.subscription_id
            WHERE sr.subscription_id = $1
              AND sr.state IN ('queued', 'running')
            ORDER BY sr.created_at DESC, sr.id DESC
            LIMIT 1
            FOR UPDATE OF sr, s
            "#,
        )
        .bind(subscription_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(locked) = locked else {
            return Err(DbError::RevisionConflict);
        };
        if locked.get::<Uuid, _>("id") != run_id {
            return Err(DbError::RevisionConflict);
        }

        sqlx::query(
            r#"
            UPDATE subscription_run_unit
            SET state = 'cancelled',
                error_class = NULL,
                error_message = NULL,
                finished_at = now()
            WHERE subscription_run_id = $1
              AND state IN ('queued', 'running')
            "#,
        )
        .bind(run_id)
        .execute(&mut *tx)
        .await?;
        let counts = sqlx::query(
            r#"
            SELECT coalesce(sum(discovered_count), 0)::int AS discovered_count,
                   coalesce(sum(ignored_count), 0)::int AS ignored_count
            FROM subscription_run_unit
            WHERE subscription_run_id = $1
            "#,
        )
        .bind(run_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"
            UPDATE subscription_run
            SET state = 'cancelled',
                finished_at = now(),
                discovered_count = $2,
                ignored_count = $3
            WHERE id = $1
            "#,
        )
        .bind(run_id)
        .bind(counts.get::<i32, _>("discovered_count"))
        .bind(counts.get::<i32, _>("ignored_count"))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r#"
            UPDATE subscription
            SET pending_run = false,
                pending_cursor_kind = 'normal',
                recent_state = 'paused',
                updated_at = now(),
                revision = revision + 1
            WHERE id = $1
            "#,
        )
        .bind(subscription_id)
        .execute(&mut *tx)
        .await?;
        append_subscription_event(&self.db, &mut tx, subscription_id).await?;
        tx.commit().await?;
        Ok(())
    }
}
