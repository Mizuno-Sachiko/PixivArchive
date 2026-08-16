use super::{
    ApiError, ApiErrorBody, ApiJson, ApiPath, ApiQuery,
    pixiv_account::{PixivAccountStateDto, account_avatar_path},
};
use crate::state::AppState;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use pixivarchive_application::subscriptions::{
    ScheduledSubscriptionRun, SubscriptionMutationRequest, SubscriptionRunView,
    SubscriptionUpdateRequest, SubscriptionView,
};
use pixivarchive_domain::subscription::{
    SubscriptionKind, SubscriptionRecentState, SubscriptionRunStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/subscriptions", get(list).post(create))
        .route(
            "/subscriptions/{subscription_id}",
            get(get_one).put(update).delete(delete_one),
        )
        .route("/subscriptions/{subscription_id}/runs", get(list_runs))
        .route(
            "/subscriptions/{subscription_id}/cursors",
            get(list_cursors),
        )
        .route(
            "/subscriptions/{subscription_id}/enabled",
            axum::routing::put(set_enabled),
        )
        .route("/subscriptions/{subscription_id}/run", post(run))
        .route("/subscriptions/{subscription_id}/stop", post(stop))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SubscriptionDto {
    pub id: Uuid,
    pub account_id: Uuid,
    pub account_pixiv_user_id: i64,
    #[schema(required)]
    pub account_avatar_url: Option<String>,
    pub account_state: PixivAccountStateDto,
    #[schema(required)]
    pub rule_id: Option<Uuid>,
    pub name: String,
    pub kind: SubscriptionKindDto,
    pub enabled: bool,
    #[schema(value_type = SubscriptionScheduleDto)]
    pub schedule: Value,
    #[schema(value_type = std::collections::BTreeMap<String, Value>)]
    pub params: Value,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required)]
    pub next_run_at: Option<OffsetDateTime>,
    pub pending_run: bool,
    pub recent_state: SubscriptionRecentState,
    pub revision: i64,
}

