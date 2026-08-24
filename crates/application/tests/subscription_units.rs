use pixivarchive_application::pixiv_accounts::{AccountCookieUpdate, PixivAccountService};
use pixivarchive_application::rules::{
    PublishRuleVersionRequest, RuleService, SaveRuleDraftRequest,
};
use pixivarchive_application::subscriptions::{
    RankingSubscriptionRequest, SubscriptionExecutionService, SubscriptionMutationRequest,
    SubscriptionService, SubscriptionUnitRequest,
};
use pixivarchive_db::{DbError, JobRepository, SubscriptionRepository};
use pixivarchive_domain::{
    job::{JobPriority, JobQuotaSelection},
    pixiv::{
        PixivBookmarkVisibility, PixivBookmarksMode, PixivFollowLatestMode, PixivFollowedArtist,
        PixivRankingContent, PixivRankingMode,
    },
    rule::{
        Condition, ConditionGroup, ConditionValue, GroupMode, PageQuantifier, RuleAction,
        RuleDefinitionV1, RuleField, RuleOperator,
    },
    subscription::{SubscriptionKind, SubscriptionRunStatus},
};
use pixivarchive_test_support::{
    DISCOVERY_LOCK_ID, FakePixivGateway, LockedDb, configure_bookmarks_subscription,
    configure_following_subscription, context, discovery_work, ranking_entry, work_page,
};
use serde_json::json;
use sqlx::Row;
use time::{Date, Duration, Month};

#[tokio::test]
async fn ranking_units_execute_independently_and_parent_finishes_after_all_units() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.set_ranking_items(vec![ranking_entry(1101, 1)]);
    let account = account(&locked, gateway.clone()).await;
    let service = SubscriptionService::new(locked.db.clone());
    let subscription = service
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "two ranking units".to_owned(),
            modes: vec![PixivRankingMode::Daily, PixivRankingMode::Weekly],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 0,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    let run = service
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();

    let units = unit_rows(&locked, run.run_id).await;
    assert_eq!(units.len(), 2);
    assert_eq!(parent_state(&locked, run.run_id).await, "queued");

    let executor = SubscriptionExecutionService::new(locked.db.clone(), gateway.clone());
    executor
        .execute_unit(SubscriptionUnitRequest {
            context: context(),
            unit_id: units[0].id,
        })
        .await
        .unwrap();
    assert_eq!(unit_state(&locked, units[0].id).await, "succeeded");
    assert_eq!(parent_state(&locked, run.run_id).await, "running");

    executor
        .execute_unit(SubscriptionUnitRequest {
            context: context(),
            unit_id: units[1].id,
        })
        .await
        .unwrap();
    assert_eq!(parent_state(&locked, run.run_id).await, "succeeded");

    assert_eq!(
        gateway.ranking_requests(),
        vec![
            pixivarchive_domain::pixiv::PixivRankingRequest {
                mode: PixivRankingMode::Daily,
                content: PixivRankingContent::All,
                date: None,
                page: 1,
            },
            pixivarchive_domain::pixiv::PixivRankingRequest {
                mode: PixivRankingMode::Weekly,
                content: PixivRankingContent::All,
                date: None,
                page: 1,
            },
        ]
    );
    assert_eq!(ranking_context_count(&locked, run.run_id, 1101).await, 2);
}

#[tokio::test]
async fn stale_subscription_worker_cannot_persist_a_successful_pixiv_response() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.set_ranking_items(vec![ranking_entry(1151, 1)]);
    let pause = gateway.pause_work_detail();
    let account = account(&locked, gateway.clone()).await;
    let subscription = SubscriptionService::new(locked.db.clone())
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "stale ranking worker".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 0,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    let run = SubscriptionService::new(locked.db.clone())
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();
    let unit = unit_rows(&locked, run.run_id).await.remove(0);
    let jobs = JobRepository::new(locked.db.clone());
    let first_claim = jobs
        .claim_next(
            uuid::Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ScheduledCollection]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let first_lease = first_claim.lease();
    let executor = SubscriptionExecutionService::new(locked.db.clone(), gateway);
    let attempt = tokio::spawn(async move {
        executor
            .execute_unit_job_attempt(
                first_lease,
                JobPriority::ScheduledCollection,
                SubscriptionUnitRequest {
                    context: context(),
                    unit_id: unit.id,
                },
            )
            .await
    });
    pause.entered.wait().await;
    sqlx::query("UPDATE job SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(first_claim.id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    let reclaimed = jobs
        .claim_next(
            uuid::Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ScheduledCollection]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reclaimed.id, first_claim.id);
    pause.resume.wait().await;

    assert!(matches!(
        attempt.await.unwrap(),
        Err(DbError::LeaseConflict)
    ));
    assert_eq!(work_count(&locked).await, 0);
    assert_eq!(ranking_context_count(&locked, run.run_id, 1151).await, 0);
    assert_eq!(queued_downloads(&locked).await, 0);
    assert_eq!(unit_state(&locked, unit.id).await, "running");
}

