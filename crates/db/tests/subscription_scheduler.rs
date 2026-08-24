mod support;

use pixivarchive_db::{
    DbError, FinishSubscriptionRunUnit, JobCompletion, JobRepository, SubscriptionCursorUpdate,
    subscriptions::{
        FinishSubscriptionRun, FinishSubscriptionRunResult, ScheduleDueSubscription,
        ScheduleDueSubscriptionResult, SubscriptionRepository,
    },
};
use pixivarchive_domain::{
    job::{JobPriority, JobQuotaSelection},
    subscription::SubscriptionRunStatus,
};
use serde_json::{Value, json};
use sqlx::Row;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn due_candidates_include_schedule_fields_and_can_create_one_run_job_event() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let now = test_time();
    let subscription_id = insert_subscription(&db, true, now - Duration::minutes(5)).await;
    let next_run_at = now + Duration::hours(1);

    let due = subscriptions.list_due_subscriptions(now, 10).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, subscription_id);
    assert_eq!(due[0].revision, 1);
    assert_eq!(due[0].kind, "ranking");
    assert_eq!(due[0].schedule, json!({ "cron": "0 * * * *" }));
    assert_eq!(due[0].next_run_at, now - Duration::minutes(5));
    assert_eq!(due[0].pixiv_account_id, test_account_id());

    let result = subscriptions
        .schedule_due_subscription(ScheduleDueSubscription {
            subscription_id: due[0].id,
            expected_revision: due[0].revision,
            expected_next_run_at: due[0].next_run_at,
            now,
            next_run_at,
        })
        .await
        .unwrap();
    let ScheduleDueSubscriptionResult::Created(claimed) = result else {
        panic!("due subscription should create a scheduled run");
    };

    assert_eq!(claimed.subscription_id, subscription_id);
    assert_eq!(claimed.trigger_kind, "scheduled");

    let run = run_rows(&db, subscription_id).await;
    assert_eq!(run.len(), 1);
    assert_eq!(run[0].id, claimed.run_id);
    assert_eq!(run[0].job_id, Some(claimed.job_id));
    assert_eq!(run[0].trigger_kind, "scheduled");
    assert_eq!(run[0].state, "queued");

    let job = sqlx::query(
        r#"
        SELECT priority_class, kind, payload, state
        FROM job
        WHERE id = $1
        "#,
    )
    .bind(claimed.job_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(
        job.get::<String, _>("priority_class"),
        "scheduled_collection"
    );
    assert_eq!(job.get::<String, _>("kind"), "ranking_collection");
    assert_eq!(job.get::<String, _>("state"), "queued");
    assert_eq!(
        job.get::<Value, _>("payload")["subscription_run_id"],
        json!(claimed.run_id.to_string())
    );
    assert_eq!(job.get::<Value, _>("payload")["mode"], json!("daily"));
    assert_eq!(job.get::<Value, _>("payload")["content"], json!("all"));
    assert_eq!(job.get::<Value, _>("payload")["page_size"], json!(50));
    assert_eq!(job.get::<Value, _>("payload")["max_rank"], json!(20));

    let subscription = sqlx::query(
        "SELECT pending_run, recent_state, last_run_at, next_run_at, revision FROM subscription WHERE id = $1",
    )
    .bind(subscription_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert!(!subscription.get::<bool, _>("pending_run"));
    assert_eq!(subscription.get::<String, _>("recent_state"), "running");
    assert_eq!(
        subscription.get::<Option<OffsetDateTime>, _>("last_run_at"),
        Some(now)
    );
    assert_eq!(
        subscription.get::<Option<OffsetDateTime>, _>("next_run_at"),
        Some(next_run_at)
    );
    assert_eq!(subscription.get::<i64, _>("revision"), 2);

    let queued_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app_event WHERE resource = 'job' AND resource_id = $1 AND payload->>'type' = 'job_queued'",
    )
    .bind(claimed.job_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(queued_events, 1);
}

#[tokio::test]
async fn stale_candidate_does_not_create_or_merge_work() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let now = test_time();
    let subscription_id = insert_subscription(&db, true, now - Duration::minutes(5)).await;
    let due = subscriptions.list_due_subscriptions(now, 10).await.unwrap();

    let created = subscriptions
        .schedule_due_subscription(ScheduleDueSubscription {
            subscription_id: due[0].id,
            expected_revision: due[0].revision,
            expected_next_run_at: due[0].next_run_at,
            now,
            next_run_at: now + Duration::hours(1),
        })
        .await
        .unwrap();
    assert!(matches!(created, ScheduleDueSubscriptionResult::Created(_)));

    let stale = subscriptions
        .schedule_due_subscription(ScheduleDueSubscription {
            subscription_id: due[0].id,
            expected_revision: due[0].revision,
            expected_next_run_at: due[0].next_run_at,
            now,
            next_run_at: now + Duration::hours(2),
        })
        .await
        .unwrap();
    assert_eq!(stale, ScheduleDueSubscriptionResult::Stale);

    let runs = run_rows(&db, subscription_id).await;
    assert_eq!(runs.len(), 1);
}

