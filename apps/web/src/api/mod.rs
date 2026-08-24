pub mod auth;
pub mod bookmarks;
pub mod events;
pub mod favorites;
pub mod following;
pub mod gallery;
pub mod imports;
pub mod pixiv_account;
pub mod rules;
pub mod subscriptions;
pub mod system;
pub mod tasks;
pub mod trash;

use crate::{
    middleware::{auth::AuthLayer, csrf::CsrfLayer, origin::OriginLayer},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{
        FromRequest, FromRequestParts, Path, Query, Request,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use pixivarchive_application::{
    auth::AuthError,
    bookmarks::BookmarkWritebackError,
    imports::ImportQueueError,
    rules::{RulePreviewError, RuleServiceError},
    settings::SettingsError,
    subscriptions::SubscriptionRunStartError,
    system::SystemError,
};
use pixivarchive_db::DbError;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes(state: &AppState) -> Router<AppState> {
    let protected = Router::new()
        .merge(auth::protected_routes())
        .merge(bookmarks::routes())
        .merge(events::routes())
        .merge(favorites::routes())
        .merge(following::routes())
        .merge(gallery::routes())
        .merge(imports::routes())
        .merge(pixiv_account::routes())
        .merge(rules::routes())
        .merge(subscriptions::routes())
        .merge(system::routes())
        .merge(tasks::routes())
        .merge(trash::routes())
        .layer(CsrfLayer::new(state.auth.clone()))
        .layer(AuthLayer::new(state.auth.clone()))
        .layer(OriginLayer::new());

    Router::new()
        .merge(auth::public_routes().layer(OriginLayer::new()))
        .merge(protected)
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
}

pub async fn not_found() -> ApiError {
    ApiError::not_found("API resource was not found")
}

pub async fn method_not_allowed() -> ApiError {
    ApiError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        "The request method is not supported for this resource",
    )
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    #[schema(value_type = Value)]
    pub details: Value,
    pub trace_id: Uuid,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    body: ApiErrorBody,
    headers: Option<Box<HeaderMap>>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            body: ApiErrorBody {
                code: code.into(),
                message: message.into(),
                details: json!({}),
                trace_id: Uuid::now_v7(),
            },
            headers: None,
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.body.details = details;
        self
    }

    pub fn with_header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers
            .get_or_insert_with(|| Box::new(HeaderMap::new()))
            .insert(name, value);
        self
    }

    pub fn authentication_required() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "authentication_required",
            "Authentication is required",
        )
    }

    pub fn forbidden(message: &'static str) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    pub fn not_found(message: &'static str) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn invalid_json(message: String) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid_json", message)
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_request", message)
    }

    pub fn revision_conflict() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "revision_conflict",
            "The resource changed after it was loaded",
        )
    }

    pub fn service_unavailable() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            "The service is temporarily unavailable",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(self.body)).into_response();
        if let Some(headers) = self.headers {
            response.headers_mut().extend(*headers);
        }
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        response
    }
}

impl From<DbError> for ApiError {
    fn from(error: DbError) -> Self {
        match error {
            DbError::NotFound => Self::not_found("Resource was not found"),
            DbError::RevisionConflict | DbError::LeaseConflict => Self::revision_conflict(),
            DbError::Constraint(message) | DbError::InvalidValue(message) => {
                Self::invalid_request(message)
            }
            DbError::RateLimited {
                retry_after_seconds,
            } => Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "Too many requests",
            )
            .with_details(json!({ "retry_after_seconds": retry_after_seconds })),
            DbError::Connection(_) | DbError::Migration(_) | DbError::Query(_) => {
                Self::service_unavailable()
            }
        }
    }
}

impl From<SubscriptionRunStartError> for ApiError {
    fn from(error: SubscriptionRunStartError) -> Self {
        match error {
            SubscriptionRunStartError::AccountUnavailable { state } => Self::new(
                StatusCode::CONFLICT,
                "pixiv_account_unavailable",
                "The Pixiv account is unavailable for subscription execution",
            )
            .with_details(json!({ "state": state.as_str() })),
            SubscriptionRunStartError::Storage(error) => error.into(),
        }
    }
}

impl From<AuthError> for ApiError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::InvalidCredentials => Self::new(
                StatusCode::UNAUTHORIZED,
                "invalid_credentials",
                "The supplied credentials are invalid",
            ),
            AuthError::RateLimited {
                retry_after_seconds,
            } => Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "Too many authentication attempts",
            )
            .with_details(json!({ "retry_after_seconds": retry_after_seconds })),
            AuthError::InvalidSession => Self::authentication_required(),
            AuthError::Forbidden => Self::forbidden("The request is forbidden"),
            AuthError::Internal => Self::service_unavailable(),
        }
    }
}

