use crate::{DbError, FinishSubscriptionRunUnit};
use pixivarchive_domain::{
    job::{JobPriority, JobState},
    subscription::ImportRunStatus,
};
use sqlx::{Row, types::Json};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct JobHeartbeatRecord {
    pub resource_revision: i64,
    pub lease_expires_at: OffsetDateTime,
}

#[derive(Clone, Debug)]
pub struct JobRecord {
    pub id: Uuid,
    pub priority: JobPriority,
    pub kind: String,
    pub payload: serde_json::Value,
    pub state: JobState,
    pub attempts: i32,
    pub available_at: OffsetDateTime,
    pub error_class: Option<String>,
    pub retryable: Option<bool>,
    pub next_retry_at: Option<OffsetDateTime>,
    pub resource_revision: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Clone, Debug)]
pub struct JobAttemptRecord {
    pub attempt_number: i32,
    pub state: String,
    pub started_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
    pub error_class: Option<String>,
    pub retryable: Option<bool>,
    pub message: Option<String>,
    pub trace_id: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum JobCompletion {
    TaskOnly,
    Import(ImportJobCompletion),
    Subscription(FinishSubscriptionRunUnit),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImportJobCompletion {
    pub status: ImportRunStatus,
    pub discovered_count: i32,
    pub saved_count: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobStats {
    pub total: u64,
    pub running: u64,
    pub waiting: u64,
    pub requires_attention: u64,
}

pub(super) fn job_from_row(row: &sqlx::postgres::PgRow) -> Result<JobRecord, DbError> {
    let priority_value: String = row.get("priority_class");
    let priority = JobPriority::from_db_value(&priority_value)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown job priority {priority_value}")))?;
    let state_value: String = row.get("state");
    let state = JobState::from_db_value(&state_value)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown job state {state_value}")))?;
    Ok(JobRecord {
        id: row.get("id"),
        priority,
        kind: row.get("kind"),
        payload: row.get::<Json<serde_json::Value>, _>("payload").0,
        state,
        attempts: row.get("attempts"),
        available_at: row.get("available_at"),
        error_class: row.get("error_class"),
        retryable: row.get("retryable"),
        next_retry_at: row.get("next_retry_at"),
        resource_revision: row.get("resource_revision"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

pub(super) fn job_count(row: &sqlx::postgres::PgRow, column: &str) -> Result<u64, DbError> {
    let value: i64 = row.try_get(column)?;
    u64::try_from(value)
        .map_err(|_| DbError::InvalidValue(format!("job {column} count is negative")))
}
