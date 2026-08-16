mod support;

use pixivarchive_db::{
    AuthRepository,
    auth::{IssueSession, LoginAttempt, RateLimitKind, RateLimitReservation},
};
use sqlx::{Connection, PgConnection};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const RATE_LIMIT_BLOCKER: i64 = 7_090_200_030;

#[tokio::test]
async fn failed_login_and_successful_login_use_the_same_parent_first_lock_order() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let auth = AuthRepository::new(db.clone());
    let now = OffsetDateTime::now_utc();
    let admin = auth
        .create_administrator("admin", "phc", now)
        .await
        .unwrap();
    let failure_lease = auth
        .reserve_rate_limit(&reservations("failure"), now)
        .await
        .unwrap();
    let success_lease = auth
        .reserve_rate_limit(&reservations("success"), now)
        .await
        .unwrap();

    install_rate_limit_update_blocker(&db).await;
    let scenario_db = db.clone();
    let mut scenario = tokio::spawn(async move {
        let database_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must point at the isolated test database");
        let mut blocker = PgConnection::connect(&database_url).await.unwrap();
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(RATE_LIMIT_BLOCKER)
            .execute(&mut blocker)
            .await
            .unwrap();

        let failure_auth = auth.clone();
        let failure_admin = admin.clone();
        let mut failure = tokio::spawn(async move {
            failure_auth
                .record_rate_limit_failure(
                    failure_lease,
                    &[
                        RateLimitKind::Entry,
                        RateLimitKind::Password,
                        RateLimitKind::Shared,
                    ],
                    LoginAttempt {
                        administrator_id: Some(failure_admin.id),
                        account_bucket: "account:admin",
                        entry_bucket: "entry:203.0.113.1",
                        source_bucket: "source:login",
                        succeeded: false,
                        failure_reason: Some("password"),
                    },
                    now + Duration::seconds(1),
                )
                .await
        });

        wait_for_rate_limit_row_lock(&scenario_db, "account:admin").await;

        let administrator_id = admin.id;
        let success_auth = auth;
        let success_admin = admin;
        let mut success = tokio::spawn(async move {
            success_auth
                .finalize_successful_login(
                    IssueSession {
                        administrator_snapshot: &success_admin,
                        token_digest: &[1; 32],
                        csrf_digest: &[2; 32],
                        now: now + Duration::seconds(2),
                        idle_timeout: Duration::hours(12),
                        absolute_timeout: Duration::days(30),
                    },
                    None,
                    success_lease,
                    LoginAttempt {
                        administrator_id: Some(success_admin.id),
                        account_bucket: "account:admin",
                        entry_bucket: "entry:203.0.113.1",
                        source_bucket: "source:login",
                        succeeded: true,
                        failure_reason: None,
                    },
                )
                .await
        });

        wait_for_administrator_lock(&scenario_db, administrator_id).await;
        sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(RATE_LIMIT_BLOCKER)
            .execute(&mut blocker)
            .await
            .unwrap();

        let login_results = tokio::time::timeout(Duration::seconds(15).unsigned_abs(), async {
            tokio::join!(&mut failure, &mut success)
        })
        .await;
        if login_results.is_err() {
            failure.abort();
            success.abort();
            let _ = failure.await;
            let _ = success.await;
        }
        let (failure_result, success_result) =
            login_results.expect("login transactions should not deadlock");
        failure_result.unwrap().unwrap();
        success_result.unwrap().unwrap();
    });

    let scenario_result =
        tokio::time::timeout(Duration::seconds(30).unsigned_abs(), &mut scenario).await;
    if scenario_result.is_err() {
        scenario.abort();
        let _ = scenario.await;
    }
    remove_rate_limit_update_blocker(&db).await;
    assert_rate_limit_update_blocker_removed(&db).await;

    scenario_result
        .expect("lock-order scenario did not finish before cleanup")
        .expect("lock-order scenario panicked");

    let failed_attempts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM login_attempt WHERE succeeded = false")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(failed_attempts, 1);

    let successful_attempts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM login_attempt WHERE succeeded = true")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(successful_attempts, 1);

    let remaining_reservations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM login_rate_limit_reservation WHERE bucket_key = 'account:admin'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(remaining_reservations, 0);

    let failure_count: i32 =
        sqlx::query_scalar("SELECT failure_count FROM login_rate_limit WHERE bucket_key = $1")
            .bind("account:admin")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(failure_count, 0);
}