#[tokio::test]
async fn active_run_merges_pending_and_advances_next_run_so_later_due_work_can_run() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let now = test_time();
    let first_id = insert_subscription(&db, true, now - Duration::minutes(10)).await;
    let second_id = insert_subscription(&db, true, now + Duration::seconds(1)).await;

    let first_due = subscriptions.list_due_subscriptions(now, 10).await.unwrap();
    assert_eq!(first_due[0].id, first_id);
    let first_next = now + Duration::seconds(1);
    assert!(matches!(
        subscriptions
            .schedule_due_subscription(ScheduleDueSubscription {
                subscription_id: first_due[0].id,
                expected_revision: first_due[0].revision,
                expected_next_run_at: first_due[0].next_run_at,
                now,
                next_run_at: first_next,
            })
            .await
            .unwrap(),
        ScheduleDueSubscriptionResult::Created(_)
    ));

    let due_again = subscriptions
        .list_due_subscriptions(now + Duration::seconds(1), 10)
        .await
        .unwrap();
    assert_eq!(due_again[0].id, first_id);
    let advanced_next = now + Duration::hours(1);
    let merged = subscriptions
        .schedule_due_subscription(ScheduleDueSubscription {
            subscription_id: due_again[0].id,
            expected_revision: due_again[0].revision,
            expected_next_run_at: due_again[0].next_run_at,
            now: now + Duration::seconds(1),
            next_run_at: advanced_next,
        })
        .await
        .unwrap();
    assert_eq!(
        merged,
        ScheduleDueSubscriptionResult::MergedPending {
            subscription_id: first_id
        }
    );

    let later_due = subscriptions
        .list_due_subscriptions(now + Duration::seconds(1), 10)
        .await
        .unwrap();
    assert_eq!(later_due[0].id, second_id);
    assert!(matches!(
        subscriptions
            .schedule_due_subscription(ScheduleDueSubscription {
                subscription_id: later_due[0].id,
                expected_revision: later_due[0].revision,
                expected_next_run_at: later_due[0].next_run_at,
                now: now + Duration::seconds(1),
                next_run_at: now + Duration::hours(2),
            })
            .await
            .unwrap(),
        ScheduleDueSubscriptionResult::Created(_)
    ));

    let first_subscription =
        sqlx::query("SELECT pending_run, next_run_at FROM subscription WHERE id = $1")
            .bind(first_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(first_subscription.get::<bool, _>("pending_run"));
    assert_eq!(
        first_subscription.get::<Option<OffsetDateTime>, _>("next_run_at"),
        Some(advanced_next)
    );
    assert_eq!(run_rows(&db, first_id).await.len(), 1);
    assert_eq!(run_rows(&db, second_id).await.len(), 1);
}

