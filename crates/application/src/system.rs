use crate::{
    settings::{SettingsError, SettingsService, effective_job_priority_policy},
    trash::TrashService,
};
use pixivarchive_db::{
    Db, DbError, MediaRepository, SettingsRepository, SourceMediaFile, SystemRepository,
    WorkerHeartbeatRepository,
};
use pixivarchive_domain::job::JobKind;
use pixivarchive_domain::media::MediaFormat;
use pixivarchive_media::MediaRoot;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration as StdDuration, Instant},
};
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use tokio::sync::{Mutex as AsyncMutex, oneshot};
use uuid::Uuid;

#[derive(Clone)]
pub struct SystemService {
    db: Db,
    repository: SystemRepository,
    media_root: PathBuf,
    version: String,
    git_commit: Option<String>,
    capabilities: SystemCapabilities,
    media_usage_tracker: MediaUsageTracker,
}

impl SystemService {
    pub fn new(
        db: Db,
        media_root: PathBuf,
        version: impl Into<String>,
        git_commit: Option<String>,
    ) -> Self {
        Self {
            repository: SystemRepository::new(db.clone()),
            db,
            media_root,
            version: version.into(),
            git_commit,
            capabilities: SystemCapabilities::default(),
            media_usage_tracker: MediaUsageTracker::default(),
        }
    }

    pub fn with_capabilities(mut self, capabilities: SystemCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub async fn readiness(&self) -> Result<(), SystemError> {
        self.repository.readiness().await?;
        let metadata = tokio::fs::metadata(&self.media_root).await?;
        if !metadata.is_dir() {
            return Err(SystemError::MediaRootUnavailable);
        }
        Ok(())
    }

    pub async fn status(&self) -> Result<SystemStatus, SystemError> {
        let database_status = self.repository.status().await?;
        let setting_revisions = SettingsRepository::new(self.db.clone())
            .list()
            .await?
            .into_iter()
            .map(|setting| (setting.group.as_str().to_owned(), setting.revision))
            .collect();
        let storage_settings = SettingsService::new(self.db.clone())
            .effective()
            .await
            .map_err(SystemError::Settings)?
            .storage;
        let capacity = storage_capacity(&self.media_root).await?;
        let media = media_status(&self.media_root).await;
        let worker = worker_status(&self.db).await?;
        Ok(SystemStatus {
            version: self.version.clone(),
            git_commit: self.git_commit.clone(),
            migration_version: database_status.migration_version,
            database: ComponentStatus {
                status: "healthy".to_owned(),
                message: None,
            },
            media,
            worker,
            queue: database_status.queue,
            setting_revisions,
            storage: StorageStatus {
                active_media_root: self.media_root.to_string_lossy().into_owned(),
                total_bytes: capacity.total_bytes,
                available_bytes: capacity.available_bytes,
                warning_threshold_bytes: storage_settings.warning_threshold_bytes,
                write_stop_threshold_bytes: storage_settings.media_write_stop_threshold_bytes,
                write_stopped: capacity.available_bytes
                    <= storage_settings.media_write_stop_threshold_bytes,
            },
            capabilities: self.capabilities,
        })
    }

    pub async fn queue_maintenance(
        &self,
        operation: MaintenanceOperation,
    ) -> Result<MaintenanceAccepted, SystemError> {
        let job_ids = match operation {
            MaintenanceOperation::RegenerateDerivatives => {
                let priority = effective_job_priority_policy(&self.db)
                    .await?
                    .priority_for(JobKind::GenerateDerivative);
                self.repository
                    .enqueue_derivative_regeneration_jobs(priority)
                    .await?
            }
            MaintenanceOperation::ScanExpiredTrash => TrashService::new(self.db.clone())
                .enqueue_due_purges(OffsetDateTime::now_utc(), 1_000)
                .await?
                .into_iter()
                .map(|purge| purge.job_id)
                .collect(),
        };
        Ok(MaintenanceAccepted { operation, job_ids })
    }

    pub async fn media_usage(&self) -> Result<MediaUsage, SystemError> {
        let root = self.media_root.clone();
        let bytes = self
            .media_usage_tracker
            .measure(move || media_directory_size(&root))
            .await
            .map_err(SystemError::Filesystem)?;
        Ok(MediaUsage {
            media_directory_bytes: bytes,
        })
    }
}

const MEDIA_USAGE_CACHE_TTL: StdDuration = StdDuration::from_secs(15);

#[derive(Clone, Copy, Debug)]
struct MediaUsageCache {
    bytes: u64,
    measured_at: Instant,
}

#[derive(Clone, Default)]
struct MediaUsageTracker {
    state: Arc<AsyncMutex<MediaUsageState>>,
}

#[derive(Default)]
struct MediaUsageState {
    cache: Option<MediaUsageCache>,
    scan_running: bool,
    waiters: Vec<oneshot::Sender<SharedMediaUsageResult>>,
}

type SharedMediaUsageResult = Result<u64, MediaUsageFailure>;

#[derive(Clone, Debug)]
struct MediaUsageFailure {
    kind: io::ErrorKind,
    message: String,
}

impl MediaUsageFailure {
    fn into_error(self) -> io::Error {
        io::Error::new(self.kind, self.message)
    }
}

impl From<io::Error> for MediaUsageFailure {
    fn from(error: io::Error) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string(),
        }
    }
}

