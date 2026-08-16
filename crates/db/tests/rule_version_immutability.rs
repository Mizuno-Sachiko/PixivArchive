use serde_json::json;
use uuid::Uuid;

mod support;

#[tokio::test]
async fn published_rule_versions_reject_update_and_delete() {
    let locked = support::LockedDb::new().await;
    let rule_id = Uuid::now_v7();
    let version_id = Uuid::now_v7();
    let original = json!({
        "schema_version": 1,
        "rules": [],
        "default_action": "ignore"
    });

    sqlx::query("INSERT INTO download_rule (id, name, match_action, default_action) VALUES ($1, 'main', 'download', 'ignore')")
        .bind(rule_id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    sqlx::query(
        r#"
        INSERT INTO rule_version (id, rule_id, version, schema_version, definition)
        VALUES ($1, $2, 1, 1, $3)
        "#,
    )
    .bind(version_id)
    .bind(rule_id)
    .bind(&original)
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE download_rule SET current_version_id = $2 WHERE id = $1")
        .bind(rule_id)
        .bind(version_id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    let update_error = sqlx::query("UPDATE rule_version SET definition = $2 WHERE id = $1")
        .bind(version_id)
        .bind(json!({ "schema_version": 1, "rules": [], "default_action": "download" }))
        .execute(locked.db.pool())
        .await
        .unwrap_err();
    assert_immutable_error(update_error);

    let delete_error = sqlx::query("DELETE FROM rule_version WHERE id = $1")
        .bind(version_id)
        .execute(locked.db.pool())
        .await
        .unwrap_err();
    assert_immutable_error(delete_error);

    let stored: serde_json::Value =
        sqlx::query_scalar("SELECT definition FROM rule_version WHERE id = $1")
            .bind(version_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(stored, original);
}

#[tokio::test]
async fn deleting_a_rule_removes_versions_and_preserves_run_snapshots() {
    let locked = support::LockedDb::new().await;
    let rule_id = Uuid::now_v7();
    let version_id = Uuid::now_v7();
    let account_id = Uuid::now_v7();
    let subscription_id = Uuid::now_v7();
    let run_id = Uuid::now_v7();
    let snapshot = json!({
        "schema_version": 1,
        "rules": [],
        "default_action": "ignore"
    });

    sqlx::query(
        "INSERT INTO download_rule (id, name, match_action, default_action) VALUES ($1, 'published', 'download', 'ignore')",
    )
    .bind(rule_id)
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO rule_version (id, rule_id, version, schema_version, definition)
        VALUES ($1, $2, 1, 1, $3)
        "#,
    )
    .bind(version_id)
    .bind(rule_id)
    .bind(&snapshot)
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE download_rule SET current_version_id = $2 WHERE id = $1")
        .bind(rule_id)
        .bind(version_id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    sqlx::query(
        r#"
        INSERT INTO pixiv_account (id, pixiv_user_id, display_name, state)
        VALUES ($1, 90001, 'archive owner', 'unconfigured')
        "#,
    )
    .bind(account_id)
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO subscription (id, pixiv_account_id, name, kind, schedule, params, rule_id)
        VALUES ($1, $2, 'ranking', 'ranking', '{}'::jsonb, '{}'::jsonb, $3)
        "#,
    )
    .bind(subscription_id)
    .bind(account_id)
    .bind(rule_id)
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO subscription_run (
            id,
            subscription_id,
            trigger_kind,
            state,
            rule_version_id,
            rule_document
        )
        VALUES ($1, $2, 'manual', 'succeeded', $3, $4)
        "#,
    )
    .bind(run_id)
    .bind(subscription_id)
    .bind(version_id)
    .bind(&snapshot)
    .execute(locked.db.pool())
    .await
    .unwrap();

    sqlx::query("DELETE FROM download_rule WHERE id = $1")
        .bind(rule_id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    let stored_version: Option<Uuid> =
        sqlx::query_scalar("SELECT rule_version_id FROM subscription_run WHERE id = $1")
            .bind(run_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    let stored_snapshot: serde_json::Value =
        sqlx::query_scalar("SELECT rule_document FROM subscription_run WHERE id = $1")
            .bind(run_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    let subscription_rule: Option<Uuid> =
        sqlx::query_scalar("SELECT rule_id FROM subscription WHERE id = $1")
            .bind(subscription_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    let version_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM rule_version WHERE id = $1)")
            .bind(version_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();

    assert_eq!(stored_version, None);
    assert_eq!(stored_snapshot, snapshot);
    assert_eq!(subscription_rule, None);
    assert!(!version_exists);
}

fn assert_immutable_error(error: sqlx::Error) {
    let sqlx::Error::Database(error) = error else {
        panic!("expected a database error, got {error}");
    };
    assert_eq!(error.code().as_deref(), Some("55000"));
    assert_eq!(error.message(), "published rule versions are immutable");
}