#[tokio::test]
async fn repeated_due_triggers_keep_one_pending_run_while_advancing_schedule() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let now = test_time();
    let subscription_id = insert_subscription(&db, true, now - Duration::minutes(5)).await;
    let first_due = subscriptions.list_due_subscriptions(now, 10).await.unwrap();

    assert!(matches!(
        subscriptions
            .schedule_due_subscription(ScheduleDueSubscription {
                subscription_id,
                expected_revision: first_due[0].revision,
                expected_next_run_at: first_due[0].next_run_at,
                now,
                next_run_at: now + Duration::seconds(1),
            })
            .await
            .unwrap(),
        ScheduleDueSubscriptionResult::Created(_)
    ));

    let second_due = subscriptions
        .list_due_subscriptions(now + Duration::seconds(1), 10)
        .await
        .unwrap();
    assert_eq!(
        subscriptions
            .schedule_due_subscription(ScheduleDueSubscription {
                subscription_id,
                expected_revision: second_due[0].revision,
                expected_next_run_at: second_due[0].next_run_at,
                now: now + Duration::seconds(1),
                next_run_at: now + Duration::seconds(2),
            })
            .await
            .unwrap(),
        ScheduleDueSubscriptionResult::MergedPending { subscription_id }
    );

    let third_due = subscriptions
        .list_due_subscriptions(now + Duration::seconds(2), 10)
        .await
        .unwrap();
    assert_eq!(
        subscriptions
            .schedule_due_subscription(ScheduleDueSubscription {
                subscription_id,
                expected_revision: third_due[0].revision,
                expected_next_run_at: third_due[0].next_run_at,
                now: now + Duration::seconds(2),
                next_run_at: now + Duration::hours(3),
            })
            .await
            .unwrap(),
        ScheduleDueSubscriptionResult::MergedPending { subscription_id }
    );

    let subscription =
        sqlx::query("SELECT pending_run, next_run_at FROM subscription WHERE id = $1")
            .bind(subscription_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(subscription.get::<bool, _>("pending_run"));
    assert_eq!(
        subscription.get::<Option<OffsetDateTime>, _>("next_run_at"),
        Some(now + Duration::hours(3))
    );
    assert_eq!(run_rows(&db, subscription_id).await.len(), 1);
}

#[tokio::test]
async fn finishing_pending_run_creates_one_merged_pending_run() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let now = test_time();
    let subscription_id = insert_subscription(&db, true, now - Duration::minutes(5)).await;
    let due = subscriptions.list_due_subscriptions(now, 10).await.unwrap();
    let ScheduleDueSubscriptionResult::Created(first) = subscriptions
        .schedule_due_subscription(ScheduleDueSubscription {
            subscription_id,
            expected_revision: due[0].revision,
            expected_next_run_at: due[0].next_run_at,
            now,
            next_run_at: now + Duration::seconds(1),
        })
        .await
        .unwrap()
    else {
        panic!("initial due subscription should create a run");
    };
    let due_again = subscriptions
        .list_due_subscriptions(now + Duration::seconds(1), 10)
        .await
        .unwrap();
    subscriptions
        .schedule_due_subscription(ScheduleDueSubscription {
            subscription_id,
            expected_revision: due_again[0].revision,
            expected_next_run_at: due_again[0].next_run_at,
            now: now + Duration::seconds(1),
            next_run_at: now + Duration::hours(2),
        })
        .await
        .unwrap();
    mark_run_running(&db, first.run_id).await;

    let result = subscriptions
        .finish_run(FinishSubscriptionRun {
            run_id: first.run_id,
            state: SubscriptionRunStatus::Succeeded,
            finished_at: now + Duration::minutes(10),
            discovered_count: 7,
            ignored_count: 2,
            error_class: None,
            trace_id: None,
        })
        .await
        .unwrap();
    let FinishSubscriptionRunResult::MergedPending(merged) = result else {
        panic!("pending flag should produce a merged run");
    };

    assert_eq!(merged.subscription_id, subscription_id);
    assert_eq!(merged.trigger_kind, "merged_pending");

    let runs = run_rows(&db, subscription_id).await;
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].id, first.run_id);
    assert_eq!(runs[0].state, "succeeded");
    assert_eq!(runs[0].discovered_count, 7);
    assert_eq!(runs[0].ignored_count, 2);
    assert_eq!(runs[1].id, merged.run_id);
    assert_eq!(runs[1].trigger_kind, "merged_pending");
    assert_eq!(runs[1].state, "queued");

    assert!(
        subscriptions
            .finish_run(FinishSubscriptionRun {
                run_id: first.run_id,
                state: SubscriptionRunStatus::Succeeded,
                finished_at: now + Duration::minutes(11),
                discovered_count: 7,
                ignored_count: 2,
                error_class: None,
                trace_id: None,
            })
            .await
            .is_err()
    );

    let subscription =
        sqlx::query("SELECT pending_run, recent_state FROM subscription WHERE id = $1")
            .bind(subscription_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(!subscription.get::<bool, _>("pending_run"));
    assert_eq!(subscription.get::<String, _>("recent_state"), "running");
}

