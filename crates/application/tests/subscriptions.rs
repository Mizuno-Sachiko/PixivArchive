use pixivarchive_application::pixiv_accounts::{AccountCookieUpdate, PixivAccountService};
use pixivarchive_application::subscriptions::{
    RankingSubscriptionRequest, SubscriptionExecutionService, SubscriptionRunRequest,
    SubscriptionService, SubscriptionUpdateRequest,
};
use pixivarchive_db::DbError;
use pixivarchive_domain::{
    job::JobKind,
    pixiv::{PixivBookmarksMode, PixivFollowLatestMode, PixivRankingContent, PixivRankingMode},
    subscription::{SubscriptionKind, SubscriptionRunStatus},
};
use pixivarchive_pixiv::PixivErrorClass;
use pixivarchive_test_support::{
    DISCOVERY_LOCK_ID, FakePixivGateway, LockedDb, all_ranking_contents, all_ranking_modes,
    configure_bookmarks_subscription, configure_following_subscription, context, discovery_work,
    ranking_entry,
};
use serde_json::{Value, json};
use sqlx::Row;
use time::{Date, Month};

#[tokio::test]
async fn ranking_subscription_requires_an_existing_account() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let service = SubscriptionService::new(locked.db.clone());

    let error = service
        .create_ranking(RankingSubscriptionRequest {
            account_id: uuid::Uuid::now_v7(),
            name: "missing account".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 1,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap_err();

    assert!(matches!(error, DbError::NotFound));
    assert_eq!(kind_count(&locked, SubscriptionKind::Ranking).await, 0);
}

#[tokio::test]
async fn ranking_subscription_creation_emits_its_initial_revision() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let account = account(&locked, FakePixivGateway::new()).await;
    let subscription = SubscriptionService::new(locked.db.clone())
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "evented ranking".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 1,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();

    let payload: Option<Value> = sqlx::query_scalar(
        r#"
        SELECT payload
        FROM app_event
        WHERE resource = 'subscription'
          AND resource_id = $1
        "#,
    )
    .bind(subscription.id)
    .fetch_optional(locked.db.pool())
    .await
    .unwrap();

    assert_eq!(
        payload,
        Some(json!({
            "type": "subscription_changed",
            "revision": subscription.revision,
        }))
    );
}

#[tokio::test]
async fn stale_page_cannot_create_a_ranking_subscription_for_the_previous_account() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let accounts = PixivAccountService::new(locked.db.clone(), gateway);
    let account_a = accounts
        .update_cookie(AccountCookieUpdate {
            context: context(),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![0; 12],
            cookie_ciphertext: vec![1],
        })
        .await
        .unwrap();
    accounts
        .update_cookie(AccountCookieUpdate {
            context: pixivarchive_pixiv::PixivRequestContext::new(
                secrecy::SecretString::from("PHPSESSID=20002_account-b"),
                20_002,
                "PixivArchiveTest/1.0",
            ),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![1; 12],
            cookie_ciphertext: vec![2],
        })
        .await
        .unwrap();

    let result = SubscriptionService::new(locked.db.clone())
        .create_ranking(RankingSubscriptionRequest {
            account_id: account_a.id,
            name: "stale ranking".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 1,
            rule_id: None,
            next_run_at: None,
        })
        .await;

    assert!(matches!(result, Err(DbError::RevisionConflict)));
    assert_eq!(kind_count(&locked, SubscriptionKind::Ranking).await, 0);
}

#[tokio::test]
async fn subscription_schedule_limits_apply_to_create_and_update() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let account = account(&locked, FakePixivGateway::new()).await;
    let service = SubscriptionService::new(locked.db.clone());
    let subscription = service
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "valid schedule".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 2,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();

    let create_error = service
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "too frequent".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 14,
            lookback_pages: 2,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap_err();
    assert!(matches!(create_error, DbError::InvalidValue(_)));

    let update_error = service
        .update(
            subscription.id,
            subscription.revision,
            subscription.enabled,
            SubscriptionUpdateRequest {
                account_id: subscription.account_id,
                rule_id: subscription.rule_id,
                name: subscription.name.clone(),
                interval_minutes: 43_201,
                lookback_pages: 8,
                params: subscription.params.clone(),
                next_run_at: subscription.next_run_at,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(update_error, DbError::InvalidValue(_)));

    assert_eq!(
        service.get(subscription.id).await.unwrap().revision,
        subscription.revision
    );
}

