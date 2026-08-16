use pixivarchive_application::{
    favorites::FavoritesAdminService,
    following::FollowingAdminService,
    jobs::JobService,
    pixiv_accounts::{
        AccountCookieUpdate, LivePixivAccountCommandPort, PixivAccountAdminService,
        PixivAccountCommandPort, PixivAccountService, PixivCookieCipher, PixivCookieKeyConfig,
        PixivCookieKeyringConfig, UpdatePixivAccountRequest,
    },
    settings::{QueueSettings, SettingValue, SettingsService},
    subscriptions::{RankingSubscriptionRequest, SubscriptionService},
};
use pixivarchive_db::{PixivAccountRepository, SubscriptionRepository};
use pixivarchive_domain::{
    job::{JobKind, JobPriority, NewJob},
    settings::SettingGroupKey,
    subscription::PixivAccountState,
};
use pixivarchive_pixiv::PixivErrorClass;
use pixivarchive_test_support::{
    DISCOVERY_LOCK_ID, FakePixivGateway, LockedDb, all_ranking_contents, all_ranking_modes, context,
};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

#[tokio::test]
async fn missing_account_is_reported_as_unconfigured_and_cookie_update_defaults_writeback_off() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let accounts = PixivAccountService::new(locked.db.clone(), gateway.clone());

    assert_eq!(
        accounts.status().await.unwrap().state,
        PixivAccountState::Unconfigured
    );

    let saved = accounts
        .update_cookie(AccountCookieUpdate {
            context: context(),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![0; 12],
            cookie_ciphertext: vec![1, 2, 3],
        })
        .await
        .unwrap();

    assert_eq!(saved.state, PixivAccountState::Normal);
    assert_eq!(saved.display_name, "Test Artist");
    assert!(!saved.bookmark_writeback_enabled);
    assert_eq!(gateway.validate_calls(), 1);
}

#[tokio::test]
async fn switching_pixiv_users_keeps_stable_account_rows_and_reactivates_the_saved_identity() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let accounts = PixivAccountService::new(locked.db.clone(), gateway);

    let account_a = accounts.update_cookie(valid_cookie_update()).await.unwrap();
    let account_b = accounts
        .update_cookie(AccountCookieUpdate {
            context: context_for(20_002),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![1; 12],
            cookie_ciphertext: vec![2],
        })
        .await
        .unwrap();

    assert_ne!(account_a.id, account_b.id);
    assert_eq!(accounts.current().await.unwrap().unwrap().id, account_b.id);
    assert_eq!(pixiv_account_count(&locked).await, 2);

    let restored_a = accounts.update_cookie(valid_cookie_update()).await.unwrap();

    assert_eq!(restored_a.id, account_a.id);
    assert_eq!(accounts.current().await.unwrap().unwrap().id, account_a.id);
    assert_eq!(pixiv_account_count(&locked).await, 2);
}

#[tokio::test]
async fn invalid_candidate_cookie_does_not_replace_the_current_pixiv_identity() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let accounts = PixivAccountService::new(locked.db.clone(), gateway.clone());
    let account_a = accounts.update_cookie(valid_cookie_update()).await.unwrap();
    gateway.fail_validation(PixivErrorClass::CredentialInvalid);

    let result = accounts
        .update_cookie(AccountCookieUpdate {
            context: context_for(20_002),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![1; 12],
            cookie_ciphertext: vec![2],
        })
        .await;

    assert!(result.is_err());
    assert_eq!(accounts.current().await.unwrap().unwrap().id, account_a.id);
    assert_eq!(pixiv_account_count(&locked).await, 1);
    assert_eq!(current_cookie_ciphertext(&locked).await, vec![1]);
}

#[tokio::test]
async fn invalid_replacement_cookie_keeps_the_saved_credential_for_the_same_identity() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let accounts = PixivAccountService::new(locked.db.clone(), gateway.clone());
    let account = accounts.update_cookie(valid_cookie_update()).await.unwrap();
    gateway.fail_validation(PixivErrorClass::CredentialInvalid);

    let result = accounts
        .update_cookie(AccountCookieUpdate {
            context: context(),
            cookie_key_id: "replacement".to_owned(),
            cookie_nonce: vec![9; 12],
            cookie_ciphertext: vec![9],
        })
        .await;

    assert!(result.is_err());
    let current = accounts.current().await.unwrap().unwrap();
    assert_eq!(current.id, account.id);
    assert_eq!(current.state, PixivAccountState::Normal);
    assert_eq!(current_cookie_ciphertext(&locked).await, vec![1]);
}