#[tokio::test]
async fn failed_run_without_pending_sets_failed_recent_state() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let now = test_time();
    let subscription_id = insert_subscription(&db, true, now - Duration::minutes(5)).await;
    let due = subscriptions.list_due_subscriptions(now, 10).await.unwrap();
    let ScheduleDueSubscriptionResult::Created(run) = subscriptions
        .schedule_due_subscription(ScheduleDueSubscription {
            subscription_id,
            expected_revision: due[0].revision,
            expected_next_run_at: due[0].next_run_at,
            now,
            next_run_at: now + Duration::hours(1),
        })
        .await
        .unwrap()
    else {
        panic!("due subscription should create a run");
    };
    mark_run_running(&db, run.run_id).await;

    let result = subscriptions
        .finish_run(FinishSubscriptionRun {
            run_id: run.run_id,
            state: SubscriptionRunStatus::Failed,
            finished_at: now + Duration::minutes(5),
            discovered_count: 0,
            ignored_count: 0,
            error_class: Some("network".to_owned()),
            trace_id: None,
        })
        .await
        .unwrap();
    assert_eq!(result, FinishSubscriptionRunResult::Completed);

    let recent_state: String =
        sqlx::query_scalar("SELECT recent_state FROM subscription WHERE id = $1")
            .bind(subscription_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(recent_state, "failed");
}

#[tokio::test]
async fn disabled_and_future_subscriptions_are_not_listed_or_scheduled() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let now = test_time();
    let disabled = insert_subscription(&db, false, now - Duration::minutes(5)).await;
    let future = insert_subscription(&db, true, now + Duration::minutes(5)).await;

    assert!(
        subscriptions
            .list_due_subscriptions(now, 10)
            .await
            .unwrap()
            .is_empty()
    );

    let stale = subscriptions
        .schedule_due_subscription(ScheduleDueSubscription {
            subscription_id: disabled,
            expected_revision: 1,
            expected_next_run_at: now - Duration::minutes(5),
            now,
            next_run_at: now + Duration::hours(1),
        })
        .await
        .unwrap();
    assert_eq!(stale, ScheduleDueSubscriptionResult::Stale);

    let unchanged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM subscription WHERE id = ANY($1) AND recent_state = 'never_run' AND pending_run = false",
    )
    .bind(vec![disabled, future])
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(unchanged, 2);
}

#[tokio::test]
async fn concurrent_schedulers_do_not_duplicate_subscription_runs() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let now = test_time();
    let subscription_id = insert_subscription(&db, true, now - Duration::minutes(5)).await;
    let due = subscriptions.list_due_subscriptions(now, 10).await.unwrap();
    assert_eq!(due.len(), 1);

    let first = subscriptions.clone();
    let second = subscriptions.clone();
    let left = ScheduleDueSubscription {
        subscription_id,
        expected_revision: due[0].revision,
        expected_next_run_at: due[0].next_run_at,
        now,
        next_run_at: now + Duration::hours(1),
    };
    let right = left.clone();
    let (a, b) = tokio::join!(
        first.schedule_due_subscription(left),
        second.schedule_due_subscription(right),
    );

    let results = [a.unwrap(), b.unwrap()];
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, ScheduleDueSubscriptionResult::Created(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, ScheduleDueSubscriptionResult::Stale))
            .count(),
        1
    );

    let runs = run_rows(&db, subscription_id).await;
    assert_eq!(runs.len(), 1);

    let jobs: i64 = sqlx::query_scalar("SELECT count(*) FROM job")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(jobs, 1);
}

