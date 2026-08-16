use super::*;
use time::Duration;

macro_rules! subscription_projection {
    () => {
        "id, pixiv_account_id, rule_id, name, kind, enabled, schedule, params, next_run_at, pending_run, recent_state, revision"
    };
}

struct FixedSubscriptionUpdate {
    account_id: Uuid,
    expected_revision: i64,
    kind: SubscriptionKind,
    enabled: bool,
    interval_minutes: i64,
    lookback_pages: i64,
    changed_at: OffsetDateTime,
    initial_backfill_priority: Option<JobPriority>,
}

impl SubscriptionRepository {
    pub async fn create_subscription(
        &self,
        input: CreateSubscription,
    ) -> Result<SubscriptionRecord, DbError> {
        if input.name.trim().is_empty() {
            return Err(DbError::InvalidValue(
                "subscription name is required".to_owned(),
            ));
        }
        validate_subscription_params(input.kind, &input.params)?;
        let schedule = subscription_schedule_value(input.interval_minutes, input.lookback_pages)?;
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(concat!(
            r#"
            INSERT INTO subscription (
                id, pixiv_account_id, rule_id, name, kind, enabled, schedule, params, next_run_at
            )
            VALUES ($1, $2, $3, $4, $5, true, $6, $7, $8)
            RETURNING "#,
            subscription_projection!()
        ))
        .bind(Uuid::now_v7())
        .bind(input.pixiv_account_id)
        .bind(input.rule_id)
        .bind(input.name.trim())
        .bind(input.kind.as_str())
        .bind(Json(schedule))
        .bind(Json(input.params))
        .bind(input.next_run_at)
        .fetch_one(&mut *tx)
        .await?;
        let record = subscription_from_row(&row)?;
        append_subscription_event(&self.db, &mut tx, record.id).await?;
        tx.commit().await?;
        Ok(record)
    }

    pub async fn ensure_following_subscription(
        &self,
        account_id: Uuid,
        next_run_at: OffsetDateTime,
    ) -> Result<SubscriptionRecord, DbError> {
        let mut tx = self.db.begin().await?;
        let (record, inserted) = self
            .ensure_following_subscription_in_tx(&mut tx, account_id, next_run_at)
            .await?;
        self.append_initial_event_if_inserted(&mut tx, &record, inserted)
            .await?;
        tx.commit().await?;
        Ok(record)
    }