#[tokio::test]
async fn cookie_update_rolls_back_account_when_waiting_job_recovery_fails() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let accounts = PixivAccountService::new(locked.db.clone(), gateway);
    let saved = accounts.update_cookie(valid_cookie_update()).await.unwrap();
    let jobs = JobService::new(locked.db.clone());
    let mut job = NewJob::for_kind(
        JobPriority::ManualImport,
        JobKind::ImportWork,
        json!({ "pixiv_work_id": 401 }),
    );
    job.pixiv_account_id = Some(saved.id);
    let job_id = jobs.enqueue(job).await.unwrap();
    jobs.block_account(saved.id).await.unwrap();
    let before = PixivAccountRepository::new(locked.db.clone())
        .get(saved.id)
        .await
        .unwrap();
    let before_cookie = current_cookie_ciphertext(&locked).await;

    sqlx::query(
        r#"
        CREATE FUNCTION fail_account_resume() RETURNS trigger
        LANGUAGE plpgsql AS $$
        BEGIN
            RAISE EXCEPTION 'forced account resume failure';
        END;
        $$;
        "#,
    )
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        CREATE TRIGGER fail_account_resume
        BEFORE UPDATE OF state ON job
        FOR EACH ROW
        WHEN (OLD.state = 'waiting_account' AND NEW.state = 'queued')
        EXECUTE FUNCTION fail_account_resume();
        "#,
    )
    .execute(locked.db.pool())
    .await
    .unwrap();

    let result = accounts
        .update_cookie(AccountCookieUpdate {
            context: context(),
            cookie_key_id: "replacement".to_owned(),
            cookie_nonce: vec![9; 12],
            cookie_ciphertext: vec![9],
        })
        .await;

    sqlx::query("DROP TRIGGER fail_account_resume ON job")
        .execute(locked.db.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION fail_account_resume()")
        .execute(locked.db.pool())
        .await
        .unwrap();

    assert!(result.is_err());
    let after = PixivAccountRepository::new(locked.db.clone())
        .get(saved.id)
        .await
        .unwrap();
    assert_eq!(after.state, before.state);
    assert_eq!(after.revision, before.revision);
    assert_eq!(current_cookie_ciphertext(&locked).await, before_cookie);
    assert_eq!(job_state(&locked, job_id).await, "waiting_account");
}

#[tokio::test]
async fn account_revision_mutations_append_matching_events() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let account = PixivAccountService::new(locked.db.clone(), FakePixivGateway::new())
        .update_cookie(valid_cookie_update())
        .await
        .unwrap();
    sqlx::query("DELETE FROM app_event")
        .execute(locked.db.pool())
        .await
        .unwrap();
    let repository = PixivAccountRepository::new(locked.db.clone());

    let validating = repository
        .set_state(account.id, PixivAccountState::Validating, None)
        .await
        .unwrap();
    let profiled = repository
        .set_profile(account.id, "Updated Artist", None)
        .await
        .unwrap();
    let writeback = repository
        .set_bookmark_writeback_enabled(account.id, true)
        .await
        .unwrap();

    let revisions: Vec<i64> = sqlx::query_scalar(
        r#"
        SELECT (payload->>'revision')::bigint
        FROM app_event
        WHERE resource = 'pixiv_account'
          AND resource_id = $1
        ORDER BY id
        "#,
    )
    .bind(account.id)
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(
        revisions,
        vec![validating.revision, profiled.revision, writeback.revision]
    );
}