fn reservations(owner: &str) -> Vec<RateLimitReservation> {
    [
        (RateLimitKind::Shared, "shared:admin"),
        (RateLimitKind::Entry, "entry:203.0.113.1"),
        (RateLimitKind::Password, "account:admin"),
    ]
    .into_iter()
    .map(|(kind, bucket_key)| RateLimitReservation {
        id: Uuid::now_v7(),
        kind,
        bucket_key: bucket_key.to_owned(),
        threshold: 10,
        window: Duration::minutes(5),
        cooldown: Duration::minutes(1),
        lease: Duration::seconds(if owner == "failure" { 30 } else { 31 }),
    })
    .collect()
}

async fn install_rate_limit_update_blocker(db: &pixivarchive_db::Db) {
    remove_rate_limit_update_blocker(db).await;
    let function_sql = format!(
        r#"
        CREATE OR REPLACE FUNCTION pause_login_rate_limit_update()
        RETURNS trigger
        LANGUAGE plpgsql
        AS $$
        BEGIN
            PERFORM pg_advisory_xact_lock({RATE_LIMIT_BLOCKER});
            RETURN NEW;
        END;
        $$;
        "#
    );
    // The interpolated value is a compile-time i64 used by both sides of the lock.
    sqlx::query(sqlx::AssertSqlSafe(function_sql))
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        r#"
        CREATE TRIGGER pause_login_rate_limit_update
        BEFORE UPDATE ON login_rate_limit
        FOR EACH ROW
        EXECUTE FUNCTION pause_login_rate_limit_update();
        "#,
    )
    .execute(db.pool())
    .await
    .unwrap();
}

async fn remove_rate_limit_update_blocker(db: &pixivarchive_db::Db) {
    sqlx::query("DROP TRIGGER IF EXISTS pause_login_rate_limit_update ON login_rate_limit")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION IF EXISTS pause_login_rate_limit_update()")
        .execute(db.pool())
        .await
        .unwrap();
}

async fn assert_rate_limit_update_blocker_removed(db: &pixivarchive_db::Db) {
    let trigger_exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM pg_trigger
            WHERE tgname = 'pause_login_rate_limit_update'
              AND NOT tgisinternal
        )
        "#,
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert!(!trigger_exists);

    let function_exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM pg_proc
            WHERE proname = 'pause_login_rate_limit_update'
              AND pg_function_is_visible(oid)
        )
        "#,
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert!(!function_exists);
}

async fn wait_for_rate_limit_row_lock(db: &pixivarchive_db::Db, bucket_key: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let mut connection = db.pool().acquire().await.unwrap();
        match sqlx::query("SELECT 1 FROM login_rate_limit WHERE bucket_key = $1 FOR UPDATE NOWAIT")
            .bind(bucket_key)
            .execute(&mut *connection)
            .await
        {
            Ok(_) => {}
            Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("55P03") => {
                return;
            }
            Err(error) => panic!("unexpected rate-limit lock probe error: {error:?}"),
        }
        assert!(
            std::time::Instant::now() < deadline,
            "expected rate-limit row lock was not acquired in time"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

async fn wait_for_administrator_lock(db: &pixivarchive_db::Db, administrator_id: Uuid) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let mut connection = db.pool().acquire().await.unwrap();
        match sqlx::query("SELECT 1 FROM administrator WHERE id = $1 FOR UPDATE NOWAIT")
            .bind(administrator_id)
            .execute(&mut *connection)
            .await
        {
            Ok(_) => {}
            Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("55P03") => {
                return;
            }
            Err(error) => panic!("unexpected administrator lock probe error: {error:?}"),
        }
        assert!(
            std::time::Instant::now() < deadline,
            "expected administrator row lock was not acquired in time"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}
