use super::*;

impl SubscriptionRepository {
    pub async fn load_unit(&self, unit_id: Uuid) -> Result<SubscriptionRunUnitRecord, DbError> {
        let row = sqlx::query(
            r#"
            SELECT u.id,
                   u.subscription_run_id,
                   u.job_id,
                   u.source_key,
                   u.cursor_kind,
                   u.params_snapshot,
                   u.cursor_snapshot,
                   u.state,
                   u.error_class,
                   u.error_message,
                   sr.rule_version_id,
                   sr.rule_document,
                   s.id AS subscription_id,
                   s.kind,
                   s.schedule,
                   s.rule_id,
                   s.pixiv_account_id
            FROM subscription_run_unit u
            JOIN subscription_run sr ON sr.id = u.subscription_run_id
            JOIN subscription s ON s.id = sr.subscription_id
            WHERE u.id = $1
            "#,
        )
        .bind(unit_id)
        .fetch_one(self.db.pool())
        .await?;
        unit_from_row(&row)
    }

    pub async fn load_unit_by_job(
        &self,
        job_id: Uuid,
    ) -> Result<SubscriptionRunUnitRecord, DbError> {
        let row = sqlx::query(
            r#"
            SELECT u.id,
                   u.subscription_run_id,
                   u.job_id,
                   u.source_key,
                   u.cursor_kind,
                   u.params_snapshot,
                   u.cursor_snapshot,
                   u.state,
                   u.error_class,
                   u.error_message,
                   sr.rule_version_id,
                   sr.rule_document,
                   s.id AS subscription_id,
                   s.kind,
                   s.schedule,
                   s.rule_id,
                   s.pixiv_account_id
            FROM subscription_run_unit u
            JOIN subscription_run sr ON sr.id = u.subscription_run_id
            JOIN subscription s ON s.id = sr.subscription_id
            WHERE u.job_id = $1
            "#,
        )
        .bind(job_id)
        .fetch_one(self.db.pool())
        .await?;
        unit_from_row(&row)
    }

