use crate::executors::{
    ExecutorOutcome, JobExecutor,
    download::MediaPipelineConfig,
    processing::{MediaProcessingFailure, prepare_media_source},
};
use crate::storage::{StorageWriteGuard, StorageWriteStatus};
use async_trait::async_trait;
use pixivarchive_db::{Db, DerivativeKind, MediaRepository, SaveDerivative};
use pixivarchive_domain::{
    job::ClaimedJob,
    media::{DerivativeFormat, MediaKind},
};
use pixivarchive_media::{
    DerivativeGenerator, DerivativeRequest, MediaRoot, PixivMediaPaths, UgoiraLimits,
};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Clone)]
pub struct DerivativeExecutor {
    repository: MediaRepository,
    media_root: MediaRoot,
    generator: DerivativeGenerator,
    ugoira_limits: UgoiraLimits,
    format: DerivativeFormat,
    max_width: u32,
    quality: u8,
    storage_write_guard: StorageWriteGuard,
}

impl DerivativeExecutor {
    pub fn new(db: Db, config: &MediaPipelineConfig) -> Self {
        Self {
            repository: MediaRepository::new(db),
            media_root: config.media_root.clone(),
            generator: DerivativeGenerator::new(
                config.derivative_program.clone(),
                config.store.probe_limits,
                config.avif_available,
            ),
            ugoira_limits: config.ugoira,
            format: config.derivative_format,
            max_width: config.derivative_max_width,
            quality: config.derivative_quality,
            storage_write_guard: StorageWriteGuard::new(
                config.media_root.path().to_path_buf(),
                config.storage_write_stop_threshold_bytes,
            ),
        }
    }

    async fn execute_job(
        &self,
        job: &ClaimedJob,
    ) -> Result<DerivativeExecutionStatus, MediaProcessingFailure> {
        if matches!(
            self.storage_write_guard
                .status()
                .await
                .map_err(|_| MediaProcessingFailure::server())?,
            StorageWriteStatus::Stopped
        ) {
            return Ok(DerivativeExecutionStatus::WaitingStorage);
        }
        let payload: MediaRevisionPayload = serde_json::from_value(job.payload.clone())
            .map_err(|_| MediaProcessingFailure::permanent())?;
        let media = self
            .repository
            .load_processing_media(payload.media_revision_id)
            .await
            .map_err(MediaProcessingFailure::database)?;
        let prepared = prepare_media_source(&self.media_root, &media, self.ugoira_limits)
            .await
            .map_err(MediaProcessingFailure::prepare_source)?;
        let (base_relative_path, kind) = match media.media_kind {
            MediaKind::SourceImage => (
                PixivMediaPaths::waterfall_derivative(
                    media.pixiv_artist_id,
                    media.pixiv_work_id,
                    media.page_index,
                    media.revision_number,
                    self.format,
                ),
                DerivativeKind::WaterfallThumbnail,
            ),
            MediaKind::UgoiraZip => (
                PixivMediaPaths::ugoira_cover(
                    media.pixiv_artist_id,
                    media.pixiv_work_id,
                    media.revision_number,
                    self.format,
                ),
                DerivativeKind::UgoiraCover,
            ),
            MediaKind::Derivative => return Err(MediaProcessingFailure::permanent()),
        };
        let base_relative_path =
            base_relative_path.map_err(|_| MediaProcessingFailure::permanent())?;
        let relative_path = if payload.regenerate {
            regeneration_path(&base_relative_path, job.id)
                .ok_or_else(MediaProcessingFailure::permanent)?
        } else {
            base_relative_path
        };
        if matches!(
            self.storage_write_guard
                .status()
                .await
                .map_err(|_| MediaProcessingFailure::server())?,
            StorageWriteStatus::Stopped
        ) {
            return Ok(DerivativeExecutionStatus::WaitingStorage);
        }
        self.repository
            .register_artifact_intent(job.lease(), &relative_path)
            .await
            .map_err(MediaProcessingFailure::database)?;
        let generated = self
            .generator
            .generate(DerivativeRequest {
                source: prepared.path().to_path_buf(),
                destination_root: self.media_root.clone(),
                relative_path: relative_path.clone(),
                format: self.format,
                max_width: self.max_width,
                quality: self.quality,
            })
            .await
            .map_err(MediaProcessingFailure::derivative)?;
        self.repository
            .save_derivative(SaveDerivative {
                lease: job.lease(),
                media_revision_id: media.media_revision_id,
                kind,
                format: generated.format,
                relative_path,
                dimensions: generated.dimensions,
                byte_size: generated.byte_size,
                dominant_color: generated.dominant_color,
                complete_job: true,
            })
            .await
            .map_err(MediaProcessingFailure::database)?;
        Ok(DerivativeExecutionStatus::Completed)
    }
}

#[async_trait]
impl JobExecutor for DerivativeExecutor {
    async fn execute(&self, job: ClaimedJob) -> ExecutorOutcome {
        match self.execute_job(&job).await {
            Ok(DerivativeExecutionStatus::Completed) => ExecutorOutcome::Finalized,
            Ok(DerivativeExecutionStatus::WaitingStorage) => ExecutorOutcome::WaitingStorage,
            Err(error) => ExecutorOutcome::failed(error.error_class(), None),
        }
    }
}

enum DerivativeExecutionStatus {
    Completed,
    WaitingStorage,
}

#[derive(Deserialize)]
struct MediaRevisionPayload {
    media_revision_id: Uuid,
    #[serde(default)]
    regenerate: bool,
}

fn regeneration_path(path: &Path, job_id: Uuid) -> Option<PathBuf> {
    let parent = path.parent()?;
    let stem = path.file_stem()?.to_str()?;
    let extension = path.extension()?.to_str()?;
    Some(parent.join(format!("{stem}_g{}.{extension}", job_id.simple())))
}
