use pixivarchive_db::Db;
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
        sqlx::query("SELECT pg_advisory_lock(709020002)")
            .execute(&mut lock)
            .await
            .unwrap();

        let db = Db::connect(&database_url).await.unwrap();
        sqlx::migrate!("../../migrations")
            .run(db.pool())
            .await
            .unwrap();
        reset_shared_tables(&db).await;
        Self { db, _lock: lock }
    }
}

async fn reset_shared_tables(db: &Db) {
    sqlx::query(
        r#"
        TRUNCATE TABLE
            worker_heartbeat,
            app_event,
            rule_draft,
            rule_version,
            download_rule,
            system_setting,
            bookmark_writeback_command,
            import_candidate,
            import_run,
            subscription_cursor,
            ranking_entry,
            subscription_run_unit,
            subscription_run,
            subscription,
            pixiv_following_author_exclusion,
            pixiv_following_author,
            pixiv_account,
            job,
            job_attempt,
            deletion_marker,
            work_tag,
            derivative,
            media_revision,
            work_page,
            work,
            work_revision_source,
            work_revision,
            tag,
            series,
            artist,
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
}