impl From<SubscriptionView> for SubscriptionDto {
    fn from(record: SubscriptionView) -> Self {
        Self {
            id: record.id,
            account_id: record.account_id,
            account_pixiv_user_id: record.account_pixiv_user_id,
            account_avatar_url: record
                .account_avatar_url
                .as_ref()
                .map(|_| account_avatar_path(record.account_id, record.account_revision)),
            account_state: record.account_state.into(),
            rule_id: record.rule_id,
            name: record.name,
            kind: record.kind.into(),
            enabled: record.enabled,
            schedule: record.schedule,
            params: record.params,
            next_run_at: record.next_run_at,
            pending_run: record.pending_run,
            recent_state: record.recent_state,
            revision: record.revision,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SubscriptionList {
    pub items: Vec<SubscriptionDto>,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct SubscriptionRunListQuery {
    #[serde(default = "default_run_limit")]
    pub limit: u16,
}

const fn default_run_limit() -> u16 {
    50
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SubscriptionRunDto {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub trigger_kind: String,
    pub state: SubscriptionRunStatus,
    pub cursor_kind: String,
    pub discovered_count: i32,
    pub ignored_count: i32,
    #[schema(required)]
    pub error_class: Option<String>,
    #[schema(required)]
    pub trace_id: Option<Uuid>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required)]
    pub started_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required)]
    pub finished_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl From<SubscriptionRunView> for SubscriptionRunDto {
    fn from(record: SubscriptionRunView) -> Self {
        Self {
            id: record.id,
            subscription_id: record.subscription_id,
            trigger_kind: record.trigger_kind,
            state: record.state,
            cursor_kind: record.cursor_kind,
            discovered_count: record.discovered_count,
            ignored_count: record.ignored_count,
            error_class: record.error_class,
            trace_id: record.trace_id,
            started_at: record.started_at,
            finished_at: record.finished_at,
            created_at: record.created_at,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SubscriptionRunList {
    pub items: Vec<SubscriptionRunDto>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SubscriptionCursorDto {
    pub cursor_kind: String,
    pub source_key: String,
    #[schema(value_type = std::collections::BTreeMap<String, Value>)]
    pub value: Value,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SubscriptionCursorList {
    pub items: Vec<SubscriptionCursorDto>,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct CreateSubscriptionBody {
    pub kind: SubscriptionKindDto,
    pub account_id: Uuid,
    pub rule_id: Option<Uuid>,
    pub name: String,
    #[schema(minimum = 15, maximum = 43200)]
    pub interval_minutes: i64,
    #[schema(minimum = 0, maximum = 7)]
    pub lookback_pages: i64,
    #[schema(value_type = std::collections::BTreeMap<String, Value>)]
    pub params: Value,
    #[serde(with = "time::serde::rfc3339::option")]
    pub next_run_at: Option<OffsetDateTime>,
}

impl CreateSubscriptionBody {
    fn into_request(self) -> SubscriptionMutationRequest {
        SubscriptionMutationRequest {
            kind: self.kind.into(),
            account_id: self.account_id,
            rule_id: self.rule_id,
            name: self.name,
            interval_minutes: self.interval_minutes,
            lookback_pages: self.lookback_pages,
            params: self.params,
            next_run_at: self.next_run_at,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/subscriptions",
    responses((status = 200, body = SubscriptionList)),
    tag = "Subscriptions"
)]
pub(crate) async fn list(
    State(state): State<AppState>,
) -> Result<Json<SubscriptionList>, ApiError> {
    let items = state
        .subscriptions
        .list()
        .await?
        .into_iter()
        .map(SubscriptionDto::from)
        .collect();
    Ok(Json(SubscriptionList { items }))
}

#[utoipa::path(
    post,
    path = "/api/subscriptions",
    request_body = CreateSubscriptionBody,
    responses(
        (status = 201, body = SubscriptionDto),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Subscriptions"
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<CreateSubscriptionBody>,
) -> Result<(StatusCode, Json<SubscriptionDto>), ApiError> {
    let created = state.subscriptions.create(body.into_request()).await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

#[utoipa::path(
    get,
    path = "/api/subscriptions/{subscription_id}",
    params(("subscription_id" = Uuid, Path)),
    responses((status = 200, body = SubscriptionDto), (status = 404, body = ApiErrorBody)),
    tag = "Subscriptions"
)]
pub(crate) async fn get_one(
    State(state): State<AppState>,
    ApiPath(subscription_id): ApiPath<Uuid>,
) -> Result<Json<SubscriptionDto>, ApiError> {
    Ok(Json(state.subscriptions.get(subscription_id).await?.into()))
}

#[utoipa::path(
    get,
    path = "/api/subscriptions/{subscription_id}/runs",
    params(
        ("subscription_id" = Uuid, Path),
        ("limit" = Option<u16>, Query)
    ),
    responses(
        (status = 200, body = SubscriptionRunList),
        (status = 404, body = ApiErrorBody)
    ),
    tag = "Subscriptions"
)]
pub(crate) async fn list_runs(
    State(state): State<AppState>,
    ApiPath(subscription_id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<SubscriptionRunListQuery>,
) -> Result<Json<SubscriptionRunList>, ApiError> {
    let items = state
        .subscriptions
        .runs(subscription_id, query.limit)
        .await?
        .into_iter()
        .map(SubscriptionRunDto::from)
        .collect();
    Ok(Json(SubscriptionRunList { items }))
}

#[utoipa::path(
    get,
    path = "/api/subscriptions/{subscription_id}/cursors",
    params(("subscription_id" = Uuid, Path)),
    responses(
        (status = 200, body = SubscriptionCursorList),
        (status = 404, body = ApiErrorBody)
    ),
    tag = "Subscriptions"
)]
pub(crate) async fn list_cursors(
    State(state): State<AppState>,
    ApiPath(subscription_id): ApiPath<Uuid>,
) -> Result<Json<SubscriptionCursorList>, ApiError> {
    let items = state
        .subscriptions
        .cursors(subscription_id)
        .await?
        .into_iter()
        .map(|record| SubscriptionCursorDto {
            cursor_kind: record.cursor_kind,
            source_key: record.source_key,
            value: record.value,
        })
        .collect();
    Ok(Json(SubscriptionCursorList { items }))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct UpdateSubscriptionBody {
    pub expected_revision: i64,
    pub enabled: bool,
    pub account_id: Uuid,
    pub rule_id: Option<Uuid>,
    pub name: String,
    #[schema(minimum = 15, maximum = 43200)]
    pub interval_minutes: i64,
    #[schema(minimum = 0, maximum = 7)]
    pub lookback_pages: i64,
    #[schema(value_type = std::collections::BTreeMap<String, Value>)]
    pub params: Value,
    #[serde(with = "time::serde::rfc3339::option")]
    pub next_run_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct SetSubscriptionEnabledBody {
    pub expected_revision: i64,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionKindDto {
    Ranking,
    Following,
    Bookmarks,
}

impl From<SubscriptionKindDto> for SubscriptionKind {
    fn from(kind: SubscriptionKindDto) -> Self {
        match kind {
            SubscriptionKindDto::Ranking => Self::Ranking,
            SubscriptionKindDto::Following => Self::Following,
            SubscriptionKindDto::Bookmarks => Self::Bookmarks,
        }
    }
}

impl From<SubscriptionKind> for SubscriptionKindDto {
    fn from(kind: SubscriptionKind) -> Self {
        match kind {
            SubscriptionKind::Ranking => Self::Ranking,
            SubscriptionKind::Following => Self::Following,
            SubscriptionKind::Bookmarks => Self::Bookmarks,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
pub struct SubscriptionScheduleDto {
    #[schema(minimum = 15, maximum = 43200)]
    pub interval_minutes: i64,
    #[schema(minimum = 0, maximum = 7)]
    pub lookback_pages: u32,
}

#[utoipa::path(
    put,
    path = "/api/subscriptions/{subscription_id}",
    params(("subscription_id" = Uuid, Path)),
    request_body = UpdateSubscriptionBody,
    responses(
        (status = 200, body = SubscriptionDto),
        (status = 409, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Subscriptions"
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ApiPath(subscription_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<UpdateSubscriptionBody>,
) -> Result<Json<SubscriptionDto>, ApiError> {
    let updated = state
        .subscriptions
        .update(
            subscription_id,
            body.expected_revision,
            body.enabled,
            SubscriptionUpdateRequest {
                account_id: body.account_id,
                rule_id: body.rule_id,
                name: body.name,
                interval_minutes: body.interval_minutes,
                lookback_pages: body.lookback_pages,
                params: body.params,
                next_run_at: body.next_run_at,
            },
        )
        .await?;
    Ok(Json(updated.into()))
}

#[utoipa::path(
    put,
    path = "/api/subscriptions/{subscription_id}/enabled",
    params(("subscription_id" = Uuid, Path)),
    request_body = SetSubscriptionEnabledBody,
    responses(
        (status = 200, body = SubscriptionDto),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody)
    ),
    tag = "Subscriptions"
)]
pub(crate) async fn set_enabled(
    State(state): State<AppState>,
    ApiPath(subscription_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<SetSubscriptionEnabledBody>,
) -> Result<Json<SubscriptionDto>, ApiError> {
    let updated = state
        .subscriptions
        .set_enabled(subscription_id, body.expected_revision, body.enabled)
        .await?;
    Ok(Json(updated.into()))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct DeleteSubscriptionQuery {
    pub expected_revision: i64,
}

#[utoipa::path(
    delete,
    path = "/api/subscriptions/{subscription_id}",
    params(
        ("subscription_id" = Uuid, Path),
        ("expected_revision" = i64, Query)
    ),
    responses((status = 204), (status = 409, body = ApiErrorBody)),
    tag = "Subscriptions"
)]
pub(crate) async fn delete_one(
    State(state): State<AppState>,
    ApiPath(subscription_id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<DeleteSubscriptionQuery>,
) -> Result<StatusCode, ApiError> {
    state
        .subscriptions
        .delete(subscription_id, query.expected_revision)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct RunSubscriptionBody {
    #[serde(default)]
    pub backfill: bool,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SubscriptionRunAccepted {
    pub subscription_id: Uuid,
    pub run_id: Uuid,
    pub job_id: Uuid,
    pub trigger_kind: String,
}

impl From<ScheduledSubscriptionRun> for SubscriptionRunAccepted {
    fn from(run: ScheduledSubscriptionRun) -> Self {
        Self {
            subscription_id: run.subscription_id,
            run_id: run.run_id,
            job_id: run.job_id,
            trigger_kind: run.trigger_kind,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/subscriptions/{subscription_id}/run",
    params(("subscription_id" = Uuid, Path)),
    request_body = RunSubscriptionBody,
    responses(
        (status = 202, body = SubscriptionRunAccepted),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody)
    ),
    tag = "Subscriptions"
)]
pub(crate) async fn run(
    State(state): State<AppState>,
    ApiPath(subscription_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<RunSubscriptionBody>,
) -> Result<(StatusCode, Json<SubscriptionRunAccepted>), ApiError> {
    let run = state
        .subscriptions
        .start_manual_run(subscription_id, body.backfill)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(run.into())))
}

#[utoipa::path(
    post,
    path = "/api/subscriptions/{subscription_id}/stop",
    params(("subscription_id" = Uuid, Path)),
    responses(
        (status = 200, body = SubscriptionDto),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody)
    ),
    tag = "Subscriptions"
)]
pub(crate) async fn stop(
    State(state): State<AppState>,
    ApiPath(subscription_id): ApiPath<Uuid>,
) -> Result<Json<SubscriptionDto>, ApiError> {
    let subscription = state.subscriptions.stop_active_run(subscription_id).await?;
    Ok(Json(subscription.into()))
}