impl MediaUsageTracker {
    async fn measure<F>(&self, scan: F) -> Result<u64, io::Error>
    where
        F: FnOnce() -> Result<u64, io::Error> + Send + 'static,
    {
        let receiver = {
            let mut state = self.state.lock().await;
            if let Some(cached) = state.cache
                && cached.measured_at.elapsed() < MEDIA_USAGE_CACHE_TTL
            {
                return Ok(cached.bytes);
            }

            let (sender, receiver) = oneshot::channel();
            state.waiters.push(sender);
            if !state.scan_running {
                state.scan_running = true;
                let tracker = self.clone();
                // The tracker owns the scan so a cancelled caller cannot discard its result.
                tokio::spawn(async move {
                    let result = match tokio::task::spawn_blocking(scan).await {
                        Ok(result) => result,
                        Err(error) => Err(io::Error::other(error)),
                    }
                    .map_err(MediaUsageFailure::from);
                    tracker.finish_scan(result).await;
                });
            }
            receiver
        };

        receiver
            .await
            .map_err(|_| io::Error::other("media usage scan ended without a result"))?
            .map_err(MediaUsageFailure::into_error)
    }

    async fn finish_scan(&self, result: SharedMediaUsageResult) {
        let waiters = {
            let mut state = self.state.lock().await;
            state.scan_running = false;
            if let Ok(bytes) = &result {
                state.cache = Some(MediaUsageCache {
                    bytes: *bytes,
                    measured_at: Instant::now(),
                });
            }
            std::mem::take(&mut state.waiters)
        };
        for waiter in waiters {
            let _ = waiter.send(result.clone());
        }
    }

    #[cfg(test)]
    async fn pending_request_count(&self) -> usize {
        self.state.lock().await.waiters.len()
    }
}

async fn worker_status(db: &Db) -> Result<ComponentStatus, DbError> {
    let heartbeat = WorkerHeartbeatRepository::new(db.clone()).current().await?;
    Ok(match heartbeat {
        Some(heartbeat)
            if heartbeat.is_online(OffsetDateTime::now_utc(), Duration::seconds(90)) =>
        {
            ComponentStatus {
                status: "healthy".to_owned(),
                message: None,
            }
        }
        Some(_) => ComponentStatus {
            status: "unavailable".to_owned(),
            message: Some("工作进程已断开".to_owned()),
        },
        None => ComponentStatus {
            status: "unavailable".to_owned(),
            message: Some("工作进程尚未连接".to_owned()),
        },
    })
}

#[derive(Clone)]
pub struct MediaSourceService {
    repository: MediaRepository,
    media_root: MediaRoot,
}

