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
    path::{Path, PathBuf},
};
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[derive(Clone)]
pub struct SystemService {
    db: Db,
    repository: SystemRepository,
    media_root: PathBuf,
    version: String,
    git_commit: Option<String>,
    capabilities: SystemCapabilities,
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

    #[tokio::test]
    async fn media_status_describes_a_non_directory_path() {
        let path = std::env::temp_dir().join(format!("pixivarchive-media-{}", Uuid::now_v7()));
        tokio::fs::write(&path, []).await.unwrap();

        let status = media_status(&path).await;

        tokio::fs::remove_file(path).await.unwrap();
        assert_eq!(status.status, "unavailable");
        assert_eq!(status.message.as_deref(), Some("配置的媒体路径不是目录"));
    }
}
