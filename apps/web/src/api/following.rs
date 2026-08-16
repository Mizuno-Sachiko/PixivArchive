use super::{ApiError, ApiErrorBody, ApiJson, ApiPath, ApiQuery};
use crate::{
    api::subscriptions::{SubscriptionDto, SubscriptionRunAccepted},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post, put},
};
use pixivarchive_application::following::{
    ArtistFollowCommandError, ArtistFollowStateView, FollowingAdminState, FollowingAuthorView,
    FollowingRefreshError, FollowingServiceError,
};
use pixivarchive_pixiv::PixivErrorClass;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

pub(crate) mod avatar;

pub(crate) use avatar::author_avatar;
use avatar::{log_avatar_cleanup, remove_author_avatar_cache, remove_unreferenced_avatar_cache};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/following", get(get_state).put(update_subscription))
        .route("/following/run", post(run))
        .route("/following/refresh", post(refresh))
        .route("/following/authors", put(update_authors))
        .route("/following/authors/{pixiv_artist_id}", put(update_author))
        .route(
            "/following/authors/{pixiv_artist_id}/pixiv",
            get(artist_follow_state).put(update_artist_follow),
        )
        .route(
            "/following/authors/{pixiv_artist_id}/avatar",
            get(author_avatar),
        )
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct FollowingStateDto {
    pub subscription: SubscriptionDto,
    pub authors: Vec<FollowingAuthorDto>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required)]
    pub last_full_reconciled_at: Option<OffsetDateTime>,
}

impl From<FollowingAdminState> for FollowingStateDto {
    fn from(state: FollowingAdminState) -> Self {
        Self {
            subscription: state.subscription.into(),
            last_full_reconciled_at: state.last_full_reconciled_at,
            authors: state
                .authors
                .into_iter()
                .map(FollowingAuthorDto::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct FollowingAuthorDto {
    pub pixiv_artist_id: i64,
    pub display_name: String,
    #[schema(required)]
    pub avatar_url: Option<String>,
    pub visibility: String,
    pub enabled: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub refreshed_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required)]
    pub last_collected_at: Option<OffsetDateTime>,
}

impl From<FollowingAuthorView> for FollowingAuthorDto {
    fn from(author: FollowingAuthorView) -> Self {
        Self {
            pixiv_artist_id: author.pixiv_artist_id,
            display_name: author.display_name,
            avatar_url: author
                .avatar_url
                .map(|_| format!("/api/following/authors/{}/avatar", author.pixiv_artist_id)),
            visibility: match author.visibility {
                pixivarchive_domain::pixiv::PixivFollowingVisibility::Public => "public".to_owned(),
                pixivarchive_domain::pixiv::PixivFollowingVisibility::Private => {
                    "private".to_owned()
                }
            },
            enabled: author.enabled,
            refreshed_at: author.refreshed_at,
            last_collected_at: author.last_collected_at,
        }
    }
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct UpdateFollowingBody {
    pub expected_account_id: Uuid,
    pub enabled: bool,
    pub interval_minutes: i64,
    pub expected_revision: i64,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct UpdateFollowingAuthorBody {
    pub expected_account_id: Uuid,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct UpdateFollowingAuthorsBody {
    pub expected_account_id: Uuid,
    pub pixiv_artist_ids: Vec<i64>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct FollowingAccountBody {
    pub expected_account_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct RunFollowingBody {
    pub expected_account_id: Uuid,
    #[serde(default)]
    pub backfill: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FollowingAccountQuery {
    pub expected_account_id: Uuid,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct ArtistFollowStateDto {
    pub pixiv_artist_id: i64,
    pub followed: bool,
}

impl From<ArtistFollowStateView> for ArtistFollowStateDto {
    fn from(state: ArtistFollowStateView) -> Self {
        Self {
            pixiv_artist_id: state.pixiv_artist_id,
            followed: state.followed,
        }
    }
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct UpdateArtistFollowBody {
    pub expected_account_id: Uuid,
    pub followed: bool,
}

#[utoipa::path(
    get,
    path = "/api/following",
    responses(
        (status = 200, body = FollowingStateDto),
        (status = 404, body = ApiErrorBody)
    ),
    tag = "Following"
)]
pub(crate) async fn get_state(
    State(state): State<AppState>,
) -> Result<Json<FollowingStateDto>, ApiError> {
    Ok(Json(state.following.current().await?.into()))
}

#[utoipa::path(
    put,
    path = "/api/following",
    request_body = UpdateFollowingBody,
    responses(
        (status = 200, body = SubscriptionDto),
        (status = 409, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody)
    ),
    tag = "Following"
)]
pub(crate) async fn update_subscription(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<UpdateFollowingBody>,
) -> Result<Json<SubscriptionDto>, ApiError> {
    Ok(Json(
        state
            .following
            .configure_subscription(
                body.expected_account_id,
                body.expected_revision,
                body.enabled,
                body.interval_minutes,
            )
            .await?
            .into(),
    ))
}

#[utoipa::path(
    put,
    path = "/api/following/authors/{pixiv_artist_id}",
    params(("pixiv_artist_id" = i64, Path)),
    request_body = UpdateFollowingAuthorBody,
    responses(
        (status = 200, body = FollowingAuthorDto),
        (status = 404, body = ApiErrorBody)
    ),
    tag = "Following"
)]
pub(crate) async fn update_author(
    State(state): State<AppState>,
    ApiPath(pixiv_artist_id): ApiPath<i64>,
    ApiJson(body): ApiJson<UpdateFollowingAuthorBody>,
) -> Result<Json<FollowingAuthorDto>, ApiError> {
    Ok(Json(
        state
            .following
            .set_author_enabled(body.expected_account_id, pixiv_artist_id, body.enabled)
            .await?
            .into(),
    ))
}

#[utoipa::path(
    put,
    path = "/api/following/authors",
    request_body = UpdateFollowingAuthorsBody,
    responses(
        (status = 200, body = FollowingStateDto),
        (status = 400, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody)
    ),
    tag = "Following"
)]
pub(crate) async fn update_authors(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<UpdateFollowingAuthorsBody>,
) -> Result<Json<FollowingStateDto>, ApiError> {
    Ok(Json(
        state
            .following
            .set_authors_enabled(
                body.expected_account_id,
                body.pixiv_artist_ids,
                body.enabled,
            )
            .await?
            .into(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/following/authors/{pixiv_artist_id}/pixiv",
    params(("pixiv_artist_id" = i64, Path)),
    responses(
        (status = 200, body = ArtistFollowStateDto),
        (status = 404, body = ApiErrorBody),
        (status = 429, body = ApiErrorBody),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "Following"
)]
pub(crate) async fn artist_follow_state(
    State(state): State<AppState>,
    ApiPath(pixiv_artist_id): ApiPath<i64>,
    ApiQuery(query): ApiQuery<FollowingAccountQuery>,
) -> Result<Json<ArtistFollowStateDto>, ApiError> {
    Ok(Json(
        state
            .artist_follow_commands
            .status(query.expected_account_id, pixiv_artist_id)
            .await?
            .into(),
    ))
}

#[utoipa::path(
    put,
    path = "/api/following/authors/{pixiv_artist_id}/pixiv",
    params(("pixiv_artist_id" = i64, Path)),
    request_body = UpdateArtistFollowBody,
    responses(
        (status = 200, body = ArtistFollowStateDto),
        (status = 404, body = ApiErrorBody),
        (status = 429, body = ApiErrorBody),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "Following"
)]
pub(crate) async fn update_artist_follow(
    State(state): State<AppState>,
    ApiPath(pixiv_artist_id): ApiPath<i64>,
    ApiJson(body): ApiJson<UpdateArtistFollowBody>,
) -> Result<Json<ArtistFollowStateDto>, ApiError> {
    let updated = state
        .artist_follow_commands
        .set_followed(body.expected_account_id, pixiv_artist_id, body.followed)
        .await?;
    if !updated.followed {
        log_avatar_cleanup(
            remove_author_avatar_cache(&state.config.cache_root, pixiv_artist_id).await,
        );
    }
    Ok(Json(updated.into()))
}

#[utoipa::path(
    post,
    path = "/api/following/run",
    request_body = RunFollowingBody,
    responses(
        (status = 202, body = SubscriptionRunAccepted),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody)
    ),
    tag = "Following"
)]
pub(crate) async fn run(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<RunFollowingBody>,
) -> Result<(StatusCode, Json<SubscriptionRunAccepted>), ApiError> {
    let run = state
        .following
        .start_manual_run(body.expected_account_id, body.backfill)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(run.into())))
}

