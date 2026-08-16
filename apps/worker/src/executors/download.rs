use crate::{
    executors::{ExecutorOutcome, JobExecutor, subscription::PixivContextProvider},
    storage::{StorageWriteGuard, StorageWriteStatus},
};
use async_trait::async_trait;
use pixivarchive_application::pixiv_accounts::PixivAccountContextError;
use pixivarchive_db::{Db, DbError, MediaDownloadItem, MediaRepository, SaveSourceMediaRevision};
use pixivarchive_domain::{
    job::{ClaimedJob, JobErrorClass, JobLease, JobPriority},
    media::{DerivativeFormat, MediaKind},
};
use pixivarchive_media::{
    MediaRoot, MediaStore, MediaStoreConfig, PixivMediaPaths, ReflinkCloner, StorageError,
    UgoiraLimits,
};
use pixivarchive_pixiv::{PixivError, PixivMediaGateway, PixivRequestContext};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use time::Duration;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct MediaPipelineConfig {
    pub media_root: MediaRoot,
    pub store: MediaStoreConfig,
    pub ugoira: UgoiraLimits,
    pub derivative_program: PathBuf,
    pub derivative_format: DerivativeFormat,
    pub derivative_max_width: u32,
    pub derivative_quality: u8,
    pub avif_available: bool,
    pub storage_write_stop_threshold_bytes: u64,
}

#[derive(Clone)]
pub struct DownloadExecutor<G> {
    repository: MediaRepository,
    gateway: G,
    context_provider: Arc<dyn PixivContextProvider>,
    media_root: MediaRoot,
    store: MediaStore,
    reflink: ReflinkCloner,
    storage_write_guard: StorageWriteGuard,
}