#[tokio::test]
async fn retrying_a_failed_subscription_job_requeues_its_unit_and_parent_run() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let jobs = JobRepository::new(db.clone());
    let subscription_id = insert_subscription(&db, true, test_time()).await;
    let run = subscriptions
        .start_manual_run(subscription_id, false)
        .await
        .unwrap();
    let unit = subscriptions
        .list_units_for_run(run.run_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ScheduledCollection]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    subscriptions.mark_unit_running(unit.id).await.unwrap();
    subscriptions
        .record_unit_attempt_failure(unit.id, "permanent", Some("invalid response"))
        .await
        .unwrap();
    jobs.fail(
        claimed.lease(),
        "permanent",
        false,
        None,
        Some("invalid response"),
    )
    .await
    .unwrap();

    let linked_state: String =
        sqlx::query_scalar("SELECT state FROM subscription_run_unit WHERE id = $1")
            .bind(unit.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(linked_state, "failed");

    let failed = jobs.get(claimed.id).await.unwrap();
    jobs.retry_requested(failed.id, failed.resource_revision)
        .await
        .unwrap();

    let state = sqlx::query(
        r#"
        SELECT j.state AS job_state,
               u.state AS unit_state,
               u.error_class,
               u.error_message,
               u.started_at,
               u.finished_at,
               sr.state AS run_state,
               sr.finished_at AS run_finished_at,
               s.recent_state
        FROM job j
        JOIN subscription_run_unit u ON u.job_id = j.id
        JOIN subscription_run sr ON sr.id = u.subscription_run_id
        JOIN subscription s ON s.id = sr.subscription_id
        WHERE j.id = $1
        "#,
    )
    .bind(claimed.id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(state.get::<String, _>("job_state"), "queued");
    assert_eq!(state.get::<String, _>("unit_state"), "queued");
    assert!(state.get::<Option<String>, _>("error_class").is_none());
    assert!(state.get::<Option<String>, _>("error_message").is_none());
    assert!(
        state
            .get::<Option<OffsetDateTime>, _>("started_at")
            .is_none()
    );
    assert!(
        state
            .get::<Option<OffsetDateTime>, _>("finished_at")
            .is_none()
    );
    assert_eq!(state.get::<String, _>("run_state"), "queued");
    assert!(
        state
            .get::<Option<OffsetDateTime>, _>("run_finished_at")
            .is_none()
    );
    assert_eq!(state.get::<String, _>("recent_state"), "running");
}

#[tokio::test]
async fn first_manual_run_updates_subscription_state_without_shifting_periodic_schedule() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let next_run_at = test_time() + Duration::hours(6);
    let subscription_id = insert_subscription(&db, true, next_run_at).await;
    let before = OffsetDateTime::now_utc();

    subscriptions
        .start_manual_run(subscription_id, false)
        .await
        .unwrap();

    let saved = sqlx::query(
        r#"
        SELECT recent_state, last_run_at, next_run_at, revision
        FROM subscription
        WHERE id = $1
        "#,
    )
    .bind(subscription_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(saved.get::<String, _>("recent_state"), "running");
    let last_run_at = saved
        .get::<Option<OffsetDateTime>, _>("last_run_at")
        .unwrap();
    assert!(last_run_at >= before && last_run_at <= OffsetDateTime::now_utc());
    assert_eq!(
        saved.get::<Option<OffsetDateTime>, _>("next_run_at"),
        Some(next_run_at)
    );
    assert_eq!(saved.get::<i64, _>("revision"), 2);

    let payload: Value = sqlx::query_scalar(
        r#"
        SELECT payload
        FROM app_event
        WHERE resource = 'subscription'
          AND resource_id = $1
        ORDER BY id DESC
        LIMIT 1
        "#,
    )
    .bind(subscription_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(
        payload,
        json!({ "type": "subscription_changed", "revision": 2 })
    );
}

#[tokio::test]
async fn bookmark_configuration_rolls_back_when_the_initial_run_cannot_be_created() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let now = test_time();
    let initial = subscriptions
        .ensure_bookmarks_subscription(test_account_id(), now)
        .await
        .unwrap();
    sqlx::query("UPDATE subscription SET params = $2 WHERE id = $1")
        .bind(initial.id)
        .bind(json!({ "mode": "all" }))
        .execute(db.pool())
        .await
        .unwrap();
    let before = subscriptions.subscription(initial.id).await.unwrap();
    let run_count_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM subscription_run WHERE subscription_id = $1")
            .bind(initial.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    let job_count_before: i64 = sqlx::query_scalar("SELECT count(*) FROM job")
        .fetch_one(db.pool())
        .await
        .unwrap();
    let event_count_before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app_event WHERE resource = 'subscription' AND resource_id = $1",
    )
    .bind(initial.id)
    .fetch_one(db.pool())
    .await
    .unwrap();

    let error = subscriptions
        .configure_bookmarks_subscription(
            test_account_id(),
            before.revision,
            true,
            60,
            now,
            Some(JobPriority::ScheduledCollection),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, DbError::InvalidValue(_)));

    let after = subscriptions.subscription(initial.id).await.unwrap();
    assert_eq!(after, before);
    let run_count_after: i64 =
        sqlx::query_scalar("SELECT count(*) FROM subscription_run WHERE subscription_id = $1")
            .bind(initial.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    let job_count_after: i64 = sqlx::query_scalar("SELECT count(*) FROM job")
        .fetch_one(db.pool())
        .await
        .unwrap();
    let event_count_after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app_event WHERE resource = 'subscription' AND resource_id = $1",
    )
    .bind(initial.id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(run_count_after, run_count_before);
    assert_eq!(job_count_after, job_count_before);
    assert_eq!(event_count_after, event_count_before);
}

#[tokio::test]
async fn stale_subscription_job_lease_cannot_record_retryable_unit_failure() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let jobs = JobRepository::new(db.clone());
    let subscription_id = insert_subscription(&db, true, test_time()).await;
    let run = subscriptions
        .start_manual_run(subscription_id, false)
        .await
        .unwrap();
    let unit = subscriptions
        .list_units_for_run(run.run_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ScheduledCollection]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    subscriptions
        .mark_unit_running_job(claimed.lease(), unit.id)
        .await
        .unwrap();
    sqlx::query("UPDATE job SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(claimed.id)
        .execute(db.pool())
        .await
        .unwrap();
    let reclaimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ScheduledCollection]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reclaimed.id, claimed.id);

    let stale_failure = subscriptions
        .record_unit_attempt_failure_job(
            claimed.lease(),
            unit.id,
            "network",
            Some("temporary failure"),
        )
        .await;

    assert!(matches!(stale_failure, Err(DbError::LeaseConflict)));
    let state = sqlx::query(
        "SELECT state, error_class, error_message FROM subscription_run_unit WHERE id = $1",
    )
    .bind(unit.id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(state.get::<String, _>("state"), "running");
    assert!(state.get::<Option<String>, _>("error_class").is_none());
    assert!(state.get::<Option<String>, _>("error_message").is_none());
}

