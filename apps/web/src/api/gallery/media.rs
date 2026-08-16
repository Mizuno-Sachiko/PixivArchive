use crate::{
    api::{ApiError, ApiErrorBody, ApiPath},
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Request, State},
    http::{StatusCode, header},
    response::Response,
};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

#[utoipa::path(
    get,
    path = "/api/media/{media_revision_id}/source",
    params(("media_revision_id" = Uuid, Path)),
    responses(
        (status = 200, description = "Complete source media"),
        (status = 206, description = "Requested byte range"),
        (status = 404, body = ApiErrorBody),
        (
            status = 416,
            body = ApiErrorBody,
            headers(
                ("Content-Range" = String, description = "Available source media byte range")
            )
        ),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "Media"
)]
pub(crate) async fn source_media(
    State(state): State<AppState>,
    ApiPath(media_revision_id): ApiPath<Uuid>,
    request: Request,
) -> Result<Response, ApiError> {
    let source = state.media.source(media_revision_id).await?;
    serve_media(source, request).await
}

#[utoipa::path(
    get,
    path = "/api/derivatives/{derivative_id}",
    params(("derivative_id" = Uuid, Path)),
    responses(
        (status = 200, description = "Static waterfall derivative"),
        (status = 304, description = "Cached derivative is current"),
        (status = 404, body = ApiErrorBody),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "Media"
)]
pub(crate) async fn derivative_media(
    State(state): State<AppState>,
    ApiPath(derivative_id): ApiPath<Uuid>,
    request: Request,
) -> Result<Response, ApiError> {
    let source = state.media.derivative(derivative_id).await?;
    serve_media(source, request).await
}

pub(super) async fn serve_media(
    source: pixivarchive_application::system::MediaSource,
    request: Request,
) -> Result<Response, ApiError> {
    let response = ServeFile::new(source.path)
        .oneshot(request)
        .await
        .unwrap_or_else(|error| match error {});
    match response.status() {
        StatusCode::OK | StatusCode::PARTIAL_CONTENT | StatusCode::NOT_MODIFIED => {
            Ok(response.map(Body::new))
        }
        StatusCode::RANGE_NOT_SATISFIABLE => {
            let mut error = ApiError::new(
                StatusCode::RANGE_NOT_SATISFIABLE,
                "range_not_satisfiable",
                "The requested media byte range is not available",
            );
            if let Some(content_range) = response.headers().get(header::CONTENT_RANGE) {
                error = error.with_header(header::CONTENT_RANGE, content_range.clone());
            }
            Err(error)
        }
        StatusCode::NOT_FOUND => Err(ApiError::not_found("Media file was not found")),
        _ => Err(ApiError::service_unavailable()),
    }
}
