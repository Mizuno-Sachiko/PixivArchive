use crate::DbError;
use pixivarchive_domain::{
    job::JobKind,
    pixiv::{PixivRankingContent, PixivRankingMode},
    subscription::{
        SubscriptionKind, SubscriptionRecentState, SubscriptionRunStatus as DomainRunStatus,
    },
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::{Row, types::Json};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
pub struct CreateSubscription {
    pub pixiv_account_id: Uuid,
    pub rule_id: Option<Uuid>,
    pub name: String,
    pub kind: SubscriptionKind,
    pub interval_minutes: i64,
    pub lookback_pages: i64,
    pub params: Value,
    pub next_run_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpdateSubscription {
    pub id: Uuid,
    pub expected_revision: i64,
    pub pixiv_account_id: Uuid,
    pub rule_id: Option<Uuid>,
    pub name: String,
    pub enabled: bool,
    pub interval_minutes: i64,
    pub lookback_pages: i64,
    pub params: Value,
    pub next_run_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubscriptionRecord {
    pub id: Uuid,
    pub pixiv_account_id: Uuid,
    pub rule_id: Option<Uuid>,
    pub name: String,
    pub kind: SubscriptionKind,
    pub enabled: bool,
    pub schedule: Value,
    pub params: Value,
    pub next_run_at: Option<OffsetDateTime>,
    pub pending_run: bool,
    pub recent_state: SubscriptionRecentState,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubscriptionRunRecord {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub job_id: Option<Uuid>,
    pub trigger_kind: String,
    pub cursor_kind: String,
    pub state: DomainRunStatus,
    pub params_snapshot: Value,
    pub rule_version_id: Option<Uuid>,
    pub rule_document: Option<Value>,
    pub kind: SubscriptionKind,
    pub subscription_params: Value,
    pub rule_id: Option<Uuid>,
    pub pixiv_account_id: Uuid,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubscriptionRunSummaryRecord {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub trigger_kind: String,
    pub state: DomainRunStatus,
    pub cursor_kind: String,
    pub discovered_count: i32,
    pub ignored_count: i32,
    pub error_class: Option<String>,
    pub trace_id: Option<Uuid>,
    pub started_at: Option<OffsetDateTime>,
    pub finished_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordRankingUnitEntry {
    pub run_id: Uuid,
    pub unit_id: Uuid,
    pub source_key: String,
    pub pixiv_work_id: i64,
    pub rank: u32,
    pub score: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubscriptionCursorRecord {
    pub cursor_kind: String,
    pub source_key: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubscriptionRunUnitRecord {
    pub id: Uuid,
    pub subscription_run_id: Uuid,
    pub job_id: Option<Uuid>,
    pub source_key: String,
    pub cursor_kind: String,
    pub params_snapshot: Value,
    pub cursor_snapshot: Option<Value>,
    pub state: DomainRunStatus,
    pub error_class: Option<String>,
    pub error_message: Option<String>,
    pub rule_version_id: Option<Uuid>,
    pub rule_document: Option<Value>,
    pub subscription_id: Uuid,
    pub kind: SubscriptionKind,
    pub schedule: Value,
    pub rule_id: Option<Uuid>,
    pub pixiv_account_id: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DueSubscription {
    pub id: Uuid,
    pub revision: i64,
    pub kind: String,
    pub schedule: Value,
    pub next_run_at: OffsetDateTime,
    pub pixiv_account_id: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduleDueSubscription {
    pub subscription_id: Uuid,
    pub expected_revision: i64,
    pub expected_next_run_at: OffsetDateTime,
    pub now: OffsetDateTime,
    pub next_run_at: OffsetDateTime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScheduleDueSubscriptionResult {
    Created(ScheduledSubscriptionRun),
    MergedPending { subscription_id: Uuid },
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FinishSubscriptionRunResult {
    Completed,
    MergedPending(ScheduledSubscriptionRun),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledSubscriptionRun {
    pub subscription_id: Uuid,
    pub run_id: Uuid,
    pub job_id: Uuid,
    pub trigger_kind: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinishSubscriptionRun {
    pub run_id: Uuid,
    pub state: DomainRunStatus,
    pub finished_at: OffsetDateTime,
    pub discovered_count: i32,
    pub ignored_count: i32,
    pub error_class: Option<String>,
    pub trace_id: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FinishSubscriptionRunUnit {
    pub unit_id: Uuid,
    pub state: DomainRunStatus,
    pub discovered_count: i32,
    pub ignored_count: i32,
    pub error_class: Option<String>,
    pub error_message: Option<String>,
    pub cursor_kind: String,
    pub source_key: String,
    pub cursor_value: Option<Value>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FinishSubscriptionRunUnitResult {
    ParentStillRunning,
    ParentCompleted,
    MergedPending(ScheduledSubscriptionRun),
}

#[derive(Clone, Debug)]
pub(super) struct SubscriptionUnitSpec {
    pub(super) source_key: String,
    pub(super) job_kind: JobKind,
    pub(super) params_snapshot: Value,
}

pub(super) fn subscription_units(
    kind: SubscriptionKind,
    params: &Value,
) -> Result<Vec<SubscriptionUnitSpec>, DbError> {
    let units = match kind {
        SubscriptionKind::Ranking => {
            let modes = string_array(params, "modes")?;
            let contents = string_array(params, "contents")?;
            let max_rank = bounded_u32_field(params, "max_rank", 20, 1, 5_000)?;
            let mut units = Vec::with_capacity(modes.len() * contents.len());
            for mode in modes {
                let typed_mode = enum_param::<PixivRankingMode>(&mode, "modes")?;
                for content in &contents {
                    let typed_content = enum_param::<PixivRankingContent>(content, "contents")?;
                    if !typed_mode.supports_content(typed_content) {
                        continue;
                    }
                    units.push(SubscriptionUnitSpec {
                        source_key: format!("ranking:{mode}:{content}"),
                        job_kind: JobKind::RankingCollection,
                        params_snapshot: json!({
                            "mode": mode,
                            "content": content,
                            "page_size": 50,
                            "max_rank": max_rank,
                        }),
                    });
                }
            }
            if units.is_empty() {
                return Err(DbError::InvalidValue(
                    "subscription params contain no supported ranking combinations".to_owned(),
                ));
            }
            units
        }
        SubscriptionKind::Following => {
            let mode = string_field(params, "mode")?;
            let source = string_field(params, "source")?;
            vec![SubscriptionUnitSpec {
                source_key: format!("following:{source}:{mode}"),
                job_kind: JobKind::FollowingCollection,
                params_snapshot: json!({
                    "mode": mode,
                    "source": source,
                    "language": params
                        .get("language")
                        .and_then(Value::as_str)
                        .unwrap_or("zh"),
                    "page_size": 50,
                }),
            }]
        }
        SubscriptionKind::Bookmarks => {
            let mode = string_field(params, "mode")?;
            let _visibility = string_field(params, "visibility")?;
            vec![SubscriptionUnitSpec {
                source_key: format!("bookmarks:all:{mode}"),
                job_kind: JobKind::BookmarksCollection,
                params_snapshot: json!({
                    "mode": mode,
                    "visibility": "all",
                    "page_size": 100,
                    "full_reconcile_hours": bounded_u32_field(
                        params,
                        "full_reconcile_hours",
                        24,
                        1,
                        168,
                    )?,
                }),
            }]
        }
    };
    Ok(units)
}

pub(super) fn validate_subscription_params(
    kind: SubscriptionKind,
    params: &Value,
) -> Result<(), DbError> {
    if !params.is_object() {
        return Err(DbError::InvalidValue(
            "subscription params must be an object".to_owned(),
        ));
    }
    subscription_units(kind, params)?;
    Ok(())
}

fn string_array(params: &Value, key: &str) -> Result<Vec<String>, DbError> {
    let Some(values) = params.get(key).and_then(Value::as_array) else {
        return Err(DbError::InvalidValue(format!(
            "subscription params missing {key}"
        )));
    };
    let result: Vec<_> = values
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if result.len() != values.len() || result.is_empty() {
        return Err(DbError::InvalidValue(format!(
            "subscription params {key} must be a non-empty string array"
        )));
    }
    Ok(result)
}

fn string_field(params: &Value, key: &str) -> Result<String, DbError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| DbError::InvalidValue(format!("subscription params missing {key}")))
}

fn enum_param<T>(value: &str, key: &str) -> Result<T, DbError>
where
    T: DeserializeOwned,
{
    serde_json::from_value(Value::String(value.to_owned())).map_err(|_| {
        DbError::InvalidValue(format!(
            "subscription params {key} contains an unsupported value"
        ))
    })
}

fn bounded_u32_field(
    params: &Value,
    key: &str,
    default: u32,
    minimum: u32,
    maximum: u32,
) -> Result<u32, DbError> {
    let value = match params.get(key) {
        None => default,
        Some(value) => value
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| {
                DbError::InvalidValue(format!("subscription params {key} must be an integer"))
            })?,
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(DbError::InvalidValue(format!(
            "subscription params {key} must be between {minimum} and {maximum}"
        )));
    }
    Ok(value)
}

pub(super) fn subscription_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<SubscriptionRecord, DbError> {
    let kind_value: String = row.get("kind");
    let kind = SubscriptionKind::from_db_value(&kind_value)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown subscription kind {kind_value}")))?;
    let recent_state_value: String = row.get("recent_state");
    let recent_state =
        SubscriptionRecentState::from_db_value(&recent_state_value).ok_or_else(|| {
            DbError::InvalidValue(format!(
                "unknown subscription recent state {recent_state_value}"
            ))
        })?;
    Ok(SubscriptionRecord {
        id: row.get("id"),
        pixiv_account_id: row.get("pixiv_account_id"),
        rule_id: row.get("rule_id"),
        name: row.get("name"),
        kind,
        enabled: row.get("enabled"),
        schedule: row.get::<Json<Value>, _>("schedule").0,
        params: row.get::<Json<Value>, _>("params").0,
        next_run_at: row.get("next_run_at"),
        pending_run: row.get("pending_run"),
        recent_state,
        revision: row.get("revision"),
    })
}

pub(super) fn run_from_row(row: &sqlx::postgres::PgRow) -> Result<SubscriptionRunRecord, DbError> {
    let kind_value: String = row.get("kind");
    let kind = SubscriptionKind::from_db_value(&kind_value)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown subscription kind {kind_value}")))?;
    let state_value: String = row.get("state");
    let state = DomainRunStatus::from_db_value(&state_value).ok_or_else(|| {
        DbError::InvalidValue(format!("unknown subscription run state {state_value}"))
    })?;
    Ok(SubscriptionRunRecord {
        id: row.get("id"),
        subscription_id: row.get("subscription_id"),
        job_id: row.get("job_id"),
        trigger_kind: row.get("trigger_kind"),
        cursor_kind: row.get("cursor_kind"),
        state,
        params_snapshot: row.get::<Json<Value>, _>("params_snapshot").0,
        rule_version_id: row.get("rule_version_id"),
        rule_document: row
            .get::<Option<Json<Value>>, _>("rule_document")
            .map(|value| value.0),
        kind,
        subscription_params: row.get::<Json<Value>, _>("params").0,
        rule_id: row.get("rule_id"),
        pixiv_account_id: row.get("pixiv_account_id"),
    })
}

pub(super) fn run_summary_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<SubscriptionRunSummaryRecord, DbError> {
    let state_value: String = row.get("state");
    let state = DomainRunStatus::from_db_value(&state_value).ok_or_else(|| {
        DbError::InvalidValue(format!("unknown subscription run state {state_value}"))
    })?;
    Ok(SubscriptionRunSummaryRecord {
        id: row.get("id"),
        subscription_id: row.get("subscription_id"),
        trigger_kind: row.get("trigger_kind"),
        state,
        cursor_kind: row.get("cursor_kind"),
        discovered_count: row.get("discovered_count"),
        ignored_count: row.get("ignored_count"),
        error_class: row.get("error_class"),
        trace_id: row.get("trace_id"),
        started_at: row.get("started_at"),
        finished_at: row.get("finished_at"),
        created_at: row.get("created_at"),
    })
}

pub(super) fn unit_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<SubscriptionRunUnitRecord, DbError> {
    let kind_value: String = row.get("kind");
    let kind = SubscriptionKind::from_db_value(&kind_value)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown subscription kind {kind_value}")))?;
    let state_value: String = row.get("state");
    let state = DomainRunStatus::from_db_value(&state_value).ok_or_else(|| {
        DbError::InvalidValue(format!("unknown subscription run unit state {state_value}"))
    })?;
    Ok(SubscriptionRunUnitRecord {
        id: row.get("id"),
        subscription_run_id: row.get("subscription_run_id"),
        job_id: row.get("job_id"),
        source_key: row.get("source_key"),
        cursor_kind: row.get("cursor_kind"),
        params_snapshot: row.get::<Json<Value>, _>("params_snapshot").0,
        cursor_snapshot: row
            .get::<Option<Json<Value>>, _>("cursor_snapshot")
            .map(|value| value.0),
        state,
        error_class: row.get("error_class"),
        error_message: row.get("error_message"),
        rule_version_id: row.get("rule_version_id"),
        rule_document: row
            .get::<Option<Json<Value>>, _>("rule_document")
            .map(|value| value.0),
        subscription_id: row.get("subscription_id"),
        kind,
        schedule: row.get::<Json<Value>, _>("schedule").0,
        rule_id: row.get("rule_id"),
        pixiv_account_id: row.get("pixiv_account_id"),
    })
}