#[utoipa::path(
    post,
    path = "/api/following/refresh",
    request_body = FollowingAccountBody,
    responses(
        (status = 200, body = FollowingStateDto),
        (status = 404, body = ApiErrorBody),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "Following"
)]
pub(crate) async fn refresh(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<FollowingAccountBody>,
) -> Result<Json<FollowingStateDto>, ApiError> {
    state
        .following_refresh
        .refresh(body.expected_account_id)
        .await
        .map_err(ApiError::from)?;
    let current = state
        .following
        .current_for(body.expected_account_id)
        .await?;
    log_avatar_cleanup(
        remove_unreferenced_avatar_cache(&state.config.cache_root, &current.authors).await,
    );
    Ok(Json(current.into()))
}

impl From<FollowingRefreshError> for ApiError {
    fn from(error: FollowingRefreshError) -> Self {
        match error {
            FollowingRefreshError::NotConfigured => {
                Self::not_found("Pixiv account is not configured")
            }
            FollowingRefreshError::Unavailable | FollowingRefreshError::Context(_) => {
                Self::service_unavailable()
            }
            FollowingRefreshError::Storage(error)
            | FollowingRefreshError::Refresh(FollowingServiceError::Storage(error)) => error.into(),
            FollowingRefreshError::Refresh(FollowingServiceError::Pixiv(error)) => {
                match error.class() {
                    PixivErrorClass::CredentialInvalid => {
                        Self::invalid_request("Pixiv Cookie is invalid")
                    }
                    PixivErrorClass::RateLimited => Self::new(
                        StatusCode::TOO_MANY_REQUESTS,
                        "rate_limited",
                        "Pixiv request rate is limited",
                    ),
                    _ => Self::service_unavailable(),
                }
            }
        }
    }
}

impl From<ArtistFollowCommandError> for ApiError {
    fn from(error: ArtistFollowCommandError) -> Self {
        match error {
            ArtistFollowCommandError::NotConfigured => {
                Self::not_found("Pixiv account is not configured")
            }
            ArtistFollowCommandError::Storage(error) => error.into(),
            ArtistFollowCommandError::Pixiv(error) => match error.class() {
                PixivErrorClass::CredentialInvalid => {
                    Self::invalid_request("Pixiv Cookie is invalid")
                }
                PixivErrorClass::HiddenOrNotFound => Self::not_found("Pixiv author was not found"),
                PixivErrorClass::RateLimited => Self::new(
                    StatusCode::TOO_MANY_REQUESTS,
                    "rate_limited",
                    "Pixiv request rate is limited",
                ),
                _ => Self::service_unavailable(),
            },
            ArtistFollowCommandError::Unavailable
            | ArtistFollowCommandError::Context(_)
            | ArtistFollowCommandError::StateMismatch => Self::service_unavailable(),
        }
    }
}
