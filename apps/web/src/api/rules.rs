use super::{ApiError, ApiErrorBody, ApiJson, ApiPath, ApiQuery};
use crate::state::AppState;
use axum::{
    Extension, Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post, put},
};
use pixivarchive_application::rules::{
    PublishRuleVersionRequest, RuleDraft, RulePreviewRequest, RuleSummary, RuleVersion,
    SaveRuleDraftRequest,
};
use pixivarchive_domain::{
    auth::SessionContext,
    rule::{EvaluationTrace, RuleAction, RuleDefinitionV1},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuleLifecycle {
    Draft,
    Published,
    Modified,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/rules", get(list).post(create))
        .route("/rules/order", put(reorder))
        .route("/rules/{rule_id}", get(get_one).delete(delete_one))
        .route("/rules/{rule_id}/copy", post(copy))
        .route("/rules/{rule_id}/draft", get(get_draft).put(save_draft))
        .route("/rules/{rule_id}/publish", post(publish))
        .route("/rules/{rule_id}/validate", post(validate))
        .route("/rules/{rule_id}/preview", post(preview))
        .route("/rules/{rule_id}/export", get(export))
        .route("/rules/{rule_id}/import", put(import))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct RuleDto {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub action: RuleAction,
    pub default_action: RuleAction,
    #[schema(required)]
    pub current_version_id: Option<Uuid>,
    #[schema(required)]
    pub current_version: Option<i64>,
    pub lifecycle: RuleLifecycle,
    pub revision: i64,
    pub sort_order: i64,
}

impl From<RuleSummary> for RuleDto {
    fn from(record: RuleSummary) -> Self {
        Self {
            id: record.id,
            name: record.name,
            enabled: record.enabled,
            action: record.match_action,
            default_action: record.default_action,
            current_version_id: record.current_version_id,
            current_version: record.current_version,
            lifecycle: match (record.current_version, record.has_draft) {
                (None, _) => RuleLifecycle::Draft,
                (Some(_), true) => RuleLifecycle::Modified,
                (Some(_), false) => RuleLifecycle::Published,
            },
            revision: record.revision,
            sort_order: record.sort_order,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct RuleList {
    pub items: Vec<RuleDto>,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct CreateRuleBody {
    pub name: String,
    pub default_action: RuleAction,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct CopyRuleBody {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct ReorderRulesBody {
    pub ordered_rule_ids: Vec<Uuid>,
}

#[utoipa::path(
    get,
    path = "/api/rules",
    responses((status = 200, body = RuleList), (status = 401, body = ApiErrorBody)),
    tag = "Rules"
)]
pub(crate) async fn list(State(state): State<AppState>) -> Result<Json<RuleList>, ApiError> {
    let items = state
        .rules
        .list_rules()
        .await?
        .into_iter()
        .map(RuleDto::from)
        .collect();
    Ok(Json(RuleList { items }))
}

#[utoipa::path(
    post,
    path = "/api/rules",
    request_body = CreateRuleBody,
    responses(
        (status = 201, body = RuleDto),
        (status = 409, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Rules"
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<CreateRuleBody>,
) -> Result<(StatusCode, Json<RuleDto>), ApiError> {
    let created = state
        .rules
        .create_rule(&body.name, body.default_action)
        .await?;
    Ok((StatusCode::CREATED, Json(created.into())))
}

#[utoipa::path(
    post,
    path = "/api/rules/{rule_id}/copy",
    params(("rule_id" = Uuid, Path)),
    request_body = CopyRuleBody,
    responses(
        (status = 201, body = RuleDto),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Rules"
)]
pub(crate) async fn copy(
    State(state): State<AppState>,
    ApiPath(rule_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CopyRuleBody>,
) -> Result<(StatusCode, Json<RuleDto>), ApiError> {
    let copied = state.rules.copy_rule(rule_id, &body.name).await?;
    Ok((StatusCode::CREATED, Json(copied.into())))
}

#[utoipa::path(
    put,
    path = "/api/rules/order",
    request_body = ReorderRulesBody,
    responses(
        (status = 200, body = RuleList),
        (status = 409, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Rules"
)]
pub(crate) async fn reorder(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<ReorderRulesBody>,
) -> Result<Json<RuleList>, ApiError> {
    let items = state
        .rules
        .reorder_rules(&body.ordered_rule_ids)
        .await?
        .into_iter()
        .map(RuleDto::from)
        .collect();
    Ok(Json(RuleList { items }))
}

#[utoipa::path(
    get,
    path = "/api/rules/{rule_id}",
    params(("rule_id" = Uuid, Path)),
    responses((status = 200, body = RuleDto), (status = 404, body = ApiErrorBody)),
    tag = "Rules"
)]
pub(crate) async fn get_one(
    State(state): State<AppState>,
    ApiPath(rule_id): ApiPath<Uuid>,
) -> Result<Json<RuleDto>, ApiError> {
    Ok(Json(state.rules.rule(rule_id).await?.into()))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct ExpectedRevisionQuery {
    pub expected_revision: i64,
}

#[utoipa::path(
    delete,
    path = "/api/rules/{rule_id}",
    params(
        ("rule_id" = Uuid, Path),
        ("expected_revision" = i64, Query)
    ),
    responses((status = 204), (status = 409, body = ApiErrorBody)),
    tag = "Rules"
)]
pub(crate) async fn delete_one(
    State(state): State<AppState>,
    ApiPath(rule_id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<ExpectedRevisionQuery>,
) -> Result<StatusCode, ApiError> {
    state
        .rules
        .delete_rule(rule_id, query.expected_revision)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct RuleDraftDto {
    pub id: Uuid,
    pub rule_id: Uuid,
    #[schema(required)]
    pub base_version: Option<i64>,
    pub schema_version: i64,
    #[schema(value_type = RuleDefinitionV1)]
    pub definition: Value,
    pub revision: i64,
}

impl From<RuleDraft> for RuleDraftDto {
    fn from(record: RuleDraft) -> Self {
        Self {
            id: record.id,
            rule_id: record.rule_id,
            base_version: record.base_version,
            schema_version: record.schema_version,
            definition: record.definition,
            revision: record.revision,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/rules/{rule_id}/draft",
    params(("rule_id" = Uuid, Path)),
    responses((status = 200, body = Option<RuleDraftDto>)),
    tag = "Rules"
)]
pub(crate) async fn get_draft(
    State(state): State<AppState>,
    ApiPath(rule_id): ApiPath<Uuid>,
) -> Result<Json<Option<RuleDraftDto>>, ApiError> {
    Ok(Json(
        state
            .rules
            .load_draft(rule_id)
            .await?
            .map(RuleDraftDto::from),
    ))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct SaveRuleDraftBody {
    pub expected_revision: Option<i64>,
    pub base_version: Option<i64>,
    #[schema(value_type = RuleDefinitionV1)]
    pub definition: Value,
}

#[utoipa::path(
    put,
    path = "/api/rules/{rule_id}/draft",
    params(("rule_id" = Uuid, Path)),
    request_body = SaveRuleDraftBody,
    responses(
        (status = 200, body = RuleDraftDto),
        (status = 409, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Rules"
)]
pub(crate) async fn save_draft(
    State(state): State<AppState>,
    ApiPath(rule_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<SaveRuleDraftBody>,
) -> Result<Json<RuleDraftDto>, ApiError> {
    let draft = state
        .rules
        .save_draft(SaveRuleDraftRequest {
            rule_id,
            expected_revision: body.expected_revision,
            base_version: body.base_version,
            definition: body.definition,
        })
        .await?;
    Ok(Json(draft.into()))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct PublishRuleBody {
    pub base_version: Option<i64>,
    pub expected_draft_revision: i64,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct RuleVersionDto {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub version: i64,
    #[schema(required)]
    pub base_version: Option<i64>,
    pub schema_version: i64,
    #[schema(value_type = RuleDefinitionV1)]
    pub definition: Value,
    #[schema(required)]
    pub created_by: Option<Uuid>,
}

impl From<RuleVersion> for RuleVersionDto {
    fn from(record: RuleVersion) -> Self {
        Self {
            id: record.id,
            rule_id: record.rule_id,
            version: record.version,
            base_version: record.base_version,
            schema_version: record.schema_version,
            definition: record.definition,
            created_by: record.created_by,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/rules/{rule_id}/publish",
    params(("rule_id" = Uuid, Path)),
    request_body = PublishRuleBody,
    responses((status = 201, body = RuleVersionDto), (status = 409, body = ApiErrorBody)),
    tag = "Rules"
)]
pub(crate) async fn publish(
    State(state): State<AppState>,
    Extension(session): Extension<SessionContext>,
    ApiPath(rule_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<PublishRuleBody>,
) -> Result<(StatusCode, Json<RuleVersionDto>), ApiError> {
    let version = state
        .rules
        .publish_version(PublishRuleVersionRequest {
            rule_id,
            base_version: body.base_version,
            expected_draft_revision: body.expected_draft_revision,
            created_by: Some(session.administrator_id),
        })
        .await?;
    Ok((StatusCode::CREATED, Json(version.into())))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct RuleDefinitionBody {
    #[schema(value_type = RuleDefinitionV1)]
    pub definition: Value,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct RuleValidationResponse {
    pub valid: bool,
}

#[utoipa::path(
    post,
    path = "/api/rules/{rule_id}/validate",
    params(("rule_id" = Uuid, Path)),
    request_body = RuleDefinitionBody,
    responses((status = 200, body = RuleValidationResponse), (status = 422, body = ApiErrorBody)),
    tag = "Rules"
)]
pub(crate) async fn validate(
    State(state): State<AppState>,
    ApiPath(_rule_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<RuleDefinitionBody>,
) -> Result<Json<RuleValidationResponse>, ApiError> {
    state.rules.validate(body.definition)?;
    Ok(Json(RuleValidationResponse { valid: true }))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct PreviewRuleBody {
    #[schema(value_type = RuleDefinitionV1)]
    pub definition: Value,
    pub pixiv_work_id: i64,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct RulePreviewItemDto {
    pub pixiv_work_id: i64,
    pub title: String,
    pub artist_name: String,
    pub content_type: String,
    pub decision: RuleAction,
    #[schema(required)]
    pub matched_rule_id: Option<Uuid>,
    #[schema(value_type = EvaluationTrace)]
    pub trace: Value,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct RulePreviewResponse {
    pub item: RulePreviewItemDto,
}

#[utoipa::path(
    post,
    path = "/api/rules/{rule_id}/preview",
    params(("rule_id" = Uuid, Path)),
    request_body = PreviewRuleBody,
    responses(
        (status = 200, body = RulePreviewResponse),
        (status = 404, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Rules"
)]
pub(crate) async fn preview(
    State(state): State<AppState>,
    ApiPath(rule_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<PreviewRuleBody>,
) -> Result<Json<RulePreviewResponse>, ApiError> {
    state.rules.rule(rule_id).await?;
    let preview = state
        .rule_preview
        .preview(RulePreviewRequest {
            pixiv_work_id: body.pixiv_work_id,
            definition: body.definition,
        })
        .await?;
    let trace = serde_json::to_value(&preview.decision.trace)
        .map_err(|_| ApiError::service_unavailable())?;
    Ok(Json(RulePreviewResponse {
        item: RulePreviewItemDto {
            pixiv_work_id: preview.pixiv_work_id,
            title: preview.title,
            artist_name: preview.artist_name,
            content_type: preview.content_type,
            decision: preview.decision.action,
            matched_rule_id: preview.decision.matched_rule_id,
            trace,
        },
    }))
}

#[utoipa::path(
    get,
    path = "/api/rules/{rule_id}/export",
    params(("rule_id" = Uuid, Path)),
    responses((status = 200, body = RuleDefinitionV1), (status = 404, body = ApiErrorBody)),
    tag = "Rules"
)]
pub(crate) async fn export(
    State(state): State<AppState>,
    ApiPath(rule_id): ApiPath<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let definition = state
        .rules
        .export_json(rule_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Published rule version was not found"))?;
    Ok(Json(definition))
}

#[utoipa::path(
    put,
    path = "/api/rules/{rule_id}/import",
    params(("rule_id" = Uuid, Path)),
    request_body = SaveRuleDraftBody,
    responses((status = 200, body = RuleDraftDto), (status = 409, body = ApiErrorBody)),
    tag = "Rules"
)]
pub(crate) async fn import(
    State(state): State<AppState>,
    ApiPath(rule_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<SaveRuleDraftBody>,
) -> Result<Json<RuleDraftDto>, ApiError> {
    let draft = state
        .rules
        .import_json(rule_id, body.expected_revision, body.definition)
        .await?;
    Ok(Json(draft.into()))
}
