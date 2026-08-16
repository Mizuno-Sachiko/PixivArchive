mod support;

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use serde_json::{Value, json};
use support::{PUBLIC_ORIGIN, TestApp, authenticated_get, login, peer, response_json};
use tower::ServiceExt;

#[tokio::test]
async fn api_errors_have_one_traceable_json_shape() {
    let test = TestApp::new(709020020).await;

    let response = test
        .app
        .clone()
        .oneshot(
            Request::get("/api/system/status")
                .extension(peer())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_error_shape(response_json(response).await, "authentication_required");

    let response = test
        .app
        .clone()
        .oneshot(
            Request::post("/api/auth/login")
                .header(header::HOST, "archive.example.test")
                .header(header::ORIGIN, PUBLIC_ORIGIN)
                .header(header::CONTENT_TYPE, "application/json")
                .extension(peer())
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_error_shape(response_json(response).await, "invalid_json");

    let response = test
        .app
        .oneshot(
            Request::get("/api/does-not-exist")
                .extension(peer())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_error_shape(response_json(response).await, "not_found");
}

#[tokio::test]
async fn stale_rule_draft_revision_returns_conflict_shape() {
    let test = TestApp::new(709020020).await;
    let auth = login(&test.app).await;
    let created = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            "/api/rules",
            &auth,
            Body::from(r#"{"name":"main","default_action":"ignore"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let rule_id = response_json(created).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let definition = json!({
        "schema_version": 1,
        "id": rule_id,
        "name": "main",
        "enabled": true,
        "group_mode": "all",
        "groups": [{
            "mode": "all",
            "conditions": [{
                "field": "bookmark_count",
                "operator": "greater_than_or_equal",
                "value": { "type": "number", "value": 0 }
            }]
        }],
        "action": "ignore",
        "default_action": "ignore"
    });

    let draft = test
        .app
        .clone()
        .oneshot(authenticated_get(
            &format!("/api/rules/{rule_id}/draft"),
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(draft.status(), StatusCode::OK);
    let mut expected_revision = response_json(draft).await["revision"].clone();
    for _ in 0..2 {
        let response = test
            .app
            .clone()
            .oneshot(
                test.mutating_request(
                    Method::PUT,
                    &format!("/api/rules/{rule_id}/draft"),
                    &auth,
                    Body::from(
                        json!({
                            "expected_revision": expected_revision,
                            "base_version": null,
                            "definition": definition
                        })
                        .to_string(),
                    ),
                ),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        expected_revision = response_json(response).await["revision"].clone();
    }

    let stale = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::PUT,
                &format!("/api/rules/{rule_id}/draft"),
                &auth,
                Body::from(
                    json!({
                        "expected_revision": 1,
                        "base_version": null,
                        "definition": definition
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_error_shape(response_json(stale).await, "revision_conflict");
}

#[tokio::test]
async fn extractor_and_method_rejections_use_the_api_error_shape() {
    let test = TestApp::new(709020020).await;
    let auth = login(&test.app).await;

    let invalid_path = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/rules/not-a-uuid", &auth))
        .await
        .unwrap();
    assert_eq!(invalid_path.status(), StatusCode::BAD_REQUEST);
    assert_error_shape(response_json(invalid_path).await, "invalid_path");

    let invalid_query = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/tasks?limit=not-a-number", &auth))
        .await
        .unwrap();
    assert_eq!(invalid_query.status(), StatusCode::BAD_REQUEST);
    assert_error_shape(response_json(invalid_query).await, "invalid_query");

    let excessive_task_limit = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/tasks?limit=201", &auth))
        .await
        .unwrap();
    assert_eq!(
        excessive_task_limit.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_error_shape(response_json(excessive_task_limit).await, "invalid_request");

    let method_not_allowed = test
        .app
        .clone()
        .oneshot(test.mutating_request(Method::PATCH, "/api/auth/session", &auth, Body::empty()))
        .await
        .unwrap();
    assert_eq!(method_not_allowed.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_error_shape(
        response_json(method_not_allowed).await,
        "method_not_allowed",
    );
}

#[tokio::test]
async fn rule_resource_errors_keep_their_http_meaning_and_deletes_emit_events() {
    let test = TestApp::new(709020020).await;
    let auth = login(&test.app).await;

    let missing = test
        .app
        .clone()
        .oneshot(authenticated_get(
            &format!("/api/rules/{}", uuid::Uuid::now_v7()),
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_error_shape(response_json(missing).await, "not_found");

    let empty_name = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            "/api/rules",
            &auth,
            Body::from(r#"{"name":" ","default_action":"ignore"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(empty_name.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_error_shape(response_json(empty_name).await, "invalid_request");

    let created = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            "/api/rules",
            &auth,
            Body::from(r#"{"name":"unique rules","default_action":"ignore"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    let rule_id = created["id"].as_str().unwrap();
    let revision = created["revision"].as_i64().unwrap();

    let duplicate = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            "/api/rules",
            &auth,
            Body::from(r#"{"name":"unique rules","default_action":"metadata_only"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);
    assert_error_shape(response_json(duplicate).await, "resource_conflict");

    let rule_id = rule_id.parse::<uuid::Uuid>().unwrap();
    let event_count_before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app_event WHERE resource = 'rule' AND resource_id = $1",
    )
    .bind(rule_id)
    .fetch_one(test.locked.db.pool())
    .await
    .unwrap();
    let deleted = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::DELETE,
            &format!("/api/rules/{rule_id}?expected_revision={revision}"),
            &auth,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    let event_count_after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app_event WHERE resource = 'rule' AND resource_id = $1",
    )
    .bind(rule_id)
    .fetch_one(test.locked.db.pool())
    .await
    .unwrap();
    assert_eq!(event_count_after, event_count_before + 1);
}

fn assert_error_shape(body: Value, code: &str) {
    assert_eq!(body["code"], code);
    assert!(body["message"].is_string());
    assert!(body.get("details").is_some());
    let trace_id = body["trace_id"].as_str().unwrap();
    assert!(uuid::Uuid::parse_str(trace_id).is_ok());
}