impl MediaSourceService {
    pub fn new(db: Db, media_root: impl Into<MediaRoot>) -> Self {
        Self {
            repository: MediaRepository::new(db),
            media_root: media_root.into(),
        }
    }

    pub async fn source(&self, media_revision_id: uuid::Uuid) -> Result<MediaSource, SystemError> {
        let source = self.repository.source_file(media_revision_id).await?;
        self.resolve(source).await
    }

    pub async fn derivative(&self, derivative_id: uuid::Uuid) -> Result<MediaSource, SystemError> {
        let source = self.repository.derivative_file(derivative_id).await?;
        self.resolve(source).await
    }

    async fn resolve(&self, source: SourceMediaFile) -> Result<MediaSource, SystemError> {
        let path = self
            .media_root
            .resolve_file_async(source.relative_path)
            .await
            .map_err(|_| SystemError::MediaNotFound)?;
        Ok(MediaSource {
            path,
            format: source.format,
            byte_size: source.byte_size,
        })
    }
}

async fn media_status(root: &Path) -> ComponentStatus {
    match tokio::fs::metadata(root).await {
        Ok(metadata) if metadata.is_dir() => ComponentStatus {
            status: "healthy".to_owned(),
            message: None,
        },
        Ok(_) => ComponentStatus {
            status: "unavailable".to_owned(),
            message: Some("配置的媒体路径不是目录".to_owned()),
        },
        Err(error) => {
            tracing::warn!(
                path = %root.display(),
                error = %error,
                "Media directory status could not be read"
            );
            ComponentStatus {
                status: "unavailable".to_owned(),
                message: Some("无法访问媒体目录".to_owned()),
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct MediaSource {
    pub path: PathBuf,
    pub format: MediaFormat,
    pub byte_size: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SystemStatus {
    pub version: String,
    pub git_commit: Option<String>,
    pub migration_version: i64,
    pub database: ComponentStatus,
    pub media: ComponentStatus,
    pub worker: ComponentStatus,
    pub queue: BTreeMap<String, BTreeMap<String, i64>>,
    pub setting_revisions: BTreeMap<String, i64>,
    pub storage: StorageStatus,
    pub capabilities: SystemCapabilities,
}

#[derive(Clone, Debug, Serialize)]
pub struct ComponentStatus {
    pub status: String,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct SystemCapabilities {
    pub webp_derivatives: bool,
    pub avif_derivatives: bool,
    pub reflink: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct StorageStatus {
    pub active_media_root: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub warning_threshold_bytes: u64,
    pub write_stop_threshold_bytes: u64,
    pub write_stopped: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct MediaUsage {
    pub media_directory_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum MaintenanceOperation {
    RegenerateDerivatives,
    ScanExpiredTrash,
}

impl MaintenanceOperation {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "regenerate_derivatives" => Some(Self::RegenerateDerivatives),
            "scan_expired_trash" => Some(Self::ScanExpiredTrash),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::RegenerateDerivatives => "regenerate_derivatives",
            Self::ScanExpiredTrash => "scan_expired_trash",
        }
    }
}

#[derive(Clone, Debug)]
pub struct MaintenanceAccepted {
    pub operation: MaintenanceOperation,
    pub job_ids: Vec<Uuid>,
}

#[derive(Debug, Error)]
pub enum SystemError {
    #[error("system storage failed")]
    Storage(#[from] DbError),
    #[error("media root is unavailable")]
    MediaRootUnavailable,
    #[error("media file was not found")]
    MediaNotFound,
    #[error("media filesystem failed")]
    Filesystem(#[from] std::io::Error),
    #[error("system settings failed")]
    Settings(#[source] SettingsError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageCapacity {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

fn media_directory_size(root: &Path) -> Result<u64, std::io::Error> {
    let metadata = std::fs::symlink_metadata(root)?;
    if !metadata.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            "media root is not a directory",
        ));
    }

    let mut total = 0_u64;
    let mut directories = vec![root.to_owned()];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                directories.push(entry.path());
            } else if file_type.is_file() {
                total = total.saturating_add(entry.metadata()?.len());
            }
        }
    }
    Ok(total)
}

#[cfg(unix)]
pub async fn storage_capacity(root: &Path) -> Result<StorageCapacity, std::io::Error> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let root = root.to_owned();
    tokio::task::spawn_blocking(move || {
        let path = CString::new(root.as_os_str().as_bytes()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "media root contains a null byte",
            )
        })?;
        let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: `path` is a valid NUL-terminated string and `stats` points to writable storage.
        let result = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: a successful `statvfs` call initializes the complete structure.
        let stats = unsafe { stats.assume_init() };
        let block_size = stats.f_frsize;
        Ok(StorageCapacity {
            total_bytes: stats.f_blocks.saturating_mul(block_size),
            available_bytes: stats.f_bavail.saturating_mul(block_size),
        })
    })
    .await
    .map_err(std::io::Error::other)?
}

#[cfg(not(unix))]
pub async fn storage_capacity(_root: &Path) -> Result<StorageCapacity, std::io::Error> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "filesystem capacity is supported by Linux deployments",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };

    #[tokio::test]
    async fn media_status_describes_a_non_directory_path() {
        let path = std::env::temp_dir().join(format!("pixivarchive-media-{}", Uuid::now_v7()));
        tokio::fs::write(&path, []).await.unwrap();

        let status = media_status(&path).await;

        tokio::fs::remove_file(path).await.unwrap();
        assert_eq!(status.status, "unavailable");
        assert_eq!(status.message.as_deref(), Some("配置的媒体路径不是目录"));
    }

    #[test]
    fn media_directory_size_counts_nested_regular_files() {
        let root = std::env::temp_dir().join(format!("pixivarchive-media-{}", Uuid::now_v7()));
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("root.bin"), [0_u8; 3]).unwrap();
        std::fs::write(root.join("nested/child.bin"), [0_u8; 5]).unwrap();

        let size = media_directory_size(&root).unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(size, 8);
    }

    #[tokio::test]
    async fn media_usage_scan_survives_a_cancelled_request_and_populates_the_cache() {
        let tracker = MediaUsageTracker::default();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let first_tracker = tracker.clone();
        let first = tokio::spawn(async move {
            first_tracker
                .measure(move || {
                    let _ = started_tx.send(());
                    release_rx.recv().unwrap();
                    Ok(17)
                })
                .await
        });
        started_rx.await.unwrap();
        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());

        let redundant_scans = Arc::new(AtomicUsize::new(0));
        let second_counter = Arc::clone(&redundant_scans);
        let second_tracker = tracker.clone();
        let second = tokio::spawn(async move {
            second_tracker
                .measure(move || {
                    second_counter.fetch_add(1, Ordering::SeqCst);
                    Ok(99)
                })
                .await
        });
        tokio::time::timeout(StdDuration::from_secs(1), async {
            while tracker.pending_request_count().await < 2 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        release_tx.send(()).unwrap();

        assert_eq!(second.await.unwrap().unwrap(), 17);
        assert_eq!(redundant_scans.load(Ordering::SeqCst), 0);
        assert_eq!(
            tracker
                .measure(|| panic!("a fresh scan should not replace the cached result"))
                .await
                .unwrap(),
            17
        );
    }

    #[cfg(unix)]
    #[test]
    fn media_directory_size_skips_symbolic_links() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!("pixivarchive-media-{}", Uuid::now_v7()));
        let external = root.with_extension("external");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&external).unwrap();
        std::fs::write(root.join("inside.bin"), [0_u8; 3]).unwrap();
        std::fs::write(external.join("outside.bin"), [0_u8; 9]).unwrap();
        symlink(external.join("outside.bin"), root.join("linked.bin")).unwrap();

        let size = media_directory_size(&root).unwrap();

        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&external).unwrap();
        assert_eq!(size, 3);
    }
}
