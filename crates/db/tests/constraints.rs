mod support;

use uuid::Uuid;

#[tokio::test]
async fn administrator_is_singleton_and_sessions_have_bounded_idle_expiry() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let first_admin = Uuid::now_v7();

    sqlx::query(
        "INSERT INTO administrator (id, username, password_phc) VALUES ($1, 'admin', 'phc')",
    )
    .bind(first_admin)
    .execute(db.pool())
    .await
    .unwrap();

    let second_admin = sqlx::query(
        "INSERT INTO administrator (id, username, password_phc) VALUES ($1, 'other', 'phc')",
    )
    .bind(Uuid::now_v7())
    .execute(db.pool())
    .await;
    assert!(second_admin.is_err());

    let invalid_session = sqlx::query(
        r#"
        INSERT INTO admin_session (
            id, administrator_id, token_digest, csrf_digest, idle_expires_at, absolute_expires_at
        )
        VALUES ($1, $2, repeat('a', 32)::bytea, repeat('b', 32)::bytea, now() + interval '2 hours', now() + interval '1 hour')
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(first_admin)
    .execute(db.pool())
    .await;
    assert!(invalid_session.is_err());
}

#[tokio::test]
async fn current_rule_work_and_media_revisions_must_belong_to_their_owner() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();

    let first_rule = Uuid::now_v7();
    let second_rule = Uuid::now_v7();
    let foreign_rule_version = Uuid::now_v7();
    sqlx::query("INSERT INTO download_rule (id, name, match_action, default_action) VALUES ($1, 'first', 'download', 'download'), ($2, 'second', 'download', 'ignore')")
        .bind(first_rule)
        .bind(second_rule)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO rule_version (id, rule_id, version, schema_version, definition) VALUES ($1, $2, 1, 1, '{}')",
    )
    .bind(foreign_rule_version)
    .bind(second_rule)
    .execute(db.pool())
    .await
    .unwrap();

    let rule_update = sqlx::query("UPDATE download_rule SET current_version_id = $2 WHERE id = $1")
        .bind(first_rule)
        .bind(foreign_rule_version)
        .execute(db.pool())
        .await;
    assert!(rule_update.is_err());

    let artist_id = Uuid::now_v7();
    let first_work = Uuid::now_v7();
    let second_work = Uuid::now_v7();
    let foreign_work_revision = Uuid::now_v7();
    sqlx::query("INSERT INTO artist (id, pixiv_artist_id, name) VALUES ($1, 810001, 'artist')")
        .bind(artist_id)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        r#"
        INSERT INTO work (id, pixiv_work_id, artist_id, collection_state, source_state)
        VALUES ($1, 910001, $3, 'metadata_only', 'present'),
               ($2, 910002, $3, 'metadata_only', 'present')
        "#,
    )
    .bind(first_work)
    .bind(second_work)
    .bind(artist_id)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO work_revision (id, work_id, title, work_kind, page_count, sanity_level)
        VALUES ($1, $2, 'foreign revision', 'illustration', 1, 'unknown')
        "#,
    )
    .bind(foreign_work_revision)
    .bind(second_work)
    .execute(db.pool())
    .await
    .unwrap();

    let work_update = sqlx::query("UPDATE work SET current_revision_id = $2 WHERE id = $1")
        .bind(first_work)
        .bind(foreign_work_revision)
        .execute(db.pool())
        .await;
    assert!(work_update.is_err());

    let first_page = Uuid::now_v7();
    let second_page = Uuid::now_v7();
    let foreign_media_revision = Uuid::now_v7();
    sqlx::query("INSERT INTO work_page (id, work_id, page_index) VALUES ($1, $3, 0), ($2, $3, 1)")
        .bind(first_page)
        .bind(second_page)
        .bind(second_work)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        r#"
        INSERT INTO media_revision (id, work_page_id, revision_number, media_kind, format, source_path, byte_size, sha256)
        VALUES ($1, $2, 1, 'source_image', 'jpg', 'pixiv/910002_p1.jpg', 128, repeat('1', 32)::bytea)
        "#,
    )
    .bind(foreign_media_revision)
    .bind(second_page)
    .execute(db.pool())
    .await
    .unwrap();

    let page_update =
        sqlx::query("UPDATE work_page SET current_media_revision_id = $2 WHERE id = $1")
            .bind(first_page)
            .bind(foreign_media_revision)
            .execute(db.pool())
            .await;
    assert!(page_update.is_err());
}

#[tokio::test]
async fn job_attempt_and_deletion_marker_constraints_match_runtime_expectations() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let job_id = Uuid::now_v7();

    sqlx::query(
        "INSERT INTO job (id, priority_class, kind, payload, state) VALUES ($1, 'immediate', 'import_work', '{}', 'queued')",
    )
    .bind(job_id)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO job_attempt (id, job_id, attempt_number, state) VALUES ($1, $2, 1, 'running')",
    )
    .bind(Uuid::now_v7())
    .bind(job_id)
    .execute(db.pool())
    .await
    .unwrap();
    let second_running = sqlx::query(
        "INSERT INTO job_attempt (id, job_id, attempt_number, state) VALUES ($1, $2, 2, 'running')",
    )
    .bind(Uuid::now_v7())
    .bind(job_id)
    .execute(db.pool())
    .await;
    assert!(second_running.is_err());

    let invalid_method = sqlx::query(
        "INSERT INTO deletion_marker (id, pixiv_work_id, deletion_method) VALUES ($1, 920001, 'admin_marker')",
    )
    .bind(Uuid::now_v7())
    .execute(db.pool())
    .await;
    assert!(invalid_method.is_err());
}

#[tokio::test]
async fn mutable_work_counts_live_on_work_not_revision_snapshots() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();

    let work_columns: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = 'work'
          AND column_name IN ('bookmark_count', 'view_count', 'like_count', 'comment_count')
        "#,
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    assert_eq!(work_columns.len(), 4);

    let revision_columns: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT column_name
        FROM information_schema.columns
        WHERE table_name = 'work_revision'
          AND column_name IN ('bookmarked_count', 'viewed_count')
        "#,
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    assert!(revision_columns.is_empty());
}

#[tokio::test]
async fn system_settings_have_uuid_identity_and_unique_keys() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let setting_id = Uuid::now_v7();

    sqlx::query(
        r#"
        INSERT INTO system_setting (id, key, value)
        VALUES ($1, 'download.max_parallel', '{"schema_version":1,"payload":4}')
        "#,
    )
    .bind(setting_id)
    .execute(db.pool())
    .await
    .unwrap();

    let duplicate_key = sqlx::query(
        r#"
        INSERT INTO system_setting (id, key, value)
        VALUES ($1, 'download.max_parallel', '{"schema_version":1,"payload":8}')
        "#,
    )
    .bind(Uuid::now_v7())
    .execute(db.pool())
    .await;
    assert!(duplicate_key.is_err());

    let primary_key_column: String = sqlx::query_scalar(
        r#"
        SELECT column_name
        FROM information_schema.key_column_usage
        WHERE table_name = 'system_setting'
          AND constraint_name = 'system_setting_pkey'
        "#,
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(primary_key_column, "id");
}
