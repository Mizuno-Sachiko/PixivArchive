mod support;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use futures_util::StreamExt;
use pixivarchive_db::WorkRepository;
use std::io::{Cursor, Read};
use support::{TestApp, authenticated_get, login, peer, relative_path, response_json};
use tower::ServiceExt;
use uuid::Uuid;
use zip::ZipArchive;

#[tokio::test]
async fn static_assets_use_long_cache_and_spa_routes_use_200_html() {
    let test = TestApp::new(709020022).await;

    let asset = test
        .app
        .clone()
        .oneshot(
            Request::get("/_app/immutable/app.01234567.js")
                .extension(peer())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(asset.status(), StatusCode::OK);
    assert_eq!(
        asset.headers()[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );

    let spa = test
        .app
        .clone()
        .oneshot(
            Request::get("/gallery/works/123")
                .extension(peer())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(spa.status(), StatusCode::OK);
    assert_eq!(body_text(spa.into_body()).await, "spa fallback");

    let missing_api = test
        .app
        .oneshot(
            Request::get("/api/not-a-route")
                .extension(peer())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing_api.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        missing_api.headers()[header::CONTENT_TYPE],
        "application/json"
    );
}

#[tokio::test]
async fn source_media_uses_opaque_id_authentication_and_byte_ranges() {
    let test = TestApp::new(709020022).await;
    let auth = login(&test.app).await;
    let work = WorkRepository::new(test.locked.db.clone())
        .create_metadata_only(45_001, 56_001, "media work")
        .await
        .unwrap();
    let page_id = Uuid::now_v7();
    let media_id = Uuid::now_v7();
    let source_path = "56001/45001/p0/source.png";
    test.files
        .write_media(source_path, b"0123456789abcdefghijklmnopqrstuvwxyz");
    sqlx::query(
        r#"
        INSERT INTO work_page (id, work_id, page_index, source_path)
        VALUES ($1, $2, 0, $3)
        "#,
    )
    .bind(page_id)
    .bind(work.id)
    .bind(source_path)
    .execute(test.locked.db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO media_revision (
            id, work_page_id, revision_number, media_kind, format,
            source_path, byte_size, sha256
        )
        VALUES ($1, $2, 1, 'source_image', 'png', $3, 36, $4)
        "#,
    )
    .bind(media_id)
    .bind(page_id)
    .bind(source_path)
    .bind(vec![7_u8; 32])
    .execute(test.locked.db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE work_page SET current_media_revision_id = $2 WHERE id = $1")
        .bind(page_id)
        .bind(media_id)
        .execute(test.locked.db.pool())
        .await
        .unwrap();

    let unauthenticated = test
        .app
        .clone()
        .oneshot(
            Request::get(format!("/api/media/{media_id}/source"))
                .extension(peer())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let mut request = authenticated_get(&format!("/api/media/{media_id}/source"), &auth);
    request
        .headers_mut()
        .insert(header::RANGE, "bytes=10-15".parse().unwrap());
    let partial = test.app.clone().oneshot(request).await.unwrap();
    assert_eq!(partial.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(partial.headers()[header::CONTENT_RANGE], "bytes 10-15/36");
    assert_eq!(body_text(partial.into_body()).await, "abcdef");

    let mut invalid_range = authenticated_get(&format!("/api/media/{media_id}/source"), &auth);
    invalid_range
        .headers_mut()
        .insert(header::RANGE, "bytes=99-100".parse().unwrap());
    let invalid_range = test.app.clone().oneshot(invalid_range).await.unwrap();
    assert_eq!(invalid_range.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(invalid_range.headers()[header::CONTENT_RANGE], "bytes */36");
    assert_eq!(
        response_json(invalid_range).await["code"],
        "range_not_satisfiable"
    );

    let download = test
        .app
        .clone()
        .oneshot(authenticated_get(
            &format!("/api/works/{}/download", work.id),
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(download.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(
        download.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"45001_p0.png\""
    );
    assert_eq!(
        body_text(download.into_body()).await,
        "0123456789abcdefghijklmnopqrstuvwxyz"
    );
    assert!(!test.files.cache_root.join("exports").exists());

    let mut original_range = authenticated_get(&format!("/api/works/{}/download", work.id), &auth);
    original_range
        .headers_mut()
        .insert(header::RANGE, "bytes=0-3".parse().unwrap());
    let original_range = test.app.clone().oneshot(original_range).await.unwrap();
    assert_eq!(original_range.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(original_range.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(
        original_range.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"45001_p0.png\""
    );
    assert_eq!(body_text(original_range.into_body()).await, "0123");

    let second_page_id = Uuid::now_v7();
    let second_media_id = Uuid::now_v7();
    let second_source_path = "56001/45001/p1/source.png";
    test.files.write_media(second_source_path, b"second");
    sqlx::query(
        r#"
        INSERT INTO work_page (id, work_id, page_index, source_path)
        VALUES ($1, $2, 1, $3)
        "#,
    )
    .bind(second_page_id)
    .bind(work.id)
    .bind(second_source_path)
    .execute(test.locked.db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO media_revision (
            id, work_page_id, revision_number, media_kind, format,
            source_path, byte_size, sha256
        )
        VALUES ($1, $2, 1, 'source_image', 'png', $3, 6, $4)
        "#,
    )
    .bind(second_media_id)
    .bind(second_page_id)
    .bind(second_source_path)
    .bind(vec![9_u8; 32])
    .execute(test.locked.db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE work_page SET current_media_revision_id = $2 WHERE id = $1")
        .bind(second_page_id)
        .bind(second_media_id)
        .execute(test.locked.db.pool())
        .await
        .unwrap();

    let archive_download = test
        .app
        .clone()
        .oneshot(authenticated_get(
            &format!("/api/works/{}/download", work.id),
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(archive_download.status(), StatusCode::OK);
    assert_eq!(
        archive_download.headers()[header::CONTENT_TYPE],
        "application/zip"
    );
    assert_eq!(
        archive_download.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"45001_all.zip\""
    );
    let mut archive =
        ZipArchive::new(Cursor::new(body_bytes(archive_download.into_body()).await)).unwrap();
    let mut first_source = String::new();
    archive
        .by_name("45001_p0.png")
        .unwrap()
        .read_to_string(&mut first_source)
        .unwrap();
    assert_eq!(first_source, "0123456789abcdefghijklmnopqrstuvwxyz");
    let mut second_source = String::new();
    archive
        .by_name("45001_p1.png")
        .unwrap()
        .read_to_string(&mut second_source)
        .unwrap();
    assert_eq!(second_source, "second");
    assert!(archive.by_name("metadata.json").is_ok());

    assert!(!test.files.media_root.join("exports").exists());
    assert!(test.files.cache_root.join("exports").is_dir());
    assert_eq!(
        directory_file_count(&test.files.cache_root.join("exports")),
        0
    );

    let traversal_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO media_revision (
            id, work_page_id, revision_number, media_kind, format,
            source_path, byte_size, sha256
        )
        VALUES ($1, $2, 2, 'source_image', 'png', '../outside.png', 1, $3)
        "#,
    )
    .bind(traversal_id)
    .bind(page_id)
    .bind(vec![8_u8; 32])
    .execute(test.locked.db.pool())
    .await
    .unwrap();
    let traversal = test
        .app
        .oneshot(authenticated_get(
            &format!("/api/media/{traversal_id}/source"),
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(traversal.status(), StatusCode::NOT_FOUND);

    assert_eq!(
        relative_path(
            &test.files.media_root.join(source_path),
            &test.files.media_root
        ),
        source_path
    );
}

async fn body_text(body: Body) -> String {
    String::from_utf8(body_bytes(body).await).unwrap()
}

async fn body_bytes(body: Body) -> Vec<u8> {
    body.into_data_stream()
        .fold(Vec::new(), |mut bytes, chunk| async move {
            bytes.extend_from_slice(&chunk.unwrap());
            bytes
        })
        .await
}

fn directory_file_count(directory: &std::path::Path) -> usize {
    std::fs::read_dir(directory)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_file()))
        .count()
}