#[tokio::test]
async fn unit_cursor_advances_atomically_and_failed_unit_preserves_cursor_date() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let ranking_date = Date::from_calendar_date(2026, Month::July, 29).unwrap();
    gateway.set_ranking_date(ranking_date);
    gateway.set_ranking_items(vec![ranking_entry(1201, 1)]);
    let account = account(&locked, gateway.clone()).await;
    let service = SubscriptionService::new(locked.db.clone());
    let subscription = service
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "cursor units".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 0,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    let first = service
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();
    let first_unit = unit_rows(&locked, first.run_id).await.remove(0);
    SubscriptionExecutionService::new(locked.db.clone(), gateway.clone())
        .execute_unit(SubscriptionUnitRequest {
            context: context(),
            unit_id: first_unit.id,
        })
        .await
        .unwrap();
    assert_eq!(
        cursor_date(&locked, subscription.id, "normal", "ranking:daily:all").await,
        ranking_date
    );

    gateway.fail_ranking(pixivarchive_pixiv::PixivErrorClass::Network);
    let second = service
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();
    let second_unit = unit_rows(&locked, second.run_id).await.remove(0);
    let failed = SubscriptionExecutionService::new(locked.db.clone(), gateway)
        .execute_unit(SubscriptionUnitRequest {
            context: context(),
            unit_id: second_unit.id,
        })
        .await
        .unwrap();
    assert_eq!(failed.status, SubscriptionRunStatus::Failed);
    assert_eq!(
        cursor_date(&locked, subscription.id, "normal", "ranking:daily:all").await,
        ranking_date
    );
}

#[tokio::test]
async fn lookback_periods_revisit_previous_dates_and_deduplicate_across_unit() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let ranking_date = Date::from_calendar_date(2026, Month::July, 29).unwrap();
    gateway.set_ranking_date(ranking_date);
    gateway.set_ranking_items(vec![ranking_entry(1301, 1), ranking_entry(1302, 2)]);
    let account = account(&locked, gateway.clone()).await;
    let service = SubscriptionService::new(locked.db.clone());
    let subscription = service
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "lookback".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 2,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    let run = service
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();
    let unit = unit_rows(&locked, run.run_id).await.remove(0);

    let result = SubscriptionExecutionService::new(locked.db.clone(), gateway.clone())
        .execute_unit(SubscriptionUnitRequest {
            context: context(),
            unit_id: unit.id,
        })
        .await
        .unwrap();

    assert_eq!(result.discovered_count, 2);
    assert_eq!(
        gateway
            .ranking_requests()
            .into_iter()
            .map(|request| request.date)
            .collect::<Vec<_>>(),
        vec![
            None,
            ranking_date.previous_day(),
            ranking_date.previous_day().and_then(Date::previous_day),
        ]
    );
}

