#![allow(dead_code)]

use axum::{
    Router,
    body::Body,
    http::{Request, Response, header},
};
use futures_util::StreamExt;
use pixivarchive_application::{
    auth::{AuthConfig, AuthService},
    events::EventStream,
    system::SystemCapabilities,
};
use pixivarchive_db::Db;
use pixivarchive_web::{
    app,
    state::{AppState, WebConfig},
};
use serde_json::Value;
use sqlx::{Connection, PgConnection};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};
use uuid::Uuid;

pub const PUBLIC_ORIGIN: &str = "https://archive.example.test";

pub struct LockedDb {
    pub db: Db,
    pub database_url: String,
    _lock: PgConnection,
}

impl LockedDb {
    pub async fn new(lock_id: i64) -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must point at the isolated test database");
        let mut lock = PgConnection::connect(&database_url).await.unwrap();
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(lock_id)
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
                worker_heartbeat,
                app_event,
                system_setting,
                login_rate_limit_reservation,
                login_rate_limit,
                login_attempt,
                admin_session,
                administrator,
                bookmark_writeback_command,
                import_candidate,
                import_run,
                ranking_entry,
                subscription_run_unit,
                subscription_run,
                subscription_cursor,
                subscription,
                pixiv_account,
                job_attempt,
                job,
                derivative,
                media_revision,
                work_page,
                work_tag,
                tag,
                trash_entry,
                deletion_marker,
                work_revision_source,
                work_revision,
                work,
                series,
                artist,
                rule_draft,
                rule_version,
                download_rule
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

pub struct TestFiles {
    root: PathBuf,
    pub static_root: PathBuf,
    pub media_root: PathBuf,
    pub cache_root: PathBuf,
}

impl TestFiles {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!("pixivarchive-web-{}", Uuid::now_v7()));
        let static_root = root.join("static");
        let media_root = root.join("media");
        let cache_root = root.join("cache");
        std::fs::create_dir_all(static_root.join("_app/immutable")).unwrap();
        std::fs::create_dir_all(&media_root).unwrap();
        std::fs::create_dir_all(&cache_root).unwrap();
        std::fs::write(static_root.join("index.html"), b"index").unwrap();
        std::fs::write(static_root.join("200.html"), b"spa fallback").unwrap();
        std::fs::write(
            static_root.join("_app/immutable/app.01234567.js"),
            b"immutable asset",
        )
        .unwrap();
        Self {
            root,
            static_root,
            media_root,
            cache_root,
        }
    }

    pub fn write_media(&self, relative_path: &str, contents: &[u8]) {
        let path = self.media_root.join(relative_path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
}

impl Drop for TestFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub struct TestApp {
    pub locked: LockedDb,
    pub files: TestFiles,
    pub app: Router,
}

impl TestApp {
    pub async fn new(lock_id: i64) -> Self {
        Self::new_with_state(lock_id, |state| state).await
    }

    pub async fn new_with_state(
        lock_id: i64,
        configure: impl FnOnce(AppState) -> AppState,
    ) -> Self {
        let locked = LockedDb::new(lock_id).await;
        let files = TestFiles::new();
        let auth = AuthService::new(locked.db.clone(), AuthConfig::new_for_tests().unwrap());
        auth.synchronize_password("correct horse battery staple")
            .await
            .unwrap();
        let events = EventStream::new(locked.db.clone(), locked.database_url.clone());
        let state = configure(AppState::new(
            locked.db.clone(),
            auth,
            events,
            WebConfig {
                static_root: files.static_root.clone(),
                media_root: files.media_root.clone().into(),
                cache_root: files.cache_root.clone().into(),
                version: "0.1.0-test".to_owned(),
                git_commit: Some("test-commit".to_owned()),
                capabilities: SystemCapabilities {
                    webp_derivatives: true,
                    avif_derivatives: false,
                    reflink: true,
                },
            },
        ));
        let app = app(state);
        Self { locked, files, app }
    }

    pub fn mutating_request(
        &self,
        method: axum::http::Method,
        uri: &str,
        auth: &BrowserAuth,
        body: impl Into<Body>,
    ) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, "archive.example.test")
            .header(header::ORIGIN, PUBLIC_ORIGIN)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, &auth.cookie)
            .header("X-CSRF-Token", &auth.csrf)
            .extension(peer())
            .body(body.into())
            .unwrap()
    }
}

#[derive(Clone, Debug)]
pub struct BrowserAuth {
    pub cookie: String,
    pub csrf: String,
}

pub async fn login(app: &Router) -> BrowserAuth {
    use tower::ServiceExt;

    let response = app
        .clone()
        .oneshot(
            Request::post("/api/auth/login")
                .header(header::HOST, "archive.example.test")
                .header(header::ORIGIN, PUBLIC_ORIGIN)
                .header(header::CONTENT_TYPE, "application/json")
                .extension(peer())
                .body(Body::from(r#"{"password":"correct horse battery staple"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| {
            value
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    let csrf = cookies
        .iter()
        .find_map(|cookie| cookie.strip_prefix("pa_csrf="))
        .unwrap()
        .to_owned();
    BrowserAuth {
        cookie: cookies.join("; "),
        csrf,
    }
}

pub async fn response_json(response: Response<Body>) -> Value {
    let status = response.status();
    let bytes = response
        .into_body()
        .into_data_stream()
        .fold(Vec::new(), |mut bytes, chunk| async move {
            bytes.extend_from_slice(&chunk.unwrap());
            bytes
        })
        .await;
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("response {status} is not JSON: {error}; body={bytes:?}"))
}

pub fn authenticated_get(uri: &str, auth: &BrowserAuth) -> Request<Body> {
    Request::get(uri)
        .header(header::COOKIE, &auth.cookie)
        .extension(peer())
        .body(Body::empty())
        .unwrap()
}

pub fn peer() -> axum::extract::ConnectInfo<SocketAddr> {
    axum::extract::ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap())
}

pub fn relative_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/")
}
