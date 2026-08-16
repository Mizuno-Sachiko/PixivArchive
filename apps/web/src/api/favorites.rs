use super::{ApiError, ApiErrorBody, ApiJson};
use crate::{
    api::subscriptions::{SubscriptionDto, SubscriptionRunAccepted},
    state::AppState,
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use pixivarchive_application::favorites::FavoritesAdminState;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/favorites", get(get_state).put(update))
        .route("/favorites/run", post(run))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct FavoritesStateDto {
    pub subscription: SubscriptionDto,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(required)]
    pub last_full_reconciled_at: Option<OffsetDateTime>,
}

impl From<FavoritesAdminState> for FavoritesStateDto {
    fn from(state: FavoritesAdminState) -> Self {
        Self {
            subscription: state.subscription.into(),
            last_full_reconciled_at: state.last_full_reconciled_at,
        }
    }
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct UpdateFavoritesBody {
    pub expected_account_id: Uuid,
    pub enabled: bool,
    pub interval_minutes: i64,
    pub expected_revision: i64,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct FavoritesAccountBody {
    pub expected_account_id: Uuid,
}

#[utoipa::path(
    get,
    path = "/api/favorites",
    responses(
        (status = 200, body = FavoritesStateDto),
        (status = 404, body = ApiErrorBody)
    ),
    tag = "Favorites"
)]
pub(crate) async fn get_state(
    State(state): State<AppState>,
) -> Result<Json<FavoritesStateDto>, ApiError> {
    Ok(Json(state.favorites.current().await?.into()))
}

#[utoipa::path(
    put,
    path = "/api/favorites",
    request_body = UpdateFavoritesBody,
    responses(
        (status = 200, body = FavoritesStateDto),
        (status = 409, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Favorites"
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<UpdateFavoritesBody>,
) -> Result<Json<FavoritesStateDto>, ApiError> {
    Ok(Json(
        state
            .favorites
            .update(
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
    post,
    path = "/api/favorites/run",
    request_body = FavoritesAccountBody,
    responses(
        (status = 202, body = SubscriptionRunAccepted),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody)
    ),
    tag = "Favorites"
)]
pub(crate) async fn run(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<FavoritesAccountBody>,
) -> Result<(StatusCode, Json<SubscriptionRunAccepted>), ApiError> {
    let run = state
        .favorites
        .start_manual_run(body.expected_account_id)
        .await?;
    Ok((StatusCode::ACCEPTED, Json(run.into())))
}