#[tokio::test]
async fn clearing_cookie_preserves_account_and_waits_dependent_jobs() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let account = PixivAccountService::new(locked.db.clone(), FakePixivGateway::new())
        .update_cookie(valid_cookie_update())
        .await
        .unwrap();
    let jobs = JobService::new(locked.db.clone());

    let running_id = jobs.enqueue(account_job(account.id, 401)).await.unwrap();
    let claimed = jobs
        .claim(
            Uuid::now_v7(),
            &pixivarchive_domain::job::JobQuotaSelection::with_fallback(JobPriority::ManualImport),
            time::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(claimed.id, running_id);

    let queued_id = jobs.enqueue(account_job(account.id, 402)).await.unwrap();
    let waiting_storage_id = jobs.enqueue(account_job(account.id, 403)).await.unwrap();
    sqlx::query(
        "UPDATE job SET kind = 'download_media', state = 'waiting_storage', error_class = 'storage_low' WHERE id = $1",
    )
    .bind(waiting_storage_id)
    .execute(locked.db.pool())
    .await
    .unwrap();
    let failed_id = jobs.enqueue(account_job(account.id, 404)).await.unwrap();
    sqlx::query(
        "UPDATE job SET state = 'failed', error_class = 'network', retryable = true, next_retry_at = now() WHERE id = $1",
    )
    .bind(failed_id)
    .execute(locked.db.pool())
    .await
    .unwrap();

    let cleared = PixivAccountAdminService::new(locked.db.clone())
        .clear_credential(account.id, account.revision)
        .await
        .unwrap();

    assert_eq!(cleared.id, account.id);
    assert_eq!(cleared.pixiv_user_id, account.pixiv_user_id);
    assert_eq!(cleared.display_name, account.display_name);
    assert_eq!(cleared.state, PixivAccountState::Unconfigured);
    assert!(credential_is_cleared(&locked, account.id).await);
    let subscription_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM subscription WHERE pixiv_account_id = $1")
            .bind(account.id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(subscription_count, 2);
    assert_eq!(job_state(&locked, queued_id).await, "waiting_account");
    assert_eq!(
        job_state(&locked, waiting_storage_id).await,
        "waiting_account"
    );
    assert_eq!(job_state(&locked, failed_id).await, "waiting_account");
    assert_eq!(job_state(&locked, running_id).await, "running");
    assert_eq!(job_error_class(&locked, queued_id).await, None);

    let new_id = jobs.enqueue(account_job(account.id, 405)).await.unwrap();
    assert_eq!(job_state(&locked, new_id).await, "waiting_account");
    assert_eq!(job_error_class(&locked, new_id).await, None);

    jobs.fail(
        &claimed,
        pixivarchive_domain::job::JobErrorClass::CredentialInvalid,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(job_state(&locked, running_id).await, "waiting_account");
    assert_eq!(
        PixivAccountRepository::new(locked.db.clone())
            .get(account.id)
            .await
            .unwrap()
            .state,
        PixivAccountState::Unconfigured
    );
}

#[tokio::test]
async fn unconfigured_account_rejects_bookmark_writeback_and_favorites_configuration() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let account = PixivAccountService::new(locked.db.clone(), FakePixivGateway::new())
        .update_cookie(valid_cookie_update())
        .await
        .unwrap();
    let favorites = FavoritesAdminService::new(locked.db.clone());
    let favorites_state = favorites.current().await.unwrap();
    let account_admin = PixivAccountAdminService::new(locked.db.clone());
    let cleared = account_admin
        .clear_credential(account.id, account.revision)
        .await
        .unwrap();

    let writeback = account_admin
        .set_bookmark_writeback(cleared.id, cleared.revision, true)
        .await;
    let favorites_update = favorites
        .update(cleared.id, favorites_state.subscription.revision, true, 30)
        .await;

    assert!(matches!(
        writeback,
        Err(pixivarchive_db::DbError::RevisionConflict)
    ));
    assert!(matches!(
        favorites_update,
        Err(pixivarchive_db::DbError::RevisionConflict)
    ));
    assert!(
        !PixivAccountRepository::new(locked.db.clone())
            .get(cleared.id)
            .await
            .unwrap()
            .bookmark_writeback_enabled
    );
    assert!(!favorites.current().await.unwrap().subscription.enabled);
}

#[tokio::test]
async fn clearing_cookie_rejects_stale_revision_and_rolls_back_job_transition_failure() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let account = PixivAccountService::new(locked.db.clone(), FakePixivGateway::new())
        .update_cookie(valid_cookie_update())
        .await
        .unwrap();
    let jobs = JobService::new(locked.db.clone());
    let job_id = jobs.enqueue(account_job(account.id, 501)).await.unwrap();
    let admin = PixivAccountAdminService::new(locked.db.clone());

    let stale = admin
        .clear_credential(account.id, account.revision - 1)
        .await;
    assert!(matches!(
        stale,
        Err(pixivarchive_db::DbError::RevisionConflict)
    ));
    assert!(!credential_is_cleared(&locked, account.id).await);

    sqlx::query(
        r#"
        CREATE FUNCTION fail_account_wait() RETURNS trigger
        LANGUAGE plpgsql AS $$
        BEGIN
            RAISE EXCEPTION 'forced account wait failure';
        END;
        $$;
        "#,
    )
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        CREATE TRIGGER fail_account_wait
        BEFORE UPDATE OF state ON job
        FOR EACH ROW
        WHEN (NEW.state = 'waiting_account')
        EXECUTE FUNCTION fail_account_wait();
        "#,
    )
    .execute(locked.db.pool())
    .await
    .unwrap();

    let failed = admin.clear_credential(account.id, account.revision).await;

    sqlx::query("DROP TRIGGER fail_account_wait ON job")
        .execute(locked.db.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION fail_account_wait()")
        .execute(locked.db.pool())
        .await
        .unwrap();

    assert!(failed.is_err());
    let unchanged = PixivAccountRepository::new(locked.db.clone())
        .get(account.id)
        .await
        .unwrap();
    assert_eq!(unchanged.state, PixivAccountState::Normal);
    assert_eq!(unchanged.revision, account.revision);
    assert!(!credential_is_cleared(&locked, account.id).await);
    assert_eq!(job_state(&locked, job_id).await, "queued");
}

