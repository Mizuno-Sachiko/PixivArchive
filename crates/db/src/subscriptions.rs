mod configuration;
mod cursors;
mod model;
mod runs;
mod scheduling;
mod units;

pub use model::*;
use model::{
    subscription_from_row, subscription_units, unit_from_row, validate_subscription_params,
};

use crate::{Db, DbError, EventRepository, JobRepository};
use pixivarchive_domain::{
    event::{EventPayload, EventResource},
    job::{JobLease, JobPriority, NewJob},
    subscription::{
        SubscriptionKind, SubscriptionRecentState, SubscriptionRunStatus, SubscriptionSchedule,
    },
};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction, types::Json};
use time::OffsetDateTime;
use uuid::Uuid;

pub const MODULE_NAME: &str = "subscriptions";

#[derive(Clone)]
pub struct SubscriptionRepository {
    db: Db,
}

impl SubscriptionRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

fn subscription_schedule_value(
    interval_minutes: i64,
    lookback_pages: i64,
) -> Result<Value, DbError> {
    SubscriptionSchedule::new(interval_minutes, lookback_pages)
        .map(SubscriptionSchedule::to_value)
        .map_err(|error| DbError::InvalidValue(error.to_string()))
}

pub(crate) async fn append_subscription_event(
    db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    subscription_id: Uuid,
) -> Result<(), DbError> {
    let revision: i64 = sqlx::query_scalar("SELECT revision FROM subscription WHERE id = $1")
        .bind(subscription_id)
        .fetch_one(&mut **tx)
        .await?;
    EventRepository::new(db.clone())
        .append_in_tx(
            tx,
            EventResource::Subscription,
            subscription_id,
            EventPayload::SubscriptionChanged { revision },
        )
        .await?;
    Ok(())
}

async fn mark_subscription_continuation_running_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    subscription_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE subscription
        SET pending_run = false,
            pending_cursor_kind = 'normal',
            recent_state = 'running',
            updated_at = now(),
            revision = revision + 1
        WHERE id = $1
        "#,
    )
    .bind(subscription_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn mark_subscription_run_finished_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    subscription_id: Uuid,
    recent_state: SubscriptionRecentState,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        UPDATE subscription
        SET recent_state = $2,
            updated_at = now(),
            revision = revision + 1
        WHERE id = $1
        "#,
    )
    .bind(subscription_id)
    .bind(recent_state.as_str())
    .execute(&mut **tx)
    .await?;
    Ok(())
}
