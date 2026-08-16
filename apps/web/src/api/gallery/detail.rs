use super::{encode_hex, search::GalleryWorkDto};
use crate::{
    api::{ApiError, ApiErrorBody, ApiPath},
    state::AppState,
};
use axum::{Json, extract::State};
use pixivarchive_domain::{
    media::{DerivativeFormat, MediaFormat, MediaKind},
    pixiv::PixivUgoiraMeta,
    pixiv::PixivWorkKind,
    work::{
        GalleryDerivative, GalleryMediaRevision, GalleryPage, GalleryWorkDetail,
        TrashActionCapabilities, WorkRevisionSummary, WorkSourceState,
    },
};
use serde::Serialize;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GalleryDerivativeDto {
    pub id: Uuid,
    pub kind: String,
    pub format: DerivativeFormat,
    pub width: u32,
    pub height: u32,
    pub byte_size: u64,
    pub dominant_color: String,
    pub url: String,
}

impl From<GalleryDerivative> for GalleryDerivativeDto {
    fn from(derivative: GalleryDerivative) -> Self {
        Self {
            id: derivative.id,
            kind: derivative.kind,
            format: derivative.format,
            width: derivative.width,
            height: derivative.height,
            byte_size: derivative.byte_size,
            dominant_color: derivative.dominant_color,
            url: format!("/api/derivatives/{}", derivative.id),
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GalleryMediaRevisionDto {
    pub id: Uuid,
    pub revision_number: u64,
    pub media_kind: MediaKind,
    pub format: MediaFormat,
    pub byte_size: u64,
    pub sha256: String,
    pub source_url: String,
    pub derivatives: Vec<GalleryDerivativeDto>,
}

impl From<GalleryMediaRevision> for GalleryMediaRevisionDto {
    fn from(media: GalleryMediaRevision) -> Self {
        Self {
            id: media.id,
            revision_number: media.revision_number,
            media_kind: media.media_kind,
            format: media.format,
            byte_size: media.byte_size,
            sha256: encode_hex(&media.sha256),
            source_url: format!("/api/media/{}/source", media.id),
            derivatives: media
                .derivatives
                .into_iter()
                .map(GalleryDerivativeDto::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GalleryPageDto {
    pub id: Uuid,
    pub page_index: u32,
    pub source_state: WorkSourceState,
    #[schema(required)]
    pub width: Option<u32>,
    #[schema(required)]
    pub height: Option<u32>,
    #[schema(required)]
    pub current_media: Option<GalleryMediaRevisionDto>,
}

impl From<GalleryPage> for GalleryPageDto {
    fn from(page: GalleryPage) -> Self {
        Self {
            id: page.id,
            page_index: page.page_index,
            source_state: page.source_state,
            width: page.width,
            height: page.height,
            current_media: page.current_media.map(GalleryMediaRevisionDto::from),
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct GalleryWorkDetailDto {
    pub work: GalleryWorkDto,
    pub pages: Vec<GalleryPageDto>,
    #[schema(required)]
    pub ugoira: Option<UgoiraManifestDto>,
    #[schema(required)]
    pub trash_capabilities: Option<TrashActionCapabilities>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct WorkIdResolutionDto {
    pub work_id: Uuid,
}

impl From<GalleryWorkDetail> for GalleryWorkDetailDto {
    fn from(detail: GalleryWorkDetail) -> Self {
        Self {
            work: detail.work.into(),
            pages: detail.pages.into_iter().map(GalleryPageDto::from).collect(),
            ugoira: detail.ugoira.map(UgoiraManifestDto::from),
            trash_capabilities: detail.trash_capabilities,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct UgoiraFrameDto {
    pub file: String,
    pub delay_ms: u32,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct UgoiraManifestDto {
    pub frame_mime_type: String,
    pub frames: Vec<UgoiraFrameDto>,
}

impl From<PixivUgoiraMeta> for UgoiraManifestDto {
    fn from(manifest: PixivUgoiraMeta) -> Self {
        Self {
            frame_mime_type: manifest.frame_mime_type,
            frames: manifest
                .frames
                .into_iter()
                .map(|frame| UgoiraFrameDto {
                    file: frame.file,
                    delay_ms: frame.delay_ms,
                })
                .collect(),
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/works/{work_id}",
    params(("work_id" = Uuid, Path)),
    responses(
        (status = 200, body = GalleryWorkDetailDto),
        (status = 404, body = ApiErrorBody)
    ),
    tag = "Gallery"
)]
pub(crate) async fn work_detail(
    State(state): State<AppState>,
    ApiPath(work_id): ApiPath<Uuid>,
) -> Result<Json<GalleryWorkDetailDto>, ApiError> {
    Ok(Json(state.gallery.work_detail(work_id).await?.into()))
}

#[utoipa::path(
    get,
    path = "/api/works/by-pixiv-id/{pixiv_work_id}",
    params(("pixiv_work_id" = i64, Path)),
    responses(
        (status = 200, body = WorkIdResolutionDto),
        (status = 404, body = ApiErrorBody)
    ),
    tag = "Gallery"
)]
pub(crate) async fn work_id_by_pixiv_id(
    State(state): State<AppState>,
    ApiPath(pixiv_work_id): ApiPath<i64>,
) -> Result<Json<WorkIdResolutionDto>, ApiError> {
    Ok(Json(WorkIdResolutionDto {
        work_id: state.gallery.work_id_by_pixiv_id(pixiv_work_id).await?,
    }))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct WorkRevisionSummaryDto {
    pub id: Uuid,
    pub title: String,
    #[schema(required)]
    pub description: Option<String>,
    pub work_kind: PixivWorkKind,
    pub page_count: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub captured_at: OffsetDateTime,
}

impl From<WorkRevisionSummary> for WorkRevisionSummaryDto {
    fn from(revision: WorkRevisionSummary) -> Self {
        Self {
            id: revision.id,
            title: revision.title,
            description: revision.description,
            work_kind: revision.work_kind,
            page_count: revision.page_count,
            captured_at: revision.captured_at,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/works/{work_id}/revisions",
    params(("work_id" = Uuid, Path)),
    responses(
        (status = 200, body = [WorkRevisionSummaryDto]),
        (status = 404, body = ApiErrorBody)
    ),
    tag = "Gallery"
)]
pub(crate) async fn work_revisions(
    State(state): State<AppState>,
    ApiPath(work_id): ApiPath<Uuid>,
) -> Result<Json<Vec<WorkRevisionSummaryDto>>, ApiError> {
    Ok(Json(
        state
            .gallery
            .revisions(work_id)
            .await?
            .into_iter()
            .map(WorkRevisionSummaryDto::from)
            .collect(),
    ))
}