    async fn ensure_following_subscription_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        account_id: Uuid,
        next_run_at: OffsetDateTime,
    ) -> Result<(SubscriptionRecord, bool), DbError> {
        let schedule = json!({
            "interval_minutes": 15,
            "lookback_pages": 1,
        });
        let params = json!({
            "mode": "all",
            "source": "following",
            "language": "zh",
        });
        let inserted = sqlx::query(concat!(
            r#"
            INSERT INTO subscription (
                id, pixiv_account_id, rule_id, name, kind, enabled, schedule, params, next_run_at
            )
            VALUES ($1, $2, NULL, '关注动态', 'following', true, $3, $4, $5)
            ON CONFLICT (pixiv_account_id) WHERE kind = 'following' DO NOTHING
            RETURNING "#,
            subscription_projection!()
        ))
        .bind(Uuid::now_v7())
        .bind(account_id)
        .bind(Json(schedule))
        .bind(Json(params))
        .bind(next_run_at)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(row) = inserted {
            return Ok((subscription_from_row(&row)?, true));
        }

        let row = sqlx::query(concat!(
            "SELECT ",
            subscription_projection!(),
            r#"
            FROM subscription
            WHERE pixiv_account_id = $1
              AND kind = 'following'
            "#
        ))
        .bind(account_id)
        .fetch_one(&mut **tx)
        .await?;
        Ok((subscription_from_row(&row)?, false))
    }

    pub async fn ensure_bookmarks_subscription(
        &self,
        account_id: Uuid,
        next_run_at: OffsetDateTime,
    ) -> Result<SubscriptionRecord, DbError> {
        let mut tx = self.db.begin().await?;
        let (record, inserted) = self
            .ensure_bookmarks_subscription_in_tx(&mut tx, account_id, next_run_at)
            .await?;
        self.append_initial_event_if_inserted(&mut tx, &record, inserted)
            .await?;
        tx.commit().await?;
        Ok(record)
    }

    async fn ensure_bookmarks_subscription_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        account_id: Uuid,
        next_run_at: OffsetDateTime,
    ) -> Result<(SubscriptionRecord, bool), DbError> {
        let schedule = json!({
            "interval_minutes": 30,
            "lookback_pages": 2,
        });
        let params = json!({
            "mode": "all",
            "visibility": "all",
            "full_reconcile_hours": 24,
        });
        let inserted = sqlx::query(concat!(
            r#"
            INSERT INTO subscription (
                id, pixiv_account_id, rule_id, name, kind, enabled, schedule, params, next_run_at
            )
            VALUES ($1, $2, NULL, '收藏同步', 'bookmarks', false, $3, $4, $5)
            ON CONFLICT (pixiv_account_id) WHERE kind = 'bookmarks' DO NOTHING
            RETURNING "#,
            subscription_projection!()
        ))
        .bind(Uuid::now_v7())
        .bind(account_id)
        .bind(Json(schedule))
        .bind(Json(params))
        .bind(next_run_at)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(row) = inserted {
            return Ok((subscription_from_row(&row)?, true));
        }

        let row = sqlx::query(concat!(
            "SELECT ",
            subscription_projection!(),
            r#"
            FROM subscription
            WHERE pixiv_account_id = $1
              AND kind = 'bookmarks'
            "#
        ))
        .bind(account_id)
        .fetch_one(&mut **tx)
        .await?;
        Ok((subscription_from_row(&row)?, false))
    }

    async fn append_initial_event_if_inserted(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        record: &SubscriptionRecord,
        inserted: bool,
    ) -> Result<(), DbError> {
        if inserted {
            append_subscription_event(&self.db, tx, record.id).await?;
        }
        Ok(())
    }

    pub(crate) async fn ensure_fixed_subscriptions_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        account_id: Uuid,
        next_run_at: OffsetDateTime,
    ) -> Result<(), DbError> {
        let (following, following_inserted) = self
            .ensure_following_subscription_in_tx(tx, account_id, next_run_at)
            .await?;
        self.append_initial_event_if_inserted(tx, &following, following_inserted)
            .await?;

        let (bookmarks, bookmarks_inserted) = self
            .ensure_bookmarks_subscription_in_tx(tx, account_id, next_run_at)
            .await?;
        self.append_initial_event_if_inserted(tx, &bookmarks, bookmarks_inserted)
            .await?;
        Ok(())
    }

    pub async fn list_subscriptions(&self) -> Result<Vec<SubscriptionRecord>, DbError> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            subscription_projection!(),
            r#"
            FROM subscription
            ORDER BY name, id
            "#
        ))
        .fetch_all(self.db.pool())
        .await?;
        rows.iter().map(subscription_from_row).collect()
    }

    pub async fn subscription(&self, subscription_id: Uuid) -> Result<SubscriptionRecord, DbError> {
        let row = sqlx::query(concat!(
            "SELECT ",
            subscription_projection!(),
            r#"
            FROM subscription
            WHERE id = $1
            "#
        ))
        .bind(subscription_id)
        .fetch_one(self.db.pool())
        .await?;
        subscription_from_row(&row)
    }

    pub async fn following_subscription(
        &self,
        account_id: Uuid,
    ) -> Result<SubscriptionRecord, DbError> {
        let row = sqlx::query(concat!(
            "SELECT ",
            subscription_projection!(),
            r#"
            FROM subscription
            WHERE pixiv_account_id = $1
              AND kind = 'following'
            "#
        ))
        .bind(account_id)
        .fetch_one(self.db.pool())
        .await?;
        subscription_from_row(&row)
    }

    pub async fn bookmarks_subscription(
        &self,
        account_id: Uuid,
    ) -> Result<SubscriptionRecord, DbError> {
        let row = sqlx::query(concat!(
            "SELECT ",
            subscription_projection!(),
            r#"
            FROM subscription
            WHERE pixiv_account_id = $1
              AND kind = 'bookmarks'
            "#
        ))
        .bind(account_id)
        .fetch_one(self.db.pool())
        .await?;
        subscription_from_row(&row)
    }

    pub async fn update_subscription(
        &self,
        input: UpdateSubscription,
    ) -> Result<SubscriptionRecord, DbError> {
        if input.name.trim().is_empty() {
            return Err(DbError::InvalidValue(
                "subscription name is required".to_owned(),
            ));
        }
        let schedule = subscription_schedule_value(input.interval_minutes, input.lookback_pages)?;
        let mut tx = self.db.begin().await?;
        let kind_value: String =
            sqlx::query_scalar("SELECT kind FROM subscription WHERE id = $1 FOR UPDATE")
                .bind(input.id)
                .fetch_one(&mut *tx)
                .await?;
        let kind = SubscriptionKind::from_db_value(&kind_value).ok_or_else(|| {
            DbError::InvalidValue(format!("unknown subscription kind {kind_value}"))
        })?;
        validate_subscription_params(kind, &input.params)?;
        let row = sqlx::query(concat!(
            r#"
            UPDATE subscription
            SET pixiv_account_id = $3,
                rule_id = $4,
                name = $5,
                enabled = $6,
                schedule = $7,
                params = $8,
                next_run_at = $9,
                updated_at = now(),
                revision = revision + 1
            WHERE id = $1
              AND revision = $2
            RETURNING "#,
            subscription_projection!()
        ))
        .bind(input.id)
        .bind(input.expected_revision)
        .bind(input.pixiv_account_id)
        .bind(input.rule_id)
        .bind(input.name.trim())
        .bind(input.enabled)
        .bind(Json(schedule))
        .bind(Json(input.params))
        .bind(input.next_run_at)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DbError::RevisionConflict)?;
        let record = subscription_from_row(&row)?;
        EventRepository::new(self.db.clone())
            .append_in_tx(
                &mut tx,
                EventResource::Subscription,
                record.id,
                EventPayload::SubscriptionChanged {
                    revision: record.revision,
                },
            )
            .await?;
        tx.commit().await?;
        Ok(record)
    }

    pub async fn set_subscription_enabled(
        &self,
        subscription_id: Uuid,
        expected_revision: i64,
        enabled: bool,
    ) -> Result<SubscriptionRecord, DbError> {
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(concat!(
            r#"
            UPDATE subscription
            SET enabled = $3,
                updated_at = now(),
                revision = revision + 1
            WHERE id = $1
              AND revision = $2
            RETURNING "#,
            subscription_projection!()
        ))
        .bind(subscription_id)
        .bind(expected_revision)
        .bind(enabled)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM subscription WHERE id = $1)")
                    .bind(subscription_id)
                    .fetch_one(&mut *tx)
                    .await?;
            tx.commit().await?;
            return if exists {
                Err(DbError::RevisionConflict)
            } else {
                Err(DbError::NotFound)
            };
        };
        let record = subscription_from_row(&row)?;
        EventRepository::new(self.db.clone())
            .append_in_tx(
                &mut tx,
                EventResource::Subscription,
                record.id,
                EventPayload::SubscriptionChanged {
                    revision: record.revision,
                },
            )
            .await?;
        tx.commit().await?;
        Ok(record)
    }

    pub async fn configure_following_subscription(
        &self,
        account_id: Uuid,
        expected_revision: i64,
        enabled: bool,
        interval_minutes: i64,
        changed_at: OffsetDateTime,
    ) -> Result<SubscriptionRecord, DbError> {
        self.configure_fixed_subscription(FixedSubscriptionUpdate {
            account_id,
            expected_revision,
            kind: SubscriptionKind::Following,
            enabled,
            interval_minutes,
            lookback_pages: 1,
            changed_at,
            initial_backfill_priority: None,
        })
        .await
    }

    pub async fn configure_bookmarks_subscription(
        &self,
        account_id: Uuid,
        expected_revision: i64,
        enabled: bool,
        interval_minutes: i64,
        changed_at: OffsetDateTime,
        initial_backfill_priority: Option<JobPriority>,
    ) -> Result<SubscriptionRecord, DbError> {
        self.configure_fixed_subscription(FixedSubscriptionUpdate {
            account_id,
            expected_revision,
            kind: SubscriptionKind::Bookmarks,
            enabled,
            interval_minutes,
            lookback_pages: 2,
            changed_at,
            initial_backfill_priority,
        })
        .await
    }

    async fn configure_fixed_subscription(
        &self,
        update: FixedSubscriptionUpdate,
    ) -> Result<SubscriptionRecord, DbError> {
        let FixedSubscriptionUpdate {
            account_id,
            expected_revision,
            kind,
            enabled,
            interval_minutes,
            lookback_pages,
            changed_at,
            initial_backfill_priority,
        } = update;
        if !(15..=1_440).contains(&interval_minutes) {
            return Err(DbError::InvalidValue(
                "synchronization interval must be between 15 and 1440 minutes".to_owned(),
            ));
        }
        let schedule = subscription_schedule_value(interval_minutes, lookback_pages)?;
        let next_run_at = changed_at
            .checked_add(Duration::minutes(interval_minutes))
            .ok_or_else(|| {
                DbError::InvalidValue("synchronization next run time is out of range".to_owned())
            })?;
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(concat!(
            r#"
            UPDATE subscription
            SET enabled = $3,
                schedule = $4,
                next_run_at = $5,
                updated_at = now(),
                revision = revision + 1
            WHERE pixiv_account_id = $1
              AND kind = $6
              AND revision = $2
            RETURNING "#,
            subscription_projection!()
        ))
        .bind(account_id)
        .bind(expected_revision)
        .bind(enabled)
        .bind(Json(schedule))
        .bind(next_run_at)
        .bind(kind.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM subscription WHERE pixiv_account_id = $1 AND kind = $2)",
            )
            .bind(account_id)
            .bind(kind.as_str())
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            return if exists {
                Err(DbError::RevisionConflict)
            } else {
                Err(DbError::NotFound)
            };
        };
        let configured = subscription_from_row(&row)?;
        if let Some(priority) = initial_backfill_priority {
            self.start_manual_run_in_tx(&mut tx, configured.id, true, priority, changed_at)
                .await?;
        }
        append_subscription_event(&self.db, &mut tx, configured.id).await?;
        let row = sqlx::query(concat!(
            "SELECT ",
            subscription_projection!(),
            " FROM subscription WHERE id = $1"
        ))
        .bind(configured.id)
        .fetch_one(&mut *tx)
        .await?;
        let record = subscription_from_row(&row)?;
        tx.commit().await?;
        Ok(record)
    }

    pub async fn delete_subscription(
        &self,
        subscription_id: Uuid,
        expected_revision: i64,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        let deleted = sqlx::query(
            r#"
            DELETE FROM subscription
            WHERE id = $1
              AND revision = $2
              AND NOT EXISTS (
                    SELECT 1
                    FROM subscription_run
                    WHERE subscription_id = $1
                      AND state IN ('queued', 'running')
              )
            RETURNING id
            "#,
        )
        .bind(subscription_id)
        .bind(expected_revision)
        .fetch_optional(&mut *tx)
        .await?;
        if deleted.is_none() {
            let exists = sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM subscription WHERE id = $1)",
            )
            .bind(subscription_id)
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            return if exists {
                Err(DbError::RevisionConflict)
            } else {
                Err(DbError::NotFound)
            };
        }
        EventRepository::new(self.db.clone())
            .append_in_tx(
                &mut tx,
                EventResource::Subscription,
                subscription_id,
                EventPayload::SubscriptionChanged {
                    revision: expected_revision + 1,
                },
            )
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn has_enabled_r18_subscription(&self, account_id: Uuid) -> Result<bool, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT params
            FROM subscription
            WHERE pixiv_account_id = $1
              AND enabled = true
            "#,
        )
        .bind(account_id)
        .fetch_all(self.db.pool())
        .await?;
        Ok(rows.into_iter().any(|row| {
            let params = row.get::<Json<Value>, _>("params").0;
            let ranking_has_r18 =
                params
                    .get("modes")
                    .and_then(Value::as_array)
                    .is_some_and(|modes| {
                        modes
                            .iter()
                            .any(|mode| matches!(mode.as_str(), Some("r18" | "r18g")))
                    });
            let single_mode_is_r18 =
                matches!(params.get("mode").and_then(Value::as_str), Some("r18"));
            ranking_has_r18 || single_mode_is_r18
        }))
    }

    pub(crate) async fn create_recovery_run_job_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        subscription_id: Uuid,
        priority: JobPriority,
    ) -> Result<ScheduledSubscriptionRun, DbError> {
        let pending_cursor_kind: String =
            sqlx::query_scalar("SELECT pending_cursor_kind FROM subscription WHERE id = $1")
                .bind(subscription_id)
                .fetch_one(&mut **tx)
                .await?;
        let run = self
            .create_run_job_in_tx(
                tx,
                subscription_id,
                "merged_pending",
                &pending_cursor_kind,
                OffsetDateTime::now_utc(),
                priority,
            )
            .await?;
        mark_subscription_continuation_running_in_tx(tx, subscription_id).await?;
        Ok(run)
    }
}
