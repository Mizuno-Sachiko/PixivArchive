use sqlx::PgPool;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

#[tokio::test]
async fn upgrading_from_v1_preserves_existing_revisions() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must point at the isolated test database");
    let pool = PgPool::connect(&database_url).await.unwrap();
    let mut upgrade = pool.acquire().await.unwrap();

    sqlx::query("CREATE SCHEMA migration_upgrade_fixture")
        .execute(&mut *upgrade)
        .await
        .unwrap();
    sqlx::query("SET search_path TO migration_upgrade_fixture, public")
        .execute(&mut *upgrade)
        .await
        .unwrap();
    MIGRATOR.run_to(1, &mut *upgrade).await.unwrap();

    let artist_id = uuid::Uuid::now_v7();
    let work_id = uuid::Uuid::now_v7();
    let revision_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO artist (id, pixiv_artist_id, name) VALUES ($1, 920001, '升级测试作者')",
    )
    .bind(artist_id)
    .execute(&mut *upgrade)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO work (id, pixiv_work_id, artist_id, collection_state, source_state)
        VALUES ($1, 920001, $2, 'metadata_only', 'present')
        "#,
    )
    .bind(work_id)
    .bind(artist_id)
    .execute(&mut *upgrade)
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO work_revision (
            id, work_id, title, work_kind, page_count, sanity_level
        )
        VALUES ($1, $2, '迁移前修订', 'illustration', 1, 'all_age')
        "#,
    )
    .bind(revision_id)
    .bind(work_id)
    .execute(&mut *upgrade)
    .await
    .unwrap();

    MIGRATOR.run(&mut *upgrade).await.unwrap();

    let migration_versions =
        sqlx::query_scalar::<_, i64>("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&mut *upgrade)
            .await
            .unwrap();
    let preserved_title: String =
        sqlx::query_scalar("SELECT title FROM work_revision WHERE id = $1")
            .bind(revision_id)
            .fetch_one(&mut *upgrade)
            .await
            .unwrap();
    let source_count: i64 = sqlx::query_scalar("SELECT count(*) FROM work_revision_source")
        .fetch_one(&mut *upgrade)
        .await
        .unwrap();
    assert_eq!(migration_versions, [1, 2]);
    assert_eq!(preserved_title, "迁移前修订");
    assert_eq!(source_count, 0);

    sqlx::query("SET search_path TO public")
        .execute(&mut *upgrade)
        .await
        .unwrap();
    sqlx::query("DROP SCHEMA migration_upgrade_fixture CASCADE")
        .execute(&mut *upgrade)
        .await
        .unwrap();
}

