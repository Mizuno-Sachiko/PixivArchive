use super::{ApiError, ApiErrorBody, ApiJson, ApiPath};
use crate::{
    api::following::avatar::{resolve_cached_pixiv_avatar, serve_cached_avatar},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{Request, State},
    response::Response,
    routing::{delete, get, post, put},
};
use pixivarchive_application::pixiv_accounts::{
    PixivAccount, PixivAccountAdminError, UpdatePixivAccountRequest,
};
use pixivarchive_domain::subscription::PixivAccountState;
use pixivarchive_pixiv::PixivErrorClass;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/pixiv/account", get(get_account).put(update_account))
        .route("/pixiv/account/credential", delete(clear_credential))
        .route("/pixiv/account/validate", post(validate_account))
        .route("/pixiv/accounts/{account_id}/avatar", get(account_avatar))
        .route(
            "/pixiv/account/bookmark-writeback",
            put(update_bookmark_writeback),
        )
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct PixivAccountDto {
    #[schema(required)]
    pub account_id: Option<Uuid>,
    #[schema(required)]
    pub pixiv_user_id: Option<i64>,
    #[schema(required)]
    pub display_name: Option<String>,
    #[schema(required)]
    pub avatar_url: Option<String>,
    pub state: PixivAccountStateDto,
    pub bookmark_writeback_enabled: bool,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required)]
    pub last_validated_at: Option<OffsetDateTime>,
    #[schema(required)]
    pub revision: Option<i64>,
}

impl PixivAccountDto {
    fn unconfigured() -> Self {
        Self {
            account_id: None,
            pixiv_user_id: None,
            display_name: None,
            avatar_url: None,
            state: PixivAccountStateDto::Unconfigured,
            bookmark_writeback_enabled: false,
            last_validated_at: None,
            revision: None,
        }
    }
}

impl From<PixivAccount> for PixivAccountDto {
    fn from(record: PixivAccount) -> Self {
        let avatar_url = record
            .avatar_url
            .as_ref()
            .map(|_| account_avatar_path(record.id, record.revision));
        Self {
            account_id: Some(record.id),
            pixiv_user_id: Some(record.pixiv_user_id),
            display_name: Some(record.display_name),
            avatar_url,
            state: record.state.into(),
            bookmark_writeback_enabled: record.bookmark_writeback_enabled,
            last_validated_at: record.last_validated_at,
            revision: Some(record.revision),
        }
    }
}

pub(super) fn account_avatar_path(account_id: Uuid, revision: i64) -> String {
    format!("/api/pixiv/accounts/{account_id}/avatar?revision={revision}")
}

#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PixivAccountStateDto {
    Unconfigured,
    Validating,
    Normal,
    Restricted,
    CredentialInvalid,
}

impl From<PixivAccountState> for PixivAccountStateDto {
    fn from(state: PixivAccountState) -> Self {
        match state {
            PixivAccountState::Unconfigured => Self::Unconfigured,
            PixivAccountState::Validating => Self::Validating,
            PixivAccountState::Normal => Self::Normal,
            PixivAccountState::Restricted => Self::Restricted,
            PixivAccountState::CredentialInvalid => Self::CredentialInvalid,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/pixiv/account",
    responses((status = 200, body = PixivAccountDto)),
    tag = "Pixiv Account"
)]
pub(crate) async fn get_account(
    State(state): State<AppState>,
) -> Result<Json<PixivAccountDto>, ApiError> {
    let account = state.pixiv_accounts.current().await?;
    Ok(Json(
        account
            .map(PixivAccountDto::from)
            .unwrap_or_else(PixivAccountDto::unconfigured),
    ))
}

#[utoipa::path(
    get,
    path = "/api/pixiv/accounts/{account_id}/avatar",
    params(("account_id" = Uuid, Path)),
    responses(
        (status = 200, description = "Cached Pixiv account avatar"),
        (status = 404, body = ApiErrorBody),
        (status = 429, body = ApiErrorBody),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "Pixiv Account"
)]
pub(crate) async fn account_avatar(
    State(state): State<AppState>,
    ApiPath(account_id): ApiPath<Uuid>,
    request: Request,
) -> Result<Response, ApiError> {
    let account = state.pixiv_accounts.get(account_id).await?;
    let source_url = account
        .avatar_url
        .ok_or_else(|| ApiError::not_found("Pixiv account avatar was not found"))?;
    let avatars = Arc::clone(&state.following_avatars);
    let cache_path = resolve_cached_pixiv_avatar(
        &state.config.cache_root,
        &format!("account-{account_id}-"),
        &source_url,
        move |source| async move {
            avatars
                .fetch_for_account(account_id, source)
                .await
                .map_err(ApiError::from)
        },
    )
    .await?;
    serve_cached_avatar(request, cache_path).await
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct UpdatePixivAccountBody {
    pub cookie: String,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct PixivAccountActionBody {
    pub expected_account_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct ClearPixivCredentialBody {
    pub expected_account_id: Uuid,
    pub expected_revision: i64,
}

#[utoipa::path(
    put,
    path = "/api/pixiv/account",
    request_body = UpdatePixivAccountBody,
    responses(
        (status = 200, body = PixivAccountDto),
        (status = 422, body = ApiErrorBody),
        (status = 429, body = ApiErrorBody),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "Pixiv Account"
)]
pub(crate) async fn update_account(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<UpdatePixivAccountBody>,
) -> Result<Json<PixivAccountDto>, ApiError> {
    let account = state
        .pixiv_account_commands
        .update(UpdatePixivAccountRequest {
            cookie: body.cookie,
        })
        .await
        .map_err(ApiError::from)?;
    Ok(Json(account.into()))
}

#[utoipa::path(
    post,
    path = "/api/pixiv/account/validate",
    request_body = PixivAccountActionBody,
    responses(
        (status = 200, body = PixivAccountDto),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody),
        (status = 429, body = ApiErrorBody),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "Pixiv Account"
)]
pub(crate) async fn validate_account(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<PixivAccountActionBody>,
) -> Result<Json<PixivAccountDto>, ApiError> {
    let account = state
        .pixiv_account_commands
        .validate(Some(body.expected_account_id))
        .await
        .map_err(ApiError::from)?;
    Ok(Json(account.into()))
}

