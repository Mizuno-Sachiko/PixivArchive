use pixivarchive_application::system::storage_capacity;
use pixivarchive_db::{Db, DbError, MediaRepository};
use pixivarchive_media::{MediaPathError as OwnedMediaPathError, MediaRoot};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageWriteStatus {
    Allowed,
    Stopped,
}

#[derive(Clone)]
pub struct StorageWriteGuard {
    media_root: PathBuf,
    stop_threshold_bytes: u64,
}

impl StorageWriteGuard {
    pub fn new(media_root: PathBuf, stop_threshold_bytes: u64) -> Self {
        Self {
            media_root,
            stop_threshold_bytes,
        }
    }

    pub async fn status(&self) -> Result<StorageWriteStatus, std::io::Error> {
        let capacity = storage_capacity(&self.media_root).await?;
        Ok(if capacity.available_bytes <= self.stop_threshold_bytes {
            StorageWriteStatus::Stopped
        } else {
            StorageWriteStatus::Allowed
        })
    }

    pub fn media_root(&self) -> &Path {
        &self.media_root
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum MediaPathError {
    #[error("media path resolves outside the media root")]
    Invalid,
    #[error("media path could not be resolved: {0}")]
    FileSystem(String),
}

pub(crate) async fn resolve_media_path(
    media_root: &MediaRoot,
    relative_path: &Path,
) -> Result<Option<PathBuf>, MediaPathError> {
    match media_root
        .resolve_optional_file_async(relative_path.to_path_buf())
        .await
    {
        Ok(path) => Ok(path),
        Err(
            OwnedMediaPathError::InvalidRoot(_)
            | OwnedMediaPathError::InvalidRelativePath(_)
            | OwnedMediaPathError::UnsafeEntry(_),
        ) => Err(MediaPathError::Invalid),
        Err(error) => Err(MediaPathError::FileSystem(error.to_string())),
    }
}

#[derive(Clone)]
pub struct MediaArtifactCleaner {
    db: Db,
    repository: MediaRepository,
    media_root: MediaRoot,
}

impl MediaArtifactCleaner {
    pub fn new(db: Db, media_root: impl Into<MediaRoot>) -> Self {
        Self {
            repository: MediaRepository::new(db.clone()),
            db,
            media_root: media_root.into(),
        }
    }

    pub async fn cleanup_terminal(&self, limit: u16) -> Result<usize, DbError> {
        let intent_ids = self.repository.terminal_artifact_intent_ids(limit).await?;
        let mut cleaned = 0;
        for intent_id in intent_ids {
            let mut tx = self.db.begin().await?;
            let Some(intent) = self
                .repository
                .lock_terminal_artifact_intent_in_tx(&mut tx, intent_id)
                .await?
            else {
                tx.rollback().await?;
                continue;
            };
            if !intent.referenced {
                let removal =
                    match resolve_media_path(&self.media_root, &intent.relative_path).await {
                        Ok(Some(path)) => match tokio::fs::remove_file(path).await {
                            Ok(()) => Ok(()),
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                            Err(error) => Err(error.to_string()),
                        },
                        Ok(None) => Ok(()),
                        Err(error) => Err(error.to_string()),
                    };
                if let Err(error) = removal {
                    self.repository
                        .record_artifact_cleanup_failure_in_tx(&mut tx, intent.id, &error)
                        .await?;
                    tx.commit().await?;
                    tracing::warn!(
                        job_id = %intent.job_id,
                        path = %intent.relative_path.display(),
                        error,
                        "media artifact cleanup failed"
                    );
                    continue;
                }
            }
            self.repository
                .delete_artifact_intent_in_tx(&mut tx, intent.id)
                .await?;
            tx.commit().await?;
            cleaned += 1;
        }
        Ok(cleaned)
    }
}
