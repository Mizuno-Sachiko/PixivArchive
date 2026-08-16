use super::{ApiError, ApiErrorBody, ApiJson, ApiPath};
use crate::state::AppState;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post, put},
};
use pixivarchive_application::{
    settings::{SettingUpdate, SettingValue},
    system::{
        ComponentStatus, MaintenanceOperation, StorageStatus, SystemCapabilities, SystemStatus,
    },
};
use pixivarchive_domain::settings::SettingGroupKey;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

mod settings_contract;

pub(crate) use settings_contract::{
    ContentSettingsDto, DerivativeFormatDto, DerivativeSettingsDto, EffectiveSettingsDto,
    FailureLimitDto, JobKindDto, JobPriorityDto, JobPriorityMappingDto, PixivSettingsDto,
    ProcessingSettingsDto, QueueQuotaWeightsDto, QueueSettingsDto, RateLimitDto, RetrySettingsDto,
    SecuritySettingsDto, SettingPayloadDto, StorageSettingsDto, UgoiraSettingsDto,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/system/status", get(status))
        .route("/system/settings", get(settings).put(update_settings))
        .route("/system/settings/{group}", put(update_setting))
        .route("/system/maintenance", post(queue_maintenance))
}

pub fn health_routes() -> Router<AppState> {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct ComponentStatusDto {
    pub status: String,
    #[schema(required)]
    pub message: Option<String>,
}

impl From<ComponentStatus> for ComponentStatusDto {
    fn from(status: ComponentStatus) -> Self {
        Self {
            status: status.status,
            message: status.message,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SystemStatusDto {
    pub version: String,
    #[schema(required)]
    pub git_commit: Option<String>,
    pub migration_version: i64,
    pub database: ComponentStatusDto,
    pub media: ComponentStatusDto,
    pub worker: ComponentStatusDto,
    pub queue: BTreeMap<String, BTreeMap<String, i64>>,
    pub setting_revisions: BTreeMap<String, i64>,
    pub storage: StorageStatusDto,
    pub capabilities: SystemCapabilitiesDto,
}

impl From<SystemStatus> for SystemStatusDto {
    fn from(status: SystemStatus) -> Self {
        Self {
            version: status.version,
            git_commit: status.git_commit,
            migration_version: status.migration_version,
            database: status.database.into(),
            media: status.media.into(),
            worker: status.worker.into(),
            queue: status.queue,
            setting_revisions: status.setting_revisions,
            storage: status.storage.into(),
            capabilities: status.capabilities.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct StorageStatusDto {
    pub active_media_root: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub warning_threshold_bytes: u64,
    pub write_stop_threshold_bytes: u64,
    pub write_stopped: bool,
}

impl From<StorageStatus> for StorageStatusDto {
    fn from(status: StorageStatus) -> Self {
        Self {
            active_media_root: status.active_media_root,
            total_bytes: status.total_bytes,
            available_bytes: status.available_bytes,
            warning_threshold_bytes: status.warning_threshold_bytes,
            write_stop_threshold_bytes: status.write_stop_threshold_bytes,
            write_stopped: status.write_stopped,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SystemCapabilitiesDto {
    pub webp_derivatives: bool,
    pub avif_derivatives: bool,
    pub reflink: bool,
}

impl From<SystemCapabilities> for SystemCapabilitiesDto {
    fn from(capabilities: SystemCapabilities) -> Self {
        Self {
            webp_derivatives: capabilities.webp_derivatives,
            avif_derivatives: capabilities.avif_derivatives,
            reflink: capabilities.reflink,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/system/status",
    responses(
        (status = 200, body = SystemStatusDto),
        (status = 401, body = ApiErrorBody),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "System"
)]
pub(crate) async fn status(
    State(state): State<AppState>,
) -> Result<Json<SystemStatusDto>, ApiError> {
    Ok(Json(state.system.status().await?.into()))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SettingsDto {
    pub value: EffectiveSettingsDto,
}

#[utoipa::path(
    get,
    path = "/api/system/settings",
    responses((status = 200, body = SettingsDto), (status = 503, body = ApiErrorBody)),
    tag = "System"
)]
pub(crate) async fn settings(State(state): State<AppState>) -> Result<Json<SettingsDto>, ApiError> {
    Ok(Json(SettingsDto {
        value: state.settings.effective().await?.into(),
    }))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct UpdateSettingBody {
    pub expected_revision: Option<i64>,
    pub value: SettingPayloadDto,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SavedSettingDto {
    pub group: String,
    pub revision: i64,
}

impl From<pixivarchive_application::settings::SavedSetting> for SavedSettingDto {
    fn from(saved: pixivarchive_application::settings::SavedSetting) -> Self {
        Self {
            group: saved.group.as_str().to_owned(),
            revision: saved.revision,
        }
    }
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct BatchSettingUpdateBody {
    pub group: String,
    pub expected_revision: Option<i64>,
    pub value: SettingPayloadDto,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct UpdateSettingsBody {
    pub updates: Vec<BatchSettingUpdateBody>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SavedSettingsDto {
    pub settings: Vec<SavedSettingDto>,
}

#[utoipa::path(
    put,
    path = "/api/system/settings",
    request_body = UpdateSettingsBody,
    responses(
        (status = 200, body = SavedSettingsDto),
        (status = 409, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "System"
)]
pub(crate) async fn update_settings(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<UpdateSettingsBody>,
) -> Result<Json<SavedSettingsDto>, ApiError> {
    let updates = body
        .updates
        .into_iter()
        .map(|update| setting_update(&update.group, update.expected_revision, update.value))
        .collect::<Result<Vec<_>, _>>()?;
    let settings = state
        .settings
        .update_many(updates)
        .await?
        .into_iter()
        .map(SavedSettingDto::from)
        .collect();
    Ok(Json(SavedSettingsDto { settings }))
}

#[utoipa::path(
    put,
    path = "/api/system/settings/{group}",
    params(("group" = String, Path)),
    request_body = UpdateSettingBody,
    responses(
        (status = 200, body = SavedSettingDto),
        (status = 409, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "System"
)]
pub(crate) async fn update_setting(
    State(state): State<AppState>,
    ApiPath(group): ApiPath<String>,
    ApiJson(body): ApiJson<UpdateSettingBody>,
) -> Result<Json<SavedSettingDto>, ApiError> {
    let update = setting_update(&group, body.expected_revision, body.value)?;
    let saved = state
        .settings
        .update(update.group, update.expected_revision, update.value)
        .await?;
    Ok(Json(saved.into()))
}

fn setting_update(
    group: &str,
    expected_revision: Option<i64>,
    value: SettingPayloadDto,
) -> Result<SettingUpdate, ApiError> {
    let group = SettingGroupKey::from_db_value(group)
        .ok_or_else(|| ApiError::invalid_request("Unknown setting group"))?;
    let value = serde_json::to_value(value)
        .map_err(|_| ApiError::invalid_request("Invalid setting value"))?;
    Ok(SettingUpdate {
        group,
        expected_revision,
        value: SettingValue::from_group_value(group, value)?,
    })
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct MaintenanceBody {
    pub operation: String,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct MaintenanceAcceptedDto {
    pub operation: String,
    pub job_ids: Vec<uuid::Uuid>,
    pub queued_count: usize,
}

#[utoipa::path(
    post,
    path = "/api/system/maintenance",
    request_body = MaintenanceBody,
    responses(
        (status = 202, body = MaintenanceAcceptedDto),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "System"
)]
pub(crate) async fn queue_maintenance(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<MaintenanceBody>,
) -> Result<(StatusCode, Json<MaintenanceAcceptedDto>), ApiError> {
    let operation = MaintenanceOperation::parse(&body.operation)
        .ok_or_else(|| ApiError::invalid_request("Unknown maintenance operation"))?;
    let accepted = state.system.queue_maintenance(operation).await?;
    let queued_count = accepted.job_ids.len();
    Ok((
        StatusCode::ACCEPTED,
        Json(MaintenanceAcceptedDto {
            operation: accepted.operation.as_str().to_owned(),
            job_ids: accepted.job_ids,
            queued_count,
        }),
    ))
}

#[utoipa::path(
    get,
    path = "/health/live",
    security(()),
    responses((status = 204)),
    tag = "Health"
)]
pub(crate) async fn live() -> StatusCode {
    StatusCode::NO_CONTENT
}

#[utoipa::path(
    get,
    path = "/health/ready",
    security(()),
    responses((status = 200), (status = 503, body = ApiErrorBody)),
    tag = "Health"
)]
pub(crate) async fn ready(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    state.system.readiness().await?;
    Ok(StatusCode::OK)
}