#[tokio::test]
async fn successful_subscription_completion_updates_job_unit_and_cursor_atomically() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let jobs = JobRepository::new(db.clone());
    let subscription_id = insert_subscription(&db, true, test_time()).await;
    let run = subscriptions
        .start_manual_run(subscription_id, false)
        .await
        .unwrap();
    let unit = subscriptions
        .list_units_for_run(run.run_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ScheduledCollection]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    subscriptions.mark_unit_running(unit.id).await.unwrap();

    jobs.complete(
        claimed.lease(),
        JobCompletion::Subscription(FinishSubscriptionRunUnit {
            unit_id: unit.id,
            state: SubscriptionRunStatus::Succeeded,
            discovered_count: 3,
            ignored_count: 1,
            error_class: None,
            error_message: None,
            cursor_kind: unit.cursor_kind.clone(),
            source_key: unit.source_key.clone(),
            cursor_update: SubscriptionCursorUpdate::Set(json!({"page": 2})),
        }),
    )
    .await
    .unwrap();

    let state = sqlx::query(
        r#"
        SELECT j.state AS job_state,
               u.state AS unit_state,
               sr.state AS run_state,
               cursor.cursor_value
        FROM job j
        JOIN subscription_run_unit u ON u.job_id = j.id
        JOIN subscription_run sr ON sr.id = u.subscription_run_id
        LEFT JOIN subscription_cursor cursor
          ON cursor.subscription_id = sr.subscription_id
         AND cursor.cursor_kind = u.cursor_kind
         AND cursor.source_key = u.source_key
        WHERE j.id = $1
        "#,
    )
    .bind(claimed.id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(state.get::<String, _>("job_state"), "completed");
    assert_eq!(state.get::<String, _>("unit_state"), "succeeded");
    assert_eq!(state.get::<String, _>("run_state"), "succeeded");
    assert_eq!(
        state.get::<sqlx::types::Json<Value>, _>("cursor_value").0,
        json!({"page": 2})
    );
}

