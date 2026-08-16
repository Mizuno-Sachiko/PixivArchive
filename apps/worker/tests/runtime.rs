use pixivarchive_application::{
    jobs::{JobService, QueueQuotaWeights},
    settings::{QueueSettings, SettingValue, SettingsService},
    trash::TrashService,
};
use pixivarchive_db::{Db, DbError, MediaRepository, WorkRepository};
use pixivarchive_domain::{
    job::{ClaimedJob, JobKind, JobPriority, NewJob},
    settings::SettingGroupKey,
};
use pixivarchive_worker::{
    executors::{ExecutorOutcome, ExecutorRegistry, JobExecutor},
    runtime::{WorkerRuntime, WorkerRuntimeConfig},
    scheduler::{self, StorageScheduler, SubscriptionScheduler, TrashScheduler},
    storage::StorageWriteGuard,
};
use serde_json::json;
use sqlx::Row;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration as StdDuration,
};
use time::Duration;

mod support;

use support::LockedDb;

#[tokio::test]
async fn unregistered_known_job_kind_is_marked_permanent() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    let job_id = service
        .enqueue(NewJob::for_kind(
            JobPriority::BackgroundMaintenance,
            JobKind::PurgeTrash,
            json!({}),
        ))
        .await
        .unwrap();
    let runtime = WorkerRuntime::new(
        service,
        ExecutorRegistry::new(),
        WorkerRuntimeConfig {
            max_concurrency: 1,
            lease_duration: Duration::milliseconds(200),
            heartbeat_interval: StdDuration::from_millis(50),
            poll_interval: StdDuration::from_millis(20),
            shutdown_grace: StdDuration::from_millis(100),
        },
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    let row = sqlx::query("SELECT state, error_class, retryable FROM job WHERE id = $1")
        .bind(job_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("state"), "failed");
    assert_eq!(
        row.get::<Option<String>, _>("error_class").as_deref(),
        Some("permanent")
    );
    assert_eq!(row.get::<Option<bool>, _>("retryable"), Some(false));
}

struct LeaseObservingExecutor {
    db: Db,
    observed_extension: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl JobExecutor for LeaseObservingExecutor {
    async fn execute(&self, job: ClaimedJob) -> ExecutorOutcome {
        let _ = tokio::time::timeout(StdDuration::from_secs(2), async {
            loop {
                let lease_expires_at: Option<time::OffsetDateTime> =
                    sqlx::query_scalar("SELECT lease_expires_at FROM job WHERE id = $1")
                        .bind(job.id)
                        .fetch_one(self.db.pool())
                        .await
                        .unwrap();
                if lease_expires_at.is_some_and(|expires_at| expires_at > job.lease_expires_at) {
                    self.observed_extension.store(true, Ordering::SeqCst);
                    return;
                }
                tokio::time::sleep(StdDuration::from_millis(20)).await;
            }
        })
        .await;
        ExecutorOutcome::completed()
    }
}

#[tokio::test]
async fn registered_executor_extends_running_job_lease() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    service
        .enqueue(NewJob::for_kind(
            JobPriority::ScheduledCollection,
            JobKind::ScheduledCollection,
            json!({}),
        ))
        .await
        .unwrap();

