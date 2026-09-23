use pixivarchive_application::jobs::JobService;
use pixivarchive_db::{Db, DbError, JobCompletion};
use pixivarchive_domain::job::{
    ClaimedJob, JobErrorClass, JobKind, JobLeaseStatus, JobPriority, JobQuotaSelection, NewJob,
};
use pixivarchive_worker::{
    executors::{ExecutorOutcome, ExecutorRegistry, JobExecutor},
    runtime::{WorkerRuntime, WorkerRuntimeConfig},
};
use serde_json::json;
use std::{sync::Mutex, time::Duration as StdDuration};
use time::Duration;
use tokio::sync::oneshot;
use uuid::Uuid;

mod support;
use support::LockedDb;

struct FirstExecution {
    started: oneshot::Sender<ClaimedJob>,
    result: oneshot::Receiver<ExecutorOutcome>,
}

struct ControlledExecutor {
    first: Mutex<Option<FirstExecution>>,
}

#[async_trait::async_trait]
impl JobExecutor for ControlledExecutor {
    async fn execute(&self, job: ClaimedJob) -> ExecutorOutcome {
        let first = self.first.lock().unwrap().take();
        if let Some(first) = first {
            first.started.send(job).unwrap();
            first.result.await.unwrap()
        } else {
            ExecutorOutcome::completed()
        }
    }
}

fn controlled_runtime(
    service: JobService,
    heartbeat_interval: StdDuration,
) -> (
    WorkerRuntime,
    oneshot::Receiver<ClaimedJob>,
    oneshot::Sender<ExecutorOutcome>,
) {
    let (started, claimed) = oneshot::channel();
    let (result, response) = oneshot::channel();
    let mut registry = ExecutorRegistry::new();
    registry.register(
        JobKind::ScheduledCollection,
        ControlledExecutor {
            first: Mutex::new(Some(FirstExecution {
                started,
                result: response,
            })),
        },
    );
    (
        WorkerRuntime::new(
            service,
            registry,
            WorkerRuntimeConfig {
                max_concurrency: 1,
                lease_duration: Duration::minutes(5),
                heartbeat_interval,
                poll_interval: StdDuration::from_millis(10),
                shutdown_grace: StdDuration::from_millis(100),
            },
        ),
        claimed,
        result,
    )
}

async fn enqueue(service: &JobService) -> Uuid {
    service
        .enqueue(NewJob::for_kind(
            JobPriority::ScheduledCollection,
            JobKind::ScheduledCollection,
            json!({}),
        ))
        .await
        .unwrap()
}

