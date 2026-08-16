use crate::{
    executors::{ExecutorOutcome, ExecutorRegistry},
    scheduler::{StorageScheduler, SubscriptionScheduler, TrashScheduler},
    state::WorkerState,
};
use anyhow::Context;
use pixivarchive_application::jobs::{JobService, QueueQuotaRotation, QueueQuotaWeights};
use pixivarchive_db::DbError;
use pixivarchive_domain::job::{JobErrorClass, JobKind, JobQuotaSelection, JobState};
use time::Duration;
use tokio::{task::JoinSet, time::MissedTickBehavior};
use uuid::Uuid;

pub const PIXIV_COLLECTION_JOB_KINDS: &[JobKind] = &[
    JobKind::ScheduledCollection,
    JobKind::RankingCollection,
    JobKind::FollowingCollection,
    JobKind::BookmarksCollection,
    JobKind::ImportArtist,
    JobKind::ImportWork,
];
pub const MEDIA_DOWNLOAD_JOB_KINDS: &[JobKind] = &[JobKind::DownloadMedia];
pub const MEDIA_PROCESSING_JOB_KINDS: &[JobKind] =
    &[JobKind::GenerateDerivative, JobKind::PurgeTrash];

#[derive(Clone)]
pub struct WorkerRuntime {
    service: JobService,
    registry: ExecutorRegistry,
    config: WorkerRuntimeConfig,
    state: WorkerState,
    quota_weights: QueueQuotaWeights,
    job_kinds: Option<Vec<JobKind>>,
    subscription_scheduler: Option<SubscriptionScheduler>,
    trash_scheduler: Option<TrashScheduler>,
    storage_scheduler: Option<StorageScheduler>,
}

#[derive(Clone, Copy)]
pub struct WorkerRuntimeConfig {
    pub max_concurrency: usize,
    pub lease_duration: Duration,
    pub heartbeat_interval: std::time::Duration,
    pub poll_interval: std::time::Duration,
    pub shutdown_grace: std::time::Duration,
}

impl Default for WorkerRuntimeConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 4,
            lease_duration: Duration::minutes(5),
            heartbeat_interval: std::time::Duration::from_secs(30),
            poll_interval: std::time::Duration::from_secs(2),
            shutdown_grace: std::time::Duration::from_secs(30),
        }
    }
}

impl WorkerRuntime {
    pub fn new(
        service: JobService,
        registry: ExecutorRegistry,
        config: WorkerRuntimeConfig,
    ) -> Self {
        Self {
            service,
            registry,
            config,
            state: WorkerState::default(),
            quota_weights: QueueQuotaWeights::default(),
            job_kinds: None,
            subscription_scheduler: None,
            trash_scheduler: None,
            storage_scheduler: None,
        }
    }

    pub fn with_quota_weights(mut self, quota_weights: QueueQuotaWeights) -> Self {
        self.quota_weights = quota_weights;
        self
    }

    pub fn with_job_kinds(mut self, job_kinds: impl IntoIterator<Item = JobKind>) -> Self {
        self.job_kinds = Some(job_kinds.into_iter().collect());
        self
    }

    pub fn with_subscription_scheduler(mut self, scheduler: SubscriptionScheduler) -> Self {
        self.subscription_scheduler = Some(scheduler);
        self
    }

    pub fn with_trash_scheduler(mut self, scheduler: TrashScheduler) -> Self {
        self.trash_scheduler = Some(scheduler);
        self
    }

    pub fn with_storage_scheduler(mut self, scheduler: StorageScheduler) -> Self {
        self.storage_scheduler = Some(scheduler);
        self
    }

    pub fn state(&self) -> WorkerState {
        self.state.clone()
    }

    pub async fn process_once(&self, rotation: &mut QueueQuotaRotation) -> anyhow::Result<bool> {
        let mut selection = rotation.next_selection();
        if let Some(job_kinds) = &self.job_kinds {
            selection = selection.restricted_to(job_kinds.iter().copied());
        }
        self.process_selection(selection).await
    }

    async fn process_tick(&self, rotation: &mut QueueQuotaRotation) -> anyhow::Result<bool> {
        if let Some(scheduler) = &self.storage_scheduler {
            scheduler
                .process_due(time::OffsetDateTime::now_utc())
                .await
                .context("checking media storage capacity")?;
        }
        if let Some(scheduler) = &self.subscription_scheduler {
            scheduler
                .process_due(time::OffsetDateTime::now_utc())
                .await
                .context("processing due subscriptions")?;
        }
        if let Some(scheduler) = &self.trash_scheduler {
            scheduler
                .process_due(time::OffsetDateTime::now_utc())
                .await
                .context("processing due trash entries")?;
        }
        self.process_once(rotation).await
    }

