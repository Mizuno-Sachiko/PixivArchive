use crate::settings::{EffectiveSettings, QueueQuotaWeights as SettingsQueueQuotaWeights};
use pixivarchive_db::jobs::JobHeartbeatRecord;
use pixivarchive_db::{
    ActivatePixivAccount, Db, DbError, JobAttemptRecord, JobCompletion, JobRecord, JobRepository,
    JobStats, PixivAccountRecord,
};
use pixivarchive_domain::job::{
    ClaimedJob, JobErrorClass, JobPriority, JobPriorityPolicy, JobQuotaSelection, JobState, NewJob,
};
use pixivarchive_pixiv::PixivErrorClass;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

pub fn database_error_class(error: &DbError) -> JobErrorClass {
    match error {
        DbError::Connection(_)
        | DbError::Migration(_)
        | DbError::RevisionConflict
        | DbError::LeaseConflict
        | DbError::Query(_) => JobErrorClass::Server,
        DbError::RateLimited { .. } => JobErrorClass::RateLimit,
        DbError::Constraint(_) | DbError::NotFound | DbError::InvalidValue(_) => {
            JobErrorClass::Permanent
        }
    }
}

pub fn pixiv_error_class(error: PixivErrorClass) -> JobErrorClass {
    match error {
        PixivErrorClass::Network => JobErrorClass::Network,
        PixivErrorClass::RateLimited => JobErrorClass::RateLimit,
        PixivErrorClass::CredentialInvalid => JobErrorClass::CredentialInvalid,
        PixivErrorClass::TemporaryPixivError | PixivErrorClass::InvalidJsonOrInterstitial => {
            JobErrorClass::Server
        }
        _ => JobErrorClass::Permanent,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetryDecision {
    RetryAfter(Duration),
    BlockAccount,
    DoNotRetry,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    network_backoff: Vec<Duration>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            network_backoff: [60, 300, 1_200, 3_600]
                .into_iter()
                .map(Duration::seconds)
                .collect(),
        }
    }
}

impl RetryPolicy {
    pub fn new(network_backoff: Vec<Duration>) -> Result<Self, RetryPolicyError> {
        if network_backoff.is_empty()
            || network_backoff
                .iter()
                .any(|duration| *duration <= Duration::ZERO)
        {
            Err(RetryPolicyError::InvalidBackoff)
        } else {
            Ok(Self { network_backoff })
        }
    }

    pub fn from_effective_settings(settings: &EffectiveSettings) -> Result<Self, RetryPolicyError> {
        Self::new(
            settings
                .retry
                .network_backoff_seconds
                .iter()
                .map(|seconds| Duration::seconds(i64::from(*seconds)))
                .collect(),
        )
    }

    pub fn next_attempt(
        &self,
        error_class: JobErrorClass,
        attempt: u32,
        retry_after: Option<Duration>,
    ) -> RetryDecision {
        match error_class {
            JobErrorClass::CredentialInvalid => RetryDecision::BlockAccount,
            JobErrorClass::Permanent => RetryDecision::DoNotRetry,
            JobErrorClass::RateLimit => {
                if attempt as usize > self.network_backoff.len() {
                    return RetryDecision::DoNotRetry;
                }
                if let Some(duration) = retry_after.filter(|duration| *duration >= Duration::ZERO) {
                    RetryDecision::RetryAfter(duration)
                } else {
                    RetryDecision::RetryAfter(self.backoff_for_attempt(attempt))
                }
            }
            JobErrorClass::Network | JobErrorClass::Server => {
                if attempt as usize > self.network_backoff.len() {
                    return RetryDecision::DoNotRetry;
                }
                RetryDecision::RetryAfter(self.backoff_for_attempt(attempt))
            }
        }
    }

    fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        let index = attempt.saturating_sub(1) as usize;
        self.network_backoff[index.min(self.network_backoff.len() - 1)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryPolicyError {
    InvalidBackoff,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueueQuotaWeights {
    pub immediate: u16,
    pub manual_import: u16,
    pub scheduled_collection: u16,
    pub background_maintenance: u16,
}

impl From<&SettingsQueueQuotaWeights> for QueueQuotaWeights {
    fn from(settings: &SettingsQueueQuotaWeights) -> Self {
        Self {
            immediate: settings.immediate.get(),
            manual_import: settings.manual_import.get(),
            scheduled_collection: settings.scheduled_collection.get(),
            background_maintenance: settings.background_maintenance.get(),
        }
    }
}

impl Default for QueueQuotaWeights {
    fn default() -> Self {
        Self::from(&SettingsQueueQuotaWeights::default())
    }
}

impl QueueQuotaWeights {
    pub fn new(
        immediate: u16,
        manual_import: u16,
        scheduled_collection: u16,
        background_maintenance: u16,
    ) -> Result<Self, QueueQuotaError> {
        if immediate == 0
            || manual_import == 0
            || scheduled_collection == 0
            || background_maintenance == 0
        {
            return Err(QueueQuotaError::ZeroWeight);
        }

        Ok(Self {
            immediate,
            manual_import,
            scheduled_collection,
            background_maintenance,
        })
    }

    pub fn rotation(self) -> QueueQuotaRotation {
        QueueQuotaRotation {
            sequence: self.sequence(),
            cursor: 0,
        }
    }

    fn sequence(self) -> Vec<JobPriority> {
        let capacity = self.immediate as usize
            + self.manual_import as usize
            + self.scheduled_collection as usize
            + self.background_maintenance as usize;
        let mut sequence = Vec::with_capacity(capacity);
        sequence.extend(std::iter::repeat_n(
            JobPriority::ManualImport,
            self.manual_import as usize,
        ));
        sequence.extend(std::iter::repeat_n(
            JobPriority::Immediate,
            self.immediate as usize,
        ));
        sequence.extend(std::iter::repeat_n(
            JobPriority::ScheduledCollection,
            self.scheduled_collection as usize,
        ));
        sequence.extend(std::iter::repeat_n(
            JobPriority::BackgroundMaintenance,
            self.background_maintenance as usize,
        ));
        sequence
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueQuotaError {
    ZeroWeight,
}

#[derive(Clone, Debug)]
pub struct QueueQuotaRotation {
    sequence: Vec<JobPriority>,
    cursor: usize,
}

impl QueueQuotaRotation {
    pub fn next_selection(&mut self) -> JobQuotaSelection {
        let priority = self.sequence[self.cursor];
        self.cursor = (self.cursor + 1) % self.sequence.len();
        JobQuotaSelection::with_fallback(priority)
    }
}

#[derive(Clone)]
pub struct JobService {
    repository: JobRepository,
    retry_policy: RetryPolicy,
}

#[derive(Clone, Debug)]
pub struct JobSnapshot {
    pub id: Uuid,
    pub priority: JobPriority,
    pub kind: String,
    pub state: JobState,
    pub attempts: i32,
    pub available_at: OffsetDateTime,
    pub error_class: Option<String>,
    pub retryable: Option<bool>,
    pub next_retry_at: Option<OffsetDateTime>,
    pub resource_revision: i64,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl From<JobRecord> for JobSnapshot {
    fn from(record: JobRecord) -> Self {
        Self {
            id: record.id,
            priority: record.priority,
            kind: record.kind,
            state: record.state,
            attempts: record.attempts,
            available_at: record.available_at,
            error_class: record.error_class,
            retryable: record.retryable,
            next_retry_at: record.next_retry_at,
            resource_revision: record.resource_revision,
            created_at: record.created_at,
            updated_at: record.updated_at,
        }
    }
}

#[derive(Clone, Debug)]
pub struct JobAttempt {
    pub attempt_number: i32,
    pub state: String,
    pub started_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
    pub error_class: Option<String>,
    pub retryable: Option<bool>,
    pub message: Option<String>,
    pub trace_id: Option<Uuid>,
}

impl From<JobAttemptRecord> for JobAttempt {
    fn from(record: JobAttemptRecord) -> Self {
        Self {
            attempt_number: record.attempt_number,
            state: record.state,
            started_at: record.started_at,
            finished_at: record.finished_at,
            error_class: record.error_class,
            retryable: record.retryable,
            message: record.message,
            trace_id: record.trace_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobStatistics {
    pub total: u64,
    pub running: u64,
    pub waiting: u64,
    pub requires_attention: u64,
}

impl From<JobStats> for JobStatistics {
    fn from(stats: JobStats) -> Self {
        Self {
            total: stats.total,
            running: stats.running,
            waiting: stats.waiting,
            requires_attention: stats.requires_attention,
        }
    }
}

#[derive(Clone, Debug)]
pub struct JobHeartbeat {
    pub resource_revision: i64,
    pub lease_expires_at: OffsetDateTime,
}

impl From<JobHeartbeatRecord> for JobHeartbeat {
    fn from(record: JobHeartbeatRecord) -> Self {
        Self {
            resource_revision: record.resource_revision,
            lease_expires_at: record.lease_expires_at,
        }
    }
}

impl JobService {
    pub fn new(db: Db) -> Self {
        Self {
            repository: JobRepository::new(db),
            retry_policy: RetryPolicy::default(),
        }
    }

    pub fn with_retry_policy(db: Db, retry_policy: RetryPolicy) -> Self {
        Self {
            repository: JobRepository::new(db),
            retry_policy,
        }
    }

    pub fn from_effective_settings(
        db: Db,
        settings: &EffectiveSettings,
    ) -> Result<Self, RetryPolicyError> {
        Ok(Self {
            repository: JobRepository::new(db),
            retry_policy: RetryPolicy::from_effective_settings(settings)?,
        })
    }

    pub async fn enqueue(&self, job: NewJob) -> Result<Uuid, DbError> {
        self.repository.enqueue(job).await
    }

    pub async fn list(&self, limit: u16) -> Result<Vec<JobSnapshot>, DbError> {
        Ok(self
            .repository
            .list(limit)
            .await?
            .into_iter()
            .map(JobSnapshot::from)
            .collect())
    }

    pub async fn list_filtered(
        &self,
        limit: u16,
        kind: Option<&str>,
        errors_only: bool,
    ) -> Result<Vec<JobSnapshot>, DbError> {
        Ok(self
            .repository
            .list_filtered(limit, kind, errors_only)
            .await?
            .into_iter()
            .map(JobSnapshot::from)
            .collect())
    }

    pub async fn stats(&self) -> Result<JobStatistics, DbError> {
        Ok(self.repository.stats().await?.into())
    }

    pub async fn get(&self, job_id: Uuid) -> Result<JobSnapshot, DbError> {
        Ok(self.repository.get(job_id).await?.into())
    }

    pub async fn attempts(&self, job_id: Uuid) -> Result<Vec<JobAttempt>, DbError> {
        Ok(self
            .repository
            .list_attempts(job_id)
            .await?
            .into_iter()
            .map(JobAttempt::from)
            .collect())
    }

    pub async fn retry_requested(
        &self,
        job_id: Uuid,
        expected_revision: i64,
    ) -> Result<JobSnapshot, DbError> {
        Ok(self
            .repository
            .retry_requested(job_id, expected_revision)
            .await?
            .into())
    }

    pub async fn cancel_requested(
        &self,
        job_id: Uuid,
        expected_revision: i64,
    ) -> Result<JobSnapshot, DbError> {
        Ok(self
            .repository
            .cancel_requested(job_id, expected_revision)
            .await?
            .into())
    }

    pub async fn claim(
        &self,
        lease_owner: Uuid,
        selection: &JobQuotaSelection,
        lease_duration: Duration,
    ) -> Result<Option<ClaimedJob>, DbError> {
        self.repository
            .claim_next(lease_owner, selection, lease_duration)
            .await
    }

    pub async fn heartbeat(
        &self,
        job_id: Uuid,
        expected_revision: i64,
        lease_owner: Uuid,
        extend_by: Duration,
    ) -> Result<JobHeartbeat, DbError> {
        Ok(self
            .repository
            .heartbeat(job_id, expected_revision, lease_owner, extend_by)
            .await?
            .into())
    }

    pub async fn complete(
        &self,
        claimed: &ClaimedJob,
        completion: JobCompletion,
    ) -> Result<(), DbError> {
        self.repository.complete(claimed.lease(), completion).await
    }

    pub async fn wait_for_storage(&self, claimed: &ClaimedJob) -> Result<(), DbError> {
        self.repository.wait_for_storage(claimed.lease()).await
    }

    pub async fn fail(
        &self,
        claimed: &ClaimedJob,
        error_class: JobErrorClass,
        retry_after: Option<Duration>,
        message: Option<&str>,
    ) -> Result<RetryDecision, DbError> {
        let decision = self
            .failure_decision(claimed, error_class, retry_after)
            .await?;
        self.apply_failure(claimed, error_class, &decision, message)
            .await?;
        Ok(decision)
    }

    pub async fn failure_decision(
        &self,
        claimed: &ClaimedJob,
        error_class: JobErrorClass,
        retry_after: Option<Duration>,
    ) -> Result<RetryDecision, DbError> {
        let retryable_failure_count = self
            .repository
            .retryable_failure_count(claimed.id)
            .await?
            .saturating_add(1);
        Ok(self
            .retry_policy
            .next_attempt(error_class, retryable_failure_count as u32, retry_after))
    }

    pub async fn apply_failure(
        &self,
        claimed: &ClaimedJob,
        error_class: JobErrorClass,
        decision: &RetryDecision,
        message: Option<&str>,
    ) -> Result<(), DbError> {
        match decision {
            RetryDecision::RetryAfter(delay) => {
                self.repository
                    .fail(
                        claimed.lease(),
                        error_class.as_str(),
                        true,
                        Some(time::OffsetDateTime::now_utc() + *delay),
                        message,
                    )
                    .await?;
            }
            RetryDecision::BlockAccount => {
                self.repository
                    .block_account_for_job(claimed.lease(), error_class.as_str(), message)
                    .await?;
            }
            RetryDecision::DoNotRetry => {
                self.repository
                    .fail(claimed.lease(), error_class.as_str(), false, None, message)
                    .await?;
            }
        }
        Ok(())
    }

    pub async fn recover_account(
        &self,
        pixiv_account_id: Uuid,
        priorities: &JobPriorityPolicy,
    ) -> Result<(), DbError> {
        self.repository
            .recover_account(pixiv_account_id, priorities)
            .await
    }

    pub async fn activate_validated_account(
        &self,
        account: ActivatePixivAccount,
        priorities: Option<&JobPriorityPolicy>,
    ) -> Result<PixivAccountRecord, DbError> {
        self.repository
            .activate_validated_account(account, priorities)
            .await
    }

    pub async fn clear_account_credential(
        &self,
        pixiv_account_id: Uuid,
        expected_revision: i64,
    ) -> Result<PixivAccountRecord, DbError> {
        self.repository
            .clear_account_credential(pixiv_account_id, expected_revision)
            .await
    }

    pub async fn block_account(&self, pixiv_account_id: Uuid) -> Result<(), DbError> {
        self.repository
            .block_account(pixiv_account_id, JobErrorClass::CredentialInvalid.as_str())
            .await
    }
}
