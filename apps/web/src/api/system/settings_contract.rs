use pixivarchive_application::settings::{
    ContentSettings, DerivativeFormat, DerivativeSettings, EffectiveSettings, FailureLimit,
    PixivSettings, ProcessingSettings, QueueQuotaWeights, QueueSettings, RateLimit, RetrySettings,
    SecuritySettings, StorageSettings, UgoiraSettings,
};
use pixivarchive_domain::job::{JobKind, JobPriority, JobPriorityMapping};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct EffectiveSettingsDto {
    pub security: SecuritySettingsDto,
    pub storage: StorageSettingsDto,
    pub retry: RetrySettingsDto,
    pub queue: QueueSettingsDto,
    #[schema(required)]
    pub processing: Option<ProcessingSettingsDto>,
    pub derivative: DerivativeSettingsDto,
    #[schema(required)]
    pub ugoira: Option<UgoiraSettingsDto>,
    pub pixiv: PixivSettingsDto,
    pub content: ContentSettingsDto,
}

impl From<EffectiveSettings> for EffectiveSettingsDto {
    fn from(settings: EffectiveSettings) -> Self {
        Self {
            security: settings.security.into(),
            storage: settings.storage.into(),
            retry: settings.retry.into(),
            queue: settings.queue.into(),
            processing: settings.processing.map(ProcessingSettingsDto::from),
            derivative: settings.derivative.into(),
            ugoira: settings.ugoira.map(UgoiraSettingsDto::from),
            pixiv: settings.pixiv.into(),
            content: settings.content.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct FailureLimitDto {
    pub threshold: u16,
    pub window_seconds: u64,
    pub cooldown_seconds: u64,
}

impl From<FailureLimit> for FailureLimitDto {
    fn from(limit: FailureLimit) -> Self {
        Self {
            threshold: limit.threshold,
            window_seconds: limit.window_seconds,
            cooldown_seconds: limit.cooldown_seconds,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct SecuritySettingsDto {
    pub session_idle_timeout_seconds: u64,
    pub session_absolute_timeout_seconds: u64,
    pub last_activity_persist_interval_seconds: u64,
    pub password_failures: FailureLimitDto,
    pub shared_account_failures: FailureLimitDto,
    pub entry_source_failures: FailureLimitDto,
}

impl From<SecuritySettings> for SecuritySettingsDto {
    fn from(settings: SecuritySettings) -> Self {
        Self {
            session_idle_timeout_seconds: settings.session_idle_timeout_seconds,
            session_absolute_timeout_seconds: settings.session_absolute_timeout_seconds,
            last_activity_persist_interval_seconds: settings.last_activity_persist_interval_seconds,
            password_failures: settings.password_failures.into(),
            shared_account_failures: settings.shared_account_failures.into(),
            entry_source_failures: settings.entry_source_failures.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct StorageSettingsDto {
    #[schema(required)]
    pub media_root: Option<String>,
    pub warning_threshold_bytes: u64,
    pub media_write_stop_threshold_bytes: u64,
    pub trash_retention_days: u16,
}

impl From<StorageSettings> for StorageSettingsDto {
    fn from(settings: StorageSettings) -> Self {
        Self {
            media_root: settings.media_root,
            warning_threshold_bytes: settings.warning_threshold_bytes,
            media_write_stop_threshold_bytes: settings.media_write_stop_threshold_bytes,
            trash_retention_days: settings.trash_retention_days,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct RetrySettingsDto {
    pub network_backoff_seconds: Vec<u32>,
}

impl From<RetrySettings> for RetrySettingsDto {
    fn from(settings: RetrySettings) -> Self {
        Self {
            network_backoff_seconds: settings.network_backoff_seconds,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobKindDto {
    ScheduledCollection,
    RankingCollection,
    FollowingCollection,
    BookmarksCollection,
    ImportArtist,
    ImportWork,
    DownloadMedia,
    GenerateDerivative,
    PurgeTrash,
}

impl From<JobKind> for JobKindDto {
    fn from(kind: JobKind) -> Self {
        match kind {
            JobKind::ScheduledCollection => Self::ScheduledCollection,
            JobKind::RankingCollection => Self::RankingCollection,
            JobKind::FollowingCollection => Self::FollowingCollection,
            JobKind::BookmarksCollection => Self::BookmarksCollection,
            JobKind::ImportArtist => Self::ImportArtist,
            JobKind::ImportWork => Self::ImportWork,
            JobKind::DownloadMedia => Self::DownloadMedia,
            JobKind::GenerateDerivative => Self::GenerateDerivative,
            JobKind::PurgeTrash => Self::PurgeTrash,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobPriorityDto {
    Immediate,
    ManualImport,
    ScheduledCollection,
    BackgroundMaintenance,
}

impl From<JobPriority> for JobPriorityDto {
    fn from(priority: JobPriority) -> Self {
        match priority {
            JobPriority::Immediate => Self::Immediate,
            JobPriority::ManualImport => Self::ManualImport,
            JobPriority::ScheduledCollection => Self::ScheduledCollection,
            JobPriority::BackgroundMaintenance => Self::BackgroundMaintenance,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct JobPriorityMappingDto {
    pub job_kind: JobKindDto,
    pub priority: JobPriorityDto,
}

impl From<JobPriorityMapping> for JobPriorityMappingDto {
    fn from(mapping: JobPriorityMapping) -> Self {
        Self {
            job_kind: mapping.job_kind.into(),
            priority: mapping.priority.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct QueueQuotaWeightsDto {
    pub immediate: u16,
    pub manual_import: u16,
    pub scheduled_collection: u16,
    pub background_maintenance: u16,
}

impl From<QueueQuotaWeights> for QueueQuotaWeightsDto {
    fn from(weights: QueueQuotaWeights) -> Self {
        Self {
            immediate: weights.immediate.get(),
            manual_import: weights.manual_import.get(),
            scheduled_collection: weights.scheduled_collection.get(),
            background_maintenance: weights.background_maintenance.get(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct QueueSettingsDto {
    pub quota_weights: QueueQuotaWeightsDto,
    pub job_priorities: Vec<JobPriorityMappingDto>,
}

impl From<QueueSettings> for QueueSettingsDto {
    fn from(settings: QueueSettings) -> Self {
        Self {
            quota_weights: settings.quota_weights.into(),
            job_priorities: settings
                .job_priorities
                .into_iter()
                .map(JobPriorityMappingDto::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct RateLimitDto {
    pub requests: u16,
    pub per_seconds: u32,
}

impl From<RateLimit> for RateLimitDto {
    fn from(limit: RateLimit) -> Self {
        Self {
            requests: limit.requests.get(),
            per_seconds: limit.per_seconds,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessingSettingsDto {
    pub pixiv_request_concurrency: u16,
    pub pixiv_request_rate: RateLimitDto,
    pub media_download_concurrency: u16,
    pub media_download_rate: RateLimitDto,
    pub media_cpu_concurrency: u16,
}

impl From<ProcessingSettings> for ProcessingSettingsDto {
    fn from(settings: ProcessingSettings) -> Self {
        Self {
            pixiv_request_concurrency: settings.pixiv_request_concurrency.get(),
            pixiv_request_rate: settings.pixiv_request_rate.into(),
            media_download_concurrency: settings.media_download_concurrency.get(),
            media_download_rate: settings.media_download_rate.into(),
            media_cpu_concurrency: settings.media_cpu_concurrency.get(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DerivativeFormatDto {
    Webp,
    Avif,
}

impl From<DerivativeFormat> for DerivativeFormatDto {
    fn from(format: DerivativeFormat) -> Self {
        match format {
            DerivativeFormat::Webp => Self::Webp,
            DerivativeFormat::Avif => Self::Avif,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct DerivativeSettingsDto {
    pub format: DerivativeFormatDto,
    pub max_width: u32,
    pub webp_quality: u8,
    pub avif_quality: u8,
}

impl From<DerivativeSettings> for DerivativeSettingsDto {
    fn from(settings: DerivativeSettings) -> Self {
        Self {
            format: settings.format.into(),
            max_width: settings.max_width,
            webp_quality: settings.webp_quality,
            avif_quality: settings.avif_quality,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct UgoiraSettingsDto {
    pub max_zip_bytes: u64,
    pub max_frames: u32,
    pub max_pixels_per_frame: u64,
    pub decoded_frame_cache_bytes: u64,
}

impl From<UgoiraSettings> for UgoiraSettingsDto {
    fn from(settings: UgoiraSettings) -> Self {
        Self {
            max_zip_bytes: settings.max_zip_bytes,
            max_frames: settings.max_frames.get(),
            max_pixels_per_frame: settings.max_pixels_per_frame,
            decoded_frame_cache_bytes: settings.decoded_frame_cache_bytes,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct PixivSettingsDto {
    pub default_private_bookmark: bool,
}

impl From<PixivSettings> for PixivSettingsDto {
    fn from(settings: PixivSettings) -> Self {
        Self {
            default_private_bookmark: settings.default_private_bookmark,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct ContentSettingsDto {
    pub overview_allow_nsfw: bool,
    pub mask_non_all_age_thumbnails: bool,
}

impl From<ContentSettings> for ContentSettingsDto {
    fn from(settings: ContentSettings) -> Self {
        Self {
            overview_allow_nsfw: settings.overview_allow_nsfw,
            mask_non_all_age_thumbnails: settings.mask_non_all_age_thumbnails,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum SettingPayloadDto {
    Security(SecuritySettingsDto),
    Storage(StorageSettingsDto),
    Retry(RetrySettingsDto),
    Queue(QueueSettingsDto),
    Processing(ProcessingSettingsDto),
    Derivative(DerivativeSettingsDto),
    Ugoira(UgoiraSettingsDto),
    Pixiv(PixivSettingsDto),
    Content(ContentSettingsDto),
}
