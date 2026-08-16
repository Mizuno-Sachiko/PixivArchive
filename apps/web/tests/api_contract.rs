mod support;

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Method, StatusCode},
};
use pixivarchive_application::rules::{
    RulePreviewError, RulePreviewPort, RulePreviewRequest, RulePreviewResult,
};
use pixivarchive_db::{PixivAccountRepository, WorkRepository};
use pixivarchive_domain::{
    rule::{EvaluationContext, RuleCandidate, RuleDefinitionV1},
    subscription::PixivAccountState,
};
use serde_json::json;
use std::sync::Arc;
use support::{TestApp, authenticated_get, login, response_json};
use time::OffsetDateTime;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn login_session_rule_and_gallery_resources_follow_the_contract() {
    let test = TestApp::new(709020021).await;
    let auth = login(&test.app).await;

    let session = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/auth/session", &auth))
        .await
        .unwrap();
    assert_eq!(session.status(), StatusCode::OK);
    let session = response_json(session).await;
    assert!(session["administrator_id"].is_string());
    assert!(session["expires_at"].as_str().unwrap().contains('T'));

    let created = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            "/api/rules",
            &auth,
            Body::from(r#"{"name":"download rules","default_action":"ignore"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    assert!(created["current_version"].is_null());
    let rule_id = created["id"].as_str().unwrap();

    let rule = test
        .app
        .clone()
        .oneshot(authenticated_get(&format!("/api/rules/{rule_id}"), &auth))
        .await
        .unwrap();
    assert_eq!(rule.status(), StatusCode::OK);
    assert_eq!(response_json(rule).await["name"], "download rules");

    let work = WorkRepository::new(test.locked.db.clone())
        .create_metadata_only(44_001, 55_001, "searchable work")
        .await
        .unwrap();
    let gallery = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::POST,
                "/api/gallery/search",
                &auth,
                Body::from(
                    json!({
                        "groups": [{
                            "mode": "all",
                            "filters": [{
                                "type": "text",
                                "field": "title",
                                "operator": "contains",
                                "value": "searchable"
                            }]
                        }]
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(gallery.status(), StatusCode::OK);
    let gallery = response_json(gallery).await;
    assert!(gallery["items"].as_array().unwrap().is_empty());

    let detail = test
        .app
        .clone()
        .oneshot(authenticated_get(&format!("/api/works/{}", work.id), &auth))
        .await
        .unwrap();
    assert_eq!(detail.status(), StatusCode::OK);
    let detail = response_json(detail).await;
    assert_eq!(detail["work"]["pixiv_work_id"], 44_001);
    assert!(detail["work"].get("cover_path").is_none());
    assert!(detail["trash_capabilities"].is_null());

    let pixiv_artist_id = detail["work"]["pixiv_artist_id"].as_i64().unwrap();
    let artist = test
        .app
        .clone()
        .oneshot(authenticated_get(
            &format!("/api/gallery/artists/{pixiv_artist_id}"),
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(artist.status(), StatusCode::OK);
    assert_eq!(response_json(artist).await["pixiv_artist_id"], 55_001);

    let invalid_directory_cursor = test
        .app
        .clone()
        .oneshot(authenticated_get(
            "/api/gallery/artists?cursor=not-base64",
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(
        invalid_directory_cursor.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        response_json(invalid_directory_cursor).await["code"],
        "invalid_request"
    );

    let revisions = test
        .app
        .clone()
        .oneshot(authenticated_get(
            &format!("/api/works/{}/revisions", work.id),
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(revisions.status(), StatusCode::OK);
    assert_eq!(response_json(revisions).await.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn gallery_search_cannot_expose_trash_but_pixiv_id_resolution_can() {
    let test = TestApp::new(709020021).await;
    let auth = login(&test.app).await;
    let work = WorkRepository::new(test.locked.db.clone())
        .create_metadata_only(44_002, 55_002, "trash detail target")
        .await
        .unwrap();
    pixivarchive_application::trash::TrashService::new(test.locked.db.clone())
        .move_to_trash(work.id, 30)
        .await
        .unwrap();

    let gallery = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::POST,
                "/api/gallery/search",
                &auth,
                Body::from(
                    json!({
                        "groups": [{
                            "mode": "all",
                            "filters": [{ "type": "pixiv_work_id", "value": 44_002 }]
                        }],
                        "include_trash": true
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(gallery.status(), StatusCode::OK);
    assert!(
        response_json(gallery).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let resolved = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/works/by-pixiv-id/44002", &auth))
        .await
        .unwrap();
    assert_eq!(resolved.status(), StatusCode::OK);
    assert_eq!(
        response_json(resolved).await["work_id"],
        work.id.to_string()
    );

    let missing = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/works/by-pixiv-id/44999", &auth))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rule_catalog_copy_and_order_follow_the_contract() {
    let test = TestApp::new(709020021).await;
    let auth = login(&test.app).await;
    let source = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            "/api/rules",
            &auth,
            Body::from(r#"{"name":"source","default_action":"ignore"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(source.status(), StatusCode::CREATED);
    let source = response_json(source).await;
    let source_id = source["id"].as_str().unwrap();

    let copied = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            &format!("/api/rules/{source_id}/copy"),
            &auth,
            Body::from(r#"{"name":"source copy"}"#),
        ))
        .await
        .unwrap();
    assert_eq!(copied.status(), StatusCode::CREATED);
    let copied = response_json(copied).await;
    let copied_id = copied["id"].as_str().unwrap();
    assert_ne!(copied_id, source_id);
    assert_eq!(copied["name"], "source copy");
    assert!(copied["sort_order"].as_i64().unwrap() > source["sort_order"].as_i64().unwrap());

    let reordered = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::PUT,
            "/api/rules/order",
            &auth,
            Body::from(json!({ "ordered_rule_ids": [copied_id, source_id] }).to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(reordered.status(), StatusCode::OK);
    let reordered = response_json(reordered).await;
    assert_eq!(reordered["items"][0]["id"], copied_id);
    assert_eq!(reordered["items"][0]["sort_order"], 1);
    assert_eq!(reordered["items"][1]["id"], source_id);
    assert_eq!(reordered["items"][1]["sort_order"], 2);
}

#[tokio::test]
async fn ranking_subscription_request_requires_account_id() {
    let test = TestApp::new(709020021).await;
    let auth = login(&test.app).await;

    let response = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::POST,
                "/api/subscriptions",
                &auth,
                Body::from(
                    json!({
                        "kind": "ranking",
                        "name": "missing account",
                        "interval_minutes": 60,
                        "lookback_pages": 1,
                        "next_run_at": null,
                        "rule_id": null,
                        "params": {
                            "modes": ["daily"],
                            "contents": ["all"]
                        }
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn subscription_import_task_and_trash_commands_are_persistent() {
    let test = TestApp::new(709020021).await;
    let auth = login(&test.app).await;
    let account_id = insert_pixiv_account(&test).await;

    let created = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::POST,
                "/api/subscriptions",
                &auth,
                Body::from(
                    json!({
                        "kind": "ranking",
                        "account_id": account_id,
                        "name": "daily and weekly",
                        "interval_minutes": 360,
                        "lookback_pages": 2,
                        "next_run_at": null,
                        "rule_id": null,
                        "params": {
                            "modes": ["daily", "weekly"],
                            "contents": ["all"]
                        }
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    let subscription_id = created["id"].as_str().unwrap();
    assert_eq!(created["revision"], 1);
    assert_eq!(created["account_pixiv_user_id"], 90_001);
    assert_eq!(created["account_state"], "normal");
    assert_eq!(
        created["account_avatar_url"],
        format!("/api/pixiv/accounts/{account_id}/avatar?revision=1")
    );

    let update = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::PUT,
                &format!("/api/subscriptions/{subscription_id}"),
                &auth,
                Body::from(
                    json!({
                        "expected_revision": 1,
                        "enabled": false,
                        "account_id": account_id,
                        "rule_id": null,
                        "name": "daily and weekly",
                        "interval_minutes": 720,
                        "lookback_pages": 3,
                        "next_run_at": null,
                        "params": {
                            "modes": ["daily", "weekly"],
                            "contents": ["all"]
                        }
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(update.status(), StatusCode::OK);
    assert_eq!(response_json(update).await["revision"], 2);

    let enabled = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::PUT,
            &format!("/api/subscriptions/{subscription_id}/enabled"),
            &auth,
            Body::from(r#"{"expected_revision":2,"enabled":true}"#),
        ))
        .await
        .unwrap();
    assert_eq!(enabled.status(), StatusCode::OK);
    let enabled = response_json(enabled).await;
    assert_eq!(enabled["enabled"], true);
    assert_eq!(enabled["revision"], 3);

    PixivAccountRepository::new(test.locked.db.clone())
        .set_state(account_id, PixivAccountState::CredentialInvalid, None)
        .await
        .unwrap();
    let blocked_run = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            &format!("/api/subscriptions/{subscription_id}/run"),
            &auth,
            Body::from(r#"{"backfill":false}"#),
        ))
        .await
        .unwrap();
    assert_eq!(blocked_run.status(), StatusCode::CONFLICT);
    let blocked_run = response_json(blocked_run).await;
    assert_eq!(blocked_run["code"], "pixiv_account_unavailable");
    assert_eq!(blocked_run["details"]["state"], "credential_invalid");

    PixivAccountRepository::new(test.locked.db.clone())
        .set_state(account_id, PixivAccountState::Restricted, None)
        .await
        .unwrap();

    let run = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            &format!("/api/subscriptions/{subscription_id}/run"),
            &auth,
            Body::from(r#"{"backfill":false}"#),
        ))
        .await
        .unwrap();
    assert_eq!(run.status(), StatusCode::ACCEPTED);
    let run = response_json(run).await;
    assert!(run["job_id"].is_string());

    let running = test
        .app
        .clone()
        .oneshot(authenticated_get(
            &format!("/api/subscriptions/{subscription_id}"),
            &auth,
        ))
        .await
        .unwrap();
    let running = response_json(running).await;
    assert_eq!(running["recent_state"], "running");
    assert_eq!(running["revision"], 4);

    let runs = test
        .app
        .clone()
        .oneshot(authenticated_get(
            &format!("/api/subscriptions/{subscription_id}/runs?limit=20"),
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(runs.status(), StatusCode::OK);
    let runs = response_json(runs).await;
    assert_eq!(runs["items"][0]["id"], run["run_id"]);
    assert_eq!(runs["items"][0]["state"], "queued");

    let stopped = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            &format!("/api/subscriptions/{subscription_id}/stop"),
            &auth,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(stopped.status(), StatusCode::OK);
    let stopped = response_json(stopped).await;
    assert_eq!(stopped["recent_state"], "paused");
    assert_eq!(stopped["pending_run"], false);

    let cursors = test
        .app
        .clone()
        .oneshot(authenticated_get(
            &format!("/api/subscriptions/{subscription_id}/cursors"),
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(cursors.status(), StatusCode::OK);
    assert!(
        response_json(cursors).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let queued_import = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::POST,
                "/api/imports",
                &auth,
                Body::from(
                    json!({
                        "account_id": account_id,
                        "kind": "work",
                        "target_pixiv_id": 44_002,
                        "strategy": { "mode": "forced" }
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(queued_import.status(), StatusCode::ACCEPTED);
    let queued_import = response_json(queued_import).await;
    assert!(queued_import["id"].is_string());
    assert_eq!(queued_import["account_id"], account_id.to_string());
    assert_eq!(queued_import["kind"], "work");
    assert_eq!(queued_import["target_pixiv_id"], 44_002);
    assert_eq!(queued_import["strategy"]["mode"], "forced");
    assert!(queued_import.get("forced").is_none());
    assert_eq!(queued_import["status"], "queued");
    assert_eq!(queued_import["discovered_count"], 0);
    assert_eq!(queued_import["saved_count"], 0);
    assert!(queued_import["created_at"].is_string());

    let imports = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/imports?limit=20", &auth))
        .await
        .unwrap();
    assert_eq!(imports.status(), StatusCode::OK);
    let imports = response_json(imports).await;
    assert_eq!(imports["items"][0], queued_import);

    let tasks = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/tasks?limit=20", &auth))
        .await
        .unwrap();
    assert_eq!(tasks.status(), StatusCode::OK);
    let tasks = response_json(tasks).await;
    assert!(tasks["items"].as_array().unwrap().len() >= 2);
    assert!(tasks["items"][0]["updated_at"].is_string());
    assert!(tasks["summary"]["total"].as_u64().unwrap() >= 2);
    assert!(tasks["summary"]["running"].is_u64());
    assert!(tasks["summary"]["waiting"].is_u64());
    assert!(tasks["summary"]["requires_attention"].is_u64());

    let task = test
        .app
        .clone()
        .oneshot(authenticated_get(
            &format!("/api/tasks/{}", queued_import["job_id"].as_str().unwrap()),
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(task.status(), StatusCode::OK);
    let task = response_json(task).await;
    assert_eq!(task["task"]["id"], queued_import["job_id"]);
    assert!(task["attempts"].as_array().unwrap().is_empty());

    let account = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/pixiv/account", &auth))
        .await
        .unwrap();
    assert_eq!(account.status(), StatusCode::OK);
    let account = response_json(account).await;
    assert_eq!(account["account_id"], account_id.to_string());
    assert_eq!(account["state"], "restricted");
    assert_eq!(account["revision"], 3);

    let bookmark_writeback = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::PUT,
            "/api/pixiv/account/bookmark-writeback",
            &auth,
            Body::from(format!(
                r#"{{"expected_account_id":"{account_id}","enabled":true,"expected_revision":3}}"#
            )),
        ))
        .await
        .unwrap();
    assert_eq!(bookmark_writeback.status(), StatusCode::OK);
    let bookmark_writeback = response_json(bookmark_writeback).await;
    assert_eq!(bookmark_writeback["bookmark_writeback_enabled"], true);
    assert_eq!(bookmark_writeback["revision"], 4);

    let stale_bookmark_writeback = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::PUT,
            "/api/pixiv/account/bookmark-writeback",
            &auth,
            Body::from(format!(
                r#"{{"expected_account_id":"{account_id}","enabled":false,"expected_revision":3}}"#
            )),
        ))
        .await
        .unwrap();
    assert_eq!(stale_bookmark_writeback.status(), StatusCode::CONFLICT);

    let cleared_credential = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::DELETE,
            "/api/pixiv/account/credential",
            &auth,
            Body::from(format!(
                r#"{{"expected_account_id":"{account_id}","expected_revision":4}}"#
            )),
        ))
        .await
        .unwrap();
    assert_eq!(cleared_credential.status(), StatusCode::OK);
    let cleared_credential = response_json(cleared_credential).await;
    assert_eq!(cleared_credential["account_id"], account_id.to_string());
    assert_eq!(cleared_credential["pixiv_user_id"], 90_001);
    assert_eq!(cleared_credential["display_name"], "test");
    assert_eq!(cleared_credential["state"], "unconfigured");
    assert_eq!(cleared_credential["revision"], 5);
    let credential_is_null: bool = sqlx::query_scalar(
        r#"
        SELECT cookie_key_id IS NULL
           AND cookie_nonce IS NULL
           AND cookie_ciphertext IS NULL
        FROM pixiv_account
        WHERE id = $1
        "#,
    )
    .bind(account_id)
    .fetch_one(test.locked.db.pool())
    .await
    .unwrap();
    assert!(credential_is_null);

    let work = WorkRepository::new(test.locked.db.clone())
        .create_metadata_only(44_003, 55_003, "trash target")
        .await
        .unwrap();
    let trashed = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            &format!("/api/works/{}/trash", work.id),
            &auth,
            Body::from(r#"{"retention_days":30}"#),
        ))
        .await
        .unwrap();
    assert_eq!(trashed.status(), StatusCode::OK);
    assert_eq!(response_json(trashed).await["work_id"], work.id.to_string());

    let trash = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/trash", &auth))
        .await
        .unwrap();
    assert_eq!(trash.status(), StatusCode::OK);
    let trash = response_json(trash).await;
    assert_eq!(trash["items"][0]["work_id"], work.id.to_string());
    assert_eq!(trash["items"][0]["pixiv_work_id"], 44_003);
    assert!(trash["items"][0]["estimated_release_bytes"].is_number());
    assert!(trash["items"][0]["trashed_at"].is_string());
    assert_eq!(trash["items"][0]["capabilities"]["can_restore"], true);
    assert_eq!(trash["items"][0]["capabilities"]["can_reschedule"], true);
    assert!(trash["items"][0]["capabilities"]["blocked_reason"].is_null());

    let projected = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::POST,
                "/api/trash/selection",
                &auth,
                Body::from(
                    json!({
                        "expression": {
                            "filter": {
                                "query": "trash target",
                                "purge_states": []
                            },
                            "base_selected": true,
                            "exception_work_ids": []
                        },
                        "visible_work_ids": [work.id]
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(projected.status(), StatusCode::OK);
    assert_eq!(
        response_json(projected).await["selected_visible_work_ids"],
        json!([work.id])
    );

    let excluded = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::POST,
                "/api/trash/selection",
                &auth,
                Body::from(
                    json!({
                        "expression": {
                            "filter": { "query": null, "purge_states": [] },
                            "base_selected": true,
                            "exception_work_ids": [work.id]
                        },
                        "visible_work_ids": [work.id]
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(excluded.status(), StatusCode::OK);
    let excluded = response_json(excluded).await;
    assert_eq!(excluded["selected_count"], 0);
    assert_eq!(excluded["blocked_count"], 0);
    assert_eq!(excluded["selected_visible_work_ids"], json!([]));

    let restored = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            &format!("/api/trash/{}/restore", work.id),
            &auth,
            Body::from("{}"),
        ))
        .await
        .unwrap();
    assert_eq!(restored.status(), StatusCode::NO_CONTENT);

    let blocked_work = WorkRepository::new(test.locked.db.clone())
        .create_metadata_only(44_004, 55_003, "queued trash target")
        .await
        .unwrap();
    let trashed = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            &format!("/api/works/{}/trash", blocked_work.id),
            &auth,
            Body::from(r#"{"retention_days":30}"#),
        ))
        .await
        .unwrap();
    assert_eq!(trashed.status(), StatusCode::OK);
    let purge = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            &format!("/api/trash/{}/purge", blocked_work.id),
            &auth,
            Body::from("{}"),
        ))
        .await
        .unwrap();
    assert_eq!(purge.status(), StatusCode::ACCEPTED);

    let blocked = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::POST,
                "/api/trash/restore",
                &auth,
                Body::from(
                    json!({
                        "expression": {
                            "filter": { "query": null, "purge_states": [] },
                            "base_selected": false,
                            "exception_work_ids": [blocked_work.id]
                        }
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::CONFLICT);
    let blocked = response_json(blocked).await;
    assert_eq!(blocked["code"], "trash_selection_blocked");
    assert_eq!(blocked["details"]["selected_count"], 1);
    assert_eq!(blocked["details"]["blocked_count"], 1);
    let collection_state: String =
        sqlx::query_scalar("SELECT collection_state FROM work WHERE id = $1")
            .bind(blocked_work.id)
            .fetch_one(test.locked.db.pool())
            .await
            .unwrap();
    assert_eq!(collection_state, "trash");
}

#[tokio::test]
async fn bookmark_writeback_is_explicitly_disabled_without_a_runtime_adapter() {
    let test = TestApp::new(709020021).await;
    let auth = login(&test.app).await;
    let account_id = insert_pixiv_account(&test).await;
    let response = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::POST,
                "/api/bookmarks",
                &auth,
                Body::from(
                    json!({
                        "account_id": account_id,
                        "work_id": 44_004,
                        "visibility": "private",
                        "tags": []
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await["status"], "disabled");
}

#[tokio::test]
async fn pixiv_account_update_accepts_cookie_only_contract() {
    let test = TestApp::new(709020021).await;
    let auth = login(&test.app).await;
    let response = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::PUT,
            "/api/pixiv/account",
            &auth,
            Body::from(r#"{"cookie":"PHPSESSID=10001_test-session"}"#),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn pixiv_account_validation_reuses_the_saved_cookie() {
    let test = TestApp::new(709020021).await;
    let auth = login(&test.app).await;
    let response = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            "/api/pixiv/account/validate",
            &auth,
            Body::from(r#"{"expected_account_id":"0198f651-0000-7000-8000-000000000001"}"#),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn rule_preview_returns_a_server_evaluation_trace_for_a_pixiv_work_id() {
    let test = TestApp::new_with_state(709020021, |state| {
        state.with_rule_preview(Arc::new(FakeRulePreview))
    })
    .await;
    let auth = login(&test.app).await;
    let created = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            "/api/rules",
            &auth,
            Body::from(r#"{"name":"preview rules","default_action":"ignore"}"#),
        ))
        .await
        .unwrap();
    let rule_id = response_json(created).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let preview = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::POST,
                &format!("/api/rules/{rule_id}/preview"),
                &auth,
                Body::from(
                    json!({
                        "definition": {
                            "schema_version": 1,
                            "id": rule_id,
                            "name": "popular",
                            "enabled": true,
                            "group_mode": "all",
                            "groups": [{
                                "mode": "all",
                                "conditions": [{
                                    "field": "bookmark_count",
                                    "operator": "greater_than_or_equal",
                                    "value": { "type": "number", "value": 500 }
                                }]
                            }],
                            "action": "download",
                            "default_action": "ignore"
                        },
                        "pixiv_work_id": 120001
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();

    assert_eq!(preview.status(), StatusCode::OK);
    let preview = response_json(preview).await;
    assert_eq!(preview["item"]["pixiv_work_id"], 120001);
    assert_eq!(preview["item"]["decision"], "download");
    assert_eq!(preview["item"]["matched_rule_id"], rule_id.to_string());
    assert_eq!(preview["item"]["trace"]["rules"][0]["state"], "matched");
}

struct FakeRulePreview;

#[async_trait]
impl RulePreviewPort for FakeRulePreview {
    async fn preview(
        &self,
        request: RulePreviewRequest,
    ) -> Result<RulePreviewResult, RulePreviewError> {
        let now = OffsetDateTime::now_utc();
        let decision =
            RuleDefinitionV1::parse(request.definition)?.evaluate(&EvaluationContext {
                now,
                candidate: RuleCandidate {
                    pixiv_work_id: request.pixiv_work_id,
                    content_type: "illustration".to_owned(),
                    title: Some("高收藏插画".to_owned()),
                    description: None,
                    artist_id: Some(22001),
                    artist_name: Some("示例作者".to_owned()),
                    published_at: Some(now),
                    updated_at: Some(now),
                    tags: vec![],
                    page_count: 1,
                    age_rating: Some("all_age".to_owned()),
                    ai_generated: Some(false),
                    original_work: Some(true),
                    bookmarked_by_current_account: Some(false),
                    bookmark_count: Some(3200),
                    view_count: Some(10000),
                    like_count: Some(1200),
                    comment_count: Some(10),
                    bookmark_rate: Some(0.32),
                    bookmarks_per_day: Some(50.0),
                    ranking_rank: None,
                    ranking_date: None,
                    series_id: None,
                    series_title: None,
                    series_order: None,
                    pages: vec![],
                },
            })?;
        Ok(RulePreviewResult {
            pixiv_work_id: request.pixiv_work_id,
            title: "高收藏插画".to_owned(),
            artist_name: "示例作者".to_owned(),
            content_type: "illustration".to_owned(),
            decision,
        })
    }
}

async fn insert_pixiv_account(test: &TestApp) -> Uuid {
    let account_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO pixiv_account (
            id, pixiv_user_id, display_name, avatar_url, state, cookie_key_id,
            cookie_nonce, cookie_ciphertext, user_agent, is_current
        )
        VALUES ($1, 90001, 'test', 'https://i.pximg.net/test-avatar.jpg', 'normal', 'test', $2, $3, 'test-agent', true)
        "#,
    )
    .bind(account_id)
    .bind(vec![1_u8; 12])
    .bind(vec![2_u8; 32])
    .execute(test.locked.db.pool())
    .await
    .unwrap();
    account_id
}