    pub async fn list_units_for_run(
        &self,
        run_id: Uuid,
    ) -> Result<Vec<SubscriptionRunUnitRecord>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT u.id,
                   u.subscription_run_id,
                   u.job_id,
                   u.source_key,
                   u.cursor_kind,
                   u.params_snapshot,
                   u.cursor_snapshot,
                   u.state,
                   u.error_class,
                   u.error_message,
                   sr.rule_version_id,
                   sr.rule_document,
                   s.id AS subscription_id,
                   s.kind,
                   s.schedule,
                   s.rule_id,
                   s.pixiv_account_id
            FROM subscription_run_unit u
            JOIN subscription_run sr ON sr.id = u.subscription_run_id
            JOIN subscription s ON s.id = sr.subscription_id
            WHERE u.subscription_run_id = $1
            ORDER BY u.source_key
            "#,
        )
        .bind(run_id)
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter().map(|row| unit_from_row(&row)).collect()
    }

    pub async fn mark_unit_running(&self, unit_id: Uuid) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        self.mark_unit_running_in_tx(&mut tx, unit_id, None).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn mark_unit_running_job(
        &self,
        lease: JobLease,
        unit_id: Uuid,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        JobRepository::new(self.db.clone())
            .lock_active_lease_in_tx(&mut tx, lease)
            .await?;
        self.mark_unit_running_in_tx(&mut tx, unit_id, Some(lease.job_id))
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn mark_unit_running_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        unit_id: Uuid,
        expected_job_id: Option<Uuid>,
    ) -> Result<(), DbError> {
        let updated_run_id: Option<Uuid> = sqlx::query_scalar(
            r#"
            UPDATE subscription_run_unit
            SET state = 'running',
                started_at = COALESCE(started_at, now())
            WHERE id = $1
              AND ($2::uuid IS NULL OR job_id = $2)
              AND state = 'queued'
            RETURNING subscription_run_id
            "#,
        )
        .bind(unit_id)
        .bind(expected_job_id)
        .fetch_optional(&mut **tx)
        .await?;
        let run_id = if let Some(run_id) = updated_run_id {
            run_id
        } else {
            let row = sqlx::query(
                r#"
                SELECT subscription_run_id, job_id
                FROM subscription_run_unit
                WHERE id = $1
                "#,
            )
            .bind(unit_id)
            .fetch_one(&mut **tx)
            .await?;
            if expected_job_id
                .is_some_and(|job_id| row.get::<Option<Uuid>, _>("job_id") != Some(job_id))
            {
                return Err(DbError::RevisionConflict);
            }
            row.get("subscription_run_id")
        };
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
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn record_ranking_entry(
        &self,
        run_id: Uuid,
        pixiv_work_id: i64,
        rank: u32,
        score: Value,
    ) -> Result<bool, DbError> {
        let result = sqlx::query(
            r#"
            INSERT INTO ranking_entry (id, subscription_run_id, source_key, pixiv_work_id, rank, score)
            VALUES ($1, $2, 'legacy', $3, $4, $5)
            ON CONFLICT (subscription_run_id, source_key, pixiv_work_id) DO NOTHING
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(run_id)
        .bind(pixiv_work_id)
        .bind(
            i32::try_from(rank)
                .map_err(|_| DbError::InvalidValue("ranking rank is too large".to_owned()))?,
        )
        .bind(Json(score))
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn record_ranking_unit_entry(
        &self,
        input: RecordRankingUnitEntry,
    ) -> Result<bool, DbError> {
        self.record_ranking_unit_entry_with_lease(None, input).await
    }

    pub async fn record_ranking_unit_entry_for_job(
        &self,
        lease: JobLease,
        input: RecordRankingUnitEntry,
    ) -> Result<bool, DbError> {
        self.record_ranking_unit_entry_with_lease(Some(lease), input)
            .await
    }

    async fn record_ranking_unit_entry_with_lease(
        &self,
        lease: Option<JobLease>,
        input: RecordRankingUnitEntry,
    ) -> Result<bool, DbError> {
        let mut tx = self.db.begin().await?;
        if let Some(lease) = lease {
            JobRepository::new(self.db.clone())
                .lock_active_lease_in_tx(&mut tx, lease)
                .await?;
        }
        let result = sqlx::query(
            r#"
            INSERT INTO ranking_entry (
                id, subscription_run_id, subscription_run_unit_id, source_key, pixiv_work_id, rank, score
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (subscription_run_id, source_key, pixiv_work_id) DO NOTHING
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(input.run_id)
        .bind(input.unit_id)
        .bind(input.source_key)
        .bind(input.pixiv_work_id)
        .bind(
            i32::try_from(input.rank)
                .map_err(|_| DbError::InvalidValue("ranking rank is too large".to_owned()))?,
        )
        .bind(Json(input.score))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn finish_unit(
        &self,
        finished: FinishSubscriptionRunUnit,
    ) -> Result<FinishSubscriptionRunUnitResult, DbError> {
        let mut tx = self.db.begin().await?;
        let result = self.finish_unit_in_tx(&mut tx, finished, None).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub(crate) async fn complete_linked_unit_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        job_id: Uuid,
        finished: FinishSubscriptionRunUnit,
    ) -> Result<FinishSubscriptionRunUnitResult, DbError> {
        if finished.state != SubscriptionRunStatus::Succeeded
            || finished.error_class.is_some()
            || finished.error_message.is_some()
        {
            return Err(DbError::InvalidValue(
                "subscription job completion is not a successful result".to_owned(),
            ));
        }
        self.finish_unit_in_tx(tx, finished, Some(job_id)).await
    }

    async fn finish_unit_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        finished: FinishSubscriptionRunUnit,
        expected_job_id: Option<Uuid>,
    ) -> Result<FinishSubscriptionRunUnitResult, DbError> {
        let row = sqlx::query(
            r#"
            SELECT u.job_id,
                   u.subscription_run_id,
                   sr.subscription_id,
                   s.pending_run,
                   s.pending_cursor_kind
            FROM subscription_run_unit u
            JOIN subscription_run sr ON sr.id = u.subscription_run_id
            JOIN subscription s ON s.id = sr.subscription_id
            WHERE u.id = $1
            FOR UPDATE OF u, sr, s
            "#,
        )
        .bind(finished.unit_id)
        .fetch_one(&mut **tx)
        .await?;
        if expected_job_id
            .is_some_and(|job_id| row.get::<Option<Uuid>, _>("job_id") != Some(job_id))
        {
            return Err(DbError::RevisionConflict);
        }
        let run_id: Uuid = row.get("subscription_run_id");
        let subscription_id: Uuid = row.get("subscription_id");
        let pending_run: bool = row.get("pending_run");
        let pending_cursor_kind: String = row.get("pending_cursor_kind");

        let updated = sqlx::query(
            r#"
            UPDATE subscription_run_unit
            SET state = $2,
                discovered_count = $3,
                ignored_count = $4,
                error_class = $5,
                error_message = $6,
                finished_at = now()
            WHERE id = $1
              AND (
                    state = 'running'
                    OR ($2 = 'failed' AND state = 'queued')
              )
            "#,
        )
        .bind(finished.unit_id)
        .bind(finished.state.as_str())
        .bind(finished.discovered_count)
        .bind(finished.ignored_count)
        .bind(finished.error_class.as_deref())
        .bind(finished.error_message.as_deref())
        .execute(&mut **tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(DbError::RevisionConflict);
        }

        if finished.state == SubscriptionRunStatus::Succeeded
            && let Some(cursor_value) = finished.cursor_value
        {
            sqlx::query(
                r#"
                INSERT INTO subscription_cursor (id, subscription_id, cursor_kind, source_key, cursor_value)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (subscription_id, cursor_kind, source_key)
                DO UPDATE SET cursor_value = excluded.cursor_value,
                              updated_at = now()
                "#,
            )
            .bind(Uuid::now_v7())
            .bind(subscription_id)
            .bind(&finished.cursor_kind)
            .bind(&finished.source_key)
            .bind(Json(cursor_value))
            .execute(&mut **tx)
            .await?;
        }

        let result = self
            .finish_parent_if_idle_in_tx(
                tx,
                run_id,
                subscription_id,
                pending_run,
                &pending_cursor_kind,
                expected_job_id,
            )
            .await?;
        Ok(result)
    }

    pub(crate) async fn cancel_linked_unit_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        job_id: Uuid,
    ) -> Result<(), DbError> {
        let Some(row) = sqlx::query(
            r#"
            SELECT u.id AS unit_id,
                   u.state AS unit_state,
                   u.subscription_run_id,
                   sr.subscription_id,
                   s.pending_run,
                   s.pending_cursor_kind
            FROM subscription_run_unit u
            JOIN subscription_run sr ON sr.id = u.subscription_run_id
            JOIN subscription s ON s.id = sr.subscription_id
            WHERE u.job_id = $1
            FOR UPDATE OF u, sr, s
            "#,
        )
        .bind(job_id)
        .fetch_optional(&mut **tx)
        .await?
        else {
            return Ok(());
        };
        let unit_state: String = row.get("unit_state");
        if !matches!(unit_state.as_str(), "queued" | "running") {
            return Ok(());
        }
        let unit_id: Uuid = row.get("unit_id");
        let run_id: Uuid = row.get("subscription_run_id");
        let subscription_id: Uuid = row.get("subscription_id");
        let pending_run: bool = row.get("pending_run");
        let pending_cursor_kind: String = row.get("pending_cursor_kind");

        sqlx::query(
            r#"
            UPDATE subscription_run_unit
            SET state = 'cancelled',
                error_class = NULL,
                error_message = NULL,
                finished_at = now()
            WHERE id = $1
            "#,
        )
        .bind(unit_id)
        .execute(&mut **tx)
        .await?;
        self.finish_parent_if_idle_in_tx(
            tx,
            run_id,
            subscription_id,
            pending_run,
            &pending_cursor_kind,
            Some(job_id),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn fail_linked_unit_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        job_id: Uuid,
        error_class: &str,
        error_message: Option<&str>,
    ) -> Result<(), DbError> {
        let Some(row) = sqlx::query(
            r#"
            SELECT u.id AS unit_id,
                   u.state AS unit_state,
                   u.subscription_run_id,
                   sr.subscription_id,
                   s.pending_run,
                   s.pending_cursor_kind
            FROM subscription_run_unit u
            JOIN subscription_run sr ON sr.id = u.subscription_run_id
            JOIN subscription s ON s.id = sr.subscription_id
            WHERE u.job_id = $1
            FOR UPDATE OF u, sr, s
            "#,
        )
        .bind(job_id)
        .fetch_optional(&mut **tx)
        .await?
        else {
            return Ok(());
        };
        let unit_state: String = row.get("unit_state");
        if unit_state == "failed" {
            return Ok(());
        }
        if !matches!(unit_state.as_str(), "queued" | "running") {
            return Err(DbError::RevisionConflict);
        }
        let unit_id: Uuid = row.get("unit_id");
        let run_id: Uuid = row.get("subscription_run_id");
        let subscription_id: Uuid = row.get("subscription_id");
        let pending_run: bool = row.get("pending_run");
        let pending_cursor_kind: String = row.get("pending_cursor_kind");

        sqlx::query(
            r#"
            UPDATE subscription_run_unit
            SET state = 'failed',
                discovered_count = 0,
                ignored_count = 0,
                error_class = $2,
                error_message = $3,
                finished_at = now()
            WHERE id = $1
            "#,
        )
        .bind(unit_id)
        .bind(error_class)
        .bind(error_message)
        .execute(&mut **tx)
        .await?;
        self.finish_parent_if_idle_in_tx(
            tx,
            run_id,
            subscription_id,
            pending_run,
            &pending_cursor_kind,
            Some(job_id),
        )
        .await?;
        Ok(())
    }

    async fn finish_parent_if_idle_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        run_id: Uuid,
        subscription_id: Uuid,
        pending_run: bool,
        pending_cursor_kind: &str,
        continuation_job_id: Option<Uuid>,
    ) -> Result<FinishSubscriptionRunUnitResult, DbError> {
        let remaining: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM subscription_run_unit
            WHERE subscription_run_id = $1
              AND state IN ('queued', 'running')
            "#,
        )
        .bind(run_id)
        .fetch_one(&mut **tx)
        .await?;
        if remaining > 0 {
            return Ok(FinishSubscriptionRunUnitResult::ParentStillRunning);
        }

        let aggregate = sqlx::query(
            r#"
            SELECT sum(discovered_count)::int AS discovered_count,
                   sum(ignored_count)::int AS ignored_count,
                   bool_or(state = 'failed') AS any_failed,
                   bool_or(state = 'cancelled') AS any_cancelled
            FROM subscription_run_unit
            WHERE subscription_run_id = $1
            "#,
        )
        .bind(run_id)
        .fetch_one(&mut **tx)
        .await?;
        let discovered_count: i32 = aggregate
            .get::<Option<i32>, _>("discovered_count")
            .unwrap_or(0);
        let ignored_count: i32 = aggregate
            .get::<Option<i32>, _>("ignored_count")
            .unwrap_or(0);
        let any_failed = aggregate
            .get::<Option<bool>, _>("any_failed")
            .unwrap_or(false);
        let any_cancelled = aggregate
            .get::<Option<bool>, _>("any_cancelled")
            .unwrap_or(false);
        let parent_state = if any_failed {
            "failed"
        } else if any_cancelled {
            "cancelled"
        } else {
            "succeeded"
        };
        sqlx::query(
            r#"
            UPDATE subscription_run
            SET state = $2,
                finished_at = now(),
                discovered_count = $3,
                ignored_count = $4
            WHERE id = $1
            "#,
        )
        .bind(run_id)
        .bind(parent_state)
        .bind(discovered_count)
        .bind(ignored_count)
        .execute(&mut **tx)
        .await?;

        let result = if pending_run {
            mark_subscription_continuation_running_in_tx(tx, subscription_id).await?;
            let priority = continuation_priority(tx, continuation_job_id).await?;
            let merged = self
                .create_run_job_in_tx(
                    tx,
                    subscription_id,
                    "merged_pending",
                    pending_cursor_kind,
                    OffsetDateTime::now_utc(),
                    priority,
                )
                .await?;
            FinishSubscriptionRunUnitResult::MergedPending(merged)
        } else {
            let recent_state = match parent_state {
                "succeeded" => SubscriptionRecentState::Succeeded,
                "failed" => SubscriptionRecentState::Failed,
                "cancelled" => SubscriptionRecentState::Paused,
                _ => {
                    return Err(DbError::InvalidValue(format!(
                        "unknown completed subscription state {parent_state}"
                    )));
                }
            };
            mark_subscription_run_finished_in_tx(tx, subscription_id, recent_state).await?;
            FinishSubscriptionRunUnitResult::ParentCompleted
        };
        append_subscription_event(&self.db, tx, subscription_id).await?;
        Ok(result)
    }

    pub async fn record_unit_attempt_failure(
        &self,
        unit_id: Uuid,
        error_class: &str,
        error_message: Option<&str>,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        self.record_unit_attempt_failure_in_tx(&mut tx, unit_id, error_class, error_message, None)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn record_unit_attempt_failure_job(
        &self,
        lease: JobLease,
        unit_id: Uuid,
        error_class: &str,
        error_message: Option<&str>,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        JobRepository::new(self.db.clone())
            .lock_active_lease_in_tx(&mut tx, lease)
            .await?;
        self.record_unit_attempt_failure_in_tx(
            &mut tx,
            unit_id,
            error_class,
            error_message,
            Some(lease.job_id),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn record_unit_attempt_failure_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        unit_id: Uuid,
        error_class: &str,
        error_message: Option<&str>,
        expected_job_id: Option<Uuid>,
    ) -> Result<(), DbError> {
        let updated = sqlx::query(
            r#"
            UPDATE subscription_run_unit
            SET state = 'queued',
                discovered_count = 0,
                ignored_count = 0,
                error_class = $2,
                error_message = $3,
                finished_at = NULL
            WHERE id = $1
              AND ($4::uuid IS NULL OR job_id = $4)
              AND state = 'running'
            "#,
        )
        .bind(unit_id)
        .bind(error_class)
        .bind(error_message)
        .bind(expected_job_id)
        .execute(&mut **tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(DbError::RevisionConflict);
        }
        Ok(())
    }
}

async fn continuation_priority(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Option<Uuid>,
) -> Result<JobPriority, DbError> {
    let Some(job_id) = job_id else {
        return Ok(JobPriority::ScheduledCollection);
    };
    let value: String = sqlx::query_scalar("SELECT priority_class FROM job WHERE id = $1")
        .bind(job_id)
        .fetch_one(&mut **tx)
        .await?;
    JobPriority::from_db_value(&value)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown job priority {value}")))
}