#[tokio::test]
async fn admin_cookie_update_creates_the_fixed_account_subscriptions() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let commands = LivePixivAccountCommandPort::new(
        locked.db.clone(),
        FakePixivGateway::new(),
        PixivCookieCipher::new(PixivCookieKeyringConfig::new(PixivCookieKeyConfig::new(
            "test", [7; 32],
        )))
        .unwrap(),
        "PixivArchive tests",
    );

    let account = commands
        .update(UpdatePixivAccountRequest {
            cookie: "10001_session-value".to_owned(),
        })
        .await
        .unwrap();

    let following = SubscriptionRepository::new(locked.db.clone())
        .following_subscription(account.id)
        .await
        .unwrap();
    assert_eq!(following.name, "关注动态");
    assert!(following.enabled);
    let bookmarks = SubscriptionRepository::new(locked.db.clone())
        .bookmarks_subscription(account.id)
        .await
        .unwrap();
    assert_eq!(bookmarks.name, "收藏同步");
    assert!(!bookmarks.enabled);
    assert_eq!(bookmarks.schedule["interval_minutes"], 30);

    let subscription_event_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT resource_id
        FROM app_event
        WHERE resource = 'subscription'
        ORDER BY id
        "#,
    )
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(subscription_event_ids, vec![following.id, bookmarks.id]);

    let subscriptions = SubscriptionRepository::new(locked.db.clone());
    subscriptions
        .ensure_following_subscription(account.id, OffsetDateTime::now_utc())
        .await
        .unwrap();
    subscriptions
        .ensure_bookmarks_subscription(account.id, OffsetDateTime::now_utc())
        .await
        .unwrap();
    let event_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app_event WHERE resource = 'subscription'")
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(event_count, 2);
}

