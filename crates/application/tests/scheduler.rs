use pixivarchive_application::jobs::{JobService, QueueQuotaWeights, RetryDecision, RetryPolicy};
use pixivarchive_db::Db;
use pixivarchive_domain::{
    event::EventPayload,
    job::{JobErrorClass, JobKind, JobPriority, JobPriorityPolicy, JobQuotaSelection, NewJob},
};
use pixivarchive_test_support::LockedDb;
use serde_json::json;
use sqlx::Row;
use time::Duration;
use uuid::Uuid;

const SCHEDULER_DB_LOCK_ID: i64 = 709020005;

#[tokio::test]
async fn default_queue_rotation_claims_manual_import_before_immediate_work() {
    let locked = LockedDb::new(SCHEDULER_DB_LOCK_ID).await;
    let service = JobService::new(locked.db.clone());
    let immediate_job = service
        .enqueue(NewJob::for_kind(
            JobPriority::Immediate,
            JobKind::GenerateDerivative,
            json!({}),
        ))
        .await
        .unwrap();
    let manual_job = service
        .enqueue(NewJob::for_kind(
            JobPriority::ManualImport,
            JobKind::ImportWork,
            json!({ "pixiv_id": 1 }),
        ))
        .await
        .unwrap();
    let mut rotation = QueueQuotaWeights::default().rotation();

    let claimed = service
        .claim(
            Uuid::now_v7(),
            &rotation.next_selection(),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(claimed.id, manual_job);
    assert_ne!(claimed.id, immediate_job);
}

#[tokio::test]
async fn job_service_stops_after_configured_retry_delays_are_used() {
    let locked = LockedDb::new(SCHEDULER_DB_LOCK_ID).await;
    let service = JobService::with_retry_policy(
        locked.db.clone(),
        RetryPolicy::new(vec![Duration::seconds(1), Duration::seconds(2)]).unwrap(),
    );
    let job_id = service
        .enqueue(NewJob::for_kind(
            JobPriority::Immediate,
            JobKind::ImportWork,
            json!({ "pixiv_id": 1 }),
        ))
        .await
        .unwrap();

    let first = claim(&service).await;
    assert_eq!(
        service
            .fail(&first, JobErrorClass::Network, None, None)
            .await
            .unwrap(),
        RetryDecision::RetryAfter(Duration::seconds(1))
    );
    make_retry_due(&locked.db, job_id).await;

    let second = claim(&service).await;
    assert_eq!(
        service
            .fail(&second, JobErrorClass::Server, None, None)
            .await
            .unwrap(),
        RetryDecision::RetryAfter(Duration::seconds(2))
    );
    make_retry_due(&locked.db, job_id).await;

    let third = claim(&service).await;
    assert_eq!(
        service
            .fail(&third, JobErrorClass::Network, None, None)
            .await
            .unwrap(),
        RetryDecision::DoNotRetry
    );

    let row = sqlx::query(
        "SELECT state, priority_class, retryable, retryable_failure_count, next_retry_at FROM job WHERE id = $1",
    )
    .bind(job_id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("state"), "failed");
    assert_eq!(row.get::<String, _>("priority_class"), "immediate");
    assert_eq!(row.get::<Option<bool>, _>("retryable"), Some(false));
    assert_eq!(row.get::<i32, _>("retryable_failure_count"), 2);
    assert!(
        row.get::<Option<time::OffsetDateTime>, _>("next_retry_at")
            .is_none()
    );
}

#[tokio::test]
async fn rate_limit_uses_retry_after_from_service() {
    let locked = LockedDb::new(SCHEDULER_DB_LOCK_ID).await;
    let service = JobService::new(locked.db.clone());
    service
        .enqueue(NewJob::for_kind(
            JobPriority::Immediate,
            JobKind::ImportWork,
            json!({ "pixiv_id": 2 }),
        ))
        .await
        .unwrap();
    let claimed = claim(&service).await;

    let decision = service
        .fail(
            &claimed,
            JobErrorClass::RateLimit,
            Some(Duration::seconds(42)),
            None,
        )
        .await
        .unwrap();

    assert_eq!(decision, RetryDecision::RetryAfter(Duration::seconds(42)));
}

#[tokio::test]
async fn credential_invalid_blocks_account_jobs_without_consuming_retry_count_and_recovery_is_idempotent()
 {
    let locked = LockedDb::new(SCHEDULER_DB_LOCK_ID).await;
    let service = JobService::new(locked.db.clone());
    let account_id = insert_pixiv_account(&locked.db).await;
    let subscription_id = insert_subscription(&locked.db, account_id).await;
    let mut first_job = NewJob::for_kind(
        JobPriority::ScheduledCollection,
        JobKind::ScheduledCollection,
        json!({ "subscription_id": subscription_id }),
    );
    first_job.pixiv_account_id = Some(account_id);
    let first_job_id = service.enqueue(first_job).await.unwrap();
    let mut second_job = NewJob::for_kind(
        JobPriority::ScheduledCollection,
        JobKind::ImportWork,
        json!({ "pixiv_id": 3 }),
    );
    second_job.pixiv_account_id = Some(account_id);
    let second_job_id = service.enqueue(second_job).await.unwrap();

    let claimed = claim(&service).await;
    assert_eq!(claimed.id, first_job_id);
    assert_eq!(
        service
            .fail(&claimed, JobErrorClass::CredentialInvalid, None, None)
            .await
            .unwrap(),
        RetryDecision::BlockAccount
    );

    for job_id in [first_job_id, second_job_id] {
        let row = sqlx::query("SELECT state, retryable_failure_count FROM job WHERE id = $1")
            .bind(job_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
        assert_eq!(row.get::<String, _>("state"), "waiting_account");
        assert_eq!(row.get::<i32, _>("retryable_failure_count"), 0);
    }
    assert!(
        service
            .claim(
                Uuid::now_v7(),
                &JobQuotaSelection::new(vec![JobPriority::ScheduledCollection]),
                Duration::minutes(5),
            )
            .await
            .unwrap()
            .is_none()
    );

    let priorities = JobPriorityPolicy::default();
    let left = service.recover_account(account_id, &priorities);
    let right = service.recover_account(account_id, &priorities);
    let (left, right) = tokio::join!(left, right);
    left.unwrap();
    right.unwrap();

    let released: i64 =
        sqlx::query_scalar("SELECT count(*) FROM job WHERE id = ANY($1) AND state = 'queued'")
            .bind(vec![first_job_id, second_job_id])
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(released, 2);
    let active_runs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM subscription_run WHERE subscription_id = $1 AND state IN ('queued', 'running')",
    )
    .bind(subscription_id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(active_runs, 1);
    let pending_run: bool =
        sqlx::query_scalar("SELECT pending_run FROM subscription WHERE id = $1")
            .bind(subscription_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert!(!pending_run);
}

#[tokio::test]
async fn account_block_and_recovery_emit_replayable_events_for_every_waiting_job_transition() {
    let locked = LockedDb::new(SCHEDULER_DB_LOCK_ID).await;
    let service = JobService::new(locked.db.clone());
    let account_id = insert_pixiv_account(&locked.db).await;
    let subscription_id = insert_subscription(&locked.db, account_id).await;
    let mut jobs = Vec::new();
    for pixiv_id in [10, 11, 12] {
        let mut job = NewJob::for_kind(
            JobPriority::ScheduledCollection,
            JobKind::ImportWork,
            json!({ "pixiv_id": pixiv_id, "subscription_id": subscription_id }),
        );
        job.pixiv_account_id = Some(account_id);
        jobs.push(service.enqueue(job).await.unwrap());
    }

    let claimed = claim(&service).await;
    service
        .fail(&claimed, JobErrorClass::CredentialInvalid, None, None)
        .await
        .unwrap();
    service
        .recover_account(account_id, &JobPriorityPolicy::default())
        .await
        .unwrap();

    for job_id in jobs {
        let events = sqlx::query(
            "SELECT payload FROM app_event WHERE resource = 'job' AND resource_id = $1 ORDER BY id",
        )
        .bind(job_id)
        .fetch_all(locked.db.pool())
        .await
        .unwrap();
        assert!(events.iter().any(|row| {
            matches!(
                serde_json::from_value::<EventPayload>(row.get("payload")).unwrap(),
                EventPayload::JobWaitingAccount { .. }
            )
        }));
        assert!(events.iter().any(|row| {
            matches!(
                serde_json::from_value::<EventPayload>(row.get("payload")).unwrap(),
                EventPayload::JobReleasedFromAccountWait { .. }
            )
        }));
    }
}

#[tokio::test]
async fn concurrent_account_recovery_creates_one_catchup_per_enabled_subscription() {
    let locked = LockedDb::new(SCHEDULER_DB_LOCK_ID).await;
    let service = JobService::new(locked.db.clone());
    let account_id = insert_pixiv_account(&locked.db).await;
    let first_subscription = insert_subscription(&locked.db, account_id).await;
    let second_subscription = insert_subscription(&locked.db, account_id).await;
    let mut job = NewJob::for_kind(
        JobPriority::ScheduledCollection,
        JobKind::ImportWork,
        json!({ "pixiv_id": 30 }),
    );
    job.pixiv_account_id = Some(account_id);
    service.enqueue(job).await.unwrap();
    let claimed = claim(&service).await;
    service
        .fail(&claimed, JobErrorClass::CredentialInvalid, None, None)
        .await
        .unwrap();

    let priorities = JobPriorityPolicy::default();
    let left = service.recover_account(account_id, &priorities);
    let right = service.recover_account(account_id, &priorities);
    let (left, right) = tokio::join!(left, right);
    left.unwrap();
    right.unwrap();

    for subscription_id in [first_subscription, second_subscription] {
        let active_runs: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM subscription_run WHERE subscription_id = $1 AND state IN ('queued', 'running')",
        )
        .bind(subscription_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
        assert_eq!(active_runs, 1);
        let pending_run: bool =
            sqlx::query_scalar("SELECT pending_run FROM subscription WHERE id = $1")
                .bind(subscription_id)
                .fetch_one(locked.db.pool())
                .await
                .unwrap();
        assert!(!pending_run);
    }
}

#[tokio::test]
async fn account_recovery_upgrades_an_active_subscription_to_pending_backfill() {
    let locked = LockedDb::new(SCHEDULER_DB_LOCK_ID).await;
    let service = JobService::new(locked.db.clone());
    let account_id = insert_pixiv_account(&locked.db).await;
    let subscription_id = insert_subscription(&locked.db, account_id).await;
    sqlx::query(
        r#"
        INSERT INTO subscription_run (
            id, subscription_id, trigger_kind, cursor_kind, params_snapshot, state
        )
        VALUES ($1, $2, 'scheduled', 'normal', '{}', 'running')
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(subscription_id)
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE subscription SET pending_run = true, pending_cursor_kind = 'normal' WHERE id = $1",
    )
    .bind(subscription_id)
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE pixiv_account SET state = 'credential_invalid' WHERE id = $1")
        .bind(account_id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    service
        .recover_account(account_id, &JobPriorityPolicy::default())
        .await
        .unwrap();

    let row =
        sqlx::query("SELECT pending_run, pending_cursor_kind FROM subscription WHERE id = $1")
            .bind(subscription_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert!(row.get::<bool, _>("pending_run"));
    assert_eq!(row.get::<String, _>("pending_cursor_kind"), "backfill");
}

async fn claim(service: &JobService) -> pixivarchive_domain::job::ClaimedJob {
    service
        .claim(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![
                JobPriority::Immediate,
                JobPriority::ScheduledCollection,
            ]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .expect("job should be claimable")
}

async fn make_retry_due(db: &Db, job_id: Uuid) {
    sqlx::query("UPDATE job SET next_retry_at = now() - interval '1 second' WHERE id = $1")
        .bind(job_id)
        .execute(db.pool())
        .await
        .unwrap();
}

async fn insert_pixiv_account(db: &Db) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO pixiv_account (
            id, pixiv_user_id, display_name, state, cookie_key_id, cookie_nonce, cookie_ciphertext
        )
        VALUES ($1, 10001, 'test', 'normal', 'test', decode('000000000000000000000000', 'hex'), decode('01', 'hex'))
        "#,
    )
    .bind(id)
    .execute(db.pool())
    .await
    .unwrap();
    id
}

async fn insert_subscription(db: &Db, account_id: Uuid) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO subscription (
            id, pixiv_account_id, name, kind, enabled, schedule, params, next_run_at
        )
        VALUES ($1, $2, 'test', 'ranking', true, '{}', '{"modes":["daily"],"contents":["all"]}', now())
        "#,
    )
    .bind(id)
    .bind(account_id)
    .execute(db.pool())
    .await
    .unwrap();
    id
}