#[tokio::test]
async fn subscription_enabled_state_is_updated_through_one_shared_command() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let account = account(&locked, FakePixivGateway::new()).await;
    let service = SubscriptionService::new(locked.db.clone());
    let ranking = service
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "daily".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 1,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    let following =
        configure_following_subscription(&locked.db, account.id, PixivFollowLatestMode::All, 60, 1)
            .await;
    let bookmarks =
        configure_bookmarks_subscription(&locked.db, account.id, PixivBookmarksMode::All, 30, 2)
            .await;

    for (subscription_id, revision) in [
        (ranking.id, ranking.revision),
        (following.id, following.revision),
        (bookmarks.id, bookmarks.revision),
    ] {
        let disabled = service
            .set_enabled(subscription_id, revision, false)
            .await
            .unwrap();
        assert!(!disabled.enabled);
        assert_eq!(disabled.revision, revision + 1);
    }
}

#[tokio::test]
async fn ranking_subscription_expands_every_mode_and_content_into_independent_jobs() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let account = account(&locked, FakePixivGateway::new()).await;
    let subscriptions = SubscriptionService::new(locked.db.clone());
    let subscription = subscriptions
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "rankings".to_owned(),
            modes: all_ranking_modes(),
            contents: all_ranking_contents(),
            interval_minutes: 60,
            lookback_pages: 2,
            rule_id: None,
            next_run_at: Some(time::OffsetDateTime::now_utc() - time::Duration::minutes(1)),
        })
        .await
        .unwrap();

    subscriptions
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();

    let rows = sqlx::query("SELECT kind, payload FROM job ORDER BY id")
        .fetch_all(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(rows.len(), 25);
    assert!(
        rows.iter()
            .all(|row| row.get::<String, _>("kind") == JobKind::RankingCollection.as_str())
    );
    assert!(rows.iter().any(|row| {
        let payload = row.get::<serde_json::Value, _>("payload");
        payload["mode"] == "r18g" && payload["content"] == "manga" && payload["page_size"] == 50
    }));
    assert!(rows.iter().any(|row| {
        let payload = row.get::<serde_json::Value, _>("payload");
        payload["mode"] == "male" && payload["content"] == "all"
    }));
    assert!(!rows.iter().any(|row| {
        let payload = row.get::<serde_json::Value, _>("payload");
        payload["mode"] == "female" && payload["content"] == "illust"
    }));
    assert!(!rows.iter().any(|row| {
        let payload = row.get::<serde_json::Value, _>("payload");
        payload["mode"] == "ai_generated" && payload["content"] == "ugoira"
    }));
}

#[tokio::test]
async fn cursor_advances_after_success_and_failed_run_preserves_previous_cursor() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let ranking_date = Date::from_calendar_date(2026, Month::July, 29).unwrap();
    gateway.set_ranking_date(ranking_date);
    gateway.set_ranking_items(vec![
        ranking_entry(501, 1),
        ranking_entry(501, 1),
        ranking_entry(502, 2),
    ]);
    let account = account(&locked, gateway.clone()).await;
    let subscription = SubscriptionService::new(locked.db.clone())
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "daily".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 1,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    let execution = SubscriptionExecutionService::new(locked.db.clone(), gateway.clone());
    let first_run = SubscriptionService::new(locked.db.clone())
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();

    let first = execution
        .execute(SubscriptionRunRequest {
            context: context(),
            subscription_run_id: first_run.run_id,
        })
        .await
        .unwrap();

    assert_eq!(first.status, SubscriptionRunStatus::Succeeded);
    assert_eq!(first.discovered_count, 2);
    let after_success = normal_cursor_date(&locked, subscription.id).await;
    assert_eq!(after_success, ranking_date);

    gateway.fail_ranking(pixivarchive_pixiv::PixivErrorClass::Network);
    let failed_run = SubscriptionService::new(locked.db.clone())
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();
    let failed = execution
        .execute(SubscriptionRunRequest {
            context: context(),
            subscription_run_id: failed_run.run_id,
        })
        .await
        .unwrap();

    assert_eq!(failed.status, SubscriptionRunStatus::Failed);
    assert_eq!(
        normal_cursor_date(&locked, subscription.id).await,
        after_success
    );
}