#[tokio::test]
async fn cancelling_a_running_subscription_job_finishes_the_run_and_keeps_merged_work() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let jobs = JobRepository::new(db.clone());
    let subscription_id = insert_subscription(&db, true, test_time()).await;
    let run = subscriptions
        .start_manual_run(subscription_id, false)
        .await
        .unwrap();
    let unit = subscriptions
        .list_units_for_run(run.run_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ScheduledCollection]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    subscriptions.mark_unit_running(unit.id).await.unwrap();
    subscriptions
        .start_manual_run(subscription_id, true)
        .await
        .unwrap();

    jobs.cancel_requested(claimed.id, claimed.resource_revision)
        .await
        .unwrap();
    let completion = jobs
        .complete(
            claimed.lease(),
            JobCompletion::Subscription(FinishSubscriptionRunUnit {
                unit_id: unit.id,
                state: SubscriptionRunStatus::Succeeded,
                discovered_count: 1,
                ignored_count: 0,
                error_class: None,
                error_message: None,
                cursor_kind: unit.cursor_kind.clone(),
                source_key: unit.source_key.clone(),
                cursor_update: SubscriptionCursorUpdate::Set(json!({"page": 2})),
            }),
        )
        .await;
    assert!(matches!(completion, Err(DbError::LeaseConflict)));

    let unit_state: String =
        sqlx::query_scalar("SELECT state FROM subscription_run_unit WHERE id = $1")
            .bind(unit.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    let run_state: String = sqlx::query_scalar("SELECT state FROM subscription_run WHERE id = $1")
        .bind(run.run_id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    let subscription = sqlx::query(
        "SELECT pending_run, pending_cursor_kind, recent_state FROM subscription WHERE id = $1",
    )
    .bind(subscription_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    let merged = sqlx::query(
        "SELECT trigger_kind, cursor_kind, state FROM subscription_run WHERE subscription_id = $1 AND id <> $2",
    )
    .bind(subscription_id)
    .bind(run.run_id)
    .fetch_one(db.pool())
    .await
    .unwrap();

    assert_eq!(unit_state, "cancelled");
    assert_eq!(run_state, "cancelled");
    assert!(!subscription.get::<bool, _>("pending_run"));
    assert_eq!(
        subscription.get::<String, _>("pending_cursor_kind"),
        "normal"
    );
    assert_eq!(subscription.get::<String, _>("recent_state"), "running");
    assert_eq!(merged.get::<String, _>("trigger_kind"), "merged_pending");
    assert_eq!(merged.get::<String, _>("cursor_kind"), "backfill");
    assert_eq!(merged.get::<String, _>("state"), "queued");
    let cursor_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM subscription_cursor WHERE subscription_id = $1")
            .bind(subscription_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(cursor_count, 0);
}

#[tokio::test]
async fn stopping_a_subscription_run_cancels_pending_work_without_creating_a_continuation() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    reset_subscriptions(&db).await;
    let subscriptions = SubscriptionRepository::new(db.clone());
    let jobs = JobRepository::new(db.clone());
    let subscription_id = insert_subscription(&db, true, test_time()).await;

    let run = subscriptions
        .start_manual_run(subscription_id, false)
        .await
        .unwrap();
    let unit = subscriptions
        .list_units_for_run(run.run_id)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let _claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ScheduledCollection]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    subscriptions.mark_unit_running(unit.id).await.unwrap();
    subscriptions
        .start_manual_run(subscription_id, true)
        .await
        .unwrap();

    subscriptions
        .stop_active_run(subscription_id)
        .await
        .unwrap();

    let active_runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM subscription_run WHERE subscription_id = $1 AND state IN ('queued', 'running')",
    )
    .bind(subscription_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    let state = sqlx::query(
        r#"
        SELECT j.state AS job_state,
               u.state AS unit_state,
               sr.state AS run_state,
               s.pending_run,
               s.pending_cursor_kind,
               s.recent_state
        FROM subscription_run sr
        JOIN subscription s ON s.id = sr.subscription_id
        JOIN subscription_run_unit u ON u.subscription_run_id = sr.id
        JOIN job j ON j.id = u.job_id
        WHERE sr.id = $1
        "#,
    )
    .bind(run.run_id)
    .fetch_one(db.pool())
    .await
    .unwrap();

    assert_eq!(active_runs, 0);
    assert_eq!(state.get::<String, _>("job_state"), "cancelled");
    assert_eq!(state.get::<String, _>("unit_state"), "cancelled");
    assert_eq!(state.get::<String, _>("run_state"), "cancelled");
    assert!(!state.get::<bool, _>("pending_run"));
    assert_eq!(state.get::<String, _>("pending_cursor_kind"), "normal");
    assert_eq!(state.get::<String, _>("recent_state"), "paused");
}

async fn reset_subscriptions(db: &pixivarchive_db::Db) {
    sqlx::query(
        r#"
        TRUNCATE TABLE
            bookmark_writeback_command,
            import_candidate,
            import_run,
            subscription_run,
            subscription_cursor,
            ranking_entry,
            subscription_run_unit,
            subscription,
            pixiv_account,
            job,
            app_event
        RESTART IDENTITY CASCADE
        "#,
    )
    .execute(db.pool())
    .await
    .unwrap();

    sqlx::query(
        r#"
        INSERT INTO pixiv_account (
            id, pixiv_user_id, display_name, state, cookie_key_id, cookie_nonce, cookie_ciphertext
        )
        VALUES ($1, 10001, 'scheduler test', 'normal', 'test', decode('000000000000000000000000', 'hex'), decode('01', 'hex'))
        "#,
    )
    .bind(test_account_id())
    .execute(db.pool())
    .await
    .unwrap();
}

async fn insert_subscription(
    db: &pixivarchive_db::Db,
    enabled: bool,
    next_run_at: OffsetDateTime,
) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO subscription (
            id,
            pixiv_account_id,
            name,
            kind,
            enabled,
            schedule,
            params,
            next_run_at
        )
        VALUES ($1, $2, $3, 'ranking', $4, $5, $6, $7)
        "#,
    )
    .bind(id)
    .bind(test_account_id())
    .bind(format!("subscription-{id}"))
    .bind(enabled)
    .bind(json!({ "cron": "0 * * * *" }))
    .bind(json!({ "modes": ["daily"], "contents": ["all"] }))
    .bind(next_run_at)
    .execute(db.pool())
    .await
    .unwrap();
    id
}