#[utoipa::path(
    delete,
    path = "/api/pixiv/account/credential",
    request_body = ClearPixivCredentialBody,
    responses(
        (status = 200, body = PixivAccountDto),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody)
    ),
    tag = "Pixiv Account"
)]
pub(crate) async fn clear_credential(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<ClearPixivCredentialBody>,
) -> Result<Json<PixivAccountDto>, ApiError> {
    let account = state
        .pixiv_accounts
        .clear_credential(body.expected_account_id, body.expected_revision)
        .await?;
    Ok(Json(account.into()))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct UpdateBookmarkWritebackBody {
    pub expected_account_id: Uuid,
    pub enabled: bool,
    pub expected_revision: i64,
}

#[utoipa::path(
    put,
    path = "/api/pixiv/account/bookmark-writeback",
    request_body = UpdateBookmarkWritebackBody,
    responses(
        (status = 200, body = PixivAccountDto),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody)
    ),
    tag = "Pixiv Account"
)]
pub(crate) async fn update_bookmark_writeback(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<UpdateBookmarkWritebackBody>,
) -> Result<Json<PixivAccountDto>, ApiError> {
    let updated = state
        .pixiv_accounts
        .set_bookmark_writeback(
            body.expected_account_id,
            body.expected_revision,
            body.enabled,
        )
        .await?;
    Ok(Json(updated.into()))
}

impl From<PixivAccountAdminError> for ApiError {
    fn from(error: PixivAccountAdminError) -> Self {
        match error {
            PixivAccountAdminError::InvalidInput => {
                Self::invalid_request("Pixiv account input is invalid")
            }
            PixivAccountAdminError::NotConfigured => {
                Self::not_found("Pixiv account is not configured")
            }
            PixivAccountAdminError::Unavailable => Self::service_unavailable(),
            PixivAccountAdminError::Validation { class, endpoint } => {
                let details = json!({
                    "error_class": class.as_str(),
                    "endpoint": endpoint.map(|value| value.as_str()),
                });
                match class {
                    PixivErrorClass::CredentialInvalid => Self::new(
                        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                        "pixiv_credential_invalid",
                        "The Pixiv Cookie is invalid or expired",
                    )
                    .with_details(details),
                    PixivErrorClass::RateLimited => Self::new(
                        axum::http::StatusCode::TOO_MANY_REQUESTS,
                        "pixiv_rate_limited",
                        "Pixiv temporarily rejected the validation request",
                    )
                    .with_details(details),
                    _ => Self::new(
                        axum::http::StatusCode::SERVICE_UNAVAILABLE,
                        "pixiv_validation_unavailable",
                        "Pixiv account validation is temporarily unavailable",
                    )
                    .with_details(details),
                }
            }
            PixivAccountAdminError::Storage(error) => error.into(),
            PixivAccountAdminError::Cipher(_) => Self::service_unavailable(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiError, PixivAccountDto, PixivAccountState, Uuid};
    use axum::http::StatusCode;
    use pixivarchive_application::pixiv_accounts::{PixivAccount, PixivAccountAdminError};
    use pixivarchive_pixiv::{PixivEndpoint, PixivErrorClass};

    #[test]
    fn account_avatar_uses_the_authenticated_local_endpoint() {
        let account_id = Uuid::now_v7();
        let dto = PixivAccountDto::from(PixivAccount {
            id: account_id,
            pixiv_user_id: 70_001,
            display_name: "reader".to_owned(),
            avatar_url: Some("https://i.pximg.net/example.jpg".to_owned()),
            state: PixivAccountState::Normal,
            bookmark_writeback_enabled: false,
            last_validated_at: None,
            revision: 1,
        });

        assert_eq!(
            dto.avatar_url.as_deref(),
            Some(format!("/api/pixiv/accounts/{account_id}/avatar?revision=1").as_str())
        );
    }

    #[test]
    fn validation_failure_preserves_the_safe_pixiv_reason() {
        let error = ApiError::from(PixivAccountAdminError::Validation {
            class: PixivErrorClass::Network,
            endpoint: Some(PixivEndpoint::Profile),
        });

        assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.body.code, "pixiv_validation_unavailable");
        assert_eq!(error.body.details["error_class"], "network");
        assert_eq!(error.body.details["endpoint"], "profile");
    }
}