impl From<RuleServiceError> for ApiError {
    fn from(error: RuleServiceError) -> Self {
        match error {
            RuleServiceError::Rule(error) => Self::invalid_request(error.to_string()),
            RuleServiceError::Evaluation(error) => Self::invalid_request(error.to_string()),
            RuleServiceError::NotFound => Self::not_found("Rule was not found"),
            RuleServiceError::InvalidRequest(message) => Self::invalid_request(message),
            RuleServiceError::Conflict => Self::new(
                StatusCode::CONFLICT,
                "resource_conflict",
                "The rule conflicts with existing data",
            ),
            RuleServiceError::RevisionConflict => Self::revision_conflict(),
            RuleServiceError::Storage => Self::service_unavailable(),
        }
    }
}

impl From<ImportQueueError> for ApiError {
    fn from(error: ImportQueueError) -> Self {
        match error {
            ImportQueueError::RuleUnavailable => {
                Self::invalid_request("The selected rule has no published version")
            }
            ImportQueueError::RuleDocument(error) => Self::invalid_request(error.to_string()),
            ImportQueueError::Storage(_) => Self::service_unavailable(),
        }
    }
}

impl From<RulePreviewError> for ApiError {
    fn from(error: RulePreviewError) -> Self {
        match error {
            RulePreviewError::InvalidPixivWorkId => {
                Self::invalid_request("Pixiv work ID must be a positive integer")
            }
            RulePreviewError::WorkNotFound => Self::not_found("Pixiv work was not found"),
            RulePreviewError::AccountNotConfigured => Self::new(
                StatusCode::CONFLICT,
                "pixiv_account_not_configured",
                "Pixiv account is not configured",
            ),
            RulePreviewError::AccountUnavailable | RulePreviewError::CredentialInvalid => {
                Self::new(
                    StatusCode::CONFLICT,
                    "pixiv_account_unavailable",
                    "Pixiv account is unavailable",
                )
            }
            RulePreviewError::Rule(error) => Self::invalid_request(error.to_string()),
            RulePreviewError::Evaluation(error) => Self::invalid_request(error.to_string()),
            RulePreviewError::Temporary
            | RulePreviewError::Unavailable
            | RulePreviewError::Storage(_)
            | RulePreviewError::Context(_)
            | RulePreviewError::Work(_) => Self::service_unavailable(),
        }
    }
}

impl From<SettingsError> for ApiError {
    fn from(error: SettingsError) -> Self {
        match error {
            SettingsError::RevisionConflict => Self::revision_conflict(),
            SettingsError::Storage => Self::service_unavailable(),
            SettingsError::UnknownGroup
            | SettingsError::WrongGroup
            | SettingsError::UnsupportedSchemaVersion
            | SettingsError::InvalidField(_)
            | SettingsError::EmptyBatch
            | SettingsError::DuplicateGroup => Self::invalid_request(error.to_string()),
        }
    }
}

impl From<SystemError> for ApiError {
    fn from(error: SystemError) -> Self {
        match error {
            SystemError::MediaNotFound => Self::not_found("Media was not found"),
            SystemError::MediaRootUnavailable => Self::service_unavailable(),
            SystemError::Filesystem(error) => {
                tracing::error!(error = %error, "Media storage could not be read");
                Self::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "media_storage_unavailable",
                    "Media storage could not be read",
                )
            }
            SystemError::Storage(_) | SystemError::Settings(_) => Self::service_unavailable(),
        }
    }
}

impl From<BookmarkWritebackError> for ApiError {
    fn from(_error: BookmarkWritebackError) -> Self {
        Self::service_unavailable()
    }
}

pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|rejection: JsonRejection| ApiError::invalid_json(rejection.body_text()))
    }
}

pub struct ApiPath<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiPath<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Path::<T>::from_request_parts(parts, state)
            .await
            .map(|Path(value)| Self(value))
            .map_err(|rejection: PathRejection| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_path",
                    rejection.body_text(),
                )
            })
    }
}

pub struct ApiQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ApiQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(value)| Self(value))
            .map_err(|rejection: QueryRejection| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "invalid_query",
                    rejection.body_text(),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_filesystem_errors_have_a_specific_api_code() {
        let error = ApiError::from(SystemError::Filesystem(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "missing media root",
        )));

        assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.body.code, "media_storage_unavailable");
    }
}
