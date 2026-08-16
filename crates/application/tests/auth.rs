use pixivarchive_application::auth::{
    AuthConfig, AuthError, AuthService, LoginRequest, PasswordSync, StaticClock,
};
use pixivarchive_application::settings::{
    DeploymentCapabilities, DerivativeFormat, DerivativeSettings, FailureLimit, JobPriorityMapping,
    PixivSettings, ProcessingSettings, QueueQuotaWeights, QueueSettings, RateLimit,
    SecuritySettings, SettingUpdate, SettingValue, SettingsError, SettingsService, StorageSettings,
    UgoiraSettings,
};
use pixivarchive_db::Db;
use pixivarchive_domain::job::{JobKind, JobPriority};
use pixivarchive_domain::settings::SettingGroupKey;
use pixivarchive_test_support::LockedDb;
use std::{
    collections::BTreeMap,
    num::{NonZeroU16, NonZeroU32},
};
use time::{Duration, OffsetDateTime};

const AUTH_DB_LOCK_ID: i64 = 709020003;

async fn auth_db() -> LockedDb {
    let locked = LockedDb::new(AUTH_DB_LOCK_ID).await;
    clear_login_attempt_failure_hook(&locked.db).await;
    locked
}

fn auth_service(db: Db) -> AuthService {
    AuthService::new(db, AuthConfig::new_for_tests().unwrap())
}

async fn rate_limit_counts(db: &Db) -> BTreeMap<String, i32> {
    sqlx::query_as::<_, (String, i32)>(
        "SELECT bucket_key, failure_count FROM login_rate_limit ORDER BY bucket_key",
    )
    .fetch_all(db.pool())
    .await
    .unwrap()
    .into_iter()
    .collect()
}

async fn reservation_count(db: &Db) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM login_rate_limit_reservation")
        .fetch_one(db.pool())
        .await
        .unwrap()
}

async fn login_error(service: &AuthService, request: LoginRequest) -> AuthError {
    match service.login(request).await {
        Ok(_) => panic!("login unexpectedly issued a session"),
        Err(error) => error,
    }
}

async fn clear_login_attempt_failure_hook(db: &Db) {
    sqlx::query("DROP TRIGGER IF EXISTS fail_login_attempt_insert ON login_attempt")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION IF EXISTS fail_login_attempt_insert()")
        .execute(db.pool())
        .await
        .unwrap();
}

async fn install_login_attempt_failure_hook(db: &Db) {
    clear_login_attempt_failure_hook(db).await;
    sqlx::query(
        r#"
        CREATE FUNCTION fail_login_attempt_insert()
        RETURNS trigger
        LANGUAGE plpgsql
        AS $$
        BEGIN
            RAISE EXCEPTION 'blocked by auth test';
        END;
        $$;
        "#,
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        CREATE TRIGGER fail_login_attempt_insert
            BEFORE INSERT ON login_attempt
            FOR EACH ROW
            EXECUTE FUNCTION fail_login_attempt_insert()
        "#,
    )
    .execute(db.pool())
    .await
    .unwrap();
}

fn weak_argon2id_phc(password: &str) -> String {
    use argon2::{Algorithm, Argon2, Params, PasswordHasher, Version, password_hash::SaltString};
    let salt = SaltString::encode_b64(&[3u8; 16]).unwrap();
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(8, 1, 1, Some(32)).unwrap(),
    )
    .hash_password(password.as_bytes(), &salt)
    .unwrap()
    .to_string()
}

fn argon2i_phc(password: &str) -> String {
    use argon2::{Algorithm, Argon2, Params, PasswordHasher, Version, password_hash::SaltString};
    let salt = SaltString::encode_b64(&[4u8; 16]).unwrap();
    Argon2::new(
        Algorithm::Argon2i,
        Version::V0x13,
        Params::new(19_456, 2, 1, Some(32)).unwrap(),
    )
    .hash_password(password.as_bytes(), &salt)
    .unwrap()
    .to_string()
}