    let observed_extension = Arc::new(AtomicBool::new(false));
    let mut registry = ExecutorRegistry::new();
    registry.register(
        JobKind::ScheduledCollection,
        LeaseObservingExecutor {
            db: locked.db.clone(),
            observed_extension: observed_extension.clone(),
        },
    );
    let runtime = WorkerRuntime::new(
        service,
        registry,
        WorkerRuntimeConfig {
            max_concurrency: 1,
            lease_duration: Duration::seconds(5),
            heartbeat_interval: StdDuration::from_millis(100),
            poll_interval: StdDuration::from_millis(20),
            shutdown_grace: StdDuration::from_millis(100),
        },
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert!(observed_extension.load(Ordering::SeqCst));
}

struct CompletingExecutor;

#[async_trait::async_trait]
impl JobExecutor for CompletingExecutor {
    async fn execute(&self, _job: ClaimedJob) -> ExecutorOutcome {
        ExecutorOutcome::completed()
    }
}

struct WaitingStorageExecutor;

#[async_trait::async_trait]
impl JobExecutor for WaitingStorageExecutor {
    async fn execute(&self, _job: ClaimedJob) -> ExecutorOutcome {
        ExecutorOutcome::WaitingStorage
    }
}

#[tokio::test]
async fn storage_wait_does_not_consume_a_retry_attempt() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    let job_id = service
        .enqueue(NewJob::for_kind(
            JobPriority::BackgroundMaintenance,
            JobKind::GenerateDerivative,
            json!({ "media_revision_id": uuid::Uuid::now_v7() }),
        ))
        .await
        .unwrap();
    let mut registry = ExecutorRegistry::new();
    registry.register(JobKind::GenerateDerivative, WaitingStorageExecutor);
    let runtime = WorkerRuntime::new(service, registry, test_config(Duration::milliseconds(300)));
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    let row =
        sqlx::query("SELECT state, retryable_failure_count, error_class FROM job WHERE id = $1")
            .bind(job_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(row.get::<String, _>("state"), "waiting_storage");
    assert_eq!(row.get::<i32, _>("retryable_failure_count"), 0);
    assert!(row.get::<Option<String>, _>("error_class").is_none());
}

#[derive(Clone)]
struct ConcurrencyTrackingExecutor {
    active: Arc<AtomicUsize>,
    maximum: Arc<AtomicUsize>,
    release: Arc<tokio::sync::Semaphore>,
}

#[async_trait::async_trait]
impl JobExecutor for ConcurrencyTrackingExecutor {
    async fn execute(&self, _job: ClaimedJob) -> ExecutorOutcome {
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum.fetch_max(active, Ordering::SeqCst);
        let permit = self.release.acquire().await.unwrap();
        permit.forget();
        self.active.fetch_sub(1, Ordering::SeqCst);
        ExecutorOutcome::completed()
    }
}

#[tokio::test]
async fn run_loop_never_exceeds_the_configured_claim_concurrency() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    let mut job_ids = Vec::new();
    for _ in 0..4 {
        job_ids.push(
            service
                .enqueue(NewJob::for_kind(
                    JobPriority::ScheduledCollection,
                    JobKind::ScheduledCollection,
                    json!({}),
                ))
                .await
                .unwrap(),
        );
    }
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(tokio::sync::Semaphore::new(0));
    let mut registry = ExecutorRegistry::new();
    registry.register(
        JobKind::ScheduledCollection,
        ConcurrencyTrackingExecutor {
            active,
            maximum: maximum.clone(),
            release: release.clone(),
        },
    );
    let runtime = WorkerRuntime::new(
        service,
        registry,
        WorkerRuntimeConfig {
            max_concurrency: 2,
            ..test_config(Duration::milliseconds(500))
        },
    );
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let runtime_task = tokio::spawn(runtime.run_until_shutdown(shutdown_rx));

    for _ in 0..50 {
        if maximum.load(Ordering::SeqCst) == 2 {
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(20)).await;
    }
    tokio::time::sleep(StdDuration::from_millis(100)).await;
    assert_eq!(maximum.load(Ordering::SeqCst), 2);
    release.add_permits(4);
    for job_id in job_ids {
        wait_for_job_state(&locked.db, job_id, "completed").await;
    }
    shutdown_tx.send(true).unwrap();
    runtime_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn download_stage_claims_newer_media_work_while_discovery_waits() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    let discovery = service
        .enqueue(NewJob::for_kind(
            JobPriority::ScheduledCollection,
            JobKind::RankingCollection,
            json!({ "mode": "daily" }),
        ))
        .await
        .unwrap();
    let download = service
        .enqueue(NewJob::for_kind(
            JobPriority::ScheduledCollection,
            JobKind::DownloadMedia,
            json!({ "work_id": uuid::Uuid::now_v7(), "pixiv_work_id": 9_901 }),
        ))
        .await
        .unwrap();
    let mut registry = ExecutorRegistry::new();
    registry.register(JobKind::RankingCollection, CompletingExecutor);
    registry.register(JobKind::DownloadMedia, CompletingExecutor);
    let runtime = WorkerRuntime::new(
        service,
        registry,
        WorkerRuntimeConfig {
            max_concurrency: 1,
            ..test_config(Duration::milliseconds(300))
        },
    )
    .with_job_kinds([JobKind::DownloadMedia]);
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(job_state(&locked.db, discovery).await, "queued");
    assert_eq!(job_state(&locked.db, download).await, "completed");
}

