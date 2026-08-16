use super::{ApiError, ApiErrorBody, ApiJson, ApiPath, ApiQuery, system::JobPriorityDto};
use crate::state::AppState;
use axum::{
    Json, Router,
    extract::State,
    routing::{get, post},
};
use pixivarchive_application::jobs::{JobAttempt, JobSnapshot, JobStatistics};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/tasks", get(list))
        .route("/tasks/{job_id}", get(get_one))
        .route("/tasks/{job_id}/retry", post(retry))
        .route("/tasks/{job_id}/cancel", post(cancel))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct TaskListQuery {
    #[serde(default = "default_limit")]
    pub limit: u16,
    pub kind: Option<String>,
    #[serde(default)]
    pub errors_only: bool,
}

const fn default_limit() -> u16 {
    200
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TaskDto {
    pub id: Uuid,
    pub priority: JobPriorityDto,
    pub kind: String,
    pub state: String,
    pub attempts: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub available_at: OffsetDateTime,
    #[schema(required)]
    pub error_class: Option<String>,
    #[schema(required)]
    pub retryable: Option<bool>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required)]
    pub next_retry_at: Option<OffsetDateTime>,
    pub revision: i64,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

impl From<JobSnapshot> for TaskDto {
    fn from(record: JobSnapshot) -> Self {
        Self {
            id: record.id,
            priority: record.priority.into(),
            kind: record.kind,
            state: record.state.as_str().to_owned(),
            attempts: record.attempts,
            available_at: record.available_at,
            error_class: record.error_class,
            retryable: record.retryable,
            next_retry_at: record.next_retry_at,
            revision: record.resource_revision,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TaskList {
    pub items: Vec<TaskDto>,
    pub summary: TaskSummaryDto,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TaskSummaryDto {
    pub total: u64,
    pub running: u64,
    pub waiting: u64,
    pub requires_attention: u64,
}

impl From<JobStatistics> for TaskSummaryDto {
    fn from(stats: JobStatistics) -> Self {
        Self {
            total: stats.total,
            running: stats.running,
            waiting: stats.waiting,
            requires_attention: stats.requires_attention,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TaskAttemptDto {
    pub attempt_number: i32,
    pub state: String,
    #[serde(with = "time::serde::rfc3339")]
    pub started_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required)]
    pub finished_at: Option<OffsetDateTime>,
    #[schema(required)]
    pub error_class: Option<String>,
    #[schema(required)]
    pub retryable: Option<bool>,
    #[schema(required)]
    pub message: Option<String>,
    #[schema(required)]
    pub trace_id: Option<Uuid>,
}

impl From<JobAttempt> for TaskAttemptDto {
    fn from(record: JobAttempt) -> Self {
        Self {
            attempt_number: record.attempt_number,
            state: record.state,
            started_at: record.started_at,
            finished_at: record.finished_at,
            error_class: record.error_class,
            retryable: record.retryable,
            message: record.message,
            trace_id: record.trace_id,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TaskDetailDto {
    pub task: TaskDto,
    pub attempts: Vec<TaskAttemptDto>,
}

#[utoipa::path(
    get,
    path = "/api/tasks",
    params(
        ("limit" = Option<u16>, Query),
        ("kind" = Option<String>, Query),
        ("errors_only" = Option<bool>, Query)
    ),
    responses((status = 200, body = TaskList)),
    tag = "Tasks"
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<TaskListQuery>,
) -> Result<Json<TaskList>, ApiError> {
    let (items, summary) = tokio::try_join!(
        state
            .jobs
            .list_filtered(query.limit, query.kind.as_deref(), query.errors_only),
        state.jobs.stats(),
    )?;
    let items = items.into_iter().map(TaskDto::from).collect();
    Ok(Json(TaskList {
        items,
        summary: summary.into(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/tasks/{job_id}",
    params(("job_id" = Uuid, Path)),
    responses((status = 200, body = TaskDetailDto), (status = 404, body = ApiErrorBody)),
    tag = "Tasks"
)]
pub(crate) async fn get_one(
    State(state): State<AppState>,
    ApiPath(job_id): ApiPath<Uuid>,
) -> Result<Json<TaskDetailDto>, ApiError> {
    let task = state.jobs.get(job_id).await?;
    let attempts = state
        .jobs
        .attempts(job_id)
        .await?
        .into_iter()
        .map(TaskAttemptDto::from)
        .collect();
    Ok(Json(TaskDetailDto {
        task: task.into(),
        attempts,
    }))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct TaskCommandBody {
    pub expected_revision: i64,
}

#[utoipa::path(
    post,
    path = "/api/tasks/{job_id}/retry",
    params(("job_id" = Uuid, Path)),
    request_body = TaskCommandBody,
    responses((status = 200, body = TaskDto), (status = 409, body = ApiErrorBody)),
    tag = "Tasks"
)]
pub(crate) async fn retry(
    State(state): State<AppState>,
    ApiPath(job_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<TaskCommandBody>,
) -> Result<Json<TaskDto>, ApiError> {
    Ok(Json(
        state
            .jobs
            .retry_requested(job_id, body.expected_revision)
            .await?
            .into(),
    ))
}

#[utoipa::path(
    post,
    path = "/api/tasks/{job_id}/cancel",
    params(("job_id" = Uuid, Path)),
    request_body = TaskCommandBody,
    responses((status = 200, body = TaskDto), (status = 409, body = ApiErrorBody)),
    tag = "Tasks"
)]
pub(crate) async fn cancel(
    State(state): State<AppState>,
    ApiPath(job_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<TaskCommandBody>,
) -> Result<Json<TaskDto>, ApiError> {
    Ok(Json(
        state
            .jobs
            .cancel_requested(job_id, body.expected_revision)
            .await?
            .into(),
    ))
}
