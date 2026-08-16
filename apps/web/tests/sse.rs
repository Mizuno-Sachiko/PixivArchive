use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use futures_util::{StreamExt, stream};
use pixivarchive_application::{
    auth::{AuthConfig, AuthService, LoginRequest},
    events::EventStream,
};
use pixivarchive_db::{Db, EventRepository};
use pixivarchive_domain::event::{EventPayload, EventResource};
use pixivarchive_web::{
    api::events::{EventApiState, router as event_router},
    middleware::auth::AuthLayer,
};
use sqlx::{Connection, PgConnection};
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

struct LockedDb {
    db: Db,
    database_url: String,
    _lock: PgConnection,
}

impl LockedDb {
    async fn new() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must point at the isolated test database");
        let mut lock = PgConnection::connect(&database_url).await.unwrap();
        sqlx::query("SELECT pg_advisory_lock(709020006)")
            .execute(&mut lock)
            .await
            .unwrap();
        let db = Db::connect(&database_url).await.unwrap();
        sqlx::migrate!("../../migrations")
            .run(db.pool())
            .await
            .unwrap();
        sqlx::query(
            r#"
            TRUNCATE TABLE
                app_event,
                login_rate_limit_reservation,
                login_rate_limit,
                login_attempt,
                admin_session,
                administrator
            RESTART IDENTITY CASCADE
            "#,
        )
        .execute(db.pool())
        .await
        .unwrap();
        Self {
            db,
            database_url,
            _lock: lock,
        }
    }
}

#[tokio::test]
async fn sse_route_requires_session_cookie() {
    let locked = LockedDb::new().await;
    let app = build_app(&locked).await.0;

    let response = app
        .oneshot(Request::get("/events").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sse_replays_after_last_event_id_with_authenticated_cookie() {
    let locked = LockedDb::new().await;
    let (app, cookie) = build_app(&locked).await;
    let first = append_event(&locked.db, EventPayload::JobQueued { revision: 1 }).await;
    let second = append_event(&locked.db, EventPayload::JobCompleted { revision: 2 }).await;

    let mut body = app
        .oneshot(
            Request::get("/events")
                .header(header::COOKIE, cookie)
                .header("Last-Event-ID", first.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .into_body()
        .into_data_stream();
    let frame = next_frame(&mut body).await;

    assert!(frame.contains(&format!("id: {second}")));
    assert!(frame.contains("event: app_event"));
    assert!(frame.contains("job_completed"));
}

#[tokio::test]
async fn sse_future_cursor_requests_snapshot_refresh() {
    let locked = LockedDb::new().await;
    let (app, cookie) = build_app(&locked).await;
    let first = append_event(&locked.db, EventPayload::JobQueued { revision: 1 }).await;

    let mut body = app
        .oneshot(
            Request::get("/events")
                .header(header::COOKIE, cookie)
                .header("Last-Event-ID", (first + 100).to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .into_body()
        .into_data_stream();
    let frame = next_frame(&mut body).await;

    assert!(frame.contains(&format!("id: {first}")));
    assert!(frame.contains("event: snapshot_refresh"));
}

#[tokio::test]
async fn sse_uses_explicit_keepalive_when_no_events_are_ready() {
    let locked = LockedDb::new().await;
    let (app, cookie) = build_app(&locked).await;

    let mut body = app
        .oneshot(
            Request::get("/events")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .into_body()
        .into_data_stream();
    let frame = tokio::time::timeout(Duration::from_secs(16), body.next())
        .await
        .expect("SSE keepalive should arrive within the configured 15 seconds")
        .unwrap()
        .unwrap();

    assert_eq!(String::from_utf8(frame.to_vec()).unwrap(), ":\n\n");
}

#[tokio::test]
async fn sse_polling_reads_events_after_listener_wake_errors() {
    let locked = LockedDb::new().await;
    let (app, cookie) = build_app_with_event_stream(
        &locked,
        EventStream::new(locked.db.clone(), locked.database_url.clone())
            .with_poll_interval(Duration::from_millis(40))
            .with_wake_stream(stream::iter([Err(pixivarchive_db::DbError::InvalidValue(
                "forced wake failure".to_owned(),
            ))])),
    )
    .await;

    let mut body = app
        .oneshot(
            Request::get("/events")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .into_body()
        .into_data_stream();
    let event_id = append_event(&locked.db, EventPayload::JobQueued { revision: 1 }).await;
    let frame = next_frame(&mut body).await;

    assert!(frame.contains(&format!("id: {event_id}")));
    assert!(frame.contains("event: app_event"));
    assert!(frame.contains("job_queued"));
}

async fn build_app(locked: &LockedDb) -> (Router, String) {
    build_app_with_event_stream(
        locked,
        EventStream::new(locked.db.clone(), locked.database_url.clone())
            .with_poll_interval(Duration::from_millis(50)),
    )
    .await
}

async fn build_app_with_event_stream(locked: &LockedDb, events: EventStream) -> (Router, String) {
    let auth = AuthService::new(locked.db.clone(), AuthConfig::new_for_tests().unwrap());
    auth.synchronize_password("correct horse battery staple")
        .await
        .unwrap();
    let issued = auth
        .login(LoginRequest::new(
            "correct horse battery staple",
            "127.0.0.1",
        ))
        .await
        .unwrap();
    let app = event_router(EventApiState { events }).layer(AuthLayer::new(auth));
    (app, format!("pa_session={}", issued.session_token()))
}

async fn append_event(db: &Db, payload: EventPayload) -> i64 {
    let events = EventRepository::new(db.clone());
    let mut tx = db.begin().await.unwrap();
    let event = events
        .append_in_tx(&mut tx, EventResource::Job, Uuid::now_v7(), payload)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    event.id
}

async fn next_frame<S>(body: &mut S) -> String
where
    S: futures_util::Stream<Item = Result<axum::body::Bytes, axum::Error>> + Unpin,
{
    let frame = tokio::time::timeout(Duration::from_secs(2), body.next())
        .await
        .expect("SSE frame should be available")
        .unwrap()
        .unwrap();
    String::from_utf8(frame.to_vec()).unwrap()
}