#[tokio::test]
async fn run_loop_starts_accepting_claims_and_stops_after_shutdown() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    let first_job = service
        .enqueue(NewJob::for_kind(
            JobPriority::ScheduledCollection,
            JobKind::ScheduledCollection,
            json!({}),
        ))
        .await
        .unwrap();
    let mut registry = ExecutorRegistry::new();
    registry.register(JobKind::ScheduledCollection, CompletingExecutor);
    let runtime = WorkerRuntime::new(
        service.clone(),
        registry,
        test_config(Duration::milliseconds(300)),
    )
    .with_quota_weights(QueueQuotaWeights::new(1, 1, 4, 1).unwrap());
    assert!(runtime.state().is_accepting_work());
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let runtime_task = tokio::spawn(runtime.run_until_shutdown(shutdown_rx));

    wait_for_job_state(&locked.db, first_job, "completed").await;
    shutdown_tx.send(true).unwrap();
    runtime_task.await.unwrap().unwrap();

    let second_job = service
        .enqueue(NewJob::for_kind(
            JobPriority::ScheduledCollection,
            JobKind::ScheduledCollection,
            json!({}),
        ))
        .await
        .unwrap();
    tokio::time::sleep(StdDuration::from_millis(120)).await;
    assert_eq!(job_state(&locked.db, second_job).await, "queued");
}

#[tokio::test]
async fn run_loop_schedules_due_subscriptions_before_claiming_jobs() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    let due_at = time::OffsetDateTime::now_utc() - Duration::seconds(5);
    let subscription_id = insert_subscription(&locked.db, due_at).await;
    let mut registry = ExecutorRegistry::new();
    registry.register(JobKind::ScheduledCollection, CompletingExecutor);
    let runtime = WorkerRuntime::new(service, registry, test_config(Duration::milliseconds(300)))
        .with_subscription_scheduler(SubscriptionScheduler::new(locked.db.clone()))
        .with_quota_weights(QueueQuotaWeights::new(1, 1, 4, 1).unwrap());
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let runtime_task = tokio::spawn(runtime.run_until_shutdown(shutdown_rx));

    wait_for_subscription_run_count(&locked.db, subscription_id, 1).await;
    shutdown_tx.send(true).unwrap();
    runtime_task.await.unwrap().unwrap();

    let row = sqlx::query("SELECT next_run_at FROM subscription WHERE id = $1")
        .bind(subscription_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    let stored_next_run_at = row
        .get::<Option<time::OffsetDateTime>, _>("next_run_at")
        .unwrap();
    let expected_next_run_at = due_at + Duration::hours(1);
    assert!(stored_next_run_at >= expected_next_run_at - Duration::milliseconds(1));
    assert!(stored_next_run_at <= expected_next_run_at + Duration::milliseconds(1));
}