    async fn process_selection(&self, selection: JobQuotaSelection) -> anyhow::Result<bool> {
        let owner = Uuid::now_v7();
        let Some(job) = self
            .service
            .claim(owner, &selection, self.config.lease_duration)
            .await?
        else {
            return Ok(false);
        };

        if let Some(executor) = self.registry.get(&job.kind) {
            let Some(outcome) = self
                .execute_with_heartbeats(executor.clone(), job.clone())
                .await?
            else {
                return Ok(true);
            };
            match outcome {
                ExecutorOutcome::Completed(completion) => {
                    if self
                        .transition_was_cancelled(
                            job.id,
                            self.service.complete(&job, completion).await,
                            "completing",
                        )
                        .await?
                    {
                        return Ok(true);
                    }
                }
                ExecutorOutcome::Finalized => {}
                ExecutorOutcome::WaitingStorage => {
                    if self
                        .transition_was_cancelled(
                            job.id,
                            self.service.wait_for_storage(&job).await,
                            "waiting",
                        )
                        .await?
                    {
                        return Ok(true);
                    }
                }
                ExecutorOutcome::Failed {
                    error_class,
                    retry_after,
                    message,
                } => {
                    let decision = self
                        .service
                        .failure_decision(&job, error_class, retry_after)
                        .await
                        .with_context(|| format!("classifying failure for job {}", job.id))?;
                    if self
                        .transition_was_cancelled(
                            job.id,
                            self.service
                                .apply_failure(&job, error_class, &decision, message.as_deref())
                                .await,
                            "failing",
                        )
                        .await?
                    {
                        return Ok(true);
                    }
                }
            }
        } else {
            let result = self
                .service
                .fail(
                    &job,
                    JobErrorClass::Permanent,
                    None,
                    Some("没有注册此任务类型的执行器"),
                )
                .await
                .map(|_| ());
            if self
                .transition_was_cancelled(job.id, result, "marking unregistered")
                .await?
            {
                return Ok(true);
            }
        }
        Ok(true)
    }

    async fn execute_with_heartbeats(
        &self,
        executor: std::sync::Arc<dyn crate::executors::JobExecutor>,
        job: pixivarchive_domain::job::ClaimedJob,
    ) -> anyhow::Result<Option<ExecutorOutcome>> {
        let mut heartbeat = tokio::time::interval(self.config.heartbeat_interval);
        heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let execute = executor.execute(job.clone());
        tokio::pin!(execute);

        // Keep the heartbeat query independently pollable. An executor may hold the job row
        // while atomically committing its business result; suspending that executor until a
        // heartbeat acquires the same row would deadlock the task with itself.
        let heartbeats = async {
            heartbeat.tick().await;
            loop {
                heartbeat.tick().await;
                if let Err(error) = self
                    .service
                    .heartbeat(
                        job.id,
                        job.resource_revision,
                        job.lease_owner,
                        self.config.lease_duration,
                    )
                    .await
                {
                    return error;
                }
            }
        };
        tokio::pin!(heartbeats);

        tokio::select! {
            biased;
            outcome = &mut execute => Ok(Some(outcome)),
            error = &mut heartbeats => {
                if matches!(error, DbError::RevisionConflict | DbError::LeaseConflict) {
                    match self.service.get(job.id).await {
                        Ok(record) if record.state == JobState::Cancelled => return Ok(None),
                        Ok(record) if record.state == JobState::Completed => {
                            return Ok(Some(execute.await));
                        }
                        _ => {}
                    }
                }
                Err(error).with_context(|| format!("heartbeating job {}", job.id))
            }
        }
    }

    async fn transition_was_cancelled(
        &self,
        job_id: Uuid,
        result: Result<(), DbError>,
        action: &str,
    ) -> anyhow::Result<bool> {
        match result {
            Ok(()) => Ok(false),
            Err(error) if self.is_cancelled_conflict(job_id, &error).await => Ok(true),
            Err(error) => Err(error).with_context(|| format!("{action} job {job_id}")),
        }
    }

    async fn is_cancelled_conflict(&self, job_id: Uuid, error: &DbError) -> bool {
        if !matches!(error, DbError::RevisionConflict | DbError::LeaseConflict) {
            return false;
        }
        matches!(
            self.service.get(job_id).await,
            Ok(record) if record.state == JobState::Cancelled
        )
    }

    pub async fn run_until_shutdown(
        self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.config.max_concurrency > 0,
            "worker max concurrency must be positive"
        );
        let mut rotation = self.quota_weights.rotation();
        let mut active = JoinSet::new();
        let mut poll = tokio::time::interval(self.config.poll_interval);
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        self.state.stop_claiming();
                        break;
                    }
                }
                _ = poll.tick(), if self.state.is_accepting_work()
                    && active.len() < self.config.max_concurrency =>
                {
                    let runtime = self.clone();
                    let mut task_rotation = rotation.clone();
                    let _ = rotation.next_selection();
                    active.spawn(async move {
                        runtime.process_tick(&mut task_rotation).await
                    });
                }
                Some(result) = active.join_next(), if !active.is_empty() => {
                    handle_task_result(result)?;
                }
            }
        }

        let grace = tokio::time::sleep(self.config.shutdown_grace);
        tokio::pin!(grace);
        loop {
            tokio::select! {
                _ = &mut grace => break,
                Some(result) = active.join_next(), if !active.is_empty() => {
                    handle_task_result(result)?;
                    if active.is_empty() {
                        break;
                    }
                }
                else => break,
            }
        }
        Ok(())
    }
}

fn handle_task_result(
    result: Result<anyhow::Result<bool>, tokio::task::JoinError>,
) -> anyhow::Result<()> {
    match result {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => {
            tracing::error!(error = %error, "worker task failed");
            Err(error)
        }
        Err(error) => {
            tracing::error!(error = %error, "worker task join failed");
            Err(anyhow::Error::new(error).context("executor panic"))
        }
    }
}
