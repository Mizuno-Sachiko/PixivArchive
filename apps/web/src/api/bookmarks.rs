use super::{ApiError, ApiErrorBody, ApiJson, ApiPath};
use crate::state::AppState;
use axum::{
    Json, Router,
    extract::State,
    routing::{delete, post},
};
use pixivarchive_application::bookmarks::{
    BookmarkCommandRequest, BookmarkWritebackResult, BookmarkWritebackStatus,
};
use pixivarchive_domain::pixiv::PixivBookmarkVisibility;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/bookmarks", post(add))
        .route("/bookmarks/works/{work_id}", delete(remove))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct AddBookmarkBody {
    pub account_id: Uuid,
    pub work_id: i64,
    pub visibility: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct RemoveBookmarkBody {
    pub account_id: Uuid,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct BookmarkCommandDto {
    pub status: String,
    #[schema(required)]
    pub bookmark_id: Option<i64>,
    #[schema(required)]
    pub error_class: Option<String>,
}

impl From<BookmarkWritebackResult> for BookmarkCommandDto {
    fn from(result: BookmarkWritebackResult) -> Self {
        Self {
            status: match result.status {
                BookmarkWritebackStatus::Disabled => "disabled",
                BookmarkWritebackStatus::Succeeded => "succeeded",
                BookmarkWritebackStatus::Failed => "failed",
            }
            .to_owned(),
            bookmark_id: result.bookmark_id,
            error_class: result.error_class,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/bookmarks",
    request_body = AddBookmarkBody,
    responses(
        (status = 200, body = BookmarkCommandDto),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Bookmarks"
)]
pub(crate) async fn add(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<AddBookmarkBody>,
) -> Result<Json<BookmarkCommandDto>, ApiError> {
    let visibility = parse_visibility(&body.visibility)?;
    let result = state
        .bookmarks
        .add(BookmarkCommandRequest {
            account_id: body.account_id,
            target_pixiv_id: body.work_id,
            visibility,
            tags: body.tags,
        })
        .await?;
    Ok(Json(result.into()))
}

#[utoipa::path(
    delete,
    path = "/api/bookmarks/works/{work_id}",
    params(("work_id" = i64, Path)),
    request_body = RemoveBookmarkBody,
    responses((status = 200, body = BookmarkCommandDto)),
    tag = "Bookmarks"
)]
pub(crate) async fn remove(
    State(state): State<AppState>,
    ApiPath(work_id): ApiPath<i64>,
    ApiJson(body): ApiJson<RemoveBookmarkBody>,
) -> Result<Json<BookmarkCommandDto>, ApiError> {
    let result = state
        .bookmarks
        .remove(BookmarkCommandRequest {
            account_id: body.account_id,
            target_pixiv_id: work_id,
            visibility: PixivBookmarkVisibility::Private,
            tags: Vec::new(),
        })
        .await?;
    Ok(Json(result.into()))
}

fn parse_visibility(value: &str) -> Result<PixivBookmarkVisibility, ApiError> {
    match value {
        "public" => Ok(PixivBookmarkVisibility::Public),
        "private" => Ok(PixivBookmarkVisibility::Private),
        _ => Err(ApiError::invalid_request("Unknown bookmark visibility")),
    }
}