#[tokio::test]
async fn subscription_scheduler_reads_priority_settings_for_each_scan() {
    let locked = LockedDb::new().await;
    let due_at = time::OffsetDateTime::now_utc() - Duration::seconds(5);
    let subscription_id = insert_subscription(&locked.db, due_at).await;
    let scheduler = SubscriptionScheduler::new(locked.db.clone());
    let mut queue = QueueSettings::default();
    queue
        .job_priorities
        .iter_mut()
        .find(|mapping| mapping.job_kind == JobKind::RankingCollection)
        .unwrap()
        .priority = JobPriority::Immediate;
    SettingsService::new(locked.db.clone())
        .update(SettingGroupKey::Queue, None, SettingValue::Queue(queue))
        .await
        .unwrap();

    assert_eq!(
        scheduler
            .process_due(time::OffsetDateTime::now_utc())
            .await
            .unwrap(),
        1
    );

    let priority: String = sqlx::query_scalar(
        "SELECT priority_class FROM job WHERE payload ->> 'subscription_id' = $1",
    )
    .bind(subscription_id.to_string())
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(priority, "immediate");
}

#[tokio::test]
async fn locked_database_resets_saved_system_settings_between_instances() {
    {
        let locked = LockedDb::new().await;
        sqlx::query("DELETE FROM system_setting")
            .execute(locked.db.pool())
            .await
            .unwrap();
        SettingsService::new(locked.db.clone())
            .update(
                SettingGroupKey::Queue,
                None,
                SettingValue::Queue(QueueSettings::default()),
            )
            .await
            .unwrap();
    }

    let locked = LockedDb::new().await;
    let setting_count: i64 = sqlx::query_scalar("SELECT count(*) FROM system_setting")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(setting_count, 0);
}

#[tokio::test]
async fn trash_scheduler_scans_on_startup_then_observes_its_interval() {
    let locked = LockedDb::new().await;
    let work = WorkRepository::new(locked.db.clone())
        .create_metadata_only(9_401, 501, "due trash")
        .await
        .unwrap();
    let trash = TrashService::new(locked.db.clone());
    trash.move_to_trash(work.id, 30).await.unwrap();
    let due_at = time::OffsetDateTime::now_utc();
    trash.reschedule(work.id, due_at).await.unwrap();
    let scheduler = TrashScheduler::new(locked.db.clone())
        .with_interval(Duration::minutes(15))
        .unwrap();
    let scan_at = due_at + Duration::seconds(1);

    assert_eq!(scheduler.process_due(scan_at).await.unwrap(), 1);
    assert_eq!(
        scheduler
            .process_due(scan_at + Duration::minutes(1))
            .await
            .unwrap(),
        0
    );
    let payload: serde_json::Value =
        sqlx::query_scalar("SELECT payload FROM job WHERE kind = 'purge_trash'")
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(payload["work_id"], work.id.to_string());
    assert_eq!(payload["deletion_method"], "retention_expired");
}

#[tokio::test]
async fn storage_scheduler_holds_and_releases_media_writing_jobs() {
    let locked = LockedDb::new().await;
    let jobs = pixivarchive_db::JobRepository::new(locked.db.clone());
    let job_id = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::BackgroundMaintenance,
            JobKind::GenerateDerivative,
            json!({ "media_revision_id": uuid::Uuid::now_v7() }),
        ))
        .await
        .unwrap();
    let media_root = std::env::temp_dir();
    let stopped = StorageScheduler::new(
        locked.db.clone(),
        StorageWriteGuard::new(media_root.clone(), u64::MAX),
    );

    assert_eq!(
        stopped
            .process_due(time::OffsetDateTime::now_utc())
            .await
            .unwrap(),
        1
    );
    assert_eq!(job_state(&locked.db, job_id).await, "waiting_storage");

    let allowed = StorageScheduler::new(locked.db.clone(), StorageWriteGuard::new(media_root, 0));
    assert_eq!(
        allowed
            .process_due(time::OffsetDateTime::now_utc())
            .await
            .unwrap(),
        1
    );
    assert_eq!(job_state(&locked.db, job_id).await, "queued");
}