#[tokio::test]
async fn initial_migration_creates_the_complete_schema() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must point at the isolated test database");
    let pool = PgPool::connect(&database_url).await.unwrap();

    MIGRATOR.run(&pool).await.unwrap();

    let migration_versions =
        sqlx::query_scalar::<_, i64>("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(migration_versions, [1, 2]);

    let tables = sqlx::query_scalar::<_, String>(
        r#"
        SELECT table_name
        FROM information_schema.tables
        WHERE table_schema = 'public'
          AND table_type = 'BASE TABLE'
        ORDER BY table_name
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        tables,
        [
            "_sqlx_migrations",
            "admin_session",
            "administrator",
            "app_event",
            "artist",
            "bookmark_writeback_command",
            "deletion_marker",
            "derivative",
            "download_rule",
            "import_candidate",
            "import_run",
            "job",
            "job_attempt",
            "login_attempt",
            "login_rate_limit",
            "login_rate_limit_reservation",
            "media_artifact_intent",
            "media_revision",
            "overview_decoration_selection",
            "pixiv_account",
            "pixiv_bookmark_sync_state",
            "pixiv_following_author",
            "pixiv_following_author_exclusion",
            "pixiv_work_bookmark",
            "ranking_entry",
            "rule_draft",
            "rule_version",
            "series",
            "subscription",
            "subscription_cursor",
            "subscription_run",
            "subscription_run_unit",
            "system_setting",
            "tag",
            "trash_entry",
            "work",
            "work_page",
            "work_revision",
            "work_revision_source",
            "work_tag",
            "worker_heartbeat",
        ]
    );

    let reservation_delete_rule: String = sqlx::query_scalar(
        r#"
        SELECT rc.delete_rule
        FROM information_schema.referential_constraints rc
        JOIN information_schema.table_constraints tc
          ON rc.constraint_name = tc.constraint_name
         AND rc.constraint_schema = tc.constraint_schema
        WHERE tc.table_schema = 'public'
          AND tc.table_name = 'login_rate_limit_reservation'
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(reservation_delete_rule, "RESTRICT");

    let event_id_type: String = sqlx::query_scalar(
        r#"
        SELECT data_type
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'app_event'
          AND column_name = 'id'
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(event_id_type, "bigint");

    let immutable_rule_version_triggers: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM pg_trigger
        JOIN pg_class ON pg_class.oid = pg_trigger.tgrelid
        JOIN pg_namespace ON pg_namespace.oid = pg_class.relnamespace
        WHERE pg_namespace.nspname = 'public'
          AND pg_class.relname = 'rule_version'
          AND pg_trigger.tgname = 'rule_version_immutable'
          AND NOT pg_trigger.tgisinternal
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(immutable_rule_version_triggers, 1);

    let security_columns: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND (
            (table_name = 'administrator' AND column_name IN ('password_phc', 'password_changed_at', 'password_version'))
            OR (table_name = 'admin_session' AND column_name IN ('token_digest', 'csrf_digest', 'idle_expires_at', 'absolute_expires_at'))
          )
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(security_columns, 7);

    let series_identity_nullable: String = sqlx::query_scalar(
        r#"
        SELECT is_nullable
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'series'
          AND column_name = 'pixiv_series_id'
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(series_identity_nullable, "NO");

    let directory_indexes: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM pg_indexes
        WHERE schemaname = 'public'
          AND indexname = ANY($1)
        "#,
    )
    .bind([
        "work_collected_artist_directory_idx",
        "work_collected_series_directory_idx",
        "work_tag_tag_work_idx",
        "work_collected_recent_idx",
    ])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(directory_indexes, 4);

    let trash_indexes: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM pg_indexes
        WHERE schemaname = 'public'
          AND indexname = ANY($1)
        "#,
    )
    .bind(["trash_entry_list_idx", "trash_entry_state_list_idx"])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(trash_indexes, 2);

    let revision_source_indexes: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM pg_indexes
        WHERE schemaname = 'public'
          AND indexname = ANY($1)
        "#,
    )
    .bind([
        "work_revision_source_revision_idx",
        "work_revision_source_subscription_idx",
        "work_revision_source_run_idx",
    ])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revision_source_indexes, 3);

    let rule_catalog_order_columns: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'download_rule'
          AND column_name = 'sort_order'
          AND is_identity = 'YES'
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rule_catalog_order_columns, 1);

    let rule_catalog_order_indexes: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM pg_indexes
        WHERE schemaname = 'public'
          AND indexname = 'download_rule_sort_order_idx'
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rule_catalog_order_indexes, 1);

    let current_account_constraints: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM pg_index
        JOIN pg_class index_relation ON index_relation.oid = pg_index.indexrelid
        JOIN pg_class table_relation ON table_relation.oid = pg_index.indrelid
        JOIN pg_namespace ON pg_namespace.oid = table_relation.relnamespace
        WHERE pg_namespace.nspname = 'public'
          AND table_relation.relname = 'pixiv_account'
          AND index_relation.relname = 'pixiv_account_one_current_idx'
          AND pg_index.indisunique
          AND pg_index.indpred IS NOT NULL
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(current_account_constraints, 1);

    let subscription_columns = sqlx::query_as::<_, (String, String, String)>(
        r#"
        SELECT column_name, is_nullable, data_type
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'subscription'
          AND column_name IN ('pixiv_account_id', 'last_run_at', 'next_run_at')
        ORDER BY column_name
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        subscription_columns,
        [
            (
                "last_run_at".to_owned(),
                "YES".to_owned(),
                "timestamp with time zone".to_owned()
            ),
            (
                "next_run_at".to_owned(),
                "YES".to_owned(),
                "timestamp with time zone".to_owned()
            ),
            (
                "pixiv_account_id".to_owned(),
                "NO".to_owned(),
                "uuid".to_owned()
            ),
        ]
    );

    let removed_timezone_columns: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND column_name = 'timezone'
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(removed_timezone_columns, 0);

    let trash_primary_key_columns = sqlx::query_scalar::<_, String>(
        r#"
        SELECT kcu.column_name
        FROM information_schema.key_column_usage AS kcu
        JOIN information_schema.table_constraints AS tc
          USING (constraint_catalog, constraint_schema, constraint_name)
        WHERE kcu.table_schema = 'public'
          AND kcu.table_name = 'trash_entry'
          AND tc.constraint_type = 'PRIMARY KEY'
        ORDER BY kcu.ordinal_position
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(trash_primary_key_columns, ["work_id"]);

    let overview_position_constraints: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM information_schema.check_constraints AS checks
        JOIN information_schema.constraint_column_usage AS columns
          ON columns.constraint_catalog = checks.constraint_catalog
         AND columns.constraint_schema = checks.constraint_schema
         AND columns.constraint_name = checks.constraint_name
        WHERE columns.table_schema = 'public'
          AND columns.table_name = 'overview_decoration_selection'
          AND columns.column_name = 'position'
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(overview_position_constraints, 1);
}
