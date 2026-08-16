use super::*;

impl SubscriptionRepository {
    pub async fn list_due_subscriptions(
        &self,
        now: OffsetDateTime,
        limit: i64,
    ) -> Result<Vec<DueSubscription>, DbError> {
        if limit <= 0 {
            return Ok(Vec::new());
        }

        let rows = sqlx::query(
            r#"
            SELECT s.id,
                   s.revision,
                   s.kind,
                   s.schedule,
                   s.next_run_at,
                   s.pixiv_account_id
            FROM subscription s
            JOIN pixiv_account a ON a.id = s.pixiv_account_id
            WHERE s.enabled = true
              AND s.next_run_at <= $1
              AND a.state IN ('normal', 'restricted')
            ORDER BY s.next_run_at, s.created_at
            LIMIT $2
            "#,
        )
        .bind(now)
        .bind(limit)
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter()
            .map(|row| {
                let next_run_at = row
                    .get::<Option<OffsetDateTime>, _>("next_run_at")
                    .ok_or_else(|| {
                        DbError::InvalidValue("due subscription has no next_run_at".to_owned())
                    })?;
                Ok(DueSubscription {
                    id: row.get("id"),
                    revision: row.get("revision"),
                    kind: row.get("kind"),
                    schedule: row.get("schedule"),
                    next_run_at,
                    pixiv_account_id: row.get("pixiv_account_id"),
                })
            })
            .collect()
    }

    pub async fn schedule_due_subscription(
        &self,
        request: ScheduleDueSubscription,
    ) -> Result<ScheduleDueSubscriptionResult, DbError> {
        self.schedule_due_subscription_with_priority(request, JobPriority::ScheduledCollection)
            .await
    }

    pub async fn schedule_due_subscription_with_priority(
        &self,
        request: ScheduleDueSubscription,
        priority: JobPriority,
    ) -> Result<ScheduleDueSubscriptionResult, DbError> {
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(
            r#"
            SELECT id
            FROM subscription
            WHERE id = $1
              AND revision = $2
              AND next_run_at = $3
              AND enabled = true
              AND next_run_at <= $4
              AND EXISTS (
                    SELECT 1
                    FROM pixiv_account
                    WHERE pixiv_account.id = subscription.pixiv_account_id
                      AND pixiv_account.state IN ('normal', 'restricted')
              )
            FOR UPDATE
            "#,
        )
        .bind(request.subscription_id)
        .bind(request.expected_revision)
        .bind(request.expected_next_run_at)
        .bind(request.now)
        .fetch_optional(&mut *tx)
        .await?;

        if row.is_none() {
            tx.commit().await?;
            return Ok(ScheduleDueSubscriptionResult::Stale);
        }

        let active_runs: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM subscription_run
            WHERE subscription_id = $1
              AND state IN ('queued', 'running')
            "#,
        )
        .bind(request.subscription_id)
        .fetch_one(&mut *tx)
        .await?;

        if active_runs > 0 {
            sqlx::query(
                r#"
                UPDATE subscription
                SET pending_run = true,
                    pending_cursor_kind = CASE
                        WHEN pending_run THEN pending_cursor_kind
                        ELSE 'normal'
                    END,
                    next_run_at = $2,
                    updated_at = now(),
                    revision = revision + 1
                WHERE id = $1
                "#,
            )
            .bind(request.subscription_id)
            .bind(request.next_run_at)
            .execute(&mut *tx)
            .await?;
            append_subscription_event(&self.db, &mut tx, request.subscription_id).await?;
            tx.commit().await?;
            return Ok(ScheduleDueSubscriptionResult::MergedPending {
                subscription_id: request.subscription_id,
            });
        }

