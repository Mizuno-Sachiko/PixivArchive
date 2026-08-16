use super::{ApiError, ApiErrorBody, ApiJson, ApiPath};
use crate::state::AppState;
use axum::{
    Json, Router,
    extract::{RawQuery, State},
    http::StatusCode,
    routing::{get, post, put},
};
use pixivarchive_application::trash::TrashSelectionCommandError;
use pixivarchive_domain::work::{
    GalleryContextSelectionExpression, GallerySelectionExpression, TrashActionCapabilities,
    TrashCollectionSummary, TrashCursor, TrashEntry, TrashFilter, TrashSelectionExpression,
    TrashSelectionProjection, TrashWorkSummary,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use url::form_urlencoded;
use utoipa::ToSchema;
use uuid::Uuid;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/trash", get(list))
        .route("/trash/selection", post(project_selection))
        .route("/works/{work_id}/trash", post(move_to_trash))
        .route("/gallery/trash", post(move_gallery_to_trash))
        .route(
            "/gallery/contexts/trash",
            post(move_gallery_contexts_to_trash),
        )
        .route("/trash/{work_id}/restore", post(restore))
        .route("/trash/{work_id}/schedule", put(reschedule))
        .route("/trash/{work_id}/purge", post(purge))
        .route("/trash/restore", post(restore_many))
        .route("/trash/schedule", put(reschedule_many))
        .route("/trash/purge", post(purge_many))
        .route("/trash/purge-all", post(purge_all))
}

#[derive(Clone, Debug)]
pub struct TrashListQuery {
    pub limit: u16,
    pub query: Option<String>,
    pub purge_states: Vec<String>,
    pub cursor_scheduled_purge_at: Option<OffsetDateTime>,
    pub cursor_work_id: Option<Uuid>,
}

const fn default_trash_limit() -> u16 {
    100
}

impl TrashListQuery {
    fn parse(raw_query: Option<&str>) -> Result<Self, ApiError> {
        let mut limit = None;
        let mut query = None;
        let mut query_seen = false;
        let mut purge_states = Vec::new();
        let mut cursor_scheduled_purge_at = None;
        let mut cursor_work_id = None;

        for (key, value) in form_urlencoded::parse(raw_query.unwrap_or_default().as_bytes()) {
            match key.as_ref() {
                "limit" => set_query_value(
                    &mut limit,
                    value
                        .parse::<u16>()
                        .map_err(|_| ApiError::invalid_request("Trash limit is invalid"))?,
                    "limit",
                )?,
                "query" => {
                    if query_seen {
                        return Err(ApiError::invalid_request(
                            "Trash query must appear only once",
                        ));
                    }
                    query_seen = true;
                    query = Some(value.into_owned());
                }
                "purge_state" => purge_states.push(value.into_owned()),
                "cursor_scheduled_purge_at" => set_query_value(
                    &mut cursor_scheduled_purge_at,
                    OffsetDateTime::parse(&value, &Rfc3339).map_err(|_| {
                        ApiError::invalid_request("Trash cursor timestamp is invalid")
                    })?,
                    "cursor_scheduled_purge_at",
                )?,
                "cursor_work_id" => set_query_value(
                    &mut cursor_work_id,
                    Uuid::parse_str(&value).map_err(|_| {
                        ApiError::invalid_request("Trash cursor work ID is invalid")
                    })?,
                    "cursor_work_id",
                )?,
                _ => {}
            }
        }

        Ok(Self {
            limit: limit.unwrap_or_else(default_trash_limit),
            query,
            purge_states,
            cursor_scheduled_purge_at,
            cursor_work_id,
        })
    }
}

