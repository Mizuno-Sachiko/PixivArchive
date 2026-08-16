use axum::{
    Json, Router,
    body::Body,
    extract::{ConnectInfo, State},
    http::{Method, Request, StatusCode, header},
    routing::{get, post},
};
use pixivarchive_application::auth::{AuthConfig, AuthService, LoginRequest};
use pixivarchive_db::Db;
use pixivarchive_web::middleware::{
    auth::AuthLayer, csrf::CsrfLayer, csrf_cookie, no_store, origin::OriginLayer, session_cookie,
};
use serde::Deserialize;
use std::{net::SocketAddr, str::FromStr};
use tower::ServiceExt;

mod support {
    use super::*;
    use sqlx::{Connection, PgConnection};

    pub struct LockedDb {
        pub db: Db,
        _lock: PgConnection,
    }

    impl LockedDb {
        pub async fn new() -> Self {
            let database_url = std::env::var("DATABASE_URL")
                .expect("DATABASE_URL must point at the isolated test database");
            let mut lock = PgConnection::connect(&database_url).await.unwrap();
            sqlx::query("SELECT pg_advisory_lock(709020004)")
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
                    system_setting,
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
            Self { db, _lock: lock }
        }
    }
}

async fn build_app() -> (support::LockedDb, Router, AuthService, String, String) {
    let locked = support::LockedDb::new().await;
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
    let app = Router::new()
        .route(
            "/mutate",
            post(|| async { StatusCode::NO_CONTENT })
                .put(|| async { StatusCode::NO_CONTENT })
                .patch(|| async { StatusCode::NO_CONTENT })
                .delete(|| async { StatusCode::NO_CONTENT }),
        )
        .route("/read", get(|| async { StatusCode::NO_CONTENT }))
        .layer(AuthLayer::new(auth.clone()))
        .layer(CsrfLayer::new(auth.clone()))
        .layer(OriginLayer::new());
    (
        locked,
        app,
        auth,
        issued.session_token().to_owned(),
        issued.csrf_token().to_owned(),
    )
}