#[tokio::test]
async fn ranking_max_rank_fetches_every_required_page_and_excludes_lower_ranks() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.set_ranking_date(Date::from_calendar_date(2026, Month::July, 29).unwrap());
    gateway.set_ranking_pages(vec![
        vec![ranking_entry(1311, 1), ranking_entry(1312, 50)],
        vec![
            ranking_entry(1313, 51),
            ranking_entry(1314, 75),
            ranking_entry(1315, 76),
        ],
    ]);
    let account = account(&locked, gateway.clone()).await;
    let service = SubscriptionService::new(locked.db.clone());
    let subscription = service
        .create(SubscriptionMutationRequest {
            account_id: account.id,
            rule_id: None,
            name: "top 75".to_owned(),
            kind: SubscriptionKind::Ranking,
            interval_minutes: 60,
            lookback_pages: 0,
            params: json!({
                "modes": ["daily"],
                "contents": ["all"],
                "max_rank": 75,
            }),
            next_run_at: None,
        })
        .await
        .unwrap();
    let run = service
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();
    let unit = unit_rows(&locked, run.run_id).await.remove(0);

    let result = SubscriptionExecutionService::new(locked.db.clone(), gateway.clone())
        .execute_unit(SubscriptionUnitRequest {
            context: context(),
            unit_id: unit.id,
        })
        .await
        .unwrap();

    assert_eq!(result.discovered_count, 4);
    assert_eq!(work_count(&locked).await, 4);
    assert_eq!(
        gateway
            .ranking_requests()
            .into_iter()
            .map(|request| request.page)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[tokio::test]
async fn following_uses_lookback_and_bookmarks_read_both_visibilities() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.set_follow_items(vec![discovery_work(1351), discovery_work(1351)]);
    gateway.set_bookmark_items(vec![discovery_work(1361), discovery_work(1361)]);
    gateway.set_following_authors(
        vec![PixivFollowedArtist {
            pixiv_id: 100,
            name: "artist 100".to_owned(),
            profile_image_url: None,
        }],
        Vec::new(),
    );
    let account = account(&locked, gateway.clone()).await;
    let service = SubscriptionService::new(locked.db.clone());
    let following =
        configure_following_subscription(&locked.db, account.id, PixivFollowLatestMode::All, 60, 2)
            .await;
    let bookmarks =
        configure_bookmarks_subscription(&locked.db, account.id, PixivBookmarksMode::All, 60, 2)
            .await;
    save_cursor_value(
        &locked,
        following.id,
        "normal",
        "following:following:all",
        json!({ "page": 4 }),
    )
    .await;
    let executor = SubscriptionExecutionService::new(locked.db.clone(), gateway.clone());
    for subscription in [following, bookmarks] {
        let run = service
            .start_manual_run(subscription.id, false)
            .await
            .unwrap();
        let unit = unit_rows(&locked, run.run_id).await.remove(0);
        let result = executor
            .execute_unit(SubscriptionUnitRequest {
                context: context(),
                unit_id: unit.id,
            })
            .await
            .unwrap();
        assert_eq!(result.discovered_count, 1, "{result:#?}");
    }

    assert_eq!(
        gateway
            .follow_requests()
            .into_iter()
            .map(|request| request.page)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(
        gateway
            .bookmark_requests()
            .into_iter()
            .map(|request| (request.visibility, request.offset))
            .collect::<Vec<_>>(),
        vec![
            (PixivBookmarkVisibility::Public, 0),
            (PixivBookmarkVisibility::Private, 0),
        ]
    );
}

#[tokio::test]
async fn following_full_sync_reads_from_the_first_page_until_pixiv_is_empty() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.set_follow_pages(vec![vec![discovery_work(1371)], vec![discovery_work(1372)]]);
    gateway.set_following_authors(
        vec![PixivFollowedArtist {
            pixiv_id: 100,
            name: "artist 100".to_owned(),
            profile_image_url: None,
        }],
        Vec::new(),
    );
    let account = account(&locked, gateway.clone()).await;
    let service = SubscriptionService::new(locked.db.clone());
    let subscription =
        configure_following_subscription(&locked.db, account.id, PixivFollowLatestMode::All, 60, 2)
            .await;
    save_cursor_value(
        &locked,
        subscription.id,
        "normal",
        "following:following:all",
        json!({ "page": 4 }),
    )
    .await;

    let run = service
        .start_manual_run(subscription.id, true)
        .await
        .unwrap();
    let unit = unit_rows(&locked, run.run_id).await.remove(0);
    let result = SubscriptionExecutionService::new(locked.db.clone(), gateway.clone())
        .execute_unit(SubscriptionUnitRequest {
            context: context(),
            unit_id: unit.id,
        })
        .await
        .unwrap();

    assert_eq!(result.discovered_count, 2);
    assert_eq!(
        gateway
            .follow_requests()
            .into_iter()
            .map(|request| request.page)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(
        SubscriptionRepository::new(locked.db.clone())
            .last_successful_backfill_at(subscription.id)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn following_incremental_runs_replace_legacy_page_progress_with_a_recent_window() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.set_follow_items(vec![discovery_work(1381)]);
    gateway.set_following_authors(
        vec![PixivFollowedArtist {
            pixiv_id: 100,
            name: "artist 100".to_owned(),
            profile_image_url: None,
        }],
        Vec::new(),
    );
    let account = account(&locked, gateway.clone()).await;
    let service = SubscriptionService::new(locked.db.clone());
    let subscription =
        configure_following_subscription(&locked.db, account.id, PixivFollowLatestMode::All, 60, 1)
            .await;
    save_cursor_value(
        &locked,
        subscription.id,
        "normal",
        "following:following:all",
        json!({ "page": 619 }),
    )
    .await;
    let executor = SubscriptionExecutionService::new(locked.db.clone(), gateway.clone());
    for _ in 0..2 {
        let run = service
            .start_manual_run(subscription.id, false)
            .await
            .unwrap();
        let unit = unit_rows(&locked, run.run_id).await.remove(0);
        let result = executor
            .execute_unit(SubscriptionUnitRequest {
                context: context(),
                unit_id: unit.id,
            })
            .await
            .unwrap();
        assert_eq!(result.status, SubscriptionRunStatus::Succeeded);
    }

    assert_eq!(
        gateway
            .follow_requests()
            .into_iter()
            .map(|request| request.page)
            .collect::<Vec<_>>(),
        vec![1, 2, 1, 2]
    );
    assert!(
        SubscriptionRepository::new(locked.db.clone())
            .source_cursor(subscription.id, "normal", "following:following:all")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn bookmark_sync_finishes_and_leaves_full_reconcile_pending_after_transient_work_failure() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.set_bookmark_items(vec![discovery_work(1_371)]);
    let account = account(&locked, gateway.clone()).await;
    gateway.fail_work_detail(pixivarchive_pixiv::PixivErrorClass::Network);
    let service = SubscriptionService::new(locked.db.clone());
    let subscription =
        configure_bookmarks_subscription(&locked.db, account.id, PixivBookmarksMode::All, 60, 2)
            .await;
    let run = service
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();
    let unit = unit_rows(&locked, run.run_id).await.remove(0);

    let result = SubscriptionExecutionService::new(locked.db.clone(), gateway)
        .execute_unit(SubscriptionUnitRequest {
            context: context(),
            unit_id: unit.id,
        })
        .await
        .unwrap();

    assert_eq!(result.status, SubscriptionRunStatus::Succeeded);
    assert_eq!(result.discovered_count, 0);
    assert_eq!(result.ignored_count, 1);
    let reconciled: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pixiv_bookmark_sync_state WHERE pixiv_account_id = $1 AND last_full_reconciled_at IS NOT NULL",
    )
    .bind(account.id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(reconciled, 0);
}

#[tokio::test]
async fn subscription_without_rule_queues_every_discovered_work_for_download() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.set_ranking_items(vec![ranking_entry(1391, 1), ranking_entry(1392, 2)]);
    let account = account(&locked, gateway.clone()).await;
    let service = SubscriptionService::new(locked.db.clone());
    let subscription = service
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "unfiltered downloads".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 0,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    let run = service
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();
    let unit = unit_rows(&locked, run.run_id).await.remove(0);

    let result = SubscriptionExecutionService::new(locked.db.clone(), gateway)
        .execute_unit(SubscriptionUnitRequest {
            context: context(),
            unit_id: unit.id,
        })
        .await
        .unwrap();

    assert_eq!(result.discovered_count, 2);
    assert_eq!(queued_downloads(&locked).await, 2);
}

#[tokio::test]
async fn subscription_ignore_rule_writes_no_work_or_ranking_entry() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.set_ranking_items(vec![ranking_entry(1401, 1)]);
    let account = account(&locked, gateway.clone()).await;
    let rule_id = published_default_rule(&locked, RuleAction::Ignore).await;
    let service = SubscriptionService::new(locked.db.clone());
    let subscription = service
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "ignore rule".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 0,
            rule_id: Some(rule_id),
            next_run_at: None,
        })
        .await
        .unwrap();
    let run = service
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();
    let unit = unit_rows(&locked, run.run_id).await.remove(0);

    let result = SubscriptionExecutionService::new(locked.db.clone(), gateway.clone())
        .execute_unit(SubscriptionUnitRequest {
            context: context(),
            unit_id: unit.id,
        })
        .await
        .unwrap();

    assert_eq!(result.discovered_count, 0);
    assert_eq!(result.ignored_count, 1);
    assert_eq!(work_count(&locked).await, 0);
    assert_eq!(ranking_context_count(&locked, run.run_id, 1401).await, 0);
    assert_eq!(gateway.work_detail_calls(), 1);
}

