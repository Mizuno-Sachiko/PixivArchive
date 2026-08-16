pub mod derivative;
pub mod download;
pub mod import;
mod processing;
pub mod subscription;
pub mod trash;

use async_trait::async_trait;
use pixivarchive_db::{Db, JobCompletion};
use pixivarchive_domain::job::{ClaimedJob, JobErrorClass, JobKind};
use pixivarchive_pixiv::{PixivGateway, PixivMediaGateway};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

use self::{
    derivative::DerivativeExecutor,
    download::{DownloadExecutor, MediaPipelineConfig},
    import::ImportExecutor,
    subscription::{PixivContextProvider, SubscriptionExecutor},
    trash::TrashCleanupExecutor,
};

#[derive(Clone, Debug, PartialEq)]
pub enum ExecutorOutcome {
    Completed(JobCompletion),
    Finalized,
    WaitingStorage,
    Failed {
        error_class: JobErrorClass,
        retry_after: Option<time::Duration>,
        message: Option<String>,
    },
}

impl ExecutorOutcome {
    pub fn completed() -> Self {
        Self::Completed(JobCompletion::TaskOnly)
    }

    pub fn failed(error_class: JobErrorClass, retry_after: Option<time::Duration>) -> Self {
        Self::Failed {
            error_class,
            retry_after,
            message: Some(default_failure_message(error_class).to_owned()),
        }
    }

    pub fn failed_with_message(
        error_class: JobErrorClass,
        retry_after: Option<time::Duration>,
        message: String,
    ) -> Self {
        Self::Failed {
            error_class,
            retry_after,
            message: Some(message),
        }
    }
}

fn default_failure_message(error_class: JobErrorClass) -> &'static str {
    match error_class {
        JobErrorClass::Network => "Pixiv 网络请求失败",
        JobErrorClass::Server => "服务暂时不可用或响应无法处理",
        JobErrorClass::RateLimit => "Pixiv 请求频率受限",
        JobErrorClass::CredentialInvalid => "Pixiv Cookie 已失效",
        JobErrorClass::Permanent => "任务参数或来源数据无法处理",
    }
}

#[async_trait]
pub trait JobExecutor: Send + Sync {
    async fn execute(&self, job: ClaimedJob) -> ExecutorOutcome;
}

#[derive(Clone)]
pub struct ExecutionGate {
    semaphore: Arc<Semaphore>,
    rate: Option<Arc<RateGate>>,
}

impl ExecutionGate {
    pub fn new(
        concurrency: usize,
        rate: Option<(u32, std::time::Duration)>,
    ) -> Result<Self, ExecutionGateError> {
        if concurrency == 0 {
            return Err(ExecutionGateError::InvalidConcurrency);
        }
        let rate = rate
            .map(|(requests, window)| RateGate::new(requests, window).map(Arc::new))
            .transpose()?;
        Ok(Self {
            semaphore: Arc::new(Semaphore::new(concurrency)),
            rate,
        })
    }

    pub async fn enter(&self) -> ExecutionPermit {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("worker execution gate is never closed");
        if let Some(rate) = &self.rate {
            rate.wait().await;
        }
        ExecutionPermit { _permit: permit }
    }
}

pub struct ExecutionPermit {
    _permit: OwnedSemaphorePermit,
}

struct RateGate {
    interval: std::time::Duration,
    next_start: Mutex<tokio::time::Instant>,
}

impl RateGate {
    fn new(requests: u32, window: std::time::Duration) -> Result<Self, ExecutionGateError> {
        if requests == 0 || window.is_zero() {
            return Err(ExecutionGateError::InvalidRate);
        }
        let interval = window / requests;
        if interval.is_zero() {
            return Err(ExecutionGateError::InvalidRate);
        }
        Ok(Self {
            interval,
            next_start: Mutex::new(tokio::time::Instant::now()),
        })
    }