        let run = self
            .create_run_job_in_tx(
                &mut tx,
                request.subscription_id,
                "scheduled",
                "normal",
                request.now,
                priority,
            )
            .await?;
        self.mark_subscription_running_in_tx(
            &mut tx,
            request.subscription_id,
            request.now,
            Some(request.next_run_at),
        )
        .await?;
        tx.commit().await?;
        Ok(ScheduleDueSubscriptionResult::Created(run))
    }

    pub async fn start_manual_run(
        &self,
        subscription_id: Uuid,
        backfill: bool,
    ) -> Result<ScheduledSubscriptionRun, DbError> {
        self.start_manual_run_with_priority(
            subscription_id,
            backfill,
            JobPriority::ScheduledCollection,
        )
        .await
    }

    pub async fn start_manual_run_with_priority(
        &self,
        subscription_id: Uuid,
        backfill: bool,
        priority: JobPriority,
    ) -> Result<ScheduledSubscriptionRun, DbError> {
        let mut tx = self.db.begin().await?;
        let (run, subscription_changed) = self
            .start_manual_run_in_tx(
                &mut tx,
                subscription_id,
                backfill,
                priority,
                OffsetDateTime::now_utc(),
            )
            .await?;
        if subscription_changed {
            append_subscription_event(&self.db, &mut tx, subscription_id).await?;
        }
        tx.commit().await?;
        Ok(run)
    }

    pub(super) async fn start_manual_run_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        subscription_id: Uuid,
        backfill: bool,
        priority: JobPriority,
        started_at: OffsetDateTime,
    ) -> Result<(ScheduledSubscriptionRun, bool), DbError> {
        sqlx::query("SELECT id FROM subscription WHERE id = $1 FOR UPDATE")
            .bind(subscription_id)
            .fetch_one(&mut **tx)
            .await?;
        let active = sqlx::query(
            r#"
            SELECT id, job_id, trigger_kind
            FROM subscription_run
            WHERE subscription_id = $1
              AND state IN ('queued', 'running')
            LIMIT 1
            "#,
        )
        .bind(subscription_id)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(row) = active {
            let updated = sqlx::query(
                r#"
                UPDATE subscription
                SET pending_run = true,
                    pending_cursor_kind = CASE
                        WHEN $2 OR pending_cursor_kind = 'backfill' THEN 'backfill'
                        ELSE 'normal'
                    END,
                    updated_at = now(),
                    revision = revision + 1
                WHERE id = $1
                  AND (
                        pending_run = false
                        OR ($2 AND pending_cursor_kind = 'normal')
                  )
                "#,
            )
            .bind(subscription_id)
            .bind(backfill)
            .execute(&mut **tx)
            .await?;
            let run = ScheduledSubscriptionRun {
                subscription_id,
                run_id: row.get("id"),
                job_id: row
                    .get::<Option<Uuid>, _>("job_id")
                    .unwrap_or_else(Uuid::nil),
                trigger_kind: row.get("trigger_kind"),
            };
            return Ok((run, updated.rows_affected() > 0));
        }

        let trigger_kind = if backfill { "backfill" } else { "manual" };
        let run = self
            .create_run_job_in_tx(
                tx,
                subscription_id,
                trigger_kind,
                if backfill { "backfill" } else { "normal" },
                started_at,
                priority,
            )
            .await?;
        self.update_subscription_running_in_tx(tx, subscription_id, started_at, None)
            .await?;
        Ok((run, true))
    }

    async fn mark_subscription_running_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        subscription_id: Uuid,
        started_at: OffsetDateTime,
        next_run_at: Option<OffsetDateTime>,
    ) -> Result<(), DbError> {
        self.update_subscription_running_in_tx(tx, subscription_id, started_at, next_run_at)
            .await?;
        append_subscription_event(&self.db, tx, subscription_id).await
    }

    async fn update_subscription_running_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        subscription_id: Uuid,
        started_at: OffsetDateTime,
        next_run_at: Option<OffsetDateTime>,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE subscription
            SET pending_run = CASE
                    WHEN $3::timestamptz IS NULL THEN pending_run
                    ELSE false
                END,
                pending_cursor_kind = CASE
                    WHEN $3::timestamptz IS NULL THEN pending_cursor_kind
                    ELSE 'normal'
                END,
                recent_state = 'running',
                last_run_at = $2,
                next_run_at = COALESCE($3, next_run_at),
                updated_at = now(),
                revision = revision + 1
            WHERE id = $1
            "#,
        )
        .bind(subscription_id)
        .bind(started_at)
        .bind(next_run_at)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub(super) async fn create_run_job_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        subscription_id: Uuid,
        trigger_kind: &str,
        cursor_kind: &str,
        available_at: OffsetDateTime,
        priority: JobPriority,
    ) -> Result<ScheduledSubscriptionRun, DbError> {
        let run_id = Uuid::now_v7();
        let subscription = sqlx::query(
            r#"
            SELECT subscription.pixiv_account_id,
                   subscription.kind,
                   subscription.params,
                   rule_version.id AS rule_version_id,
                   rule_version.definition AS rule_document
            FROM subscription
            LEFT JOIN download_rule ON download_rule.id = subscription.rule_id
            LEFT JOIN rule_version ON rule_version.id = download_rule.current_version_id
            WHERE subscription.id = $1
            FOR UPDATE OF subscription
            "#,
        )
        .bind(subscription_id)
        .fetch_one(&mut **tx)
        .await?;
        let pixiv_account_id: Uuid = subscription.get("pixiv_account_id");
        let params_snapshot = subscription.get::<Json<Value>, _>("params").0;
        let rule_version_id: Option<Uuid> = subscription.get("rule_version_id");
        let rule_document: Option<Json<Value>> = subscription.get("rule_document");
        let kind_value: String = subscription.get("kind");
        let kind = SubscriptionKind::from_db_value(&kind_value).ok_or_else(|| {
            DbError::InvalidValue(format!("unknown subscription kind {kind_value}"))
        })?;
        if !matches!(cursor_kind, "normal" | "backfill") {
            return Err(DbError::InvalidValue(
                "subscription cursor kind is invalid".to_owned(),
            ));
        }

        sqlx::query(
            r#"
            INSERT INTO subscription_run (
                id,
                subscription_id,
                trigger_kind,
                cursor_kind,
                params_snapshot,
                rule_version_id,
                rule_document,
                state
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, 'queued')
            "#,
        )
        .bind(run_id)
        .bind(subscription_id)
        .bind(trigger_kind)
        .bind(cursor_kind)
        .bind(Json(params_snapshot.clone()))
        .bind(rule_version_id)
        .bind(rule_document)
        .execute(&mut **tx)
        .await?;

        let mut first_job_id = None;
        for unit in subscription_units(kind, &params_snapshot)? {
            let unit_id = Uuid::now_v7();
            let cursor_snapshot = sqlx::query(
                r#"
                SELECT cursor_value
                FROM subscription_cursor
                WHERE subscription_id = $1
                  AND cursor_kind = $2
                  AND source_key = $3
                "#,
            )
            .bind(subscription_id)
            .bind(cursor_kind)
            .bind(&unit.source_key)
            .fetch_optional(&mut **tx)
            .await?
            .map(|row| row.get::<Json<Value>, _>("cursor_value").0);
            sqlx::query(
                r#"
                INSERT INTO subscription_run_unit (
                    id,
                    subscription_run_id,
                    source_key,
                    cursor_kind,
                    params_snapshot,
                    cursor_snapshot,
                    state
                )
                VALUES ($1, $2, $3, $4, $5, $6, 'queued')
                "#,
            )
            .bind(unit_id)
            .bind(run_id)
            .bind(&unit.source_key)
            .bind(cursor_kind)
            .bind(Json(unit.params_snapshot.clone()))
            .bind(cursor_snapshot.map(Json))
            .execute(&mut **tx)
            .await?;

            let mut payload = json!({
                "subscription_id": subscription_id.to_string(),
                "subscription_run_id": run_id.to_string(),
                "subscription_run_unit_id": unit_id.to_string(),
                "trigger_kind": trigger_kind,
                "cursor_kind": cursor_kind,
                "source_key": unit.source_key,
            });
            if let Some(fields) = unit.params_snapshot.as_object() {
                for (key, value) in fields {
                    payload[key] = value.clone();
                }
            }
            let mut job = NewJob::for_kind(priority, unit.job_kind, payload);
            job.pixiv_account_id = Some(pixiv_account_id);
            job.available_at = available_at;
            let job_id = JobRepository::new(self.db.clone())
                .enqueue_in_tx(tx, job)
                .await?;
            first_job_id.get_or_insert(job_id);
            sqlx::query("UPDATE subscription_run_unit SET job_id = $2 WHERE id = $1")
                .bind(unit_id)
                .bind(job_id)
                .execute(&mut **tx)
                .await?;
        }
        let job_id = first_job_id.ok_or_else(|| {
            DbError::InvalidValue("subscription did not produce collection jobs".to_owned())
        })?;

        sqlx::query("UPDATE subscription_run SET job_id = $2 WHERE id = $1")
            .bind(run_id)
            .bind(job_id)
            .execute(&mut **tx)
            .await?;

        Ok(ScheduledSubscriptionRun {
            subscription_id,
            run_id,
            job_id,
            trigger_kind: trigger_kind.to_owned(),
        })
    }
}