async fn expire(db: &Db, job_id: Uuid) {
    sqlx::query("UPDATE job SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(job_id)
        .execute(db.pool())
        .await
        .unwrap();
}

async fn reclaim(service: &JobService) -> ClaimedJob {
    service
        .claim(
            Uuid::now_v7(),
            &JobQuotaSelection::with_fallback(JobPriority::ScheduledCollection),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap()
}

async fn wait_for_completion(service: &JobService, job_id: Uuid) {
    tokio::time::timeout(StdDuration::from_secs(5), async {
        loop {
            if service.get(job_id).await.unwrap().state
                == pixivarchive_domain::job::JobState::Completed
            {
                break;
            }
            tokio::time::sleep(StdDuration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn stale_results_do_not_change_the_replacement_attempt_or_stop_the_run_loop() {
    for outcome in [
        ExecutorOutcome::completed(),
        ExecutorOutcome::failed(JobErrorClass::Server, None),
        ExecutorOutcome::WaitingStorage,
    ] {
        let locked = LockedDb::new().await;
        let service = JobService::new(locked.db.clone());
        let job_id = enqueue(&service).await;
        let (runtime, started, result) =
            controlled_runtime(service.clone(), StdDuration::from_secs(60));
        let (shutdown, receiver) = tokio::sync::watch::channel(false);
        let running = tokio::spawn(runtime.run_until_shutdown(receiver));
        let old = started.await.unwrap();
        expire(&locked.db, job_id).await;
        let replacement = reclaim(&service).await;
        assert_eq!(replacement.id, job_id);
        let following = enqueue(&service).await;
        result.send(outcome).unwrap();
        wait_for_completion(&service, following).await;

        assert_eq!(
            service.lease_status(&replacement).await.unwrap(),
            JobLeaseStatus::Active
        );
        assert_eq!(service.get(job_id).await.unwrap().attempts, 2);
        let attempts = service.attempts(job_id).await.unwrap();
        let previous = attempts
            .iter()
            .find(|attempt| attempt.attempt_number == old.attempt_number)
            .unwrap();
        assert_eq!(previous.error_class.as_deref(), Some("lease_expired"));
        assert_eq!(previous.state, "failed");
        service
            .complete(&replacement, JobCompletion::TaskOnly)
            .await
            .unwrap();
        assert_eq!(
            service.lease_status(&old).await.unwrap(),
            JobLeaseStatus::Superseded
        );
        shutdown.send(true).unwrap();
        running.await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn heartbeat_loss_drops_the_old_executor_and_keeps_processing_jobs() {
    // Include a completed replacement: it must not make the runtime await the old executor.
    for complete_replacement in [false, true] {
        let locked = LockedDb::new().await;
        let service = JobService::new(locked.db.clone());
        let job_id = enqueue(&service).await;
        let (runtime, started, mut result) =
            controlled_runtime(service.clone(), StdDuration::from_millis(100));
        // Keep claiming under test control while arranging the expired and replacement
        // states; the old runtime must not reclaim the fixture before this test does.
        let old_runtime = runtime.clone();
        let running = tokio::spawn(async move {
            old_runtime
                .process_once(&mut pixivarchive_worker::scheduler::default_rotation())
                .await
        });
        started.await.unwrap();
        expire(&locked.db, job_id).await;
        let replacement = reclaim(&service).await;
        assert_eq!(replacement.id, job_id);
        if complete_replacement {
            service
                .complete(&replacement, JobCompletion::TaskOnly)
                .await
                .unwrap();
        }
        tokio::time::timeout(StdDuration::from_secs(5), result.closed())
            .await
            .unwrap();
        assert!(running.await.unwrap().unwrap());
        let following = enqueue(&service).await;
        assert!(
            runtime
                .process_once(&mut pixivarchive_worker::scheduler::default_rotation())
                .await
                .unwrap()
        );
        wait_for_completion(&service, following).await;
        if !complete_replacement {
            assert_eq!(
                service.lease_status(&replacement).await.unwrap(),
                JobLeaseStatus::Active
            );
            service
                .complete(&replacement, JobCompletion::TaskOnly)
                .await
                .unwrap();
        }
    }
}

#[tokio::test]
async fn expired_unclaimed_attempt_stops_and_is_recoverable() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    let job_id = enqueue(&service).await;
    let (runtime, started, mut result) =
        controlled_runtime(service.clone(), StdDuration::from_millis(40));
    let running = tokio::spawn(async move {
        runtime
            .process_once(&mut pixivarchive_worker::scheduler::default_rotation())
            .await
    });
    let old = started.await.unwrap();
    expire(&locked.db, job_id).await;
    assert_eq!(
        service.lease_status(&old).await.unwrap(),
        JobLeaseStatus::Expired
    );
    tokio::time::timeout(StdDuration::from_secs(5), result.closed())
        .await
        .unwrap();
    assert!(running.await.unwrap().unwrap());
    let replacement = reclaim(&service).await;
    service
        .complete(&replacement, JobCompletion::TaskOnly)
        .await
        .unwrap();
}

#[tokio::test]
async fn completion_of_the_same_attempt_keeps_post_commit_work_alive() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    enqueue(&service).await;
    let (runtime, started, result) =
        controlled_runtime(service.clone(), StdDuration::from_millis(40));
    let running = tokio::spawn(async move {
        runtime
            .process_once(&mut pixivarchive_worker::scheduler::default_rotation())
            .await
    });
    let job = started.await.unwrap();
    service
        .complete(&job, JobCompletion::TaskOnly)
        .await
        .unwrap();
    assert_eq!(
        service.lease_status(&job).await.unwrap(),
        JobLeaseStatus::Completed
    );
    tokio::time::sleep(StdDuration::from_millis(200)).await;
    assert!(!result.is_closed());
    result.send(ExecutorOutcome::Finalized).unwrap();
    assert!(running.await.unwrap().unwrap());
}

#[tokio::test]
async fn conflicts_with_an_active_lease_still_surface_as_errors() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    let job_id = enqueue(&service).await;
    let (runtime, started, result) =
        controlled_runtime(service.clone(), StdDuration::from_secs(60));
    let running = tokio::spawn(async move {
        runtime
            .process_once(&mut pixivarchive_worker::scheduler::default_rotation())
            .await
    });
    let job = started.await.unwrap();
    // Break the attempt invariant while keeping write authority valid. The terminal
    // transaction must roll back, and its conflict must remain a process-level error.
    sqlx::query("DELETE FROM job_attempt WHERE job_id = $1")
        .bind(job_id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    result.send(ExecutorOutcome::completed()).unwrap();
    let error = running.await.unwrap().unwrap_err();
    assert!(matches!(
        error.downcast_ref::<DbError>(),
        Some(DbError::LeaseConflict)
    ));
    assert_eq!(
        service.lease_status(&job).await.unwrap(),
        JobLeaseStatus::Active
    );
}

#[tokio::test]
async fn missing_job_during_conflict_resolution_is_not_hidden() {
    let locked = LockedDb::new().await;
    let service = JobService::new(locked.db.clone());
    let job_id = enqueue(&service).await;
    let (runtime, started, result) = controlled_runtime(service, StdDuration::from_secs(60));
    let running = tokio::spawn(async move {
        runtime
            .process_once(&mut pixivarchive_worker::scheduler::default_rotation())
            .await
    });
    started.await.unwrap();
    sqlx::query("DELETE FROM job WHERE id = $1")
        .bind(job_id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    result.send(ExecutorOutcome::completed()).unwrap();
    let error = running.await.unwrap().unwrap_err();
    assert!(matches!(
        error.downcast_ref::<DbError>(),
        Some(DbError::NotFound)
    ));
}