#[tokio::test]
async fn fixed_subscription_failure_rolls_back_the_account_activation() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    sqlx::query(
        r#"
        CREATE FUNCTION fail_fixed_bookmarks_creation() RETURNS trigger
        LANGUAGE plpgsql AS $$
        BEGIN
            RAISE EXCEPTION 'forced fixed bookmarks failure';
        END;
        $$;
        "#,
    )
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        CREATE TRIGGER fail_fixed_bookmarks_creation
        BEFORE INSERT ON subscription
        FOR EACH ROW
        WHEN (NEW.kind = 'bookmarks')
        EXECUTE FUNCTION fail_fixed_bookmarks_creation();
        "#,
    )
    .execute(locked.db.pool())
    .await
    .unwrap();

    let result = LivePixivAccountCommandPort::new(
        locked.db.clone(),
        FakePixivGateway::new(),
        PixivCookieCipher::new(PixivCookieKeyringConfig::new(PixivCookieKeyConfig::new(
            "test", [7; 32],
        )))
        .unwrap(),
        "PixivArchive tests",
    )
    .update(UpdatePixivAccountRequest {
        cookie: "10001_session-value".to_owned(),
    })
    .await;

    sqlx::query("DROP TRIGGER fail_fixed_bookmarks_creation ON subscription")
        .execute(locked.db.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION fail_fixed_bookmarks_creation()")
        .execute(locked.db.pool())
        .await
        .unwrap();

    assert!(result.is_err());
    let account_count: i64 = sqlx::query_scalar("SELECT count(*) FROM pixiv_account")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    let subscription_count: i64 = sqlx::query_scalar("SELECT count(*) FROM subscription")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    let event_count: i64 = sqlx::query_scalar("SELECT count(*) FROM app_event")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(account_count, 0);
    assert_eq!(subscription_count, 0);
    assert_eq!(event_count, 0);
}

#[tokio::test]
async fn stale_following_page_cannot_update_the_new_current_account() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let commands = LivePixivAccountCommandPort::new(
        locked.db.clone(),
        FakePixivGateway::new(),
        PixivCookieCipher::new(PixivCookieKeyringConfig::new(PixivCookieKeyConfig::new(
            "test", [7; 32],
        )))
        .unwrap(),
        "PixivArchive tests",
    );
    commands
        .update(UpdatePixivAccountRequest {
            cookie: "10001_account-a".to_owned(),
        })
        .await
        .unwrap();
    let following = FollowingAdminService::new(locked.db.clone());
    let stale = following.current().await.unwrap();
    commands
        .update(UpdatePixivAccountRequest {
            cookie: "20002_account-b".to_owned(),
        })
        .await
        .unwrap();

    let result = following
        .configure_subscription(
            stale.subscription.account_id,
            stale.subscription.revision,
            false,
            15,
        )
        .await;
    let batch_result = following
        .set_authors_enabled(stale.subscription.account_id, vec![70_001], false)
        .await;

    assert!(matches!(
        result,
        Err(pixivarchive_db::DbError::RevisionConflict)
    ));
    assert!(matches!(
        batch_result,
        Err(pixivarchive_db::DbError::RevisionConflict)
    ));
    assert!(following.current().await.unwrap().subscription.enabled);
}

#[tokio::test]
async fn stale_favorites_page_cannot_update_or_run_for_the_new_current_account() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let commands = LivePixivAccountCommandPort::new(
        locked.db.clone(),
        FakePixivGateway::new(),
        PixivCookieCipher::new(PixivCookieKeyringConfig::new(PixivCookieKeyConfig::new(
            "test", [7; 32],
        )))
        .unwrap(),
        "PixivArchive tests",
    );
    commands
        .update(UpdatePixivAccountRequest {
            cookie: "10001_account-a".to_owned(),
        })
        .await
        .unwrap();
    let favorites = FavoritesAdminService::new(locked.db.clone());
    let stale = favorites.current().await.unwrap();
    commands
        .update(UpdatePixivAccountRequest {
            cookie: "20002_account-b".to_owned(),
        })
        .await
        .unwrap();

    let update = favorites
        .update(
            stale.subscription.account_id,
            stale.subscription.revision,
            true,
            30,
        )
        .await;
    let run = favorites
        .start_manual_run(stale.subscription.account_id)
        .await;

    assert!(matches!(
        update,
        Err(pixivarchive_db::DbError::RevisionConflict)
    ));
    assert!(matches!(
        run,
        Err(
            pixivarchive_application::subscriptions::SubscriptionRunStartError::Storage(
                pixivarchive_db::DbError::RevisionConflict
            )
        )
    ));
    assert!(!favorites.current().await.unwrap().subscription.enabled);
}