#[tokio::test]
async fn backfill_cursor_is_independent_and_overlapping_triggers_are_merged() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let ranking_date = Date::from_calendar_date(2026, Month::July, 29).unwrap();
    gateway.set_ranking_date(ranking_date);
    gateway.set_ranking_items(vec![ranking_entry(601, 1)]);
    let account = account(&locked, gateway.clone()).await;
    let service = SubscriptionService::new(locked.db.clone());
    let subscription = service
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "backfill".to_owned(),
            modes: vec![PixivRankingMode::Weekly],
            contents: vec![PixivRankingContent::Manga],
            interval_minutes: 60,
            lookback_pages: 1,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();

    let first = service
        .start_manual_run(subscription.id, true)
        .await
        .unwrap();
    let merged = service
        .start_manual_run(subscription.id, true)
        .await
        .unwrap();
    assert_eq!(merged.run_id, first.run_id);

    SubscriptionExecutionService::new(locked.db.clone(), gateway)
        .execute(SubscriptionRunRequest {
            context: context(),
            subscription_run_id: first.run_id,
        })
        .await
        .unwrap();

    let merged_cursor_kind: String = sqlx::query_scalar(
        r#"
        SELECT cursor_kind
        FROM subscription_run
        WHERE subscription_id = $1
          AND id <> $2
          AND state = 'queued'
        "#,
    )
    .bind(subscription.id)
    .bind(first.run_id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();

    assert!(!cursor_exists(&locked, subscription.id, "normal").await);
    assert_eq!(merged_cursor_kind, "backfill");
    assert_eq!(
        backfill_cursor_date(&locked, subscription.id).await,
        ranking_date
            .previous_day()
            .and_then(Date::previous_day)
            .unwrap()
    );
}

#[tokio::test]
async fn following_and_bookmark_subscriptions_use_their_source_page_sizes() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.set_follow_items(vec![discovery_work(701)]);
    gateway.set_bookmark_items(vec![discovery_work(801)]);
    let account = account(&locked, gateway.clone()).await;
    let service = SubscriptionService::new(locked.db.clone());
    let following =
        configure_following_subscription(&locked.db, account.id, PixivFollowLatestMode::All, 60, 1)
            .await;
    let bookmarks =
        configure_bookmarks_subscription(&locked.db, account.id, PixivBookmarksMode::Safe, 60, 1)
            .await;
    let executor = SubscriptionExecutionService::new(locked.db.clone(), gateway.clone());

    for subscription in [following, bookmarks] {
        let run = service
            .start_manual_run(subscription.id, false)
            .await
            .unwrap();
        executor
            .execute(SubscriptionRunRequest {
                context: context(),
                subscription_run_id: run.run_id,
            })
            .await
            .unwrap();
    }

    let requests = gateway.bookmark_requests();
    assert_eq!(requests[0].offset, 0);
    assert_eq!(requests[0].mode, PixivBookmarksMode::Safe);
    assert_eq!(requests[0].user_id, 10001);
    assert_eq!(gateway.ranking_requests().len(), 0);
    assert_eq!(kind_count(&locked, SubscriptionKind::Following).await, 1);
    assert_eq!(kind_count(&locked, SubscriptionKind::Bookmarks).await, 1);
}