fn test_account_id() -> Uuid {
    Uuid::from_u128(1)
}

fn test_time() -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(OffsetDateTime::now_utc().unix_timestamp()).unwrap()
}

#[derive(Debug)]
struct RunRow {
    id: Uuid,
    job_id: Option<Uuid>,
    trigger_kind: String,
    state: String,
    discovered_count: i32,
    ignored_count: i32,
}

async fn run_rows(db: &pixivarchive_db::Db, subscription_id: Uuid) -> Vec<RunRow> {
    let rows = sqlx::query(
        r#"
        SELECT id, job_id, trigger_kind, state, discovered_count, ignored_count
        FROM subscription_run
        WHERE subscription_id = $1
        ORDER BY created_at, id
        "#,
    )
    .bind(subscription_id)
    .fetch_all(db.pool())
    .await
    .unwrap();

    rows.into_iter()
        .map(|row| RunRow {
            id: row.get("id"),
            job_id: row.get("job_id"),
            trigger_kind: row.get("trigger_kind"),
            state: row.get("state"),
            discovered_count: row.get("discovered_count"),
            ignored_count: row.get("ignored_count"),
        })
        .collect()
}

async fn mark_run_running(db: &pixivarchive_db::Db, run_id: Uuid) {
    sqlx::query("UPDATE subscription_run SET state = 'running', started_at = now() WHERE id = $1")
        .bind(run_id)
        .execute(db.pool())
        .await
        .unwrap();
}
