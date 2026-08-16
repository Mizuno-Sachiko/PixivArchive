use super::{ApiError, ApiErrorBody};
use crate::state::AppState;
use axum::{
    BoxError, Router,
    extract::{FromRef, State},
    http::{HeaderMap, StatusCode, header},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
};
use futures_util::{Stream, TryStreamExt};
use pixivarchive_application::events::{EventStream, StreamEvent};
use std::time::Duration;

#[derive(Clone)]
pub struct EventApiState {
    pub events: EventStream,
}

pub fn router(state: EventApiState) -> Router {
    Router::new()
        .route("/events", get(sse_events))
        .with_state(state)
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/events", get(sse_events))
}

impl FromRef<AppState> for EventApiState {
    fn from_ref(state: &AppState) -> Self {
        Self {
            events: state.events.clone(),
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/events",
    responses(
        (status = 200, description = "Replayable server-sent event stream"),
        (status = 400, body = ApiErrorBody),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "Events"
)]
pub(crate) async fn sse_events(
    State(state): State<EventApiState>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, BoxError>>>, ApiError> {
    let last_event_id = parse_last_event_id(&headers)?;
    let opened = state
        .events
        .after(last_event_id)
        .await
        .map_err(ApiError::from)?;
    let stream = opened
        .into_stream()
        .map_ok(to_sse_event)
        .map_err(Into::into);
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}

fn parse_last_event_id(headers: &HeaderMap) -> Result<Option<i64>, ApiError> {
    let Some(value) = headers.get(header::HeaderName::from_static("last-event-id")) else {
        return Ok(None);
    };
    let value = value
        .to_str()
        .map_err(|_| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                "invalid_last_event_id",
                "Last-Event-ID is invalid",
            )
        })?
        .trim();
    if value.is_empty() {
        return Ok(None);
    }
    value.parse::<i64>().map(Some).map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_last_event_id",
            "Last-Event-ID is invalid",
        )
    })
}

fn to_sse_event(event: StreamEvent) -> Event {
    match event {
        StreamEvent::AppEvent(event) => Event::default()
            .id(event.id.to_string())
            .event("app_event")
            .data(serde_json::to_string(&event).unwrap()),
        StreamEvent::SnapshotRefresh { latest_event_id } => Event::default()
            .id(latest_event_id.to_string())
            .event("snapshot_refresh")
            .data(serde_json::json!({ "latest_event_id": latest_event_id }).to_string()),
    }
}
