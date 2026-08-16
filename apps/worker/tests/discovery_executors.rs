use async_trait::async_trait;
use pixivarchive_application::{
    jobs::{JobService, RetryPolicy},
    pixiv_accounts::{
        AccountCookieUpdate, PixivAccount, PixivAccountContextError, PixivAccountService,
    },
    subscriptions::{RankingSubscriptionRequest, SubscriptionService},
};
use pixivarchive_domain::{
    job::JobKind,
    pixiv::{PixivBookmarksMode, PixivFollowLatestMode, PixivRankingContent, PixivRankingMode},
};
use pixivarchive_pixiv::{PixivErrorClass, PixivRequestContext};
use pixivarchive_test_support::{
    FakePixivGateway, configure_bookmarks_subscription, configure_following_subscription, context,
    discovery_work, ranking_entry,
};
use pixivarchive_worker::{
    executors::{ExecutorRegistry, subscription::PixivContextProvider},
    runtime::{WorkerRuntime, WorkerRuntimeConfig},
    scheduler,
};
use secrecy::SecretString;
use std::sync::Arc;
use time::Duration;

mod support;

use support::LockedDb;

#[derive(Clone)]
struct StaticContextProvider;

#[async_trait]
impl PixivContextProvider for StaticContextProvider {
    async fn context_for_account(
        &self,
        _account_id: uuid::Uuid,
    ) -> Result<PixivRequestContext, PixivAccountContextError> {
        Ok(PixivRequestContext::new(
            SecretString::from("PHPSESSID=worker"),
            10_001,
            "PixivArchiveWorkerTest/1.0",
        ))
    }
}

#[tokio::test]
async fn worker_executes_ranking_collection_unit_and_completes_job() {
    let locked = LockedDb::new().await;
    let gateway = FakePixivGateway::new();
    gateway.set_ranking_items(vec![ranking_entry(3101, 1)]);
    let account = account(&locked, gateway.clone()).await;
    let subscription = SubscriptionService::new(locked.db.clone())
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "worker ranking".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 0,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    SubscriptionService::new(locked.db.clone())
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();

    let registry = pixiv_registry(&locked, gateway);
    let runtime = WorkerRuntime::new(JobService::new(locked.db.clone()), registry, test_config());
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(
        job_state(&locked, JobKind::RankingCollection).await,
        "completed"
    );
    assert_eq!(parent_state(&locked).await, "succeeded");
}

#[tokio::test]
async fn ranking_collection_recovers_business_state_after_a_retryable_failure() {
    let locked = LockedDb::new().await;
    let gateway = FakePixivGateway::new();
    gateway.set_ranking_items(vec![ranking_entry(3111, 1)]);
    gateway.fail_ranking(PixivErrorClass::Network);
    let account = account(&locked, gateway.clone()).await;
    let subscription = SubscriptionService::new(locked.db.clone())
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "retrying ranking".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 0,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    SubscriptionService::new(locked.db.clone())
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();

    let runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        pixiv_registry(&locked, gateway.clone()),
        test_config(),
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(
        job_state(&locked, JobKind::RankingCollection).await,
        "failed"
    );
    assert_eq!(unit_state(&locked).await, "queued");
    assert_eq!(parent_state(&locked).await, "running");

    gateway.clear_ranking_failure();
    sqlx::query("UPDATE job SET next_retry_at = now() - interval '1 second' WHERE kind = $1")
        .bind(JobKind::RankingCollection.as_str())
        .execute(locked.db.pool())
        .await
        .unwrap();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(
        job_state(&locked, JobKind::RankingCollection).await,
        "completed"
    );
    assert_eq!(unit_state(&locked).await, "succeeded");
    assert_eq!(parent_state(&locked).await, "succeeded");
}

