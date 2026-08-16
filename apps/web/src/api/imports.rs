use super::{ApiError, ApiErrorBody, ApiJson, ApiQuery};
use crate::state::AppState;
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use pixivarchive_application::imports::{ImportStrategy, QueueImportRequest};
use pixivarchive_domain::subscription::ImportKind;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new().route("/imports", get(list).post(queue))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct ImportListQuery {
    #[serde(default = "default_limit")]
    pub limit: u16,
}

const fn default_limit() -> u16 {
    100
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct ImportRunDto {
    pub id: Uuid,
    #[schema(required)]
    pub job_id: Option<Uuid>,
    pub account_id: Uuid,
    pub kind: ImportKindDto,
    pub target_pixiv_id: i64,
    pub strategy: ImportStrategyDto,
    pub status: String,
    pub discovered_count: i32,
    pub saved_count: i32,
    #[schema(required)]
    pub error_class: Option<String>,
    #[schema(required)]
    pub error_message: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required)]
    pub finished_at: Option<OffsetDateTime>,
}

impl From<pixivarchive_application::imports::ImportRunSummary> for ImportRunDto {
    fn from(record: pixivarchive_application::imports::ImportRunSummary) -> Self {
        Self {
            id: record.run_id,
            job_id: record.job_id,
            account_id: record.account_id,
            kind: record.kind.into(),
            target_pixiv_id: record.target_pixiv_id,
            strategy: record.strategy.into(),
            status: record.status.as_str().to_owned(),
            discovered_count: record.discovered_count,
            saved_count: record.saved_count,
            error_class: record.error_class,
            error_message: record.error_message,
            created_at: record.created_at,
            finished_at: record.finished_at,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct ImportRunList {
    pub items: Vec<ImportRunDto>,
}

#[utoipa::path(
    get,
    path = "/api/imports",
    params(("limit" = Option<u16>, Query)),
    responses((status = 200, body = ImportRunList)),
    tag = "Imports"
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<ImportListQuery>,
) -> Result<Json<ImportRunList>, ApiError> {
    let items = state
        .imports
        .list(query.limit)
        .await?
        .into_iter()
        .map(ImportRunDto::from)
        .collect();
    Ok(Json(ImportRunList { items }))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct QueueImportBody {
    pub account_id: Uuid,
    pub kind: ImportKindDto,
    pub target_pixiv_id: i64,
    pub strategy: ImportStrategyDto,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ImportStrategyDto {
    Default,
    Rule { rule_id: Uuid },
    Forced,
}

impl From<ImportStrategyDto> for ImportStrategy {
    fn from(strategy: ImportStrategyDto) -> Self {
        match strategy {
            ImportStrategyDto::Default => Self::Default,
            ImportStrategyDto::Rule { rule_id } => Self::Rule { rule_id },
            ImportStrategyDto::Forced => Self::Forced,
        }
    }
}

impl From<ImportStrategy> for ImportStrategyDto {
    fn from(strategy: ImportStrategy) -> Self {
        match strategy {
            ImportStrategy::Default => Self::Default,
            ImportStrategy::Rule { rule_id } => Self::Rule { rule_id },
            ImportStrategy::Forced => Self::Forced,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/imports",
    request_body = QueueImportBody,
    responses(
        (status = 202, body = ImportRunDto),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Imports"
)]
pub(crate) async fn queue(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<QueueImportBody>,
) -> Result<(StatusCode, Json<ImportRunDto>), ApiError> {
    let kind = body.kind.into();
    let queued = state
        .imports
        .queue(QueueImportRequest {
            account_id: body.account_id,
            kind,
            target_pixiv_id: body.target_pixiv_id,
            strategy: body.strategy.into(),
        })
        .await?;
    Ok((StatusCode::ACCEPTED, Json(ImportRunDto::from(queued))))
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImportKindDto {
    Artist,
    Work,
}

impl From<ImportKindDto> for ImportKind {
    fn from(kind: ImportKindDto) -> Self {
        match kind {
            ImportKindDto::Artist => Self::Artist,
            ImportKindDto::Work => Self::Work,
        }
    }
}

impl From<ImportKind> for ImportKindDto {
    fn from(kind: ImportKind) -> Self {
        match kind {
            ImportKind::Artist => Self::Artist,
            ImportKind::Work => Self::Work,
        }
    }
}