#[tokio::test]
async fn stale_account_page_cannot_change_bookmark_writeback_for_an_old_account() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let commands = LivePixivAccountCommandPort::new(
        locked.db.clone(),
        FakePixivGateway::new(),
        PixivCookieCipher::new(PixivCookieKeyringConfig::new(PixivCookieKeyConfig::new(
            "test", [7; 32],
        )))
        .unwrap(),
        "PixivArchive tests",
    );
    let account_a = commands
        .update(UpdatePixivAccountRequest {
            cookie: "10001_account-a".to_owned(),
        })
        .await
        .unwrap();
    commands
        .update(UpdatePixivAccountRequest {
            cookie: "20002_account-b".to_owned(),
        })
        .await
        .unwrap();
    let account_a = PixivAccountRepository::new(locked.db.clone())
        .get(account_a.id)
        .await
        .unwrap();

    let validation = commands.validate(Some(account_a.id)).await;
    let result = PixivAccountAdminService::new(locked.db.clone())
        .set_bookmark_writeback(account_a.id, account_a.revision, true)
        .await;

    assert!(matches!(
        validation,
        Err(
            pixivarchive_application::pixiv_accounts::PixivAccountAdminError::Storage(
                pixivarchive_db::DbError::RevisionConflict
            )
        )
    ));
    assert!(matches!(
        result,
        Err(pixivarchive_db::DbError::RevisionConflict)
    ));
    assert!(
        !PixivAccountRepository::new(locked.db)
            .get(account_a.id)
            .await
            .unwrap()
            .bookmark_writeback_enabled
    );
}

#[tokio::test]
async fn manual_favorite_sync_always_starts_a_full_reconciliation() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let account = PixivAccountService::new(locked.db.clone(), FakePixivGateway::new())
        .update_cookie(valid_cookie_update())
        .await
        .unwrap();

    let run = FavoritesAdminService::new(locked.db.clone())
        .start_manual_run(account.id)
        .await
        .unwrap();

    assert_eq!(run.trigger_kind, "backfill");
}

#[tokio::test]
async fn fixed_subscription_commands_use_the_saved_task_priority_mapping() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let account = LivePixivAccountCommandPort::new(
        locked.db.clone(),
        FakePixivGateway::new(),
        PixivCookieCipher::new(PixivCookieKeyringConfig::new(PixivCookieKeyConfig::new(
            "test", [7; 32],
        )))
        .unwrap(),
        "PixivArchive tests",
    )
    .update(UpdatePixivAccountRequest {
        cookie: "10001_session-value".to_owned(),
    })
    .await
    .unwrap();
    let mut queue = QueueSettings::default();
    for kind in [JobKind::BookmarksCollection, JobKind::FollowingCollection] {
        queue
            .job_priorities
            .iter_mut()
            .find(|mapping| mapping.job_kind == kind)
            .unwrap()
            .priority = JobPriority::Immediate;
    }
    SettingsService::new(locked.db.clone())
        .update(SettingGroupKey::Queue, None, SettingValue::Queue(queue))
        .await
        .unwrap();

    FavoritesAdminService::new(locked.db.clone())
        .start_manual_run(account.id)
        .await
        .unwrap();
    FollowingAdminService::new(locked.db.clone())
        .start_manual_run(account.id, false)
        .await
        .unwrap();

    let priorities: Vec<String> = sqlx::query_scalar(
        "SELECT priority_class FROM job WHERE kind IN ('bookmarks_collection', 'following_collection') ORDER BY kind",
    )
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(priorities, vec!["immediate", "immediate"]);
}