#[tokio::test]
async fn ranking_collection_replay_completes_without_repeating_a_succeeded_unit() {
    let locked = LockedDb::new().await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    let subscription = SubscriptionService::new(locked.db.clone())
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "completed ranking".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 0,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    SubscriptionService::new(locked.db.clone())
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();
    sqlx::query("UPDATE subscription_run_unit SET state = 'succeeded', finished_at = now()")
        .execute(locked.db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE subscription_run SET state = 'succeeded', finished_at = now()")
        .execute(locked.db.pool())
        .await
        .unwrap();
    gateway.fail_ranking(PixivErrorClass::Network);

    let runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        pixiv_registry(&locked, gateway),
        test_config(),
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(
        job_state(&locked, JobKind::RankingCollection).await,
        "completed"
    );
    assert_eq!(unit_state(&locked).await, "succeeded");
}

#[tokio::test]
async fn ranking_collection_finishes_failed_after_retry_exhaustion() {
    let locked = LockedDb::new().await;
    let gateway = FakePixivGateway::new();
    gateway.fail_ranking(PixivErrorClass::Network);
    let account = account(&locked, gateway.clone()).await;
    let subscription = SubscriptionService::new(locked.db.clone())
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "exhausted ranking".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 0,
            rule_id: None,
            next_run_at: None,
        })
        .await
        .unwrap();
    SubscriptionService::new(locked.db.clone())
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();

    let service = JobService::with_retry_policy(
        locked.db.clone(),
        RetryPolicy::new(vec![Duration::milliseconds(1)]).unwrap(),
    );
    let runtime = WorkerRuntime::new(service, pixiv_registry(&locked, gateway), test_config());
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    sqlx::query("UPDATE job SET next_retry_at = now() - interval '1 second' WHERE kind = $1")
        .bind(JobKind::RankingCollection.as_str())
        .execute(locked.db.pool())
        .await
        .unwrap();
    assert!(runtime.process_once(&mut rotation).await.unwrap());

    assert_eq!(
        job_state(&locked, JobKind::RankingCollection).await,
        "failed"
    );
    assert_eq!(unit_state(&locked).await, "failed");
    assert_eq!(parent_state(&locked).await, "failed");
}

#[tokio::test]
async fn worker_executes_following_collection_unit_and_completes_job() {
    let locked = LockedDb::new().await;
    let gateway = FakePixivGateway::new();
    gateway.set_follow_items(vec![discovery_work(3151)]);
    let account = account(&locked, gateway.clone()).await;
    let subscription =
        configure_following_subscription(&locked.db, account.id, PixivFollowLatestMode::All, 60, 0)
            .await;
    SubscriptionService::new(locked.db.clone())
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();

    let runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        pixiv_registry(&locked, gateway),
        test_config(),
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(
        job_state(&locked, JobKind::FollowingCollection).await,
        "completed"
    );
    assert_eq!(parent_state(&locked).await, "succeeded");
}

#[tokio::test]
async fn worker_executes_bookmarks_collection_unit_and_completes_job() {
    let locked = LockedDb::new().await;
    let gateway = FakePixivGateway::new();
    gateway.set_bookmark_items(vec![discovery_work(3161)]);
    let account = account(&locked, gateway.clone()).await;
    let subscription =
        configure_bookmarks_subscription(&locked.db, account.id, PixivBookmarksMode::All, 60, 0)
            .await;
    SubscriptionService::new(locked.db.clone())
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();

    let runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        pixiv_registry(&locked, gateway),
        test_config(),
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(
        job_state(&locked, JobKind::BookmarksCollection).await,
        "completed"
    );
    assert_eq!(parent_state(&locked).await, "succeeded");
}

#[tokio::test]
async fn worker_executes_queued_import_work_job() {
    let locked = LockedDb::new().await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    let queued =
        pixivarchive_application::imports::ImportService::new(locked.db.clone(), gateway.clone())
            .queue(
                pixivarchive_application::imports::ImportRequest::work(account.id, context(), 3201)
                    .forced(),
            )
            .await
            .unwrap();

    let runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        pixiv_registry(&locked, gateway),
        test_config(),
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(
        import_status(&locked, queued.run_id).await,
        "download_queued"
    );
}

#[tokio::test]
async fn queued_import_recovers_after_a_retryable_pixiv_failure() {
    let locked = LockedDb::new().await;
    let gateway = FakePixivGateway::new();
    gateway.fail_work_detail(PixivErrorClass::Network);
    let account = account(&locked, gateway.clone()).await;
    let queued =
        pixivarchive_application::imports::ImportService::new(locked.db.clone(), gateway.clone())
            .queue(
                pixivarchive_application::imports::ImportRequest::work(account.id, context(), 3211)
                    .forced(),
            )
            .await
            .unwrap();

    let runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        pixiv_registry(&locked, gateway.clone()),
        test_config(),
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(job_state(&locked, JobKind::ImportWork).await, "failed");
    assert_eq!(import_status(&locked, queued.run_id).await, "queued");

    gateway.clear_work_detail_failure();
    sqlx::query("UPDATE job SET next_retry_at = now() - interval '1 second' WHERE kind = $1")
        .bind(JobKind::ImportWork.as_str())
        .execute(locked.db.pool())
        .await
        .unwrap();
    assert!(runtime.process_once(&mut rotation).await.unwrap());

    assert_eq!(job_state(&locked, JobKind::ImportWork).await, "completed");
    assert_eq!(
        import_status(&locked, queued.run_id).await,
        "download_queued"
    );
}

