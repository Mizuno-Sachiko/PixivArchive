mod support;

use axum::{body::Body, http::StatusCode};
use pixivarchive_db::{WorkerHeartbeatRepository, WorkerHeartbeatUpdate};
use pixivarchive_web::openapi;
use serde_json::Value;
use std::collections::HashSet;
use support::{TestApp, authenticated_get, login, peer, response_json};
use tower::ServiceExt;
use utoipa::OpenApi;

#[tokio::test]
async fn system_settings_batch_is_atomic_over_http() {
    let test = TestApp::new(709020026).await;
    let auth = login(&test.app).await;
    let initial = serde_json::json!({
        "updates": [
            {
                "group": "pixiv",
                "expected_revision": null,
                "value": { "default_private_bookmark": false }
            },
            {
                "group": "retry",
                "expected_revision": null,
                "value": { "network_backoff_seconds": [60, 300] }
            }
        ]
    });
    let response = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            axum::http::Method::PUT,
            "/api/system/settings",
            &auth,
            Body::from(initial.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let saved = response_json(response).await;
    assert_eq!(saved["settings"].as_array().unwrap().len(), 2);

    let conflicting = serde_json::json!({
        "updates": [
            {
                "group": "pixiv",
                "expected_revision": 1,
                "value": { "default_private_bookmark": true }
            },
            {
                "group": "retry",
                "expected_revision": 0,
                "value": { "network_backoff_seconds": [120, 600] }
            }
        ]
    });
    let response = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            axum::http::Method::PUT,
            "/api/system/settings",
            &auth,
            Body::from(conflicting.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let response = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/system/settings", &auth))
        .await
        .unwrap();
    let settings = response_json(response).await;
    assert_eq!(
        settings["value"]["pixiv"]["default_private_bookmark"],
        false
    );
    assert_eq!(
        settings["value"]["retry"]["network_backoff_seconds"],
        serde_json::json!([60, 300])
    );
}

#[tokio::test]
async fn storage_media_root_requires_an_absolute_path_over_http() {
    let test = TestApp::new(709020026).await;
    let auth = login(&test.app).await;
    let update = |media_root: &str| {
        serde_json::json!({
            "expected_revision": null,
            "value": {
                "media_root": media_root,
                "warning_threshold_bytes": 200 * 1024 * 1024 * 1024_u64,
                "media_write_stop_threshold_bytes": 64 * 1024 * 1024 * 1024_u64,
                "trash_retention_days": 30
            }
        })
    };

    let relative = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            axum::http::Method::PUT,
            "/api/system/settings/storage",
            &auth,
            Body::from(update("relative/media").to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(relative.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let absolute = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            axum::http::Method::PUT,
            "/api/system/settings/storage",
            &auth,
            Body::from(update("/mnt/archive/pixiv").to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(absolute.status(), StatusCode::OK);

    let settings = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/system/settings", &auth))
        .await
        .unwrap();
    assert_eq!(
        response_json(settings).await["value"]["storage"]["media_root"],
        "/mnt/archive/pixiv"
    );
}

#[tokio::test]
async fn content_settings_keep_nsfw_decorations_and_thumbnail_masking_exclusive() {
    let test = TestApp::new(709020026).await;
    let auth = login(&test.app).await;

    let initial = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/system/settings", &auth))
        .await
        .unwrap();
    let initial = response_json(initial).await;
    assert_eq!(
        initial["value"]["content"],
        serde_json::json!({
            "overview_allow_nsfw": false,
            "mask_non_all_age_thumbnails": false
        })
    );

    let allowed = serde_json::json!({
        "expected_revision": null,
        "value": {
            "overview_allow_nsfw": true,
            "mask_non_all_age_thumbnails": false
        }
    });
    let response = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            axum::http::Method::PUT,
            "/api/system/settings/content",
            &auth,
            Body::from(allowed.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let conflicting = serde_json::json!({
        "expected_revision": 1,
        "value": {
            "overview_allow_nsfw": true,
            "mask_non_all_age_thumbnails": true
        }
    });
    let response = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            axum::http::Method::PUT,
            "/api/system/settings/content",
            &auth,
            Body::from(conflicting.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let current = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/system/settings", &auth))
        .await
        .unwrap();
    assert_eq!(
        response_json(current).await["value"]["content"],
        allowed["value"]
    );
}

#[tokio::test]
async fn overview_decorations_use_a_valid_local_calendar_date_for_read_and_shuffle() {
    let test = TestApp::new(709020026).await;
    let auth = login(&test.app).await;

    let invalid = test
        .app
        .clone()
        .oneshot(authenticated_get(
            "/api/gallery/overview-decorations?date=2026-02-30",
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let selected = test
        .app
        .clone()
        .oneshot(authenticated_get(
            "/api/gallery/overview-decorations?date=2026-08-12",
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(selected.status(), StatusCode::OK);
    assert_eq!(
        response_json(selected).await["items"],
        serde_json::json!([null, null, null])
    );

    let shuffled = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            axum::http::Method::POST,
            "/api/gallery/overview-decorations?date=2026-08-12",
            &auth,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(shuffled.status(), StatusCode::OK);
    assert_eq!(
        response_json(shuffled).await["items"],
        serde_json::json!([null, null, null])
    );
}

#[tokio::test]
async fn health_and_system_status_report_current_process_and_database() {
    let test = TestApp::new(709020026).await;

    let live = test
        .app
        .clone()
        .oneshot(
            axum::http::Request::get("/health/live")
                .extension(peer())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(live.status(), StatusCode::NO_CONTENT);

    let ready = test
        .app
        .clone()
        .oneshot(
            axum::http::Request::get("/health/ready")
                .extension(peer())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::OK);

    let now = time::OffsetDateTime::now_utc();
    WorkerHeartbeatRepository::new(test.locked.db.clone())
        .update(WorkerHeartbeatUpdate {
            worker_id: uuid::Uuid::now_v7(),
            version: "0.1.0-test".to_owned(),
            git_commit: Some("test-commit".to_owned()),
            started_at: now - time::Duration::minutes(1),
            seen_at: now,
        })
        .await
        .unwrap();
    let auth = login(&test.app).await;
    let status = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/system/status", &auth))
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let status = response_json(status).await;
    assert_eq!(status["version"], "0.1.0-test");
    assert_eq!(status["git_commit"], "test-commit");
    assert_eq!(status["database"]["status"], "healthy");
    assert_eq!(status["media"]["status"], "healthy");
    assert_eq!(status["worker"]["status"], "healthy");
    assert!(status["queue"].is_object());
    assert_eq!(
        status["storage"]["active_media_root"],
        test.files.media_root.to_string_lossy().as_ref()
    );
    assert!(status["storage"]["total_bytes"].as_u64().unwrap() > 0);
    assert!(status["capabilities"].is_object());
}

#[tokio::test]
async fn storage_usage_reports_media_directory_bytes() {
    let test = TestApp::new(709020026).await;
    test.files.write_media("usage/nested.bin", &[0_u8; 7]);
    let auth = login(&test.app).await;

    let response = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/system/storage-usage", &auth))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response_json(response).await["media_directory_bytes"]
            .as_u64()
            .unwrap()
            >= 7
    );
}

#[tokio::test]
async fn system_status_reports_worker_before_first_heartbeat() {
    let test = TestApp::new(709020026).await;
    let auth = login(&test.app).await;
    let response = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/system/status", &auth))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let status = response_json(response).await;
    assert_eq!(status["worker"]["status"], "unavailable");
    assert_eq!(status["worker"]["message"], "工作进程尚未连接");
}

#[tokio::test]
async fn system_status_identifies_unavailable_media_storage() {
    let test = TestApp::new(709020026).await;
    let auth = login(&test.app).await;
    tokio::fs::remove_dir_all(&test.files.media_root)
        .await
        .unwrap();

    let response = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/system/status", &auth))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response_json(response).await["code"],
        "media_storage_unavailable"
    );
}

#[tokio::test]
async fn system_status_translates_non_directory_media_root() {
    let test = TestApp::new(709020026).await;
    let auth = login(&test.app).await;
    tokio::fs::remove_dir_all(&test.files.media_root)
        .await
        .unwrap();
    tokio::fs::write(&test.files.media_root, []).await.unwrap();

    let response = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/system/status", &auth))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let status = response_json(response).await;
    assert_eq!(status["media"]["status"], "unavailable");
    assert_eq!(status["media"]["message"], "配置的媒体路径不是目录");
}

#[test]
fn openapi_is_31_and_contains_every_management_surface() {
    let document = openapi::ApiDoc::openapi();
    let value = serde_json::to_value(document).unwrap();
    assert_eq!(value["openapi"], "3.1.0");
    let paths = value["paths"].as_object().unwrap();
    for path in [
        "/api/auth/login",
        "/api/auth/session",
        "/api/rules",
        "/api/subscriptions",
        "/api/subscriptions/{subscription_id}/runs",
        "/api/subscriptions/{subscription_id}/cursors",
        "/api/imports",
        "/api/pixiv/account",
        "/api/pixiv/account/credential",
        "/api/pixiv/account/validate",
        "/api/pixiv/account/bookmark-writeback",
        "/api/bookmarks",
        "/api/tasks",
        "/api/tasks/{job_id}",
        "/api/gallery/search",
        "/api/gallery/count",
        "/api/gallery/contexts/selection",
        "/api/gallery/overview-decorations",
        "/api/gallery/trash",
        "/api/gallery/contexts/trash",
        "/api/gallery/artists/{pixiv_artist_id}",
        "/api/gallery/tags/{tag_name}",
        "/api/gallery/series/{pixiv_series_id}",
        "/api/works/by-pixiv-id/{pixiv_work_id}",
        "/api/media/{media_revision_id}/source",
        "/api/derivatives/{derivative_id}",
        "/api/works/{work_id}/download",
        "/api/works/{work_id}/revisions",
        "/api/works/{work_id}/trash",
        "/api/trash",
        "/api/trash/selection",
        "/api/system/status",
        "/api/system/storage-usage",
        "/api/system/settings",
        "/api/system/maintenance",
        "/api/following",
        "/api/following/authors/{pixiv_artist_id}/avatar",
        "/api/events",
        "/health/live",
        "/health/ready",
    ] {
        assert!(paths.contains_key(path), "OpenAPI is missing {path}");
    }
    for path in [
        "/api/auth/security",
        "/api/auth/password",
        "/api/auth/totp/begin",
        "/api/auth/totp/enable",
        "/api/auth/totp/disable",
        "/api/gallery/hidden/search",
        "/api/gallery/duplicates",
        "/api/media/{media_revision_id}/similar",
        "/api/works/{work_id}/visibility",
    ] {
        assert!(!paths.contains_key(path), "OpenAPI still exposes {path}");
    }
    assert!(value["components"]["schemas"]["ApiErrorBody"].is_object());
    assert_eq!(
        value["components"]["securitySchemes"]["session_cookie"]["name"],
        "pa_session"
    );
    assert_eq!(
        value["components"]["securitySchemes"]["session_cookie"]["in"],
        "cookie"
    );
    assert_eq!(
        value["security"][0]["session_cookie"],
        serde_json::json!([])
    );
    assert_eq!(
        value["paths"]["/api/auth/login"]["post"]["security"],
        serde_json::json!([{}])
    );
    assert_eq!(
        value["paths"]["/health/live"]["get"]["security"],
        serde_json::json!([{}])
    );
    assert_mutating_operations_require_csrf(paths);
    assert_unique_operation_ids(paths);
    assert_media_error_contract(paths);
    assert_gallery_contracts(&value);
    assert_management_contracts(&value);
    assert_no_filesystem_paths(&value);
}

fn assert_gallery_contracts(openapi: &Value) {
    assert_eq!(
        openapi["paths"]["/api/gallery/search"]["post"]["requestBody"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/GallerySearch"
    );
    assert_eq!(
        openapi["paths"]["/api/gallery/selection"]["post"]["requestBody"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/GallerySelectionProjectionBody"
    );
    assert_eq!(
        openapi["paths"]["/api/gallery/contexts/selection"]["post"]["requestBody"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/GalleryContextSelectionProjectionBody"
    );

    let schemas = &openapi["components"]["schemas"];
    assert!(
        schemas["GallerySearch"]["properties"]
            .get("include_trash")
            .is_none()
    );
    assert_required_fields(&schemas["GalleryTagDto"], &["translation"]);
    assert_required_fields(
        &schemas["GalleryWorkDto"],
        &[
            "description",
            "series_id",
            "series_title",
            "bookmark_id",
            "bookmark_count",
            "view_count",
            "like_count",
            "comment_count",
            "pixiv_published_at",
            "pixiv_updated_at",
            "cover_url",
            "cover_width",
            "cover_height",
            "media_kind",
        ],
    );
    assert_required_fields(&schemas["GallerySearchPageDto"], &["next_cursor"]);
    assert_eq!(
        schemas["GallerySelectionProjectionBody"]["properties"]["expression"]["$ref"],
        "#/components/schemas/GallerySelectionExpression"
    );
    assert_eq!(
        schemas["GallerySelectionProjectionBody"]["properties"]["visible_work_ids"]["items"]["format"],
        "uuid"
    );
    assert_required_fields(
        &schemas["GallerySelectionProjectionDto"],
        &["selected_count", "selected_visible_work_ids"],
    );
    assert_eq!(
        schemas["GalleryContextSelectionProjectionBody"]["properties"]["expression"]["$ref"],
        "#/components/schemas/GalleryContextSelectionExpression"
    );
    assert_required_fields(
        &schemas["GalleryContextSelectionProjectionDto"],
        &[
            "selected_context_count",
            "selected_work_count",
            "selected_visible_context_ids",
        ],
    );
    assert_eq!(
        schemas["MoveGalleryToTrashBody"]["properties"]["expression"]["$ref"],
        "#/components/schemas/GallerySelectionExpression"
    );
    assert!(
        schemas["MoveGalleryToTrashBody"]["properties"]
            .get("search")
            .is_none()
    );
    assert!(
        schemas["MoveGalleryToTrashBody"]["properties"]
            .get("excluded_work_ids")
            .is_none()
    );
    assert_eq!(
        schemas["MoveGalleryContextsToTrashBody"]["properties"]["expression"]["$ref"],
        "#/components/schemas/GalleryContextSelectionExpression"
    );
    assert_required_fields(
        &schemas["GalleryPageDto"],
        &["width", "height", "current_media"],
    );
    assert_required_fields(
        &schemas["GalleryWorkDetailDto"],
        &["ugoira", "trash_capabilities"],
    );
    assert_required_fields(
        &schemas["WorkRevisionSummaryDto"],
        &["description", "sources"],
    );
    assert_required_fields(
        &schemas["WorkRevisionSourceDto"],
        &["subscription_name", "pixiv_user_id"],
    );
    assert_schema_enum_values(
        &schemas["PixivWorkKind"],
        &["illustration", "manga", "ugoira"],
    );
    assert_schema_enum_values(
        &schemas["PixivAgeRating"],
        &["all_age", "r18", "r18g", "unknown"],
    );
    assert_schema_enum_values(
        &schemas["CollectionState"],
        &["collected", "metadata_only", "trash"],
    );
    assert_schema_enum_values(
        &schemas["WorkSourceState"],
        &["present", "missing", "deleted", "restricted"],
    );
    assert_schema_enum_values(
        &schemas["MediaKind"],
        &["source_image", "ugoira_zip", "derivative"],
    );
    assert_schema_enum_values(
        &schemas["MediaFormat"],
        &["jpg", "png", "gif", "zip", "webp", "avif"],
    );
    assert_schema_enum_values(&schemas["DerivativeFormat"], &["webp", "avif"]);
    assert_required_fields(
        &schemas["GalleryArtistDetailDto"],
        &[
            "account_name",
            "cover_url",
            "cover_width",
            "cover_height",
            "cover_age_rating",
        ],
    );
    assert_required_fields(&schemas["GalleryArtistPageDto"], &["next_cursor"]);
    assert_required_fields(
        &schemas["GalleryTagDetailDto"],
        &[
            "cover_url",
            "cover_width",
            "cover_height",
            "cover_age_rating",
        ],
    );
    assert_required_fields(&schemas["GalleryTagPageDto"], &["next_cursor"]);
    assert_required_fields(
        &schemas["GallerySeriesDetailDto"],
        &[
            "pixiv_artist_id",
            "cover_url",
            "cover_width",
            "cover_height",
            "cover_age_rating",
        ],
    );
    assert_required_fields(&schemas["GallerySeriesPageDto"], &["next_cursor"]);

    let overview = &openapi["paths"]["/api/gallery/overview-decorations"];
    for method in ["get", "post"] {
        assert!(overview[method].is_object());
        let parameters = overview[method]["parameters"].as_array().unwrap();
        assert!(parameters.iter().any(|parameter| {
            parameter["name"] == "date"
                && parameter["in"] == "query"
                && parameter["required"] == true
        }));
    }
}

fn assert_management_contracts(openapi: &Value) {
    let schemas = &openapi["components"]["schemas"];
    assert_eq!(
        schemas["SettingsDto"]["properties"]["value"]["$ref"],
        "#/components/schemas/EffectiveSettingsDto"
    );
    assert_eq!(
        schemas["QueueSettingsDto"]["properties"]["job_priorities"]["items"]["$ref"],
        "#/components/schemas/JobPriorityMappingDto"
    );
    assert_required_fields(&schemas["StorageSettingsDto"], &["media_root"]);
    assert!(schemas["ContentSettingsDto"].is_object());
    assert_required_fields(&schemas["StorageStatusDto"], &["active_media_root"]);
    assert_required_fields(&schemas["MediaUsageDto"], &["media_directory_bytes"]);
    assert_eq!(
        schemas["JobKindDto"]["enum"],
        serde_json::json!([
            "scheduled_collection",
            "ranking_collection",
            "following_collection",
            "bookmarks_collection",
            "import_artist",
            "import_work",
            "download_media",
            "generate_derivative",
            "purge_trash",
        ])
    );
    assert_eq!(
        schemas["SubscriptionDto"]["properties"]["schedule"]["$ref"],
        "#/components/schemas/SubscriptionScheduleDto"
    );
    assert_eq!(
        schemas["SubscriptionDto"]["properties"]["params"]["type"],
        "object"
    );
    assert!(schemas["SubscriptionDto"]["properties"]["params"]["additionalProperties"].is_object());
    assert_required_fields(&schemas["PixivAccountDto"], &["revision"]);
    assert_required_fields(&schemas["QueueImportBody"], &["strategy"]);
    assert_eq!(
        schemas["QueueImportBody"]["properties"]["strategy"]["$ref"],
        "#/components/schemas/ImportStrategyDto"
    );
    assert_required_fields(&schemas["ImportRunDto"], &["strategy"]);
    assert_eq!(
        schemas["ImportRunDto"]["properties"]["strategy"]["$ref"],
        "#/components/schemas/ImportStrategyDto"
    );
    assert!(
        schemas["ImportRunDto"]["properties"]
            .get("forced")
            .is_none()
    );
    assert!(
        schemas["QueueImportBody"]["properties"]
            .get("forced")
            .is_none()
    );
    assert!(
        schemas["QueueImportBody"]["properties"]
            .get("rule_document")
            .is_none()
    );
    assert_required_fields(&schemas["TaskDto"], &["error_class"]);
    assert_eq!(
        schemas["TaskDto"]["properties"]["priority"]["$ref"],
        "#/components/schemas/JobPriorityDto"
    );
    assert_required_fields(&schemas["TrashListDto"], &["next_cursor"]);
    assert_eq!(
        value_at_request_schema(openapi, "/api/trash/selection"),
        "#/components/schemas/TrashSelectionBody"
    );
    assert_required_fields(
        &schemas["TrashSelectionBody"],
        &["expression", "visible_work_ids"],
    );
    assert_eq!(
        schemas["TrashSelectionBody"]["properties"]["expression"]["$ref"],
        "#/components/schemas/TrashSelectionExpression"
    );
    assert_eq!(
        schemas["TrashSelectionBody"]["properties"]["visible_work_ids"]["items"]["format"],
        "uuid"
    );
    assert_required_fields(
        &schemas["TrashSelectionDto"],
        &[
            "selected_count",
            "blocked_count",
            "selected_visible_work_ids",
        ],
    );
    assert_required_fields(
        &schemas["TrashActionCapabilities"],
        &["can_restore", "can_reschedule", "blocked_reason"],
    );
    for schema_name in ["TrashSelectionCommandBody", "RescheduleTrashManyBody"] {
        assert_eq!(
            schemas[schema_name]["properties"]["expression"]["$ref"],
            "#/components/schemas/TrashSelectionExpression"
        );
    }
    assert!(schemas.get("TrashSelectionOperation").is_none());
    assert!(schemas.get("TrashWorkIdsBody").is_none());
}

fn value_at_request_schema<'a>(openapi: &'a Value, path: &str) -> &'a str {
    openapi["paths"][path]["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"]
        .as_str()
        .unwrap()
}

fn assert_required_fields(schema: &Value, fields: &[&str]) {
    let required = schema["required"].as_array().unwrap();
    for field in fields {
        assert!(
            required.iter().any(|value| value == field),
            "OpenAPI schema is missing required field {field}"
        );
    }
}

fn assert_schema_enum_values(schema: &Value, expected: &[&str]) {
    let values = schema["enum"]
        .as_array()
        .unwrap_or_else(|| panic!("schema is missing enum values: {schema}"))
        .iter()
        .map(|value| value.as_str().expect("enum value is a string"))
        .collect::<Vec<_>>();
    assert_eq!(values, expected);
}

fn assert_media_error_contract(paths: &serde_json::Map<String, Value>) {
    let responses = &paths["/api/media/{media_revision_id}/source"]["get"]["responses"];
    assert!(responses["416"]["content"]["application/json"]["schema"].is_object());
    assert_eq!(
        responses["416"]["headers"]["Content-Range"]["schema"]["type"],
        "string"
    );
    assert!(responses["503"]["content"]["application/json"]["schema"].is_object());
}

fn assert_mutating_operations_require_csrf(paths: &serde_json::Map<String, Value>) {
    for (path, path_item) in paths {
        for method in ["post", "put", "patch", "delete"] {
            let Some(operation) = path_item.get(method) else {
                continue;
            };
            if path == "/api/auth/login" {
                continue;
            }
            let has_csrf = operation["parameters"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|parameter| {
                    parameter["name"] == "X-CSRF-Token"
                        && parameter["in"] == "header"
                        && parameter["required"] == true
                });
            assert!(has_csrf, "{method} {path} is missing its CSRF header");
        }
    }
}

fn assert_unique_operation_ids(paths: &serde_json::Map<String, Value>) {
    let mut operation_ids = HashSet::new();
    for (path, path_item) in paths {
        for method in ["get", "post", "put", "patch", "delete"] {
            let Some(operation) = path_item.get(method) else {
                continue;
            };
            let operation_id = operation["operationId"]
                .as_str()
                .unwrap_or_else(|| panic!("{method} {path} has no operationId"));
            assert!(
                operation_ids.insert(operation_id),
                "duplicate operationId {operation_id}"
            );
        }
    }
}

fn assert_no_filesystem_paths(value: &Value) {
    let serialized = serde_json::to_string(value).unwrap();
    assert!(!serialized.contains("source_path"));
    assert!(!serialized.contains("relative_path"));
}