#[tokio::test]
async fn environment_password_sync_is_idempotent_and_revokes_only_after_change() {
    let locked = auth_db().await;
    let service = auth_service(locked.db.clone());

    assert_eq!(
        service
            .synchronize_password("correct horse battery staple")
            .await
            .unwrap(),
        PasswordSync::Created
    );
    let issued = service
        .login(LoginRequest::new(
            "correct horse battery staple",
            "127.0.0.60",
        ))
        .await
        .unwrap();
    let before: (i64, i64) = sqlx::query_as("SELECT password_version, revision FROM administrator")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();

    assert_eq!(
        service
            .synchronize_password("correct horse battery staple")
            .await
            .unwrap(),
        PasswordSync::Unchanged
    );
    assert_eq!(
        sqlx::query_as::<_, (i64, i64)>("SELECT password_version, revision FROM administrator")
            .fetch_one(locked.db.pool())
            .await
            .unwrap(),
        before
    );
    service.authenticate(issued.session_token()).await.unwrap();

    assert_eq!(
        service
            .synchronize_password("new environment password")
            .await
            .unwrap(),
        PasswordSync::Updated
    );
    assert!(service.authenticate(issued.session_token()).await.is_err());
    assert!(
        service
            .login(LoginRequest::new(
                "correct horse battery staple",
                "127.0.0.61",
            ))
            .await
            .is_err()
    );
    service
        .login(LoginRequest::new("new environment password", "127.0.0.62"))
        .await
        .unwrap();
}