#[tokio::test]
async fn completed_import_replay_does_not_repeat_pixiv_requests() {
    let locked = LockedDb::new().await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    let queued =
        pixivarchive_application::imports::ImportService::new(locked.db.clone(), gateway.clone())
            .queue(
                pixivarchive_application::imports::ImportRequest::work(account.id, context(), 3212)
                    .forced(),
            )
            .await
            .unwrap();
    sqlx::query(
        "UPDATE import_run SET status = 'download_queued', finished_at = now() WHERE id = $1",
    )
    .bind(queued.run_id)
    .execute(locked.db.pool())
    .await
    .unwrap();
    gateway.fail_work_detail(PixivErrorClass::Network);

    let runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        pixiv_registry(&locked, gateway),
        test_config(),
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(job_state(&locked, JobKind::ImportWork).await, "completed");
    assert_eq!(
        import_status(&locked, queued.run_id).await,
        "download_queued"
    );
}

#[tokio::test]
async fn interrupted_running_import_can_resume_after_its_lease_expires() {
    let locked = LockedDb::new().await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    let queued =
        pixivarchive_application::imports::ImportService::new(locked.db.clone(), gateway.clone())
            .queue(
                pixivarchive_application::imports::ImportRequest::work(account.id, context(), 3213)
                    .forced(),
            )
            .await
            .unwrap();
    sqlx::query("UPDATE import_run SET status = 'running' WHERE id = $1")
        .bind(queued.run_id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    let runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        pixiv_registry(&locked, gateway),
        test_config(),
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(job_state(&locked, JobKind::ImportWork).await, "completed");
    assert_eq!(
        import_status(&locked, queued.run_id).await,
        "download_queued"
    );
}

#[tokio::test]
async fn terminal_executor_error_does_not_leave_import_run_running() {
    let locked = LockedDb::new().await;
    let gateway = FakePixivGateway::new();
    gateway.fail_work_detail(PixivErrorClass::HiddenOrNotFound);
    let account = account(&locked, gateway.clone()).await;
    let queued =
        pixivarchive_application::imports::ImportService::new(locked.db.clone(), gateway.clone())
            .queue(
                pixivarchive_application::imports::ImportRequest::work(account.id, context(), 3221)
                    .forced(),
            )
            .await
            .unwrap();

    let runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        pixiv_registry(&locked, gateway),
        test_config(),
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(job_state(&locked, JobKind::ImportWork).await, "failed");
    assert_eq!(import_status(&locked, queued.run_id).await, "failed");
}

#[tokio::test]
async fn worker_executes_queued_import_artist_job() {
    let locked = LockedDb::new().await;
    let gateway = FakePixivGateway::new();
    gateway.set_artist_work_ids(vec![3301, 3302]);
    let account = account(&locked, gateway.clone()).await;
    let queued =
        pixivarchive_application::imports::ImportService::new(locked.db.clone(), gateway.clone())
            .queue(
                pixivarchive_application::imports::ImportRequest::artist(
                    account.id,
                    context(),
                    300,
                )
                .forced(),
            )
            .await
            .unwrap();

    let runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        pixiv_registry(&locked, gateway),
        test_config(),
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(job_state(&locked, JobKind::ImportArtist).await, "completed");
    assert_eq!(
        import_status(&locked, queued.run_id).await,
        "metadata_saved"
    );
}

async fn account(locked: &LockedDb, gateway: FakePixivGateway) -> PixivAccount {
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

fn test_config() -> WorkerRuntimeConfig {
    WorkerRuntimeConfig {
        max_concurrency: 1,
        lease_duration: Duration::minutes(5),
        heartbeat_interval: std::time::Duration::from_millis(40),
        poll_interval: std::time::Duration::from_millis(20),
        shutdown_grace: std::time::Duration::from_millis(100),
    }
}

fn pixiv_registry(locked: &LockedDb, gateway: FakePixivGateway) -> ExecutorRegistry {
    let mut registry = ExecutorRegistry::new();
    registry.register_pixiv_discovery(locked.db.clone(), gateway, Arc::new(StaticContextProvider));
    registry
}

async fn job_state(locked: &LockedDb, kind: JobKind) -> String {
    sqlx::query_scalar("SELECT state FROM job WHERE kind = $1")
        .bind(kind.as_str())
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn parent_state(locked: &LockedDb) -> String {
    sqlx::query_scalar("SELECT state FROM subscription_run LIMIT 1")
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn unit_state(locked: &LockedDb) -> String {
    sqlx::query_scalar("SELECT state FROM subscription_run_unit LIMIT 1")
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn import_status(locked: &LockedDb, run_id: uuid::Uuid) -> String {
    sqlx::query_scalar("SELECT status FROM import_run WHERE id = $1")
        .bind(run_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}
