use super::{ApiError, ApiErrorBody, ApiJson};
use crate::{
    middleware::{
        clear_csrf_cookie, clear_session_cookie, csrf_cookie, no_store, request_is_secure,
        session_cookie,
    },
    source_bucket,
    state::AppState,
};
use axum::{
    Extension, Json, Router,
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use pixivarchive_application::auth::LoginRequest;
use pixivarchive_domain::auth::SessionContext;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

pub fn public_routes() -> Router<AppState> {
    Router::new().route("/auth/login", post(login))
}

pub fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/session", get(session))
        .route("/auth/logout", post(logout))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct LoginBody {
    pub password: String,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct SessionDto {
    pub administrator_id: Uuid,
    pub session_id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
}

impl From<&SessionContext> for SessionDto {
    fn from(context: &SessionContext) -> Self {
        Self {
            administrator_id: context.administrator_id,
            session_id: context.session_id,
            expires_at: context.expires_at,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/auth/login",
    security(()),
    request_body = LoginBody,
    responses(
        (status = 200, body = SessionDto),
        (status = 400, body = ApiErrorBody),
        (status = 401, body = ApiErrorBody),
        (status = 429, body = ApiErrorBody)
    ),
    tag = "Auth"
)]
pub(crate) async fn login(
    State(state): State<AppState>,
    connect_info: ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<LoginBody>,
) -> Result<Response, ApiError> {
    let issued = state
        .auth
        .login(LoginRequest::new(
            body.password,
            source_bucket(&connect_info),
        ))
        .await?;
    let mut response = Json(SessionDto::from(issued.context())).into_response();
    let secure = request_is_secure(&headers);
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&session_cookie(issued.session_token(), secure))
            .map_err(|_| ApiError::service_unavailable())?,
    );
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&csrf_cookie(issued.csrf_token(), secure))
            .map_err(|_| ApiError::service_unavailable())?,
    );
    no_store(&mut response);
    Ok(response)
}

#[utoipa::path(
    get,
    path = "/api/auth/session",
    responses(
        (status = 200, body = SessionDto),
        (status = 401, body = ApiErrorBody)
    ),
    tag = "Auth"
)]
pub(crate) async fn session(Extension(context): Extension<SessionContext>) -> Json<SessionDto> {
    Json(SessionDto::from(&context))
}

#[utoipa::path(
    post,
    path = "/api/auth/logout",
    responses(
        (status = 204),
        (status = 401, body = ApiErrorBody)
    ),
    tag = "Auth"
)]
pub(crate) async fn logout(
    State(state): State<AppState>,
    Extension(context): Extension<SessionContext>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    state.auth.logout(&context).await?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    let secure = request_is_secure(&headers);
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&clear_session_cookie(secure))
            .map_err(|_| ApiError::service_unavailable())?,
    );
    response.headers_mut().append(
        header::SET_COOKIE,
        HeaderValue::from_str(&clear_csrf_cookie(secure))
            .map_err(|_| ApiError::service_unavailable())?,
    );
    no_store(&mut response);
    Ok(response)
}
