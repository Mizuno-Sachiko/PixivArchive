use crate::storage::{MediaArtifactCleaner, StorageWriteGuard, StorageWriteStatus};
use pixivarchive_application::trash::TrashService;
use pixivarchive_application::{
    jobs::{QueueQuotaRotation, QueueQuotaWeights},
    settings::effective_job_priority_policy,
};
use pixivarchive_db::{
    Db, DbError, JobRepository,
    subscriptions::{ScheduleDueSubscription, SubscriptionRepository},
};
use pixivarchive_domain::{
    job::JobKind,
    subscription::{SubscriptionKind, SubscriptionSchedule},
};
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use tokio::sync::Mutex;

pub fn default_rotation() -> QueueQuotaRotation {
    QueueQuotaWeights::default().rotation()
}

#[derive(Clone)]
pub struct SubscriptionScheduler {
    db: Db,
    repository: SubscriptionRepository,
    due_limit: i64,
}

impl SubscriptionScheduler {
    pub fn new(db: Db) -> Self {
        Self {
            db: db.clone(),
            repository: SubscriptionRepository::new(db),
            due_limit: 100,
        }
    }

    pub async fn process_due(&self, now: OffsetDateTime) -> Result<usize, SchedulerError> {
        let priorities = effective_job_priority_policy(&self.db).await?;
        let due = self
            .repository
            .list_due_subscriptions(now, self.due_limit)
            .await?;
        let mut scheduled = 0;
        for candidate in due {
            let next_run_at = SubscriptionSchedule::parse(&candidate.schedule)
                .and_then(|schedule| schedule.next_run_after(candidate.next_run_at, now))
                .map_err(|error| SchedulerError::NextRun(error.to_string()))?;
            let kind = SubscriptionKind::from_db_value(&candidate.kind).ok_or_else(|| {
                SchedulerError::NextRun(format!("unknown subscription kind {}", candidate.kind))
            })?;
            if !matches!(
                self.repository
                    .schedule_due_subscription_with_priority(
                        ScheduleDueSubscription {
                            subscription_id: candidate.id,
                            expected_revision: candidate.revision,
                            expected_next_run_at: candidate.next_run_at,
                            now,
                            next_run_at,
                        },
                        priorities.priority_for(JobKind::for_subscription(kind)),
                    )
                    .await?,
                pixivarchive_db::subscriptions::ScheduleDueSubscriptionResult::Stale
            ) {
                scheduled += 1;
            }
        }
        Ok(scheduled)
    }
}

#[derive(Clone)]
pub struct TrashScheduler {
    service: TrashService,
    interval: Duration,
    due_limit: u32,
    next_scan: Arc<Mutex<Option<OffsetDateTime>>>,
}

#[derive(Clone)]
pub struct StorageScheduler {
    repository: JobRepository,
    artifact_cleaner: MediaArtifactCleaner,
    guard: StorageWriteGuard,
    interval: Duration,
    next_scan: Arc<Mutex<Option<OffsetDateTime>>>,
}

impl StorageScheduler {
    pub fn new(db: Db, guard: StorageWriteGuard) -> Self {
        Self {
            repository: JobRepository::new(db.clone()),
            artifact_cleaner: MediaArtifactCleaner::new(db, guard.media_root().to_path_buf()),
            guard,
            interval: Duration::seconds(15),
            next_scan: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_interval(mut self, interval: Duration) -> Result<Self, SchedulerError> {
        if interval <= Duration::ZERO {
            return Err(SchedulerError::InvalidInterval);
        }
        self.interval = interval;
        Ok(self)
    }

    pub async fn process_due(&self, now: OffsetDateTime) -> Result<usize, SchedulerError> {
        let mut next_scan = self.next_scan.lock().await;
        if next_scan.is_some_and(|scheduled| scheduled > now) {
            return Ok(0);
        }
        let allowed = matches!(self.guard.status().await?, StorageWriteStatus::Allowed);
        let changed = self.repository.set_storage_write_allowed(allowed).await?;
        let cleaned = self.artifact_cleaner.cleanup_terminal(100).await?;
        *next_scan = Some(now + self.interval);
        Ok(changed + cleaned)
    }
}

impl TrashScheduler {
    pub fn new(db: Db) -> Self {
        Self {
            service: TrashService::new(db),
            interval: Duration::minutes(15),
            due_limit: 100,
            next_scan: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_interval(mut self, interval: Duration) -> Result<Self, SchedulerError> {
        if interval <= Duration::ZERO {
            return Err(SchedulerError::InvalidInterval);
        }
        self.interval = interval;
        Ok(self)
    }

    pub async fn process_due(&self, now: OffsetDateTime) -> Result<usize, SchedulerError> {
        let mut next_scan = self.next_scan.lock().await;
        if next_scan.is_some_and(|scheduled| scheduled > now) {
            return Ok(0);
        }
        let enqueued = self.service.enqueue_due_purges(now, self.due_limit).await?;
        *next_scan = Some(now + self.interval);
        Ok(enqueued.len())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("subscription schedule storage failed")]
    Storage(#[from] DbError),
    #[error("next run calculation failed: {0}")]
    NextRun(String),
    #[error("scheduler interval must be positive")]
    InvalidInterval,
    #[error("media filesystem capacity could not be read")]
    Filesystem(#[from] std::io::Error),
}
