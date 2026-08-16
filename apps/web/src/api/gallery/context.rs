use super::search::GalleryTagDto;
use crate::{
    api::{ApiError, ApiErrorBody, ApiPath, ApiQuery},
    state::AppState,
};
use axum::{Json, extract::State};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use pixivarchive_domain::pixiv::PixivAgeRating;
use pixivarchive_domain::work::{
    GalleryArtistDetail, GalleryContextCursor, GalleryContextPage,
    GalleryContextSelectionExpression, GalleryContextSelectionProjection, GallerySeriesDetail,
    GalleryTagDetail,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize)]
pub struct ContextListQuery {
    #[serde(default = "default_context_limit")]
    pub limit: u16,
    pub cursor: Option<String>,
    pub q: Option<String>,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct GalleryContextSelectionProjectionBody {
    pub expression: GalleryContextSelectionExpression,
    pub visible_context_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GalleryContextSelectionProjectionDto {
    pub selected_context_count: u64,
    pub selected_work_count: u64,
    pub selected_visible_context_ids: Vec<Uuid>,
}

impl From<GalleryContextSelectionProjection> for GalleryContextSelectionProjectionDto {
    fn from(projection: GalleryContextSelectionProjection) -> Self {
        Self {
            selected_context_count: projection.selected_context_count,
            selected_work_count: projection.selected_work_count,
            selected_visible_context_ids: projection.selected_visible_context_ids,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/gallery/contexts/selection",
    request_body = GalleryContextSelectionProjectionBody,
    responses(
        (status = 200, body = GalleryContextSelectionProjectionDto),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Gallery"
)]
pub(crate) async fn context_selection_projection(
    State(state): State<AppState>,
    crate::api::ApiJson(body): crate::api::ApiJson<GalleryContextSelectionProjectionBody>,
) -> Result<Json<GalleryContextSelectionProjectionDto>, ApiError> {
    Ok(Json(
        state
            .gallery
            .context_selection_projection(&body.expression, &body.visible_context_ids)
            .await?
            .into(),
    ))
}

const fn default_context_limit() -> u16 {
    100
}

const MAX_CONTEXT_CURSOR_LENGTH: usize = 2048;

fn decode_context_cursor(encoded: Option<&str>) -> Result<Option<GalleryContextCursor>, ApiError> {
    let Some(encoded) = encoded else {
        return Ok(None);
    };
    if encoded.is_empty() || encoded.len() > MAX_CONTEXT_CURSOR_LENGTH {
        return Err(ApiError::invalid_request(
            "Invalid gallery directory cursor",
        ));
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| ApiError::invalid_request("Invalid gallery directory cursor"))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| ApiError::invalid_request("Invalid gallery directory cursor"))
}

fn encode_context_cursor(cursor: Option<GalleryContextCursor>) -> Option<String> {
    cursor.map(|cursor| {
        let bytes = serde_json::to_vec(&cursor).expect("gallery context cursor is serializable");
        URL_SAFE_NO_PAD.encode(bytes)
    })
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GalleryArtistDetailDto {
    pub id: Uuid,
    pub pixiv_artist_id: i64,
    pub name: String,
    #[schema(required)]
    pub account_name: Option<String>,
    pub work_count: u64,
    #[schema(required)]
    pub cover_url: Option<String>,
    #[schema(required)]
    pub cover_width: Option<u32>,
    #[schema(required)]
    pub cover_height: Option<u32>,
    #[schema(required)]
    pub cover_age_rating: Option<PixivAgeRating>,
}

impl From<GalleryArtistDetail> for GalleryArtistDetailDto {
    fn from(artist: GalleryArtistDetail) -> Self {
        Self {
            id: artist.id,
            pixiv_artist_id: artist.pixiv_artist_id,
            name: artist.name,
            account_name: artist.account_name,
            work_count: artist.work_count,
            cover_url: artist
                .cover_derivative_id
                .map(|id| format!("/api/derivatives/{id}")),
            cover_width: artist.cover_width,
            cover_height: artist.cover_height,
            cover_age_rating: artist.cover_age_rating,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GalleryArtistPageDto {
    pub items: Vec<GalleryArtistDetailDto>,
    pub total: u64,
    #[schema(required)]
    pub next_cursor: Option<String>,
}

impl From<GalleryContextPage<GalleryArtistDetail>> for GalleryArtistPageDto {
    fn from(page: GalleryContextPage<GalleryArtistDetail>) -> Self {
        Self {
            items: page
                .items
                .into_iter()
                .map(GalleryArtistDetailDto::from)
                .collect(),
            total: page.total,
            next_cursor: encode_context_cursor(page.next_cursor),
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/gallery/artists",
    params(
        ("limit" = Option<u16>, Query),
        ("cursor" = Option<String>, Query),
        ("q" = Option<String>, Query)
    ),
    responses((status = 200, body = GalleryArtistPageDto)),
    tag = "Gallery"
)]
pub(crate) async fn artists(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<ContextListQuery>,
) -> Result<Json<GalleryArtistPageDto>, ApiError> {
    let cursor = decode_context_cursor(query.cursor.as_deref())?;
    Ok(Json(
        state
            .gallery
            .artists(query.limit, cursor.as_ref(), query.q.as_deref())
            .await?
            .into(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/gallery/artists/{pixiv_artist_id}",
    params(("pixiv_artist_id" = i64, Path)),
    responses(
        (status = 200, body = GalleryArtistDetailDto),
        (status = 404, body = ApiErrorBody)
    ),
    tag = "Gallery"
)]
pub(crate) async fn artist_detail(
    State(state): State<AppState>,
    ApiPath(pixiv_artist_id): ApiPath<i64>,
) -> Result<Json<GalleryArtistDetailDto>, ApiError> {
    Ok(Json(
        state.gallery.artist_detail(pixiv_artist_id).await?.into(),
    ))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GalleryTagDetailDto {
    pub tag: GalleryTagDto,
    pub work_count: u64,
    #[schema(required)]
    pub cover_url: Option<String>,
    #[schema(required)]
    pub cover_width: Option<u32>,
    #[schema(required)]
    pub cover_height: Option<u32>,
    #[schema(required)]
    pub cover_age_rating: Option<PixivAgeRating>,
}

impl From<GalleryTagDetail> for GalleryTagDetailDto {
    fn from(detail: GalleryTagDetail) -> Self {
        Self {
            tag: detail.tag.into(),
            work_count: detail.work_count,
            cover_url: detail
                .cover_derivative_id
                .map(|id| format!("/api/derivatives/{id}")),
            cover_width: detail.cover_width,
            cover_height: detail.cover_height,
            cover_age_rating: detail.cover_age_rating,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GalleryTagPageDto {
    pub items: Vec<GalleryTagDetailDto>,
    pub total: u64,
    #[schema(required)]
    pub next_cursor: Option<String>,
}

impl From<GalleryContextPage<GalleryTagDetail>> for GalleryTagPageDto {
    fn from(page: GalleryContextPage<GalleryTagDetail>) -> Self {
        Self {
            items: page
                .items
                .into_iter()
                .map(GalleryTagDetailDto::from)
                .collect(),
            total: page.total,
            next_cursor: encode_context_cursor(page.next_cursor),
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/gallery/tags",
    params(
        ("limit" = Option<u16>, Query),
        ("cursor" = Option<String>, Query),
        ("q" = Option<String>, Query)
    ),
    responses((status = 200, body = GalleryTagPageDto)),
    tag = "Gallery"
)]
pub(crate) async fn tags(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<ContextListQuery>,
) -> Result<Json<GalleryTagPageDto>, ApiError> {
    let cursor = decode_context_cursor(query.cursor.as_deref())?;
    Ok(Json(
        state
            .gallery
            .tags(query.limit, cursor.as_ref(), query.q.as_deref())
            .await?
            .into(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/gallery/tags/{tag_name}",
    params(("tag_name" = String, Path)),
    responses(
        (status = 200, body = GalleryTagDetailDto),
        (status = 404, body = ApiErrorBody)
    ),
    tag = "Gallery"
)]
pub(crate) async fn tag_detail(
    State(state): State<AppState>,
    ApiPath(tag_name): ApiPath<String>,
) -> Result<Json<GalleryTagDetailDto>, ApiError> {
    Ok(Json(state.gallery.tag_detail(&tag_name).await?.into()))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GallerySeriesDetailDto {
    pub id: Uuid,
    pub pixiv_series_id: i64,
    #[schema(required)]
    pub pixiv_artist_id: Option<i64>,
    pub title: String,
    pub work_count: u64,
    #[schema(required)]
    pub cover_url: Option<String>,
    #[schema(required)]
    pub cover_width: Option<u32>,
    #[schema(required)]
    pub cover_height: Option<u32>,
    #[schema(required)]
    pub cover_age_rating: Option<PixivAgeRating>,
}

impl From<GallerySeriesDetail> for GallerySeriesDetailDto {
    fn from(series: GallerySeriesDetail) -> Self {
        Self {
            id: series.id,
            pixiv_series_id: series.pixiv_series_id,
            pixiv_artist_id: series.pixiv_artist_id,
            title: series.title,
            work_count: series.work_count,
            cover_url: series
                .cover_derivative_id
                .map(|id| format!("/api/derivatives/{id}")),
            cover_width: series.cover_width,
            cover_height: series.cover_height,
            cover_age_rating: series.cover_age_rating,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GallerySeriesPageDto {
    pub items: Vec<GallerySeriesDetailDto>,
    pub total: u64,
    #[schema(required)]
    pub next_cursor: Option<String>,
}

impl From<GalleryContextPage<GallerySeriesDetail>> for GallerySeriesPageDto {
    fn from(page: GalleryContextPage<GallerySeriesDetail>) -> Self {
        Self {
            items: page
                .items
                .into_iter()
                .map(GallerySeriesDetailDto::from)
                .collect(),
            total: page.total,
            next_cursor: encode_context_cursor(page.next_cursor),
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/gallery/series",
    params(
        ("limit" = Option<u16>, Query),
        ("cursor" = Option<String>, Query),
        ("q" = Option<String>, Query)
    ),
    responses((status = 200, body = GallerySeriesPageDto)),
    tag = "Gallery"
)]
pub(crate) async fn series(
    State(state): State<AppState>,
    ApiQuery(query): ApiQuery<ContextListQuery>,
) -> Result<Json<GallerySeriesPageDto>, ApiError> {
    let cursor = decode_context_cursor(query.cursor.as_deref())?;
    Ok(Json(
        state
            .gallery
            .series(query.limit, cursor.as_ref(), query.q.as_deref())
            .await?
            .into(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/gallery/series/{pixiv_series_id}",
    params(("pixiv_series_id" = i64, Path)),
    responses(
        (status = 200, body = GallerySeriesDetailDto),
        (status = 404, body = ApiErrorBody)
    ),
    tag = "Gallery"
)]
pub(crate) async fn series_detail(
    State(state): State<AppState>,
    ApiPath(pixiv_series_id): ApiPath<i64>,
) -> Result<Json<GallerySeriesDetailDto>, ApiError> {
    Ok(Json(
        state.gallery.series_detail(pixiv_series_id).await?.into(),
    ))
}