#[tokio::test]
async fn storage_scheduler_removes_uncommitted_files_from_cancelled_jobs() {
    let locked = LockedDb::new().await;
    let jobs = pixivarchive_db::JobRepository::new(locked.db.clone());
    let job_id = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::BackgroundMaintenance,
            JobKind::GenerateDerivative,
            json!({ "media_revision_id": uuid::Uuid::now_v7() }),
        ))
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            uuid::Uuid::now_v7(),
            &pixivarchive_domain::job::JobQuotaSelection::with_fallback(
                JobPriority::BackgroundMaintenance,
            ),
            Duration::milliseconds(80),
        )
        .await
        .unwrap()
        .unwrap();
    let media_root = std::env::temp_dir().join(format!(
        "pixivarchive-artifact-cleanup-{}",
        uuid::Uuid::now_v7()
    ));
    let relative_path = std::path::PathBuf::from("derivatives/pixiv/test.webp");
    let absolute_path = media_root.join(&relative_path);
    tokio::fs::create_dir_all(absolute_path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&absolute_path, b"uncommitted")
        .await
        .unwrap();
    MediaRepository::new(locked.db.clone())
        .register_artifact_intent(claimed.lease(), &relative_path)
        .await
        .unwrap();
    jobs.cancel_requested(job_id, claimed.resource_revision)
        .await
        .unwrap();
    let rejected = MediaRepository::new(locked.db.clone())
        .register_artifact_intent(
            claimed.lease(),
            std::path::Path::new("derivatives/pixiv/late.webp"),
        )
        .await;
    assert!(matches!(rejected, Err(DbError::LeaseConflict)));
    tokio::time::sleep(StdDuration::from_millis(120)).await;
    let scheduler = StorageScheduler::new(
        locked.db.clone(),
        StorageWriteGuard::new(media_root.clone(), 0),
    );

    assert_eq!(
        scheduler
            .process_due(time::OffsetDateTime::now_utc())
            .await
            .unwrap(),
        1
    );
    assert!(!absolute_path.exists());
    let intents: i64 = sqlx::query_scalar("SELECT count(*) FROM media_artifact_intent")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(intents, 0);
    tokio::fs::remove_dir_all(media_root).await.unwrap();
}

#[tokio::test]
async fn media_intents_reject_an_expired_job_lease() {
    let locked = LockedDb::new().await;
    let jobs = pixivarchive_db::JobRepository::new(locked.db.clone());
    let job_id = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::BackgroundMaintenance,
            JobKind::GenerateDerivative,
            json!({ "media_revision_id": uuid::Uuid::now_v7() }),
        ))
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            uuid::Uuid::now_v7(),
            &pixivarchive_domain::job::JobQuotaSelection::with_fallback(
                JobPriority::BackgroundMaintenance,
            ),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE job SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(job_id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    let result = MediaRepository::new(locked.db.clone())
        .register_artifact_intent(
            claimed.lease(),
            std::path::Path::new("derivatives/pixiv/expired.webp"),
        )
        .await;

    assert!(matches!(result, Err(DbError::LeaseConflict)));
    let intents: i64 =
        sqlx::query_scalar("SELECT count(*) FROM media_artifact_intent WHERE job_id = $1")
            .bind(job_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(intents, 0);
}

#[tokio::test]
async fn storage_scheduler_waits_for_a_cancelled_running_jobs_lease() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    let job_id = service
        .enqueue(NewJob::for_kind(
            JobPriority::BackgroundMaintenance,
            JobKind::GenerateDerivative,
            json!({ "media_revision_id": uuid::Uuid::now_v7() }),
        ))
        .await
        .unwrap();
    let claimed = service
        .claim(
            uuid::Uuid::now_v7(),
            &pixivarchive_domain::job::JobQuotaSelection::with_fallback(
                JobPriority::BackgroundMaintenance,
            ),
            Duration::milliseconds(180),
        )
        .await
        .unwrap()
        .unwrap();
    let media_root = std::env::temp_dir().join(format!(
        "pixivarchive-artifact-cancel-{}",
        uuid::Uuid::now_v7()
    ));
    let relative_path = std::path::PathBuf::from("derivatives/pixiv/running.webp");
    let absolute_path = media_root.join(&relative_path);
    tokio::fs::create_dir_all(absolute_path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&absolute_path, b"still-running")
        .await
        .unwrap();
    MediaRepository::new(locked.db.clone())
        .register_artifact_intent(claimed.lease(), &relative_path)
        .await
        .unwrap();
    service
        .cancel_requested(job_id, claimed.resource_revision)
        .await
        .unwrap();
    let scheduler = StorageScheduler::new(
        locked.db.clone(),
        StorageWriteGuard::new(media_root.clone(), 0),
    )
    .with_interval(Duration::milliseconds(10))
    .unwrap();

    assert_eq!(
        scheduler
            .process_due(time::OffsetDateTime::now_utc())
            .await
            .unwrap(),
        0
    );
    assert!(absolute_path.exists());

    tokio::time::sleep(StdDuration::from_millis(220)).await;
    assert_eq!(
        scheduler
            .process_due(time::OffsetDateTime::now_utc())
            .await
            .unwrap(),
        1
    );
    assert!(!absolute_path.exists());
    tokio::fs::remove_dir_all(media_root).await.unwrap();
}

