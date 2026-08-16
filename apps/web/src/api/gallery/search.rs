use crate::{
    api::{ApiError, ApiErrorBody, ApiJson},
    state::AppState,
};
use axum::{Json, extract::State};
use pixivarchive_domain::{
    job::CollectionState,
    media::MediaKind,
    pixiv::{PixivAgeRating, PixivWorkKind},
    work::{
        GalleryCursor, GallerySearch, GallerySearchPage, GallerySelectionExpression,
        GallerySelectionProjection, GalleryTag, GalleryWork, WorkSourceState,
    },
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GalleryTagDto {
    pub id: Uuid,
    pub original: String,
    #[schema(required)]
    pub translation: Option<String>,
}

impl From<GalleryTag> for GalleryTagDto {
    fn from(tag: GalleryTag) -> Self {
        Self {
            id: tag.id,
            original: tag.original,
            translation: tag.translation,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GalleryWorkDto {
    pub id: Uuid,
    pub pixiv_work_id: i64,
    pub title: String,
    #[schema(required)]
    pub description: Option<String>,
    pub artist_id: Uuid,
    pub pixiv_artist_id: i64,
    pub artist_name: String,
    #[schema(required)]
    pub series_id: Option<Uuid>,
    #[schema(required)]
    pub series_title: Option<String>,
    pub work_kind: PixivWorkKind,
    pub age_rating: PixivAgeRating,
    pub ai_generated: bool,
    pub page_count: u32,
    pub collection_state: CollectionState,
    pub source_state: WorkSourceState,
    pub bookmarked_by_current_account: bool,
    #[schema(required)]
    pub bookmark_id: Option<i64>,
    #[schema(required)]
    pub bookmark_count: Option<i64>,
    #[schema(required)]
    pub view_count: Option<i64>,
    #[schema(required)]
    pub like_count: Option<i64>,
    #[schema(required)]
    pub comment_count: Option<i64>,
    #[schema(required)]
    #[serde(with = "time::serde::rfc3339::option")]
    pub pixiv_published_at: Option<OffsetDateTime>,
    #[schema(required)]
    #[serde(with = "time::serde::rfc3339::option")]
    pub pixiv_updated_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    pub local_updated_at: OffsetDateTime,
    pub cover_available: bool,
    #[schema(required)]
    pub cover_url: Option<String>,
    #[schema(required)]
    pub cover_width: Option<u32>,
    #[schema(required)]
    pub cover_height: Option<u32>,
    #[schema(required)]
    pub media_kind: Option<MediaKind>,
    pub tags: Vec<GalleryTagDto>,
}

impl From<GalleryWork> for GalleryWorkDto {
    fn from(work: GalleryWork) -> Self {
        Self {
            id: work.id,
            pixiv_work_id: work.pixiv_work_id,
            title: work.title,
            description: work.description,
            artist_id: work.artist_id,
            pixiv_artist_id: work.pixiv_artist_id,
            artist_name: work.artist_name,
            series_id: work.series_id,
            series_title: work.series_title,
            work_kind: work.work_kind,
            age_rating: work.age_rating,
            ai_generated: work.ai_generated,
            page_count: work.page_count,
            collection_state: work.collection_state,
            source_state: work.source_state,
            bookmarked_by_current_account: work.bookmarked_by_current_account,
            bookmark_id: work.bookmark_id,
            bookmark_count: work.bookmark_count,
            view_count: work.view_count,
            like_count: work.like_count,
            comment_count: work.comment_count,
            pixiv_published_at: work.pixiv_published_at,
            pixiv_updated_at: work.pixiv_updated_at,
            local_updated_at: work.local_updated_at,
            cover_available: work.cover_path.is_some(),
            cover_url: work
                .cover_derivative_id
                .map(|id| format!("/api/derivatives/{id}")),
            cover_width: work.cover_width,
            cover_height: work.cover_height,
            media_kind: work.media_kind,
            tags: work.tags.into_iter().map(GalleryTagDto::from).collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GallerySearchPageDto {
    pub items: Vec<GalleryWorkDto>,
    #[schema(required)]
    pub next_cursor: Option<GalleryCursor>,
}

impl From<GallerySearchPage> for GallerySearchPageDto {
    fn from(page: GallerySearchPage) -> Self {
        Self {
            items: page.items.into_iter().map(GalleryWorkDto::from).collect(),
            next_cursor: page.next_cursor,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/gallery/search",
    request_body = GallerySearch,
    responses(
        (status = 200, body = GallerySearchPageDto),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Gallery"
)]
pub(crate) async fn search(
    State(state): State<AppState>,
    ApiJson(search): ApiJson<GallerySearch>,
) -> Result<Json<GallerySearchPageDto>, ApiError> {
    Ok(Json(state.gallery.search(search).await?.into()))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GalleryCountDto {
    pub count: u64,
}

#[utoipa::path(
    post,
    path = "/api/gallery/count",
    request_body = GallerySearch,
    responses(
        (status = 200, body = GalleryCountDto),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Gallery"
)]
pub(crate) async fn count(
    State(state): State<AppState>,
    ApiJson(mut search): ApiJson<GallerySearch>,
) -> Result<Json<GalleryCountDto>, ApiError> {
    search.cursor = None;
    Ok(Json(GalleryCountDto {
        count: state.gallery.count(&search).await?,
    }))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct GallerySelectionProjectionBody {
    pub expression: GallerySelectionExpression,
    pub visible_work_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GallerySelectionProjectionDto {
    pub selected_count: u64,
    pub selected_visible_work_ids: Vec<Uuid>,
}

impl From<GallerySelectionProjection> for GallerySelectionProjectionDto {
    fn from(projection: GallerySelectionProjection) -> Self {
        Self {
            selected_count: projection.selected_count,
            selected_visible_work_ids: projection.selected_visible_work_ids,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/gallery/selection",
    request_body = GallerySelectionProjectionBody,
    responses(
        (status = 200, body = GallerySelectionProjectionDto),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Gallery"
)]
pub(crate) async fn selection_projection(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<GallerySelectionProjectionBody>,
) -> Result<Json<GallerySelectionProjectionDto>, ApiError> {
    Ok(Json(
        state
            .gallery
            .selection_projection(&body.expression, &body.visible_work_ids)
            .await?
            .into(),
    ))
}