fn set_query_value<T>(slot: &mut Option<T>, value: T, field: &str) -> Result<(), ApiError> {
    if slot.replace(value).is_some() {
        return Err(ApiError::invalid_request(format!(
            "Trash query field {field} must appear only once"
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct MoveToTrashBody {
    pub retention_days: u16,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TrashEntryDto {
    pub work_id: Uuid,
    pub previous_collection_state: String,
    #[serde(with = "time::serde::rfc3339")]
    pub trashed_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub scheduled_purge_at: OffsetDateTime,
    pub purge_state: String,
    pub purge_attempts: u32,
    #[schema(required)]
    pub failure_message: Option<String>,
    pub capabilities: TrashActionCapabilities,
}

impl From<TrashEntry> for TrashEntryDto {
    fn from(entry: TrashEntry) -> Self {
        Self {
            work_id: entry.work_id,
            previous_collection_state: entry.previous_collection_state,
            trashed_at: entry.trashed_at,
            scheduled_purge_at: entry.scheduled_purge_at,
            purge_state: entry.purge_state,
            purge_attempts: entry.purge_attempts,
            failure_message: entry.failure_message,
            capabilities: entry.capabilities,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TrashWorkSummaryDto {
    #[serde(flatten)]
    pub entry: TrashEntryDto,
    pub pixiv_work_id: i64,
    pub title: String,
    pub artist_name: String,
    pub page_count: u32,
    pub estimated_release_bytes: u64,
}

impl From<TrashWorkSummary> for TrashWorkSummaryDto {
    fn from(summary: TrashWorkSummary) -> Self {
        Self {
            entry: summary.entry.into(),
            pixiv_work_id: summary.pixiv_work_id,
            title: summary.title,
            artist_name: summary.artist_name,
            page_count: summary.page_count,
            estimated_release_bytes: summary.estimated_release_bytes,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TrashListDto {
    pub items: Vec<TrashWorkSummaryDto>,
    #[schema(required)]
    pub next_cursor: Option<TrashCursorDto>,
    pub summary: TrashCollectionSummaryDto,
    pub all_summary: TrashCollectionSummaryDto,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TrashCursorDto {
    #[serde(with = "time::serde::rfc3339")]
    pub scheduled_purge_at: OffsetDateTime,
    pub work_id: Uuid,
}

impl From<TrashCursor> for TrashCursorDto {
    fn from(cursor: TrashCursor) -> Self {
        Self {
            scheduled_purge_at: cursor.scheduled_purge_at,
            work_id: cursor.work_id,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TrashCollectionSummaryDto {
    pub total_count: u64,
    pub logical_bytes: u64,
    pub estimated_reclaimable_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct TrashSelectionBody {
    pub expression: TrashSelectionExpression,
    pub visible_work_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TrashSelectionDto {
    pub selected_count: u64,
    pub blocked_count: u64,
    pub selected_visible_work_ids: Vec<Uuid>,
}

impl From<TrashSelectionProjection> for TrashSelectionDto {
    fn from(projection: TrashSelectionProjection) -> Self {
        Self {
            selected_count: projection.selected_count,
            blocked_count: projection.blocked_count,
            selected_visible_work_ids: projection.selected_visible_work_ids,
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/trash/selection",
    request_body = TrashSelectionBody,
    responses(
        (status = 200, body = TrashSelectionDto),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Trash"
)]
pub(crate) async fn project_selection(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<TrashSelectionBody>,
) -> Result<Json<TrashSelectionDto>, ApiError> {
    Ok(Json(
        state
            .trash
            .project_selection(&body.expression, &body.visible_work_ids)
            .await?
            .into(),
    ))
}

impl From<TrashCollectionSummary> for TrashCollectionSummaryDto {
    fn from(summary: TrashCollectionSummary) -> Self {
        Self {
            total_count: summary.total_count,
            logical_bytes: summary.logical_bytes,
            estimated_reclaimable_bytes: summary.estimated_reclaimable_bytes,
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/trash",
    params(
        ("limit" = Option<u16>, Query),
        ("query" = Option<String>, Query),
        ("purge_state" = Option<Vec<String>>, Query),
        ("cursor_scheduled_purge_at" = Option<OffsetDateTime>, Query),
        ("cursor_work_id" = Option<Uuid>, Query)
    ),
    responses((status = 200, body = TrashListDto)),
    tag = "Trash"
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<TrashListDto>, ApiError> {
    let query = TrashListQuery::parse(raw_query.as_deref())?;
    let filter = TrashFilter {
        query: query.query,
        purge_states: query.purge_states,
    };
    let cursor = match (query.cursor_scheduled_purge_at, query.cursor_work_id) {
        (Some(scheduled_purge_at), Some(work_id)) => Some(TrashCursor {
            scheduled_purge_at,
            work_id,
        }),
        (None, None) => None,
        _ => return Err(ApiError::invalid_request("Trash cursor is incomplete")),
    };
    let all_filter = TrashFilter::default();
    let all_summary = async {
        if filter == all_filter {
            Ok(None)
        } else {
            state.trash.summary(&all_filter).await.map(Some)
        }
    };
    let (page, summary, all_summary) = tokio::try_join!(
        state.trash.page(&filter, cursor.as_ref(), query.limit),
        state.trash.summary(&filter),
        all_summary,
    )?;
    let all_summary = all_summary.unwrap_or(summary);
    Ok(Json(TrashListDto {
        items: page
            .items
            .into_iter()
            .map(TrashWorkSummaryDto::from)
            .collect(),
        next_cursor: page.next_cursor.map(TrashCursorDto::from),
        summary: summary.into(),
        all_summary: all_summary.into(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/works/{work_id}/trash",
    params(("work_id" = Uuid, Path)),
    request_body = MoveToTrashBody,
    responses(
        (status = 200, body = TrashEntryDto),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Trash"
)]
pub(crate) async fn move_to_trash(
    State(state): State<AppState>,
    ApiPath(work_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<MoveToTrashBody>,
) -> Result<Json<TrashEntryDto>, ApiError> {
    Ok(Json(
        state
            .trash
            .move_to_trash(work_id, body.retention_days)
            .await?
            .into(),
    ))
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct MoveGalleryToTrashBody {
    pub expression: GallerySelectionExpression,
    pub retention_days: u16,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct MoveGalleryToTrashDto {
    pub moved_count: u64,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct MoveGalleryContextsToTrashBody {
    pub expression: GalleryContextSelectionExpression,
    pub retention_days: u16,
}

#[utoipa::path(
    post,
    path = "/api/gallery/trash",
    request_body = MoveGalleryToTrashBody,
    responses(
        (status = 200, body = MoveGalleryToTrashDto),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Trash"
)]
pub(crate) async fn move_gallery_to_trash(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<MoveGalleryToTrashBody>,
) -> Result<Json<MoveGalleryToTrashDto>, ApiError> {
    Ok(Json(MoveGalleryToTrashDto {
        moved_count: state
            .trash
            .move_selection_to_trash(body.expression, body.retention_days)
            .await?,
    }))
}

#[utoipa::path(
    post,
    path = "/api/gallery/contexts/trash",
    request_body = MoveGalleryContextsToTrashBody,
    responses(
        (status = 200, body = MoveGalleryToTrashDto),
        (status = 422, body = ApiErrorBody)
    ),
    tag = "Trash"
)]
pub(crate) async fn move_gallery_contexts_to_trash(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<MoveGalleryContextsToTrashBody>,
) -> Result<Json<MoveGalleryToTrashDto>, ApiError> {
    Ok(Json(MoveGalleryToTrashDto {
        moved_count: state
            .trash
            .move_context_selection_to_trash(body.expression, body.retention_days)
            .await?,
    }))
}

#[utoipa::path(
    post,
    path = "/api/trash/{work_id}/restore",
    params(("work_id" = Uuid, Path)),
    responses((status = 204), (status = 404, body = ApiErrorBody)),
    tag = "Trash"
)]
pub(crate) async fn restore(
    State(state): State<AppState>,
    ApiPath(work_id): ApiPath<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.trash.restore(work_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct RescheduleTrashBody {
    #[serde(with = "time::serde::rfc3339")]
    pub scheduled_purge_at: OffsetDateTime,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct TrashSelectionCommandBody {
    pub expression: TrashSelectionExpression,
}

#[derive(Clone, Debug, Deserialize, ToSchema)]
pub struct RescheduleTrashManyBody {
    pub expression: TrashSelectionExpression,
    #[serde(with = "time::serde::rfc3339")]
    pub scheduled_purge_at: OffsetDateTime,
}

#[utoipa::path(
    put,
    path = "/api/trash/{work_id}/schedule",
    params(("work_id" = Uuid, Path)),
    request_body = RescheduleTrashBody,
    responses((status = 204), (status = 404, body = ApiErrorBody)),
    tag = "Trash"
)]
pub(crate) async fn reschedule(
    State(state): State<AppState>,
    ApiPath(work_id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<RescheduleTrashBody>,
) -> Result<StatusCode, ApiError> {
    state
        .trash
        .reschedule(work_id, body.scheduled_purge_at)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/trash/restore",
    request_body = TrashSelectionCommandBody,
    responses(
        (status = 200, body = TrashBatchAccepted),
        (status = 409, body = ApiErrorBody)
    ),
    tag = "Trash"
)]
pub(crate) async fn restore_many(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<TrashSelectionCommandBody>,
) -> Result<Json<TrashBatchAccepted>, ApiError> {
    Ok(Json(TrashBatchAccepted {
        affected_count: state.trash.restore_selection(&body.expression).await?,
    }))
}

#[utoipa::path(
    put,
    path = "/api/trash/schedule",
    request_body = RescheduleTrashManyBody,
    responses(
        (status = 200, body = TrashBatchAccepted),
        (status = 409, body = ApiErrorBody)
    ),
    tag = "Trash"
)]
pub(crate) async fn reschedule_many(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<RescheduleTrashManyBody>,
) -> Result<Json<TrashBatchAccepted>, ApiError> {
    Ok(Json(TrashBatchAccepted {
        affected_count: state
            .trash
            .reschedule_selection(&body.expression, body.scheduled_purge_at)
            .await?,
    }))
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct PurgeAccepted {
    pub job_id: Uuid,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct TrashBatchAccepted {
    pub affected_count: u64,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct PurgeAllAccepted {
    pub accepted_count: u64,
}

impl From<TrashSelectionCommandError> for ApiError {
    fn from(error: TrashSelectionCommandError) -> Self {
        match error {
            TrashSelectionCommandError::Blocked {
                selected_count,
                blocked_count,
            } => Self::new(
                StatusCode::CONFLICT,
                "trash_selection_blocked",
                "The trash selection contains works that cannot be restored or rescheduled",
            )
            .with_details(json!({
                "selected_count": selected_count,
                "blocked_count": blocked_count,
            })),
            TrashSelectionCommandError::Storage(error) => error.into(),
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/trash/{work_id}/purge",
    params(("work_id" = Uuid, Path)),
    responses((status = 202, body = PurgeAccepted), (status = 404, body = ApiErrorBody)),
    tag = "Trash"
)]
pub(crate) async fn purge(
    State(state): State<AppState>,
    ApiPath(work_id): ApiPath<Uuid>,
) -> Result<(StatusCode, Json<PurgeAccepted>), ApiError> {
    let job_id = state.trash.purge(work_id).await?;
    Ok((StatusCode::ACCEPTED, Json(PurgeAccepted { job_id })))
}

#[utoipa::path(
    post,
    path = "/api/trash/purge",
    request_body = TrashSelectionCommandBody,
    responses((status = 202, body = TrashBatchAccepted)),
    tag = "Trash"
)]
pub(crate) async fn purge_many(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<TrashSelectionCommandBody>,
) -> Result<(StatusCode, Json<TrashBatchAccepted>), ApiError> {
    let affected_count = state.trash.purge_selection(&body.expression).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(TrashBatchAccepted { affected_count }),
    ))
}

#[utoipa::path(
    post,
    path = "/api/trash/purge-all",
    responses((status = 202, body = PurgeAllAccepted)),
    tag = "Trash"
)]
pub(crate) async fn purge_all(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<PurgeAllAccepted>), ApiError> {
    let accepted_count = state.trash.purge_all().await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(PurgeAllAccepted { accepted_count }),
    ))
}

#[cfg(test)]
mod tests {
    use super::TrashListQuery;
    use uuid::Uuid;

    #[test]
    fn trash_query_preserves_repeated_purge_states() {
        let work_id = Uuid::now_v7();
        let query = TrashListQuery::parse(Some(&format!(
            "limit=50&query=archive&purge_state=pending&purge_state=failed&cursor_scheduled_purge_at=2026-08-06T12%3A00%3A00Z&cursor_work_id={work_id}"
        )))
        .unwrap();

        assert_eq!(query.limit, 50);
        assert_eq!(query.query.as_deref(), Some("archive"));
        assert_eq!(query.purge_states, vec!["pending", "failed"]);
        assert_eq!(query.cursor_work_id, Some(work_id));
        assert!(query.cursor_scheduled_purge_at.is_some());
    }
}
