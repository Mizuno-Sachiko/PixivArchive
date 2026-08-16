use crate::{
    api::{ApiError, ApiErrorBody, ApiQuery},
    state::AppState,
};
use axum::{Json, extract::State};
use pixivarchive_domain::{pixiv::PixivAgeRating, work::GalleryOverviewDecoration};
use serde::{Deserialize, Serialize};
use time::{Date, macros::format_description};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct OverviewDecorationDto {
    pub pixiv_work_id: i64,
    pub title: String,
    pub age_rating: PixivAgeRating,
    pub cover_url: String,
}

impl From<GalleryOverviewDecoration> for OverviewDecorationDto {
    fn from(decoration: GalleryOverviewDecoration) -> Self {
        Self {
            pixiv_work_id: decoration.pixiv_work_id,
            title: decoration.title,
            age_rating: decoration.age_rating,
            cover_url: format!("/api/derivatives/{}", decoration.cover_derivative_id),
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct OverviewDecorationsDto {
    pub items: Vec<Option<OverviewDecorationDto>>,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct OverviewDecorationsQuery {
    pub date: String,
}

impl OverviewDecorationsQuery {
    fn date(&self) -> Result<Date, ApiError> {
        Date::parse(&self.date, format_description!("[year]-[month]-[day]"))
            .map_err(|_| ApiError::invalid_request("Overview decoration date is invalid"))
    }
}

#[utoipa::path(
    get,
    path = "/api/gallery/overview-decorations",
    params(("date" = String, Query)),
    responses(
        (status = 200, body = OverviewDecorationsDto),
        (status = 401, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "Gallery"
)]
pub(crate) async fn overview_decorations(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<OverviewDecorationsQuery>,
) -> Result<Json<OverviewDecorationsDto>, ApiError> {
    select_overview_decorations(&state, query.date()?, false).await
}

#[utoipa::path(
    post,
    path = "/api/gallery/overview-decorations",
    params(("date" = String, Query)),
    responses(
        (status = 200, body = OverviewDecorationsDto),
        (status = 401, body = ApiErrorBody),
        (status = 422, body = ApiErrorBody),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "Gallery"
)]
pub(crate) async fn shuffle_overview_decorations(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<OverviewDecorationsQuery>,
) -> Result<Json<OverviewDecorationsDto>, ApiError> {
    select_overview_decorations(&state, query.date()?, true).await
}

async fn select_overview_decorations(
    state: &AppState,
    date: Date,
    replace: bool,
) -> Result<Json<OverviewDecorationsDto>, ApiError> {
    let items = if replace {
        state.gallery.shuffle_overview_decorations(date).await?
    } else {
        state.gallery.overview_decorations(date).await?
    }
    .into_iter()
    .map(|item| item.map(OverviewDecorationDto::from))
    .collect();
    Ok(Json(OverviewDecorationsDto { items }))
}
