use crate::state::AppState;
use axum::{
    Router,
    routing::{get, post},
};

pub(crate) mod context;
pub(crate) mod detail;
pub(crate) mod download;
pub(crate) mod media;
pub(crate) mod overview;
pub(crate) mod search;

pub use context::{
    ContextListQuery, GalleryArtistDetailDto, GalleryArtistPageDto, GallerySeriesDetailDto,
    GallerySeriesPageDto, GalleryTagDetailDto, GalleryTagPageDto,
};
pub use context::{GalleryContextSelectionProjectionBody, GalleryContextSelectionProjectionDto};
pub use detail::{
    GalleryDerivativeDto, GalleryMediaRevisionDto, GalleryPageDto, GalleryWorkDetailDto,
    UgoiraFrameDto, UgoiraManifestDto, WorkIdResolutionDto, WorkRevisionSummaryDto,
};
pub use overview::{OverviewDecorationDto, OverviewDecorationsDto, OverviewDecorationsQuery};
pub use search::{
    GalleryCountDto, GallerySearchPageDto, GallerySelectionProjectionBody,
    GallerySelectionProjectionDto, GalleryTagDto, GalleryWorkDto,
};

pub(crate) use context::{
    artist_detail, artists, context_selection_projection, series, series_detail, tag_detail, tags,
};
pub(crate) use detail::{work_detail, work_id_by_pixiv_id, work_revisions};
pub(crate) use download::download_work;
pub(crate) use media::{derivative_media, source_media};
pub(crate) use overview::{overview_decorations, shuffle_overview_decorations};
pub(crate) use search::{count, search, selection_projection};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/gallery/search", post(search))
        .route("/gallery/count", post(count))
        .route("/gallery/selection", post(selection_projection))
        .route(
            "/gallery/contexts/selection",
            post(context_selection_projection),
        )
        .route(
            "/gallery/overview-decorations",
            get(overview_decorations).post(shuffle_overview_decorations),
        )
        .route("/gallery/artists", get(artists))
        .route("/gallery/artists/{pixiv_artist_id}", get(artist_detail))
        .route("/gallery/tags", get(tags))
        .route("/gallery/tags/{tag_name}", get(tag_detail))
        .route("/gallery/series", get(series))
        .route("/gallery/series/{pixiv_series_id}", get(series_detail))
        .route(
            "/works/by-pixiv-id/{pixiv_work_id}",
            get(work_id_by_pixiv_id),
        )
        .route("/works/{work_id}", get(work_detail))
        .route("/works/{work_id}/revisions", get(work_revisions))
        .route("/works/{work_id}/download", get(download_work))
        .route("/media/{media_revision_id}/source", get(source_media))
        .route("/derivatives/{derivative_id}", get(derivative_media))
}

pub(super) fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}