    async fn wait(&self) {
        let scheduled = {
            let mut next_start = self.next_start.lock().await;
            let scheduled = (*next_start).max(tokio::time::Instant::now());
            *next_start = scheduled + self.interval;
            scheduled
        };
        tokio::time::sleep_until(scheduled).await;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ExecutionGateError {
    #[error("worker concurrency must be positive")]
    InvalidConcurrency,
    #[error("worker rate limit must be positive and representable")]
    InvalidRate,
}

#[cfg(test)]
mod error_classification_tests {
    use pixivarchive_application::jobs::database_error_class;
    use pixivarchive_db::DbError;
    use pixivarchive_domain::job::JobErrorClass;

    #[test]
    fn database_runtime_errors_remain_retryable() {
        assert_eq!(
            database_error_class(&DbError::RevisionConflict),
            JobErrorClass::Server
        );
        assert_eq!(
            database_error_class(&DbError::InvalidValue("bad payload".to_owned())),
            JobErrorClass::Permanent
        );
        assert_eq!(
            database_error_class(&DbError::RateLimited {
                retry_after_seconds: 30,
            }),
            JobErrorClass::RateLimit
        );
    }
}

struct GatedExecutor<E> {
    inner: E,
    gate: ExecutionGate,
}

#[async_trait]
impl<E> JobExecutor for GatedExecutor<E>
where
    E: JobExecutor,
{
    async fn execute(&self, job: ClaimedJob) -> ExecutorOutcome {
        let _permit = self.gate.enter().await;
        self.inner.execute(job).await
    }
}

#[derive(Clone, Default)]
pub struct ExecutorRegistry {
    executors: HashMap<&'static str, Arc<dyn JobExecutor>>,
}

impl ExecutorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<E>(&mut self, kind: JobKind, executor: E)
    where
        E: JobExecutor + 'static,
    {
        self.executors.insert(kind.as_str(), Arc::new(executor));
    }

    pub fn register_gated<E>(&mut self, kind: JobKind, executor: E, gate: ExecutionGate)
    where
        E: JobExecutor + 'static,
    {
        self.register(
            kind,
            GatedExecutor {
                inner: executor,
                gate,
            },
        );
    }

    pub fn get(&self, kind: &str) -> Option<Arc<dyn JobExecutor>> {
        self.executors.get(kind).cloned()
    }

    pub fn register_pixiv_discovery<G>(
        &mut self,
        db: Db,
        gateway: G,
        context_provider: Arc<dyn PixivContextProvider>,
    ) where
        G: PixivGateway + Clone + 'static,
    {
        let subscription =
            SubscriptionExecutor::new(db.clone(), gateway.clone(), context_provider.clone());
        self.register(JobKind::RankingCollection, subscription.clone());
        self.register(JobKind::FollowingCollection, subscription.clone());
        self.register(JobKind::BookmarksCollection, subscription);

        let import = ImportExecutor::new(db, gateway, context_provider);
        self.register(JobKind::ImportArtist, import.clone());
        self.register(JobKind::ImportWork, import);
    }

    pub fn register_pixiv_media<G>(
        &mut self,
        db: Db,
        gateway: G,
        context_provider: Arc<dyn PixivContextProvider>,
        config: MediaPipelineConfig,
    ) where
        G: PixivMediaGateway + Clone + 'static,
    {
        self.register(
            JobKind::DownloadMedia,
            DownloadExecutor::new(db.clone(), gateway, context_provider, &config),
        );
        self.register(
            JobKind::GenerateDerivative,
            DerivativeExecutor::new(db.clone(), &config),
        );
        self.register(
            JobKind::PurgeTrash,
            TrashCleanupExecutor::new(db, config.media_root.clone()),
        );
    }

    pub fn register_pixiv_media_with_cpu_gate<G>(
        &mut self,
        db: Db,
        gateway: G,
        context_provider: Arc<dyn PixivContextProvider>,
        config: MediaPipelineConfig,
        cpu_gate: ExecutionGate,
    ) where
        G: PixivMediaGateway + Clone + 'static,
    {
        self.register(
            JobKind::DownloadMedia,
            DownloadExecutor::new(db.clone(), gateway, context_provider, &config),
        );
        self.register_gated(
            JobKind::GenerateDerivative,
            DerivativeExecutor::new(db.clone(), &config),
            cpu_gate,
        );
        self.register(
            JobKind::PurgeTrash,
            TrashCleanupExecutor::new(db, config.media_root.clone()),
        );
    }
}