#[tokio::test]
async fn mutating_requests_require_origin_and_bound_csrf_evidence() {
    let (_locked, app, _auth, session, csrf) = build_app().await;
    let cookie = format!("pa_session={session}; pa_csrf={csrf}");
    let request = Request::builder()
        .method(Method::POST)
        .uri("/mutate")
        .header(header::HOST, "archive.example.test")
        .header(header::ORIGIN, "https://archive.example.test")
        .header(header::COOKIE, cookie)
        .header("X-CSRF-Token", &csrf)
        .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(request).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    let bad_request = Request::builder()
        .method(Method::POST)
        .uri("/mutate")
        .header(header::HOST, "archive.example.test")
        .header(header::ORIGIN, "https://evil.example.test")
        .header(
            header::COOKIE,
            format!("pa_session={session}; pa_csrf={csrf}"),
        )
        .header("X-CSRF-Token", &csrf)
        .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(bad_request).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let mismatch = Request::builder()
        .method(Method::POST)
        .uri("/mutate")
        .header(header::HOST, "archive.example.test")
        .header(header::ORIGIN, "https://archive.example.test")
        .header(
            header::COOKIE,
            format!("pa_session={session}; pa_csrf={csrf}x"),
        )
        .header("X-CSRF-Token", csrf)
        .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(mismatch).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn csrf_rejects_missing_header_and_cross_session_tokens() {
    let (_locked, app, auth, first_session, first_csrf) = build_app().await;
    let second = auth
        .login(LoginRequest::new(
            "correct horse battery staple",
            "127.0.0.2",
        ))
        .await
        .unwrap();
    let second_session = second.session_token().to_owned();
    let second_csrf = second.csrf_token().to_owned();
    let missing_header = Request::builder()
        .method(Method::POST)
        .uri("/mutate")
        .header(header::HOST, "archive.example.test")
        .header(header::ORIGIN, "https://archive.example.test")
        .header(
            header::COOKIE,
            format!("pa_session={first_session}; pa_csrf={first_csrf}"),
        )
        .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(missing_header).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let cross_session = Request::builder()
        .method(Method::POST)
        .uri("/mutate")
        .header(header::HOST, "archive.example.test")
        .header(header::ORIGIN, "https://archive.example.test")
        .header(
            header::COOKIE,
            format!("pa_session={first_session}; pa_csrf={second_csrf}"),
        )
        .header("X-CSRF-Token", second_csrf)
        .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(cross_session).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let mismatched_cookie = Request::builder()
        .method(Method::POST)
        .uri("/mutate")
        .header(header::HOST, "archive.example.test")
        .header(header::ORIGIN, "https://archive.example.test")
        .header(
            header::COOKIE,
            format!("pa_session={second_session}; pa_csrf={first_csrf}"),
        )
        .header("X-CSRF-Token", first_csrf)
        .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(mismatched_cookie).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn origin_allows_referer_fallback_and_rejects_null_and_suffix_lookalikes() {
    let (_locked, app, _auth, session, csrf) = build_app().await;
    let cookie = format!("pa_session={session}; pa_csrf={csrf}");
    let referer = Request::builder()
        .method(Method::POST)
        .uri("/mutate")
        .header(header::HOST, "archive.example.test")
        .header(header::REFERER, "https://archive.example.test/system")
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(referer).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );

    for origin in ["null", "https://archive.example.test.evil.test"] {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/mutate")
            .header(header::HOST, "archive.example.test")
            .header(header::ORIGIN, origin)
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
}

#[tokio::test]
async fn origin_header_takes_priority_over_referer_and_checks_origin_parts() {
    let (_locked, app, _auth, session, csrf) = build_app().await;
    let cookie = format!("pa_session={session}; pa_csrf={csrf}");

    let priority = Request::builder()
        .method(Method::POST)
        .uri("/mutate")
        .header(header::HOST, "archive.example.test")
        .header(header::ORIGIN, "https://evil.example.test")
        .header(header::REFERER, "https://archive.example.test/system")
        .header(header::COOKIE, &cookie)
        .header("X-CSRF-Token", &csrf)
        .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(priority).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    for origin in [
        "https://archive.example.test:444",
        "https://user@archive.example.test",
        "https://archive.example.test/path",
        "not a url",
    ] {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/mutate")
            .header(header::HOST, "archive.example.test")
            .header(header::ORIGIN, origin)
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
}

#[tokio::test]
async fn origin_checks_all_mutating_methods() {
    let (_locked, app, _auth, session, csrf) = build_app().await;
    let cookie = format!("pa_session={session}; pa_csrf={csrf}");

    for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
        let request = Request::builder()
            .method(method)
            .uri("/mutate")
            .header(header::HOST, "archive.example.test")
            .header(header::ORIGIN, "https://evil.example.test")
            .header(header::COOKIE, &cookie)
            .header("X-CSRF-Token", &csrf)
            .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
}

#[tokio::test]
async fn cookie_helpers_set_project_required_attributes() {
    let session = session_cookie("session-token", false);
    assert_eq!(
        session,
        "pa_session=session-token; HttpOnly; SameSite=Strict; Path=/"
    );
    assert!(!session.contains("Domain="));
    assert!(session.contains("HttpOnly"));

    let csrf = csrf_cookie("csrf-token", false);
    assert_eq!(csrf, "pa_csrf=csrf-token; SameSite=Strict; Path=/");
    assert!(!csrf.contains("Domain="));
    assert!(!csrf.contains("HttpOnly"));

    assert_eq!(
        session_cookie("session-token", true),
        "pa_session=session-token; Secure; HttpOnly; SameSite=Strict; Path=/"
    );
    assert_eq!(
        csrf_cookie("csrf-token", true),
        "pa_csrf=csrf-token; Secure; SameSite=Strict; Path=/"
    );
}

#[tokio::test]
async fn origin_uses_the_current_request_host_for_http_and_https() {
    let app = Router::new()
        .route("/mutate", post(|| async { StatusCode::NO_CONTENT }))
        .layer(OriginLayer::new());

    for origin in ["http://127.0.0.1:7088", "https://archive.example.test"] {
        let host = origin.split_once("://").unwrap().1;
        let request = Request::builder()
            .method(Method::POST)
            .uri("/mutate")
            .header(header::HOST, host)
            .header(header::ORIGIN, origin)
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::NO_CONTENT
        );
    }

    let mismatched_host = Request::builder()
        .method(Method::POST)
        .uri("/mutate")
        .header(header::HOST, "archive.example.test")
        .header(header::ORIGIN, "https://evil.example.test")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.oneshot(mismatched_host).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn no_store_helper_sets_cache_control() {
    let mut response = axum::http::Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap();
    no_store(&mut response);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
}

#[tokio::test]
async fn invalid_or_duplicated_session_cookie_is_rejected_and_cleared() {
    let (_locked, app, _auth, session, csrf) = build_app().await;
    let duplicate = Request::builder()
        .method(Method::POST)
        .uri("/mutate")
        .header(header::HOST, "archive.example.test")
        .header(header::ORIGIN, "https://archive.example.test")
        .header(
            header::COOKIE,
            format!("pa_session={session}; pa_session={session}x; pa_csrf={csrf}"),
        )
        .header("X-CSRF-Token", csrf)
        .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(duplicate).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );

    let invalid = Request::builder()
        .method(Method::POST)
        .uri("/mutate")
        .header(header::HOST, "archive.example.test")
        .header(header::ORIGIN, "https://archive.example.test")
        .header(
            header::COOKIE,
            "pa_session=invalid-session-token; pa_csrf=invalid-csrf-token",
        )
        .header("X-CSRF-Token", "invalid-csrf-token")
        .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(invalid).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let set_cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert!(set_cookies.iter().any(
        |cookie| cookie == "pa_session=; Secure; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"
    ));
    assert!(
        set_cookies
            .iter()
            .any(|cookie| cookie == "pa_csrf=; Secure; SameSite=Strict; Path=/; Max-Age=0")
    );
}

#[tokio::test]
async fn auth_layer_keeps_valid_cookies_when_session_store_fails() {
    let (locked, app, _auth, session, csrf) = build_app().await;
    locked.db.pool().close().await;
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/read")
                .header(
                    header::COOKIE,
                    format!("pa_session={session}; pa_csrf={csrf}"),
                )
                .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_service_failure_without_cookie_clear(response);
}

#[tokio::test]
async fn csrf_layer_keeps_valid_cookies_when_session_store_fails() {
    let (locked, app, _auth, session, csrf) = build_app().await;
    locked.db.pool().close().await;
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mutate")
                .header(header::HOST, "archive.example.test")
                .header(header::ORIGIN, "https://archive.example.test")
                .header(
                    header::COOKIE,
                    format!("pa_session={session}; pa_csrf={csrf}"),
                )
                .header("X-CSRF-Token", csrf)
                .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_service_failure_without_cookie_clear(response);
}

#[tokio::test]
async fn csrf_layer_keeps_valid_cookies_when_csrf_store_fails() {
    let (locked, _app, auth, session, csrf) = build_app().await;
    let context = auth.authenticate(&session).await.unwrap();
    locked.db.pool().close().await;
    let app = Router::new()
        .route("/mutate", post(|| async { StatusCode::NO_CONTENT }))
        .layer(CsrfLayer::new(auth));
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/mutate")
                .header(
                    header::COOKIE,
                    format!("pa_session={session}; pa_csrf={csrf}"),
                )
                .header("X-CSRF-Token", csrf)
                .extension(context)
                .extension(ConnectInfo(SocketAddr::from_str("127.0.0.1:4000").unwrap()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_service_failure_without_cookie_clear(response);
}

#[tokio::test]
async fn forwarded_headers_do_not_change_the_source_bucket() {
    let locked = support::LockedDb::new().await;
    let auth = AuthService::new(locked.db.clone(), AuthConfig::new_for_tests().unwrap());
    auth.synchronize_password("correct horse battery staple")
        .await
        .unwrap();

    #[derive(Deserialize)]
    struct LoginBody {
        password: String,
    }

    async fn login(
        State(auth): State<AuthService>,
        ConnectInfo(peer): ConnectInfo<SocketAddr>,
        Json(body): Json<LoginBody>,
    ) -> StatusCode {
        match auth
            .login(LoginRequest::new(body.password, peer.ip().to_string()))
            .await
        {
            Ok(_) => StatusCode::NO_CONTENT,
            Err(error) if error.is_rate_limited() => StatusCode::TOO_MANY_REQUESTS,
            Err(error) if error.is_invalid_credentials() => StatusCode::UNAUTHORIZED,
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    let app = Router::new()
        .route("/login", post(login))
        .with_state(auth)
        .layer(OriginLayer::new());

    for _ in 0..5 {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/login")
            .header(header::HOST, "archive.example.test")
            .header(header::ORIGIN, "https://archive.example.test")
            .header(header::CONTENT_TYPE, "application/json")
            .header("X-Forwarded-For", "203.0.113.10")
            .extension(ConnectInfo(
                SocketAddr::from_str("127.0.0.55:4000").unwrap(),
            ))
            .body(Body::from(r#"{"password":"wrong password"}"#))
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }
    let request = Request::builder()
        .method(Method::POST)
        .uri("/login")
        .header(header::HOST, "archive.example.test")
        .header(header::ORIGIN, "https://archive.example.test")
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Forwarded-For", "198.51.100.10")
        .extension(ConnectInfo(
            SocketAddr::from_str("127.0.0.55:4000").unwrap(),
        ))
        .body(Body::from(r#"{"password":"wrong password"}"#))
        .unwrap();
    assert_eq!(
        app.oneshot(request).await.unwrap().status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

fn assert_service_failure_without_cookie_clear(response: axum::http::Response<Body>) {
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    assert_eq!(
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .count(),
        0
    );
}