impl<G> DownloadExecutor<G>
where
    G: PixivMediaGateway + Clone + 'static,
{
    pub fn new(
        db: Db,
        gateway: G,
        context_provider: Arc<dyn PixivContextProvider>,
        config: &MediaPipelineConfig,
    ) -> Self {
        Self {
            repository: MediaRepository::new(db),
            gateway,
            context_provider,
            media_root: config.media_root.clone(),
            store: MediaStore::new(config.media_root.clone(), config.store),
            reflink: ReflinkCloner::new(),
            storage_write_guard: StorageWriteGuard::new(
                config.media_root.path().to_path_buf(),
                config.storage_write_stop_threshold_bytes,
            ),
        }
    }

    async fn execute_job(&self, job: &ClaimedJob) -> Result<MediaExecutionStatus, MediaFailure> {
        let payload: DownloadMediaPayload = serde_json::from_value(job.payload.clone())
            .map_err(|_| MediaFailure::permanent("下载任务参数无法读取"))?;
        if payload.pixiv_work_id <= 0 {
            return Err(MediaFailure::permanent("Pixiv作品ID无效"));
        }
        let plan = self
            .repository
            .load_download_plan(job.id, payload.work_id)
            .await
            .map_err(MediaFailure::database)?;
        if plan.pixiv_work_id != payload.pixiv_work_id {
            return Err(MediaFailure::permanent("下载任务与作品记录不一致"));
        }
        let context = self
            .context_provider
            .context_for_account(plan.account_id)
            .await
            .map_err(MediaFailure::context)?;

        let pending_items = plan
            .items
            .iter()
            .filter(|item| {
                item.page
                    .current
                    .as_ref()
                    .is_none_or(|current| current.download_job_id != Some(job.id))
            })
            .cloned()
            .collect::<Vec<_>>();
        if pending_items.is_empty() {
            self.repository
                .complete_artifact_job(job.lease())
                .await
                .map_err(MediaFailure::database)?;
            return Ok(MediaExecutionStatus::Finalized);
        }
        let last_item = pending_items.len() - 1;
        for (index, item) in pending_items.into_iter().enumerate() {
            if matches!(
                self.storage_write_guard
                    .status()
                    .await
                    .map_err(|_| MediaFailure::server("存储状态暂时无法读取"))?,
                StorageWriteStatus::Stopped
            ) {
                return Ok(MediaExecutionStatus::WaitingStorage);
            }
            self.download_item(
                job.lease(),
                job.priority,
                &context,
                &plan,
                item,
                index == last_item,
            )
            .await?;
        }
        Ok(MediaExecutionStatus::Finalized)
    }

    async fn download_item(
        &self,
        lease: JobLease,
        priority: JobPriority,
        context: &PixivRequestContext,
        plan: &pixivarchive_db::MediaDownloadPlan,
        item: MediaDownloadItem,
        complete_job: bool,
    ) -> Result<(), MediaFailure> {
        let response = self
            .gateway
            .media(context, plan.pixiv_work_id, item.page.source_url.clone())
            .await
            .map_err(MediaFailure::pixiv)?;
        let expected = match item.media_kind {
            MediaKind::SourceImage => {
                pixivarchive_media::ExpectedMedia::source_image(item.page.format)
            }
            MediaKind::UgoiraZip => pixivarchive_media::ExpectedMedia::ugoira_zip(),
            MediaKind::Derivative => return Err(MediaFailure::permanent("媒体类型无法下载")),
        };
        let expected = response
            .content_type
            .as_ref()
            .map_or(expected.clone(), |content_type| {
                expected.clone().with_content_type(content_type)
            });
        let relative_path = match item.media_kind {
            MediaKind::SourceImage => PixivMediaPaths::original_image(
                plan.pixiv_artist_id,
                plan.pixiv_work_id,
                item.page.page_index,
                item.page.revision,
                item.page.format,
            ),
            MediaKind::UgoiraZip => PixivMediaPaths::ugoira_zip(
                plan.pixiv_artist_id,
                plan.pixiv_work_id,
                item.page.revision,
            ),
            MediaKind::Derivative => return Err(MediaFailure::permanent("媒体类型无法下载")),
        }
        .map_err(|_| MediaFailure::permanent("媒体保存路径无法生成"))?;
        let mut request = pixivarchive_media::IngestRequest::new(relative_path.clone(), expected);
        if let Some(content_length) = response.content_length {
            request = request.with_content_length(content_length);
        }
        self.repository
            .register_artifact_intent(lease, &relative_path)
            .await
            .map_err(MediaFailure::database)?;
        let stored = match item.media_kind {
            MediaKind::UgoiraZip => {
                let manifest = item
                    .ugoira
                    .as_ref()
                    .ok_or_else(|| MediaFailure::permanent("动图帧清单缺失"))?;
                self.store
                    .ingest_ugoira(request, response.body, manifest)
                    .await
                    .map_err(MediaFailure::storage)?
            }
            MediaKind::SourceImage => self
                .store
                .ingest(request, response.body)
                .await
                .map_err(MediaFailure::storage)?,
            MediaKind::Derivative => return Err(MediaFailure::permanent("媒体类型无法下载")),
        };

        if let Some(current) = item.page.current.as_ref()
            && current.sha256 == stored.sha256
            && self
                .media_root
                .resolve_optional_file_async(current.relative_path.clone())
                .await
                .map_err(|_| MediaFailure::server("媒体文件状态暂时无法读取"))?
                .is_some()
        {
            remove_redundant_file(&stored.absolute_path).await?;
            if complete_job {
                self.repository
                    .complete_artifact_job(lease)
                    .await
                    .map_err(MediaFailure::database)?;
            }
            return Ok(());
        }

        if let Some(duplicate) = self
            .repository
            .find_duplicate_source(
                stored.byte_size,
                stored.sha256,
                item.page.current.as_ref().map(|current| current.id),
            )
            .await
            .map_err(MediaFailure::database)?
        {
            let reflink = self.reflink;
            let source_for_log = duplicate.clone();
            let destination_for_log = stored.relative_path.clone();
            let media_root = self.media_root.clone();
            let destination_relative = stored.relative_path.clone();
            let reflink_result = tokio::task::spawn_blocking(move || {
                reflink.clone_identical(&media_root, &duplicate, &destination_relative)
            })
            .await
            .map_err(|_| MediaFailure::server("媒体去重任务意外终止"))?;
            if let Err(error) = reflink_result {
                tracing::warn!(
                    source = %source_for_log.display(),
                    destination = %destination_for_log.display(),
                    error = %error,
                    "keeping validated media because reflink optimization was unavailable"
                );
            }
        }

        self.repository
            .save_source_revision(SaveSourceMediaRevision {
                lease,
                derivative_priority: priority,
                work_id: plan.work_id,
                work_page_id: item.page.work_page_id,
                expected_current_media_revision_id: item
                    .page
                    .current
                    .as_ref()
                    .map(|current| current.id),
                revision_number: item.page.revision,
                media_kind: item.media_kind,
                format: item.page.format,
                source_url: item.page.source_url,
                relative_path,
                byte_size: stored.byte_size,
                sha256: stored.sha256,
                dimensions: stored.probe.dimensions,
                ugoira: item.ugoira,
                complete_job,
            })
            .await
            .map_err(MediaFailure::database)?;
        Ok(())
    }
}