struct BlockingExecutor {
    started: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

#[async_trait::async_trait]
impl JobExecutor for BlockingExecutor {
    async fn execute(&self, _job: ClaimedJob) -> ExecutorOutcome {
        if let Some(started) = self.started.lock().unwrap().take() {
            let _ = started.send(());
        }
        std::future::pending().await
    }
}

#[tokio::test]
async fn cancelling_a_running_job_stops_its_executor_without_failing_the_worker() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    let job_id = service
        .enqueue(NewJob::for_kind(
            JobPriority::ScheduledCollection,
            JobKind::ScheduledCollection,
            json!({}),
        ))
        .await
        .unwrap();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let mut registry = ExecutorRegistry::new();
    registry.register(
        JobKind::ScheduledCollection,
        BlockingExecutor {
            started: std::sync::Mutex::new(Some(started_tx)),
        },
    );
    let runtime = WorkerRuntime::new(
        service.clone(),
        registry,
        test_config(Duration::milliseconds(300)),
    )
    .with_quota_weights(QueueQuotaWeights::new(1, 1, 4, 1).unwrap());
    let task = tokio::spawn(async move {
        let mut rotation = QueueQuotaWeights::new(1, 1, 4, 1).unwrap().rotation();
        runtime.process_once(&mut rotation).await
    });

    started_rx.await.unwrap();
    let running = service.get(job_id).await.unwrap();
    service
        .cancel_requested(job_id, running.resource_revision)
        .await
        .unwrap();

    assert!(task.await.unwrap().unwrap());
    assert_eq!(job_state(&locked.db, job_id).await, "cancelled");
}

#[tokio::test]
async fn shutdown_grace_leaves_unfinished_work_recoverable_after_lease_expiry() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    let job_id = service
        .enqueue(NewJob::for_kind(
            JobPriority::ScheduledCollection,
            JobKind::ScheduledCollection,
            json!({}),
        ))
        .await
        .unwrap();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let mut registry = ExecutorRegistry::new();
    registry.register(
        JobKind::ScheduledCollection,
        BlockingExecutor {
            started: std::sync::Mutex::new(Some(started_tx)),
        },
    );
    let runtime = WorkerRuntime::new(
        service.clone(),
        registry,
        WorkerRuntimeConfig {
            max_concurrency: 1,
            lease_duration: Duration::milliseconds(180),
            heartbeat_interval: StdDuration::from_secs(60),
            poll_interval: StdDuration::from_millis(20),
            shutdown_grace: StdDuration::from_millis(50),
        },
    )
    .with_quota_weights(QueueQuotaWeights::new(1, 1, 4, 1).unwrap());
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let runtime_task = tokio::spawn(runtime.run_until_shutdown(shutdown_rx));
    started_rx.await.unwrap();
    shutdown_tx.send(true).unwrap();
    runtime_task.await.unwrap().unwrap();
    assert_eq!(job_state(&locked.db, job_id).await, "running");

    tokio::time::sleep(StdDuration::from_millis(220)).await;
    assert!(
        service
            .claim(
                uuid::Uuid::now_v7(),
                &pixivarchive_domain::job::JobQuotaSelection::with_fallback(
                    JobPriority::ScheduledCollection
                ),
                Duration::milliseconds(180),
            )
            .await
            .unwrap()
            .is_some()
    );
}