#[tokio::test]
async fn saved_cookie_can_be_revalidated_without_reentering_it() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let accounts = PixivAccountService::new(locked.db.clone(), gateway.clone());
    let saved = accounts.update_cookie(valid_cookie_update()).await.unwrap();
    SubscriptionService::new(locked.db.clone())
        .create_ranking(RankingSubscriptionRequest {
            account_id: saved.id,
            name: "startup validation".to_owned(),
            modes: vec![pixivarchive_domain::pixiv::PixivRankingMode::Daily],
            contents: vec![pixivarchive_domain::pixiv::PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 0,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    sqlx::query("UPDATE pixiv_account SET display_name = '显示名称' WHERE id = $1")
        .bind(saved.id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    let validated = accounts.validate_saved(&context()).await.unwrap();

    assert_eq!(validated.state, PixivAccountState::Normal);
    assert_eq!(validated.display_name, "Test Artist");
    assert!(validated.last_validated_at.is_some());
    assert_eq!(gateway.validate_calls(), 2);
    let active_runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM subscription_run WHERE state IN ('queued', 'running')",
    )
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(active_runs, 0);
}

#[tokio::test]
async fn r18_probe_runs_only_when_an_enabled_r18_subscription_exists() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let accounts = PixivAccountService::new(locked.db.clone(), gateway.clone());

    accounts.update_cookie(valid_cookie_update()).await.unwrap();
    assert!(gateway.ranking_requests().is_empty());

    SubscriptionService::new(locked.db.clone())
        .create_ranking(RankingSubscriptionRequest {
            account_id: accounts.status().await.unwrap().account_id.unwrap(),
            name: "r18 ranking".to_owned(),
            modes: vec![pixivarchive_domain::pixiv::PixivRankingMode::R18],
            contents: vec![pixivarchive_domain::pixiv::PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 1,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();

    accounts.update_cookie(valid_cookie_update()).await.unwrap();
    assert_eq!(gateway.ranking_requests().len(), 1);
}

#[tokio::test]
async fn invalid_cookie_blocks_dependent_jobs_and_pauses_due_scheduling() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let accounts = PixivAccountService::new(locked.db.clone(), gateway.clone());
    let saved = accounts.update_cookie(valid_cookie_update()).await.unwrap();
    let subscriptions = SubscriptionService::new(locked.db.clone());
    subscriptions
        .create_ranking(RankingSubscriptionRequest {
            account_id: saved.id,
            name: "due ranking".to_owned(),
            modes: vec![pixivarchive_domain::pixiv::PixivRankingMode::Daily],
            contents: vec![pixivarchive_domain::pixiv::PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 1,
            rule_id: None,
            next_run_at: Some(time::OffsetDateTime::now_utc() - time::Duration::minutes(1)),
        })
        .await
        .unwrap();

    let jobs = JobService::new(locked.db.clone());
    let mut existing = NewJob::for_kind(
        JobPriority::ManualImport,
        JobKind::ImportWork,
        json!({ "pixiv_work_id": 401 }),
    );
    existing.pixiv_account_id = Some(saved.id);
    let existing_id = jobs.enqueue(existing).await.unwrap();

    gateway.fail_validation(PixivErrorClass::CredentialInvalid);
    let invalid = accounts.validate_saved(&context()).await.unwrap();
    assert_eq!(invalid.state, PixivAccountState::CredentialInvalid);
    assert_eq!(job_state(&locked, existing_id).await, "waiting_account");

    let mut created_while_invalid = NewJob::for_kind(
        JobPriority::ManualImport,
        JobKind::ImportWork,
        json!({ "pixiv_work_id": 402 }),
    );
    created_while_invalid.pixiv_account_id = Some(saved.id);
    let waiting_id = jobs.enqueue(created_while_invalid).await.unwrap();
    assert_eq!(job_state(&locked, waiting_id).await, "waiting_account");
    assert!(
        SubscriptionRepository::new(locked.db.clone())
            .list_due_subscriptions(time::OffsetDateTime::now_utc(), 10)
            .await
            .unwrap()
            .is_empty()
    );

    gateway.clear_validation_failure();
    accounts.update_cookie(valid_cookie_update()).await.unwrap();
    assert_eq!(job_state(&locked, existing_id).await, "queued");
    assert_eq!(job_state(&locked, waiting_id).await, "queued");
}

#[tokio::test]
async fn restricted_account_returns_to_normal_and_releases_waiting_jobs() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let accounts = PixivAccountService::new(locked.db.clone(), gateway.clone());
    let account = accounts.update_cookie(valid_cookie_update()).await.unwrap();
    SubscriptionService::new(locked.db.clone())
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "r18g".to_owned(),
            modes: vec![pixivarchive_domain::pixiv::PixivRankingMode::R18g],
            contents: vec![pixivarchive_domain::pixiv::PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 1,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    let jobs = JobService::new(locked.db.clone());
    let mut job = NewJob::for_kind(
        JobPriority::ManualImport,
        JobKind::ImportWork,
        json!({ "pixiv_work_id": 401 }),
    );
    job.pixiv_account_id = Some(account.id);
    let job_id = jobs.enqueue(job).await.unwrap();
    jobs.block_account(account.id).await.unwrap();
    assert_eq!(job_state(&locked, job_id).await, "waiting_account");

    gateway.fail_ranking(PixivErrorClass::AgeRestrictedDisabled);

    let restricted = accounts.update_cookie(valid_cookie_update()).await.unwrap();

    assert_eq!(restricted.state, PixivAccountState::Restricted);
    assert_eq!(job_state(&locked, job_id).await, "waiting_account");

    gateway.clear_ranking_failure();
    let recovered = accounts.validate_saved(&context()).await.unwrap();

    assert_eq!(recovered.state, PixivAccountState::Normal);
    assert_eq!(job_state(&locked, job_id).await, "queued");
}

#[tokio::test]
async fn successful_cookie_update_releases_waiting_jobs_and_merges_one_catchup_per_subscription() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let accounts = PixivAccountService::new(locked.db.clone(), gateway.clone());
    let saved = accounts.update_cookie(valid_cookie_update()).await.unwrap();
    let subscriptions = SubscriptionService::new(locked.db.clone());
    let subscription = subscriptions
        .create_ranking(RankingSubscriptionRequest {
            account_id: saved.id,
            name: "all rankings".to_owned(),
            modes: all_ranking_modes(),
            contents: all_ranking_contents(),
            interval_minutes: 60,
            lookback_pages: 2,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    let jobs = JobService::new(locked.db.clone());
    let mut job = NewJob::for_kind(
        JobPriority::ScheduledCollection,
        JobKind::RankingCollection,
        json!({ "subscription_id": subscription.id }),
    );
    job.pixiv_account_id = Some(saved.id);
    jobs.enqueue(job).await.unwrap();
    let claimed = jobs
        .claim(
            Uuid::now_v7(),
            &pixivarchive_domain::job::JobQuotaSelection::with_fallback(
                JobPriority::ScheduledCollection,
            ),
            time::Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    jobs.fail(
        &claimed,
        pixivarchive_domain::job::JobErrorClass::CredentialInvalid,
        None,
        None,
    )
    .await
    .unwrap();

    accounts.update_cookie(valid_cookie_update()).await.unwrap();

    let waiting: i64 =
        sqlx::query_scalar("SELECT count(*) FROM job WHERE state = 'waiting_account'")
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    let active_runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM subscription_run WHERE subscription_id = $1 AND state IN ('queued', 'running')",
    )
    .bind(subscription.id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(waiting, 0);
    assert_eq!(active_runs, 1);
}

fn valid_cookie_update() -> AccountCookieUpdate {
    AccountCookieUpdate {
        context: context(),
        cookie_key_id: "test".to_owned(),
        cookie_nonce: vec![0; 12],
        cookie_ciphertext: vec![1],
    }
}

fn account_job(account_id: Uuid, pixiv_work_id: i64) -> NewJob {
    let mut job = NewJob::for_kind(
        JobPriority::ManualImport,
        JobKind::ImportWork,
        json!({ "pixiv_work_id": pixiv_work_id }),
    );
    job.pixiv_account_id = Some(account_id);
    job
}

fn context_for(user_id: i64) -> pixivarchive_pixiv::PixivRequestContext {
    pixivarchive_pixiv::PixivRequestContext::new(
        secrecy::SecretString::from(format!("PHPSESSID={user_id}")),
        user_id,
        "PixivArchiveTest/1.0",
    )
}

async fn pixiv_account_count(locked: &LockedDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM pixiv_account")
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn current_cookie_ciphertext(locked: &LockedDb) -> Vec<u8> {
    sqlx::query_scalar("SELECT cookie_ciphertext FROM pixiv_account WHERE is_current = true")
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn credential_is_cleared(locked: &LockedDb, account_id: Uuid) -> bool {
    sqlx::query_scalar(
        r#"
        SELECT cookie_key_id IS NULL
           AND cookie_nonce IS NULL
           AND cookie_ciphertext IS NULL
        FROM pixiv_account
        WHERE id = $1
        "#,
    )
    .bind(account_id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap()
}

async fn job_error_class(locked: &LockedDb, job_id: Uuid) -> Option<String> {
    sqlx::query_scalar("SELECT error_class FROM job WHERE id = $1")
        .bind(job_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn job_state(locked: &LockedDb, job_id: Uuid) -> String {
    sqlx::query_scalar("SELECT state FROM job WHERE id = $1")
        .bind(job_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}