#[async_trait]
impl<G> JobExecutor for DownloadExecutor<G>
where
    G: PixivMediaGateway + Clone + 'static,
{
    async fn execute(&self, job: ClaimedJob) -> ExecutorOutcome {
        match self.execute_job(&job).await {
            Ok(MediaExecutionStatus::Finalized) => ExecutorOutcome::Finalized,
            Ok(MediaExecutionStatus::WaitingStorage) => ExecutorOutcome::WaitingStorage,
            Err(error) => ExecutorOutcome::failed_with_message(
                error.error_class,
                error.retry_after,
                error.message,
            ),
        }
    }
}

enum MediaExecutionStatus {
    Finalized,
    WaitingStorage,
}

#[derive(Deserialize)]
struct DownloadMediaPayload {
    work_id: Uuid,
    pixiv_work_id: i64,
}

struct MediaFailure {
    error_class: JobErrorClass,
    retry_after: Option<Duration>,
    message: String,
}

impl MediaFailure {
    fn permanent(message: impl Into<String>) -> Self {
        Self {
            error_class: JobErrorClass::Permanent,
            retry_after: None,
            message: message.into(),
        }
    }

    fn server(message: impl Into<String>) -> Self {
        Self {
            error_class: JobErrorClass::Server,
            retry_after: None,
            message: message.into(),
        }
    }

    fn database(error: DbError) -> Self {
        let message = error.to_string();
        match pixivarchive_application::jobs::database_error_class(&error) {
            JobErrorClass::Server => Self::server(message),
            JobErrorClass::RateLimit => Self {
                error_class: JobErrorClass::RateLimit,
                retry_after: None,
                message,
            },
            _ => Self::permanent(message),
        }
    }

    fn context(error: PixivAccountContextError) -> Self {
        let message = error.to_string();
        Self {
            error_class: super::subscription::context_error_class(error),
            retry_after: None,
            message,
        }
    }

    fn pixiv(error: PixivError) -> Self {
        let message = error.to_string();
        let retry_after = error.retry_after();
        let error_class = pixivarchive_application::jobs::pixiv_error_class(error.class());
        Self {
            error_class,
            retry_after,
            message,
        }
    }

    fn storage(error: StorageError) -> Self {
        let message = error.to_string();
        match error {
            StorageError::Stream => Self {
                error_class: JobErrorClass::Network,
                retry_after: None,
                message,
            },
            StorageError::Storage => Self::server(message),
            _ => Self::permanent(message),
        }
    }
}

async fn remove_redundant_file(path: &Path) -> Result<(), MediaFailure> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(MediaFailure::server("多余媒体文件无法清理")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pixivarchive_pixiv::PixivErrorClass;

    #[test]
    fn oversized_pixiv_responses_are_not_retried_by_media_downloads() {
        let failure = MediaFailure::pixiv(PixivError::new(PixivErrorClass::ResponseTooLarge, None));

        assert_eq!(failure.error_class, JobErrorClass::Permanent);
    }
}