#[tokio::test]
async fn subscription_evaluates_page_metadata_without_downloading_media() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let work_id = 1402;
    gateway.set_ranking_items(vec![ranking_entry(work_id, 1)]);
    let page = work_page(work_id, 0);
    gateway.set_work_pages(pixivarchive_domain::pixiv::PixivWorkPages {
        work_id,
        pages: vec![page],
    });
    let account = account(&locked, gateway.clone()).await;
    let rule_id = published_rule(&locked, page_width_metadata_definition()).await;
    let service = SubscriptionService::new(locked.db.clone());
    let subscription = service
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "page width rule".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 0,
            rule_id: Some(rule_id),
            next_run_at: None,
        })
        .await
        .unwrap();
    let run = service
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();
    let unit = unit_rows(&locked, run.run_id).await.remove(0);

    let result = SubscriptionExecutionService::new(locked.db.clone(), gateway.clone())
        .execute_unit(SubscriptionUnitRequest {
            context: context(),
            unit_id: unit.id,
        })
        .await
        .unwrap();

    assert_eq!(result.discovered_count, 1);
    assert!(gateway.media_requests().is_empty());
    assert_eq!(queued_downloads(&locked).await, 0);
    assert_eq!(work_count(&locked).await, 1);
}

#[tokio::test]
async fn subscription_run_keeps_its_rule_snapshot_after_a_new_version_is_published() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.set_ranking_items(vec![ranking_entry(1501, 1)]);
    let account = account(&locked, gateway.clone()).await;
    let rule_id = published_default_rule(&locked, RuleAction::Download).await;
    let service = SubscriptionService::new(locked.db.clone());
    let subscription = service
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "download rule".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 0,
            rule_id: Some(rule_id),
            next_run_at: None,
        })
        .await
        .unwrap();
    let run = service
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();
    let unit = unit_rows(&locked, run.run_id).await.remove(0);
    let rules = RuleService::new(locked.db.clone());
    let replacement = rules
        .save_draft(SaveRuleDraftRequest {
            rule_id,
            expected_revision: None,
            base_version: Some(1),
            definition: serde_json::to_value(disabled_definition(rule_id, RuleAction::Ignore))
                .unwrap(),
        })
        .await
        .unwrap();
    rules
        .publish_version(PublishRuleVersionRequest {
            rule_id,
            base_version: Some(1),
            expected_draft_revision: replacement.revision,
            created_by: None,
        })
        .await
        .unwrap();

    let result = SubscriptionExecutionService::new(locked.db.clone(), gateway)
        .execute_unit(SubscriptionUnitRequest {
            context: context(),
            unit_id: unit.id,
        })
        .await
        .unwrap();

    assert_eq!(result.discovered_count, 1);
    assert_eq!(work_title(&locked, 1501).await, "work 1501");
    assert_eq!(queued_downloads(&locked).await, 1);
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