#[tokio::test]
async fn full_bookmark_sync_marks_archived_works_as_active_bookmarks() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.set_public_bookmark_pages(vec![vec![901], vec![902]]);
    gateway.set_private_bookmark_pages(vec![vec![903]]);
    let account = account(&locked, gateway.clone()).await;
    let subscription =
        configure_bookmarks_subscription(&locked.db, account.id, PixivBookmarksMode::All, 60, 1)
            .await;
    let run = SubscriptionService::new(locked.db.clone())
        .start_manual_run(subscription.id, true)
        .await
        .unwrap();

    let result = SubscriptionExecutionService::new(locked.db.clone(), gateway)
        .execute(SubscriptionRunRequest {
            context: context(),
            subscription_run_id: run.run_id,
        })
        .await
        .unwrap();

    assert_eq!(result.status, SubscriptionRunStatus::Succeeded);
    assert_eq!(active_bookmark_count(&locked, account.id).await, 3);
}

#[tokio::test]
async fn bookmark_page_interstitial_is_recorded_as_retryable_server_failure() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.fail_bookmarks(PixivErrorClass::InvalidJsonOrInterstitial);
    let account = account(&locked, gateway.clone()).await;
    let subscription =
        configure_bookmarks_subscription(&locked.db, account.id, PixivBookmarksMode::All, 60, 1)
            .await;
    let run = SubscriptionService::new(locked.db.clone())
        .start_manual_run(subscription.id, true)
        .await
        .unwrap();

    let result = SubscriptionExecutionService::new(locked.db.clone(), gateway)
        .execute(SubscriptionRunRequest {
            context: context(),
            subscription_run_id: run.run_id,
        })
        .await
        .unwrap();

    assert_eq!(result.status, SubscriptionRunStatus::Failed);
    assert_eq!(result.error_class.as_deref(), Some("server"));
    assert_eq!(
        result.error_message.as_deref(),
        Some("Pixiv 服务暂时不可用或响应无法处理")
    );
}

async fn account(
    locked: &LockedDb,
    gateway: FakePixivGateway,
) -> pixivarchive_application::pixiv_accounts::PixivAccount {
    PixivAccountService::new(locked.db.clone(), gateway)
        .update_cookie(AccountCookieUpdate {
            context: context(),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![0; 12],
            cookie_ciphertext: vec![1],
        })
        .await
        .unwrap()
}

async fn normal_cursor_date(locked: &LockedDb, subscription_id: uuid::Uuid) -> Date {
    cursor_date(locked, subscription_id, "normal").await
}

async fn backfill_cursor_date(locked: &LockedDb, subscription_id: uuid::Uuid) -> Date {
    cursor_date(locked, subscription_id, "backfill").await
}

async fn cursor_date(locked: &LockedDb, subscription_id: uuid::Uuid, cursor_kind: &str) -> Date {
    sqlx::query_scalar(
        "SELECT (cursor_value->>'date')::date FROM subscription_cursor WHERE subscription_id = $1 AND cursor_kind = $2",
    )
    .bind(subscription_id)
    .bind(cursor_kind)
    .fetch_one(locked.db.pool())
    .await
    .unwrap()
}

async fn cursor_exists(locked: &LockedDb, subscription_id: uuid::Uuid, cursor_kind: &str) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM subscription_cursor WHERE subscription_id = $1 AND cursor_kind = $2)",
    )
    .bind(subscription_id)
    .bind(cursor_kind)
    .fetch_one(locked.db.pool())
    .await
    .unwrap()
}

async fn kind_count(locked: &LockedDb, kind: SubscriptionKind) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM subscription WHERE kind = $1")
        .bind(kind.as_str())
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn active_bookmark_count(locked: &LockedDb, account_id: uuid::Uuid) -> i64 {
    sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM pixiv_work_bookmark bookmark
        JOIN work ON work.id = bookmark.work_id
        WHERE bookmark.pixiv_account_id = $1
          AND bookmark.active = true
          AND work.pixiv_work_id = ANY($2::bigint[])
        "#,
    )
    .bind(account_id)
    .bind([901_i64, 902, 903])
    .fetch_one(locked.db.pool())
    .await
    .unwrap()
}