#[tokio::test]
async fn environment_password_sync_replaces_an_unsupported_hash_and_revokes_sessions() {
    let locked = auth_db().await;
    let service = auth_service(locked.db.clone());
    service
        .synchronize_password("correct horse battery staple")
        .await
        .unwrap();
    let issued = service
        .login(LoginRequest::new(
            "correct horse battery staple",
            "127.0.0.63",
        ))
        .await
        .unwrap();
    sqlx::query("UPDATE administrator SET password_phc = $1")
        .bind(argon2i_phc("correct horse battery staple"))
        .execute(locked.db.pool())
        .await
        .unwrap();

    assert_eq!(
        service
            .synchronize_password("correct horse battery staple")
            .await
            .unwrap(),
        PasswordSync::Updated
    );
    assert!(service.authenticate(issued.session_token()).await.is_err());
    service
        .login(LoginRequest::new(
            "correct horse battery staple",
            "127.0.0.64",
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn login_stores_only_session_and_csrf_digests() {
    let locked = auth_db().await;
    let service = auth_service(locked.db.clone());
    service
        .synchronize_password("correct horse battery staple")
        .await
        .unwrap();

    let issued = service
        .login(LoginRequest::new(
            "correct horse battery staple",
            "127.0.0.1",
        ))
        .await
        .unwrap();

    assert_eq!(issued.session_token().len(), 43);
    assert_eq!(issued.csrf_token().len(), 43);
    let stored_session_token: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT token_digest FROM admin_session WHERE id = $1")
            .bind(issued.context().session_id)
            .fetch_optional(locked.db.pool())
            .await
            .unwrap();
    let stored_csrf_token: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT csrf_digest FROM admin_session WHERE id = $1")
            .bind(issued.context().session_id)
            .fetch_optional(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(stored_session_token.unwrap().len(), 32);
    assert_eq!(stored_csrf_token.unwrap().len(), 32);
}

#[tokio::test]
async fn authenticate_respects_idle_absolute_and_refresh_throttle() {
    let locked = auth_db().await;
    let clock = StaticClock::new(OffsetDateTime::UNIX_EPOCH + Duration::hours(1));
    let service = AuthService::with_clock(
        locked.db.clone(),
        AuthConfig::new_for_tests().unwrap(),
        clock.clone(),
    );
    service
        .synchronize_password("correct horse battery staple")
        .await
        .unwrap();
    let issued = service
        .login(LoginRequest::new(
            "correct horse battery staple",
            "127.0.0.1",
        ))
        .await
        .unwrap();

    service.authenticate(issued.session_token()).await.unwrap();
    clock.advance(Duration::seconds(59));
    service.authenticate(issued.session_token()).await.unwrap();
    let first_seen: OffsetDateTime =
        sqlx::query_scalar("SELECT last_activity_at FROM admin_session WHERE id = $1")
            .bind(issued.context().session_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(first_seen, OffsetDateTime::UNIX_EPOCH + Duration::hours(1));

    clock.advance(Duration::seconds(1));
    service.authenticate(issued.session_token()).await.unwrap();
    let second_seen: OffsetDateTime =
        sqlx::query_scalar("SELECT last_activity_at FROM admin_session WHERE id = $1")
            .bind(issued.context().session_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(
        second_seen,
        OffsetDateTime::UNIX_EPOCH + Duration::hours(1) + Duration::minutes(1)
    );

    clock.advance(Duration::days(180));
    let expired_error = service
        .authenticate(issued.session_token())
        .await
        .unwrap_err();
    assert!(matches!(expired_error, AuthError::InvalidSession));
    let after_absolute_expiry: OffsetDateTime =
        sqlx::query_scalar("SELECT last_activity_at FROM admin_session WHERE id = $1")
            .bind(issued.context().session_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(after_absolute_expiry, second_seen);
}

#[tokio::test]
async fn auth_reads_security_timeouts_from_stored_settings() {
    let locked = auth_db().await;
    let clock = StaticClock::new(OffsetDateTime::UNIX_EPOCH + Duration::hours(3));
    let settings = SettingsService::new(locked.db.clone());
    settings
        .update(
            SettingGroupKey::Security,
            None,
            SettingValue::Security(SecuritySettings {
                session_idle_timeout_seconds: 90,
                session_absolute_timeout_seconds: 300,
                last_activity_persist_interval_seconds: 10,
                password_failures: FailureLimit {
                    threshold: 8,
                    window_seconds: 900,
                    cooldown_seconds: 900,
                },
                shared_account_failures: FailureLimit {
                    threshold: 12,
                    window_seconds: 900,
                    cooldown_seconds: 900,
                },
                entry_source_failures: FailureLimit {
                    threshold: 20,
                    window_seconds: 600,
                    cooldown_seconds: 600,
                },
            }),
        )
        .await
        .unwrap();
    let service = AuthService::with_clock(
        locked.db.clone(),
        AuthConfig::new_for_tests().unwrap(),
        clock.clone(),
    );
    service
        .synchronize_password("correct horse battery staple")
        .await
        .unwrap();

    let issued = service
        .login(LoginRequest::new(
            "correct horse battery staple",
            "127.0.0.18",
        ))
        .await
        .unwrap();
    assert_eq!(
        issued.context().expires_at,
        OffsetDateTime::UNIX_EPOCH + Duration::hours(3) + Duration::seconds(90)
    );

    clock.advance(Duration::seconds(10));
    let refreshed = service.authenticate(issued.session_token()).await.unwrap();
    assert_eq!(
        refreshed.expires_at,
        OffsetDateTime::UNIX_EPOCH + Duration::hours(3) + Duration::seconds(100)
    );
}

#[tokio::test]
async fn rate_limit_counts_in_flight_attempts_before_argon2_work() {
    let locked = auth_db().await;
    let service = auth_service(locked.db.clone());
    service
        .synchronize_password("correct horse battery staple")
        .await
        .unwrap();

    let mut handles = Vec::new();
    for _ in 0..10 {
        let service = service.clone();
        handles.push(tokio::spawn(async move {
            service
                .login(LoginRequest::new("wrong password", "127.0.0.44"))
                .await
        }));
    }

    let mut throttled = 0;
    let mut credential_failures = 0;
    for handle in handles {
        match handle.await.unwrap() {
            Err(error) if error.is_rate_limited() => throttled += 1,
            Err(error) if error.is_invalid_credentials() => credential_failures += 1,
            Ok(_) => panic!("wrong password issued a session"),
            Err(_) => panic!("unexpected non-rate-limit login error"),
        }
    }
    assert_eq!(credential_failures, 5);
    assert_eq!(throttled, 5);
}

#[tokio::test]
async fn password_failure_counts_password_shared_entry_and_cleans_reservations() {
    let locked = auth_db().await;
    let service = auth_service(locked.db.clone());
    service
        .synchronize_password("correct horse battery staple")
        .await
        .unwrap();

    let error = login_error(&service, LoginRequest::new("wrong password", "127.0.0.45")).await;

    assert!(matches!(error, AuthError::InvalidCredentials));
    let counts = rate_limit_counts(&locked.db).await;
    assert_eq!(counts["entry:127.0.0.45"], 1);
    assert_eq!(counts["password:admin"], 1);
    assert_eq!(counts["shared:admin"], 1);
    assert_eq!(reservation_count(&locked.db).await, 0);
}

#[tokio::test]
async fn password_failure_releases_reservations_when_audit_insert_fails() {
    let locked = auth_db().await;
    let service = auth_service(locked.db.clone());
    service
        .synchronize_password("correct horse battery staple")
        .await
        .unwrap();
    install_login_attempt_failure_hook(&locked.db).await;

    let result = service
        .login(LoginRequest::new("wrong password", "127.0.0.46"))
        .await;
    let reservations = reservation_count(&locked.db).await;
    clear_login_attempt_failure_hook(&locked.db).await;

    let error = result
        .err()
        .expect("audit failure login unexpectedly issued a session");

    assert!(matches!(error, AuthError::Internal));
    assert_eq!(reservations, 0);
}

#[tokio::test]
async fn weak_argon2id_is_rehashed_without_incrementing_password_version() {
    let locked = auth_db().await;
    let service = auth_service(locked.db.clone());
    service
        .synchronize_password("correct horse battery staple")
        .await
        .unwrap();
    let weak = weak_argon2id_phc("correct horse battery staple");
    sqlx::query("UPDATE administrator SET password_phc = $1")
        .bind(&weak)
        .execute(locked.db.pool())
        .await
        .unwrap();
    let before_version: i64 = sqlx::query_scalar("SELECT password_version FROM administrator")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();

    service
        .login(LoginRequest::new(
            "correct horse battery staple",
            "127.0.0.21",
        ))
        .await
        .unwrap();

    let (after_version, after_phc): (i64, String) =
        sqlx::query_as("SELECT password_version, password_phc FROM administrator")
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(after_version, before_version);
    assert_ne!(after_phc, weak);
    assert!(after_phc.starts_with("$argon2id$v=19$"));
}

#[tokio::test]
async fn argon2i_phc_is_rejected_even_when_password_matches() {
    let locked = auth_db().await;
    let service = auth_service(locked.db.clone());
    service
        .synchronize_password("correct horse battery staple")
        .await
        .unwrap();
    sqlx::query("UPDATE administrator SET password_phc = $1")
        .bind(argon2i_phc("correct horse battery staple"))
        .execute(locked.db.pool())
        .await
        .unwrap();

    let error = match service
        .login(LoginRequest::new(
            "correct horse battery staple",
            "127.0.0.22",
        ))
        .await
    {
        Ok(_) => panic!("argon2i PHC issued a session"),
        Err(error) => error,
    };
    assert!(error.is_invalid_credentials());
}

#[tokio::test]
async fn finalize_login_revision_conflict_clears_reservations_and_does_not_issue_session() {
    use sqlx::Connection;

    let locked = auth_db().await;
    let service = auth_service(locked.db.clone());
    service
        .synchronize_password("correct horse battery staple")
        .await
        .unwrap();
    let administrator_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM administrator")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    let database_url = std::env::var("DATABASE_URL").unwrap();
    let mut row_lock = sqlx::PgConnection::connect(&database_url).await.unwrap();
    let mut tx = row_lock.begin().await.unwrap();
    sqlx::query("SELECT id FROM administrator WHERE id = $1 FOR UPDATE")
        .bind(administrator_id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();

    let service_for_task = service.clone();
    let mut login = tokio::spawn(async move {
        service_for_task
            .login(LoginRequest::new(
                "correct horse battery staple",
                "127.0.0.48",
            ))
            .await
    });
    for _ in 0..50 {
        if reservation_count(&locked.db).await == 3 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(reservation_count(&locked.db).await, 3);
    sqlx::query("UPDATE administrator SET revision = revision + 1 WHERE id = $1")
        .bind(administrator_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let login_result =
        match tokio::time::timeout(std::time::Duration::from_secs(10), &mut login).await {
            Ok(result) => result.expect("conflicting login task panicked"),
            Err(_) => {
                login.abort();
                let _ = login.await;
                panic!("conflicting login did not finish after the row lock was released");
            }
        };
    let error = match login_result {
        Ok(_) => panic!("conflicting login unexpectedly issued a session"),
        Err(error) => error,
    };
    assert!(matches!(error, AuthError::InvalidCredentials));
    assert_eq!(reservation_count(&locked.db).await, 0);
    let session_count: i64 = sqlx::query_scalar("SELECT count(*) FROM admin_session")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(session_count, 0);
}

#[tokio::test]
async fn successful_login_preserves_other_in_flight_reservations() {
    let locked = auth_db().await;
    let service = auth_service(locked.db.clone());
    service
        .synchronize_password("correct horse battery staple")
        .await
        .unwrap();
    sqlx::query(
        r#"
        INSERT INTO login_rate_limit (bucket_key, failure_count, window_started_at, updated_at)
        VALUES ('password:admin', 2, now(), now())
        ON CONFLICT (bucket_key) DO UPDATE SET failure_count = 2
        "#,
    )
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO login_rate_limit_reservation (id, bucket_key, leased_until, created_at)
        VALUES (gen_random_uuid(), 'password:admin', now() + interval '5 minutes', now())
        "#,
    )
    .execute(locked.db.pool())
    .await
    .unwrap();

    service
        .login(LoginRequest::new(
            "correct horse battery staple",
            "127.0.0.24",
        ))
        .await
        .unwrap();

    let failure_count: i32 = sqlx::query_scalar(
        "SELECT failure_count FROM login_rate_limit WHERE bucket_key = 'password:admin'",
    )
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    let reservations: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM login_rate_limit_reservation WHERE bucket_key = 'password:admin'",
    )
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(failure_count, 0);
    assert_eq!(reservations, 1);
    let bucket_count: i64 = sqlx::query_scalar("SELECT count(*) FROM login_rate_limit")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(bucket_count, 3);
}

#[tokio::test]
async fn typed_settings_defaults_validation_revision_and_event_are_transactional() {
    let locked = auth_db().await;
    let settings = SettingsService::new(locked.db.clone());
    let defaults = settings.effective().await.unwrap();
    assert_eq!(
        defaults.queue.quota_weights.immediate,
        NonZeroU16::new(4).unwrap()
    );
    assert_eq!(
        defaults.queue.quota_weights.manual_import,
        NonZeroU16::new(8).unwrap()
    );
    assert_eq!(
        defaults.queue.quota_weights.scheduled_collection,
        NonZeroU16::new(2).unwrap()
    );
    assert_eq!(
        defaults.queue.quota_weights.background_maintenance,
        NonZeroU16::new(1).unwrap()
    );
    assert!(
        defaults
            .queue
            .job_priorities
            .iter()
            .any(|mapping| mapping.job_kind == JobKind::ImportWork
                && mapping.priority == JobPriority::ManualImport)
    );
    assert_eq!(
        defaults
            .processing
            .as_ref()
            .unwrap()
            .pixiv_request_concurrency,
        NonZeroU16::new(2).unwrap()
    );
    assert_eq!(
        defaults.ugoira.as_ref().unwrap().max_frames,
        NonZeroU32::new(3_000).unwrap()
    );
    assert_eq!(
        defaults.storage.warning_threshold_bytes,
        100 * 1024 * 1024 * 1024
    );
    assert_eq!(
        defaults.storage.media_write_stop_threshold_bytes,
        32 * 1024 * 1024 * 1024
    );

    assert!(
        settings
            .validate(SettingValue::Storage(StorageSettings {
                media_root: None,
                warning_threshold_bytes: 32,
                media_write_stop_threshold_bytes: 100,
                trash_retention_days: 30,
            }))
            .is_err()
    );
    let saved = settings
        .update(
            SettingGroupKey::Storage,
            None,
            SettingValue::Storage(StorageSettings {
                media_root: Some("/srv/pixivarchive/media".to_owned()),
                warning_threshold_bytes: 200 * 1024 * 1024 * 1024,
                media_write_stop_threshold_bytes: 64 * 1024 * 1024 * 1024,
                trash_retention_days: 45,
            }),
        )
        .await
        .unwrap();
    assert_eq!(saved.revision, 1);
    assert!(
        settings
            .update(
                SettingGroupKey::Storage,
                Some(0),
                SettingValue::Storage(StorageSettings {
                    media_root: Some("/srv/pixivarchive/media".to_owned()),
                    warning_threshold_bytes: 300 * 1024 * 1024 * 1024,
                    media_write_stop_threshold_bytes: 64 * 1024 * 1024 * 1024,
                    trash_retention_days: 45,
                }),
            )
            .await
            .is_err()
    );
    let event_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app_event WHERE resource = 'system_setting'")
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(event_count, 1);
    let stored_value: serde_json::Value =
        sqlx::query_scalar("SELECT value FROM system_setting WHERE key = 'storage'")
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(stored_value["schema_version"], 1);
    assert!(stored_value.get("group").is_none());
    assert_eq!(
        stored_value["payload"]["trash_retention_days"],
        serde_json::json!(45)
    );
}

#[tokio::test]
async fn concurrent_settings_update_with_same_revision_has_one_winner() {
    let locked = auth_db().await;
    let settings = SettingsService::new(locked.db.clone());
    let saved = settings
        .update(
            SettingGroupKey::Storage,
            None,
            SettingValue::Storage(StorageSettings {
                media_root: None,
                warning_threshold_bytes: 200 * 1024 * 1024 * 1024,
                media_write_stop_threshold_bytes: 64 * 1024 * 1024 * 1024,
                trash_retention_days: 45,
            }),
        )
        .await
        .unwrap();

    let left = settings.clone();
    let right = settings.clone();
    let left = tokio::spawn(async move {
        left.update(
            SettingGroupKey::Storage,
            Some(saved.revision),
            SettingValue::Storage(StorageSettings {
                media_root: None,
                warning_threshold_bytes: 220 * 1024 * 1024 * 1024,
                media_write_stop_threshold_bytes: 64 * 1024 * 1024 * 1024,
                trash_retention_days: 50,
            }),
        )
        .await
    });
    let right = tokio::spawn(async move {
        right
            .update(
                SettingGroupKey::Storage,
                Some(saved.revision),
                SettingValue::Storage(StorageSettings {
                    media_root: None,
                    warning_threshold_bytes: 240 * 1024 * 1024 * 1024,
                    media_write_stop_threshold_bytes: 64 * 1024 * 1024 * 1024,
                    trash_retention_days: 55,
                }),
            )
            .await
    });
    let results = futures_util::future::join(left, right).await;
    let saved_count = [results.0.unwrap().is_ok(), results.1.unwrap().is_ok()]
        .into_iter()
        .filter(|saved| *saved)
        .count();
    assert_eq!(saved_count, 1);

    let revision: i64 =
        sqlx::query_scalar("SELECT revision FROM system_setting WHERE key = 'storage'")
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    let event_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app_event WHERE resource = 'system_setting' AND payload->>'group' = 'storage' AND (payload->>'revision')::bigint = $1",
    )
    .bind(revision)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(revision, saved.revision + 1);
    assert_eq!(event_count, 1);
}

#[tokio::test]
async fn settings_batch_rolls_back_every_group_when_one_revision_conflicts() {
    let locked = auth_db().await;
    let settings = SettingsService::new(locked.db.clone());
    let pixiv = settings
        .update(
            SettingGroupKey::Pixiv,
            None,
            SettingValue::Pixiv(PixivSettings {
                default_private_bookmark: false,
            }),
        )
        .await
        .unwrap();
    let retry = settings
        .update(
            SettingGroupKey::Retry,
            None,
            SettingValue::Retry(pixivarchive_application::settings::RetrySettings {
                network_backoff_seconds: vec![60, 300],
            }),
        )
        .await
        .unwrap();

    let result = settings
        .update_many(vec![
            SettingUpdate {
                group: SettingGroupKey::Pixiv,
                expected_revision: Some(pixiv.revision),
                value: SettingValue::Pixiv(PixivSettings {
                    default_private_bookmark: true,
                }),
            },
            SettingUpdate {
                group: SettingGroupKey::Retry,
                expected_revision: Some(retry.revision - 1),
                value: SettingValue::Retry(pixivarchive_application::settings::RetrySettings {
                    network_backoff_seconds: vec![120, 600],
                }),
            },
        ])
        .await;
    assert!(matches!(result, Err(SettingsError::RevisionConflict)));

    let effective = settings.effective().await.unwrap();
    assert!(!effective.pixiv.default_private_bookmark);
    assert_eq!(effective.retry.network_backoff_seconds, vec![60, 300]);
    let revisions: Vec<(String, i64)> = sqlx::query_as(
        "SELECT key, revision FROM system_setting WHERE key IN ('pixiv', 'retry') ORDER BY key",
    )
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(
        revisions,
        vec![("pixiv".to_owned(), 1), ("retry".to_owned(), 1)]
    );
    let event_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app_event WHERE resource = 'system_setting'")
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(event_count, 2);
}

#[tokio::test]
async fn settings_validation_covers_group_boundaries() {
    let locked = auth_db().await;
    let settings = SettingsService::new(locked.db.clone());
    assert!(
        settings
            .validate(SettingValue::Security(SecuritySettings {
                session_idle_timeout_seconds: 0,
                ..SecuritySettings::default()
            }))
            .is_err()
    );
    assert!(
        settings
            .validate(SettingValue::Retry(
                pixivarchive_application::settings::RetrySettings {
                    network_backoff_seconds: vec![1, 1],
                },
            ))
            .is_err()
    );
    assert!(
        settings
            .validate(SettingValue::Derivative(DerivativeSettings {
                format: DerivativeFormat::Avif,
                ..DerivativeSettings::default()
            }))
            .is_err()
    );
    let avif = SettingsService::with_capabilities(
        locked.db,
        DeploymentCapabilities {
            avif_derivatives: true,
        },
    );
    assert!(
        avif.validate(SettingValue::Derivative(DerivativeSettings {
            format: DerivativeFormat::Avif,
            ..DerivativeSettings::default()
        }))
        .is_ok()
    );
    assert!(
        settings
            .validate(SettingValue::Queue(QueueSettings {
                quota_weights: QueueQuotaWeights {
                    immediate: NonZeroU16::new(1).unwrap(),
                    manual_import: NonZeroU16::new(1).unwrap(),
                    scheduled_collection: NonZeroU16::new(1).unwrap(),
                    background_maintenance: NonZeroU16::new(1).unwrap(),
                },
                job_priorities: vec![JobPriorityMapping {
                    job_kind: JobKind::ImportWork,
                    priority: JobPriority::ManualImport,
                }],
            }))
            .is_err()
    );
    assert!(
        settings
            .validate(SettingValue::Processing(ProcessingSettings {
                pixiv_request_concurrency: NonZeroU16::new(1).unwrap(),
                pixiv_request_rate: RateLimit {
                    requests: NonZeroU16::new(1).unwrap(),
                    per_seconds: 0,
                },
                media_download_concurrency: NonZeroU16::new(1).unwrap(),
                media_download_rate: RateLimit {
                    requests: NonZeroU16::new(1).unwrap(),
                    per_seconds: 1,
                },
                media_cpu_concurrency: NonZeroU16::new(1).unwrap(),
            }))
            .is_err()
    );
    assert!(
        settings
            .validate(SettingValue::Ugoira(UgoiraSettings {
                max_zip_bytes: 0,
                max_frames: NonZeroU32::new(1).unwrap(),
                max_pixels_per_frame: 1,
                decoded_frame_cache_bytes: 1,
            }))
            .is_err()
    );
}
