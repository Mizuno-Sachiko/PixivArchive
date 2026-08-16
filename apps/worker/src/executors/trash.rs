use crate::executors::{ExecutorOutcome, JobExecutor};
use crate::storage::{MediaPathError, resolve_media_path};
use async_trait::async_trait;
use pixivarchive_db::{Db, DbError, TrashPurgeFailure, TrashRepository};
use pixivarchive_domain::job::{ClaimedJob, JobErrorClass};
use pixivarchive_media::MediaRoot;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Clone)]
pub struct TrashCleanupExecutor {
    repository: TrashRepository,
    media_root: MediaRoot,
}

impl TrashCleanupExecutor {
    pub fn new(db: Db, media_root: impl Into<MediaRoot>) -> Self {
        Self {
            repository: TrashRepository::new(db),
            media_root: media_root.into(),
        }
    }

    async fn execute_job(&self, job: &ClaimedJob) -> Result<(), TrashCleanupError> {
        let payload: TrashCleanupPayload = serde_json::from_value(job.payload.clone())
            .map_err(|_| TrashCleanupError::InvalidPayload)?;
        if !matches!(
            payload.deletion_method.as_str(),
            "manual_purge" | "retention_expired"
        ) {
            return Err(TrashCleanupError::InvalidPayload);
        }
        let plan = match self.repository.load_purge_plan(payload.work_id).await {
            Ok(plan) => plan,
            Err(DbError::NotFound)
                if self
                    .repository
                    .purge_completed(payload.work_id)
                    .await
                    .map_err(TrashCleanupError::Database)? =>
            {
                self.repository
                    .complete_purge_job(job.lease(), payload.work_id, &payload.deletion_method)
                    .await
                    .map_err(TrashCleanupError::Database)?;
                return Ok(());
            }
            Err(error) => return Err(TrashCleanupError::Database(error)),
        };
        self.repository
            .begin_purge_job(job.lease(), payload.work_id)
            .await
            .map_err(TrashCleanupError::Database)?;

        let mut failures = Vec::new();
        let mut permanent_failure = false;
        for relative_path in plan.relative_paths {
            let absolute_path = match resolve_media_path(&self.media_root, &relative_path).await {
                Ok(Some(path)) => path,
                Ok(None) => continue,
                Err(error) => {
                    let invalid_path = matches!(error, MediaPathError::Invalid);
                    if invalid_path {
                        permanent_failure = true;
                    }
                    failures.push(TrashPurgeFailure {
                        relative_path,
                        error: error.to_string(),
                    });
                    continue;
                }
            };
            if let Err(error) = tokio::fs::remove_file(&absolute_path).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                failures.push(TrashPurgeFailure {
                    relative_path,
                    error: error.to_string(),
                });
            }
        }
        if !failures.is_empty() {
            self.repository
                .record_failure_job(job.lease(), payload.work_id, &failures)
                .await
                .map_err(TrashCleanupError::Database)?;
            return Err(if permanent_failure {
                TrashCleanupError::InvalidMediaPath
            } else {
                TrashCleanupError::FileSystem
            });
        }

        self.repository
            .complete_purge_job(job.lease(), payload.work_id, &payload.deletion_method)
            .await
            .map_err(TrashCleanupError::Database)
    }
}

#[async_trait]
impl JobExecutor for TrashCleanupExecutor {
    async fn execute(&self, job: ClaimedJob) -> ExecutorOutcome {
        match self.execute_job(&job).await {
            Ok(()) => ExecutorOutcome::Finalized,
            Err(error) => ExecutorOutcome::failed(error.error_class(), None),
        }
    }
}

#[derive(Deserialize)]
struct TrashCleanupPayload {
    work_id: Uuid,
    deletion_method: String,
}

enum TrashCleanupError {
    InvalidPayload,
    InvalidMediaPath,
    FileSystem,
    Database(DbError),
}

impl TrashCleanupError {
    fn error_class(&self) -> JobErrorClass {
        match self {
            Self::InvalidPayload | Self::InvalidMediaPath => JobErrorClass::Permanent,
            Self::FileSystem => JobErrorClass::Server,
            Self::Database(
                DbError::Connection(_)
                | DbError::Query(_)
                | DbError::LeaseConflict
                | DbError::RevisionConflict,
            ) => JobErrorClass::Server,
            Self::Database(_) => JobErrorClass::Permanent,
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    #[tokio::test]
    async fn media_paths_cannot_follow_a_symlink_outside_the_media_root() {
        let test_root = std::env::temp_dir().join(format!("pixivarchive-trash-{}", Uuid::now_v7()));
        let media_root = test_root.join("media");
        let outside = test_root.join("outside.bin");
        tokio::fs::create_dir_all(&media_root).await.unwrap();
        tokio::fs::write(&outside, b"outside").await.unwrap();
        symlink(&outside, media_root.join("linked.bin")).unwrap();

        let result = resolve_media_path(&MediaRoot::new(media_root), Path::new("linked.bin")).await;

        assert!(matches!(result, Err(MediaPathError::Invalid)));
        assert_eq!(tokio::fs::read(&outside).await.unwrap(), b"outside");
        tokio::fs::remove_dir_all(test_root).await.unwrap();
    }
}