#[derive(Clone)]
struct PanickingExecutor;

#[async_trait::async_trait]
impl JobExecutor for PanickingExecutor {
    async fn execute(&self, _job: ClaimedJob) -> ExecutorOutcome {
        panic!("executor panic should surface from runtime");
    }
}

#[tokio::test]
async fn run_loop_returns_executor_panic_instead_of_dropping_join_errors() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    service
        .enqueue(NewJob::for_kind(
            JobPriority::ScheduledCollection,
            JobKind::ScheduledCollection,
            json!({}),
        ))
        .await
        .unwrap();
    let mut registry = ExecutorRegistry::new();
    registry.register(JobKind::ScheduledCollection, PanickingExecutor);
    let runtime = WorkerRuntime::new(service, registry, test_config(Duration::milliseconds(300)))
        .with_quota_weights(QueueQuotaWeights::new(1, 1, 4, 1).unwrap());
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let error = runtime.run_until_shutdown(shutdown_rx).await.unwrap_err();
    assert!(error.to_string().contains("executor panic"));
}

fn test_config(lease_duration: Duration) -> WorkerRuntimeConfig {
    WorkerRuntimeConfig {
        max_concurrency: 4,
        lease_duration,
        heartbeat_interval: StdDuration::from_millis(40),
        poll_interval: StdDuration::from_millis(20),
        shutdown_grace: StdDuration::from_millis(100),
    }
}

async fn job_state(db: &Db, job_id: uuid::Uuid) -> String {
    sqlx::query_scalar("SELECT state FROM job WHERE id = $1")
        .bind(job_id)
        .fetch_one(db.pool())
        .await
        .unwrap()
}

async fn wait_for_job_state(db: &Db, job_id: uuid::Uuid, expected: &str) {
    for _ in 0..50 {
        if job_state(db, job_id).await == expected {
            return;
        }
        tokio::time::sleep(StdDuration::from_millis(20)).await;
    }
    panic!("job did not reach expected state {expected}");
}

async fn wait_for_subscription_run_count(db: &Db, subscription_id: uuid::Uuid, expected: i64) {
    for _ in 0..50 {
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM subscription_run WHERE subscription_id = $1")
                .bind(subscription_id)
                .fetch_one(db.pool())
                .await
                .unwrap();
        if count == expected {
            return;
        }
        tokio::time::sleep(StdDuration::from_millis(20)).await;
    }
    panic!("subscription run count did not reach {expected}");
}

async fn insert_subscription(db: &Db, next_run_at: time::OffsetDateTime) -> uuid::Uuid {
    let id = uuid::Uuid::now_v7();
    let account_id = uuid::Uuid::now_v7();
    let pixiv_user_id = (account_id.as_u128() % i64::MAX as u128 + 1) as i64;
    sqlx::query(
        r#"
        INSERT INTO pixiv_account (
            id, pixiv_user_id, display_name, state, cookie_key_id, cookie_nonce, cookie_ciphertext
        )
        VALUES ($1, $2, 'runtime test', 'normal', 'test', decode('000000000000000000000000', 'hex'), decode('01', 'hex'))
        "#,
    )
    .bind(account_id)
    .bind(pixiv_user_id)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO subscription (
            id, pixiv_account_id, name, kind, enabled, schedule, params, next_run_at
        )
        VALUES ($1, $2, 'runtime-test', 'ranking', true, '{"interval_minutes":60,"lookback_pages":2}', '{"modes":["daily"],"contents":["all"]}', $3)
        "#,
    )
    .bind(id)
    .bind(account_id)
    .bind(next_run_at)
    .execute(db.pool())
    .await
    .unwrap();
    id
}