#[derive(Clone, Debug)]
struct UnitRow {
    id: uuid::Uuid,
}

async fn unit_rows(locked: &LockedDb, run_id: uuid::Uuid) -> Vec<UnitRow> {
    sqlx::query(
        "SELECT id, source_key FROM subscription_run_unit WHERE subscription_run_id = $1 ORDER BY source_key",
    )
    .bind(run_id)
    .fetch_all(locked.db.pool())
    .await
    .unwrap()
    .into_iter()
    .map(|row| UnitRow { id: row.get("id") })
    .collect()
}

async fn unit_state(locked: &LockedDb, unit_id: uuid::Uuid) -> String {
    sqlx::query_scalar("SELECT state FROM subscription_run_unit WHERE id = $1")
        .bind(unit_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn parent_state(locked: &LockedDb, run_id: uuid::Uuid) -> String {
    sqlx::query_scalar("SELECT state FROM subscription_run WHERE id = $1")
        .bind(run_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn ranking_context_count(locked: &LockedDb, run_id: uuid::Uuid, pixiv_work_id: i64) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM ranking_entry WHERE subscription_run_id = $1 AND pixiv_work_id = $2",
    )
    .bind(run_id)
    .bind(pixiv_work_id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap()
}

async fn save_cursor_value(
    locked: &LockedDb,
    subscription_id: uuid::Uuid,
    cursor_kind: &str,
    source_key: &str,
    cursor_value: serde_json::Value,
) {
    sqlx::query(
        r#"
        INSERT INTO subscription_cursor (id, subscription_id, cursor_kind, source_key, cursor_value)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(uuid::Uuid::now_v7())
    .bind(subscription_id)
    .bind(cursor_kind)
    .bind(source_key)
    .bind(cursor_value)
    .execute(locked.db.pool())
    .await
    .unwrap();
}

async fn cursor_date(
    locked: &LockedDb,
    subscription_id: uuid::Uuid,
    cursor_kind: &str,
    source_key: &str,
) -> Date {
    sqlx::query_scalar(
        "SELECT (cursor_value->>'date')::date FROM subscription_cursor WHERE subscription_id = $1 AND cursor_kind = $2 AND source_key = $3",
    )
    .bind(subscription_id)
    .bind(cursor_kind)
    .bind(source_key)
    .fetch_one(locked.db.pool())
    .await
        .unwrap()
}

async fn published_default_rule(locked: &LockedDb, action: RuleAction) -> uuid::Uuid {
    let service = RuleService::new(locked.db.clone());
    let rule = service.create_rule("subscription", action).await.unwrap();
    let initial_draft = service.load_draft(rule.id).await.unwrap().unwrap();
    let draft = service
        .save_draft(SaveRuleDraftRequest {
            rule_id: rule.id,
            expected_revision: Some(initial_draft.revision),
            base_version: None,
            definition: serde_json::to_value(disabled_definition(rule.id, action)).unwrap(),
        })
        .await
        .unwrap();
    service
        .publish_version(PublishRuleVersionRequest {
            rule_id: rule.id,
            base_version: None,
            expected_draft_revision: draft.revision,
            created_by: None,
        })
        .await
        .unwrap();
    rule.id
}

async fn published_rule(locked: &LockedDb, mut definition: RuleDefinitionV1) -> uuid::Uuid {
    let service = RuleService::new(locked.db.clone());
    let rule = service
        .create_rule(&definition.name, definition.default_action)
        .await
        .unwrap();
    definition.id = rule.id;
    let initial_draft = service.load_draft(rule.id).await.unwrap().unwrap();
    let draft = service
        .save_draft(SaveRuleDraftRequest {
            rule_id: rule.id,
            expected_revision: Some(initial_draft.revision),
            base_version: None,
            definition: serde_json::to_value(definition).unwrap(),
        })
        .await
        .unwrap();
    service
        .publish_version(PublishRuleVersionRequest {
            rule_id: rule.id,
            base_version: None,
            expected_draft_revision: draft.revision,
            created_by: None,
        })
        .await
        .unwrap();
    rule.id
}

fn disabled_definition(rule_id: uuid::Uuid, action: RuleAction) -> RuleDefinitionV1 {
    let mut definition =
        RuleDefinitionV1::match_all(rule_id, "subscription", RuleAction::Download, action);
    definition.enabled = false;
    definition
}

fn page_width_metadata_definition() -> RuleDefinitionV1 {
    RuleDefinitionV1 {
        schema_version: 1,
        id: uuid::Uuid::nil(),
        name: "page width".to_owned(),
        enabled: true,
        group_mode: GroupMode::All,
        groups: vec![ConditionGroup {
            mode: GroupMode::All,
            conditions: vec![Condition {
                field: RuleField::PageWidth,
                operator: RuleOperator::GreaterThan,
                value: Some(ConditionValue::Number(1_000.0)),
                case_sensitive: None,
                tag_scope: None,
                page_quantifier: Some(PageQuantifier::AnyPage),
            }],
        }],
        action: RuleAction::MetadataOnly,
        default_action: RuleAction::Ignore,
    }
}

async fn work_count(locked: &LockedDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM work")
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn work_title(locked: &LockedDb, pixiv_work_id: i64) -> String {
    sqlx::query_scalar(
        r#"
        SELECT work_revision.title
        FROM work
        JOIN work_revision ON work_revision.id = work.current_revision_id
        WHERE work.pixiv_work_id = $1
        "#,
    )
    .bind(pixiv_work_id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap()
}

async fn queued_downloads(locked: &LockedDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM job WHERE kind = 'download_media'")
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}
