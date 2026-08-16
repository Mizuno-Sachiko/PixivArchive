use pixivarchive_db::{Db, DbError, SettingWrite, SettingsRepository};
pub use pixivarchive_domain::job::JobPriorityMapping;
use pixivarchive_domain::{job::JobPriorityPolicy, settings::SettingGroupKey};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeSet,
    num::{NonZeroU16, NonZeroU32},
    path::PathBuf,
};
use thiserror::Error;
use time::Duration;

#[derive(Clone)]
pub struct SettingsService {
    repository: SettingsRepository,
    capabilities: DeploymentCapabilities,
}

impl SettingsService {
    pub fn new(db: Db) -> Self {
        Self::with_capabilities(db, DeploymentCapabilities::default())
    }

    pub fn with_capabilities(db: Db, capabilities: DeploymentCapabilities) -> Self {
        Self {
            repository: SettingsRepository::new(db),
            capabilities,
        }
    }

    pub async fn effective(&self) -> Result<EffectiveSettings, SettingsError> {
        let mut settings = EffectiveSettings::default();
        for stored in self.repository.list().await? {
            let value = SettingValue::deserialize_for_group(stored.group, stored.value)?;
            self.validate(value.clone())?;
            settings.apply(value);
        }
        Ok(settings)
    }

    pub fn validate(&self, value: SettingValue) -> Result<(), SettingsError> {
        value.validate(&self.capabilities)
    }

    pub async fn update(
        &self,
        group: SettingGroupKey,
        expected_revision: Option<i64>,
        value: SettingValue,
    ) -> Result<SavedSetting, SettingsError> {
        self.update_many(vec![SettingUpdate {
            group,
            expected_revision,
            value,
        }])
        .await?
        .into_iter()
        .next()
        .ok_or(SettingsError::EmptyBatch)
    }

    pub async fn update_many(
        &self,
        updates: Vec<SettingUpdate>,
    ) -> Result<Vec<SavedSetting>, SettingsError> {
        if updates.is_empty() {
            return Err(SettingsError::EmptyBatch);
        }

        let mut groups = BTreeSet::new();
        let mut writes = Vec::with_capacity(updates.len());
        for update in updates {
            if update.group != update.value.group() {
                return Err(SettingsError::WrongGroup);
            }
            if !groups.insert(update.group.as_str()) {
                return Err(SettingsError::DuplicateGroup);
            }
            self.validate(update.value.clone())?;
            writes.push(SettingWrite {
                group: update.group,
                expected_revision: update.expected_revision,
                value: update.value.to_stored_value()?,
            });
        }

        Ok(self
            .repository
            .update_many(writes)
            .await?
            .into_iter()
            .map(|stored| SavedSetting {
                group: stored.group,
                revision: stored.revision,
            })
            .collect())
    }
}

pub async fn effective_job_priority_policy(db: &Db) -> Result<JobPriorityPolicy, DbError> {
    SettingsService::new(db.clone())
        .effective()
        .await
        .map_err(|_| DbError::InvalidValue("stored queue settings are invalid".to_owned()))?
        .queue
        .priority_policy()
        .map_err(|_| DbError::InvalidValue("stored queue settings are invalid".to_owned()))
}

#[derive(Clone, Debug)]
pub struct SettingUpdate {
    pub group: SettingGroupKey,
    pub expected_revision: Option<i64>,
    pub value: SettingValue,
}

#[derive(Clone, Debug, Serialize)]
pub struct SavedSetting {
    pub group: SettingGroupKey,
    pub revision: i64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DeploymentCapabilities {
    pub avif_derivatives: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct EffectiveSettings {
    pub security: SecuritySettings,
    pub storage: StorageSettings,
    pub retry: RetrySettings,
    pub queue: QueueSettings,
    pub processing: Option<ProcessingSettings>,
    pub derivative: DerivativeSettings,
    pub ugoira: Option<UgoiraSettings>,
    pub pixiv: PixivSettings,
    pub content: ContentSettings,
}

impl Default for EffectiveSettings {
    fn default() -> Self {
        Self {
            security: SecuritySettings::default(),
            storage: StorageSettings::default(),
            retry: RetrySettings::default(),
            queue: QueueSettings::default(),
            processing: Some(ProcessingSettings::default()),
            derivative: DerivativeSettings::default(),
            ugoira: Some(UgoiraSettings::default()),
            pixiv: PixivSettings::default(),
            content: ContentSettings::default(),
        }
    }
}

impl EffectiveSettings {
    fn apply(&mut self, value: SettingValue) {
        match value {
            SettingValue::Security(value) => self.security = value,
            SettingValue::Storage(value) => self.storage = value,
            SettingValue::Retry(value) => self.retry = value,
            SettingValue::Queue(value) => self.queue = value,
            SettingValue::Processing(value) => self.processing = Some(value),
            SettingValue::Derivative(value) => self.derivative = value,
            SettingValue::Ugoira(value) => self.ugoira = Some(value),
            SettingValue::Pixiv(value) => self.pixiv = value,
            SettingValue::Content(value) => self.content = value,
        }
    }
}

#[derive(Clone, Debug)]
pub enum SettingValue {
    Security(SecuritySettings),
    Storage(StorageSettings),
    Retry(RetrySettings),
    Queue(QueueSettings),
    Processing(ProcessingSettings),
    Derivative(DerivativeSettings),
    Ugoira(UgoiraSettings),
    Pixiv(PixivSettings),
    Content(ContentSettings),
}

impl SettingValue {
    pub fn from_group_value(
        group: SettingGroupKey,
        value: serde_json::Value,
    ) -> Result<Self, SettingsError> {
        Ok(match group {
            SettingGroupKey::Security => Self::Security(from_payload(value)?),
            SettingGroupKey::Storage => Self::Storage(from_payload(value)?),
            SettingGroupKey::Retry => Self::Retry(from_payload(value)?),
            SettingGroupKey::Queue => Self::Queue(from_payload(value)?),
            SettingGroupKey::Processing => Self::Processing(from_payload(value)?),
            SettingGroupKey::Derivative => Self::Derivative(from_payload(value)?),
            SettingGroupKey::Ugoira => Self::Ugoira(from_payload(value)?),
            SettingGroupKey::Pixiv => Self::Pixiv(from_payload(value)?),
            SettingGroupKey::Content => Self::Content(from_payload(value)?),
        })
    }

    fn group(&self) -> SettingGroupKey {
        match self {
            Self::Security(_) => SettingGroupKey::Security,
            Self::Storage(_) => SettingGroupKey::Storage,
            Self::Retry(_) => SettingGroupKey::Retry,
            Self::Queue(_) => SettingGroupKey::Queue,
            Self::Processing(_) => SettingGroupKey::Processing,
            Self::Derivative(_) => SettingGroupKey::Derivative,
            Self::Ugoira(_) => SettingGroupKey::Ugoira,
            Self::Pixiv(_) => SettingGroupKey::Pixiv,
            Self::Content(_) => SettingGroupKey::Content,
        }
    }

    fn deserialize_for_group(
        group: SettingGroupKey,
        value: serde_json::Value,
    ) -> Result<Self, SettingsError> {
        let payload = StoredSettingEnvelope::payload(value)?;
        Ok(match group {
            SettingGroupKey::Security => Self::Security(from_payload(payload)?),
            SettingGroupKey::Storage => Self::Storage(from_payload(payload)?),
            SettingGroupKey::Retry => Self::Retry(from_payload(payload)?),
            SettingGroupKey::Queue => Self::Queue(from_payload(payload)?),
            SettingGroupKey::Processing => Self::Processing(from_payload(payload)?),
            SettingGroupKey::Derivative => Self::Derivative(from_payload(payload)?),
            SettingGroupKey::Ugoira => Self::Ugoira(from_payload(payload)?),
            SettingGroupKey::Pixiv => Self::Pixiv(from_payload(payload)?),
            SettingGroupKey::Content => Self::Content(from_payload(payload)?),
        })
    }

    fn to_stored_value(&self) -> Result<serde_json::Value, SettingsError> {
        let payload = match self {
            Self::Security(value) => serde_json::to_value(value)?,
            Self::Storage(value) => serde_json::to_value(value)?,
            Self::Retry(value) => serde_json::to_value(value)?,
            Self::Queue(value) => serde_json::to_value(value)?,
            Self::Processing(value) => serde_json::to_value(value)?,
            Self::Derivative(value) => serde_json::to_value(value)?,
            Self::Ugoira(value) => serde_json::to_value(value)?,
            Self::Pixiv(value) => serde_json::to_value(value)?,
            Self::Content(value) => serde_json::to_value(value)?,
        };
        serde_json::to_value(StoredSettingEnvelope {
            schema_version: 1,
            payload,
        })
        .map_err(SettingsError::from)
    }

    pub fn validate(&self, capabilities: &DeploymentCapabilities) -> Result<(), SettingsError> {
        match self {
            Self::Security(value) => value.validate(),
            Self::Storage(value) => value.validate(),
            Self::Retry(value) => value.validate(),
            Self::Queue(value) => value.validate(),
            Self::Processing(value) => value.validate(),
            Self::Derivative(value) => value.validate(capabilities),
            Self::Ugoira(value) => value.validate(),
            Self::Pixiv(_) => Ok(()),
            Self::Content(value) => value.validate(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct StoredSettingEnvelope {
    schema_version: u16,
    payload: serde_json::Value,
}

impl StoredSettingEnvelope {
    fn payload(value: serde_json::Value) -> Result<serde_json::Value, SettingsError> {
        let envelope: Self = serde_json::from_value(value)?;
        if envelope.schema_version != 1 {
            return Err(SettingsError::UnsupportedSchemaVersion);
        }
        Ok(envelope.payload)
    }
}

fn from_payload<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, SettingsError> {
    serde_json::from_value(value).map_err(SettingsError::from)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecuritySettings {
    pub session_idle_timeout_seconds: u64,
    pub session_absolute_timeout_seconds: u64,
    pub last_activity_persist_interval_seconds: u64,
    pub password_failures: FailureLimit,
    pub shared_account_failures: FailureLimit,
    pub entry_source_failures: FailureLimit,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            session_idle_timeout_seconds: 30 * 24 * 60 * 60,
            session_absolute_timeout_seconds: 180 * 24 * 60 * 60,
            last_activity_persist_interval_seconds: 60,
            password_failures: FailureLimit::new(5, 10 * 60, 15 * 60),
            shared_account_failures: FailureLimit::new(12, 15 * 60, 15 * 60),
            entry_source_failures: FailureLimit::new(20, 10 * 60, 10 * 60),
        }
    }
}

impl SecuritySettings {
    fn validate(&self) -> Result<(), SettingsError> {
        if !valid_duration_seconds(self.session_idle_timeout_seconds)
            || !valid_duration_seconds(self.session_absolute_timeout_seconds)
            || !valid_duration_seconds(self.last_activity_persist_interval_seconds)
            || self.session_absolute_timeout_seconds < self.session_idle_timeout_seconds
            || self.last_activity_persist_interval_seconds > self.session_idle_timeout_seconds
        {
            return Err(SettingsError::InvalidField("security.session_timeout"));
        }
        self.password_failures
            .validate("security.password_failures")?;
        self.shared_account_failures
            .validate("security.shared_account_failures")?;
        self.entry_source_failures
            .validate("security.entry_source_failures")?;
        Ok(())
    }

    pub fn session_idle_timeout(&self) -> Duration {
        Duration::seconds(
            i64::try_from(self.session_idle_timeout_seconds)
                .expect("validated session idle timeout fits i64"),
        )
    }

    pub fn session_absolute_timeout(&self) -> Duration {
        Duration::seconds(
            i64::try_from(self.session_absolute_timeout_seconds)
                .expect("validated session absolute timeout fits i64"),
        )
    }

    pub fn last_activity_persist_interval(&self) -> Duration {
        Duration::seconds(
            i64::try_from(self.last_activity_persist_interval_seconds)
                .expect("validated activity persistence interval fits i64"),
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FailureLimit {
    pub threshold: u16,
    pub window_seconds: u64,
    pub cooldown_seconds: u64,
}

impl FailureLimit {
    fn new(threshold: u16, window_seconds: u64, cooldown_seconds: u64) -> Self {
        Self {
            threshold,
            window_seconds,
            cooldown_seconds,
        }
    }

    fn validate(&self, field: &'static str) -> Result<(), SettingsError> {
        if self.threshold == 0
            || !valid_duration_seconds(self.window_seconds)
            || !valid_duration_seconds(self.cooldown_seconds)
        {
            Err(SettingsError::InvalidField(field))
        } else {
            Ok(())
        }
    }

    pub fn window(&self) -> Duration {
        Duration::seconds(
            i64::try_from(self.window_seconds).expect("validated failure window fits i64"),
        )
    }

    pub fn cooldown(&self) -> Duration {
        Duration::seconds(
            i64::try_from(self.cooldown_seconds).expect("validated failure cooldown fits i64"),
        )
    }
}

fn valid_duration_seconds(value: u64) -> bool {
    value > 0 && i64::try_from(value).is_ok()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorageSettings {
    #[serde(default)]
    pub media_root: Option<String>,
    pub warning_threshold_bytes: u64,
    pub media_write_stop_threshold_bytes: u64,
    pub trash_retention_days: u16,
}

impl Default for StorageSettings {
    fn default() -> Self {
        Self {
            media_root: None,
            warning_threshold_bytes: 100 * 1024 * 1024 * 1024,
            media_write_stop_threshold_bytes: 32 * 1024 * 1024 * 1024,
            trash_retention_days: 30,
        }
    }
}

impl StorageSettings {
    fn validate(&self) -> Result<(), SettingsError> {
        if self
            .media_root
            .as_deref()
            .is_some_and(|media_root| !media_root.starts_with('/'))
        {
            return Err(SettingsError::InvalidField("storage.media_root"));
        }
        if self.media_write_stop_threshold_bytes >= self.warning_threshold_bytes {
            return Err(SettingsError::InvalidField("storage.thresholds"));
        }
        if !(1..=365).contains(&self.trash_retention_days) {
            return Err(SettingsError::InvalidField("storage.trash_retention_days"));
        }
        Ok(())
    }

    pub fn active_media_root(&self, bootstrap: PathBuf) -> PathBuf {
        self.media_root
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or(bootstrap)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RetrySettings {
    pub network_backoff_seconds: Vec<u32>,
}

impl Default for RetrySettings {
    fn default() -> Self {
        Self {
            network_backoff_seconds: vec![60, 300, 1_200, 3_600],
        }
    }
}

impl RetrySettings {
    fn validate(&self) -> Result<(), SettingsError> {
        if self.network_backoff_seconds.is_empty()
            || self.network_backoff_seconds.contains(&0)
            || self
                .network_backoff_seconds
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
        {
            Err(SettingsError::InvalidField("retry.network_backoff_seconds"))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueSettings {
    pub quota_weights: QueueQuotaWeights,
    pub job_priorities: Vec<JobPriorityMapping>,
}

impl Default for QueueSettings {
    fn default() -> Self {
        Self {
            quota_weights: QueueQuotaWeights::default(),
            job_priorities: JobPriorityPolicy::default().mappings(),
        }
    }
}

impl QueueSettings {
    fn validate(&self) -> Result<(), SettingsError> {
        self.priority_policy().map(|_| ())
    }

    pub fn priority_policy(&self) -> Result<JobPriorityPolicy, SettingsError> {
        JobPriorityPolicy::from_mappings(&self.job_priorities)
            .map_err(|_| SettingsError::InvalidField("queue.job_priorities"))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueueQuotaWeights {
    pub immediate: NonZeroU16,
    pub manual_import: NonZeroU16,
    pub scheduled_collection: NonZeroU16,
    pub background_maintenance: NonZeroU16,
}

impl Default for QueueQuotaWeights {
    fn default() -> Self {
        Self {
            immediate: NonZeroU16::new(4).unwrap(),
            manual_import: NonZeroU16::new(8).unwrap(),
            scheduled_collection: NonZeroU16::new(2).unwrap(),
            background_maintenance: NonZeroU16::new(1).unwrap(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProcessingSettings {
    pub pixiv_request_concurrency: NonZeroU16,
    pub pixiv_request_rate: RateLimit,
    pub media_download_concurrency: NonZeroU16,
    pub media_download_rate: RateLimit,
    pub media_cpu_concurrency: NonZeroU16,
}

impl Default for ProcessingSettings {
    fn default() -> Self {
        Self {
            pixiv_request_concurrency: NonZeroU16::new(2).unwrap(),
            pixiv_request_rate: RateLimit {
                requests: NonZeroU16::new(60).unwrap(),
                per_seconds: 60,
            },
            media_download_concurrency: NonZeroU16::new(2).unwrap(),
            media_download_rate: RateLimit {
                requests: NonZeroU16::new(20).unwrap(),
                per_seconds: 60,
            },
            media_cpu_concurrency: NonZeroU16::new(1).unwrap(),
        }
    }
}

impl ProcessingSettings {
    fn validate(&self) -> Result<(), SettingsError> {
        self.pixiv_request_rate
            .validate("processing.pixiv_request_rate")?;
        self.media_download_rate
            .validate("processing.media_download_rate")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DerivativeSettings {
    pub format: DerivativeFormat,
    pub max_width: u32,
    pub webp_quality: u8,
    pub avif_quality: u8,
}

impl Default for DerivativeSettings {
    fn default() -> Self {
        Self {
            format: DerivativeFormat::Webp,
            max_width: 768,
            webp_quality: 80,
            avif_quality: 50,
        }
    }
}

impl DerivativeSettings {
    fn validate(&self, capabilities: &DeploymentCapabilities) -> Result<(), SettingsError> {
        if self.max_width == 0
            || !(1..=100).contains(&self.webp_quality)
            || !(1..=100).contains(&self.avif_quality)
        {
            return Err(SettingsError::InvalidField("derivative.output"));
        }
        if self.format == DerivativeFormat::Avif {
            if capabilities.avif_derivatives {
                Ok(())
            } else {
                Err(SettingsError::InvalidField("derivative.format"))
            }
        } else {
            Ok(())
        }
    }
}

pub use pixivarchive_domain::media::DerivativeFormat;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UgoiraSettings {
    pub max_zip_bytes: u64,
    pub max_frames: NonZeroU32,
    pub max_pixels_per_frame: u64,
    pub decoded_frame_cache_bytes: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PixivSettings {
    pub default_private_bookmark: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ContentSettings {
    pub overview_allow_nsfw: bool,
    pub mask_non_all_age_thumbnails: bool,
}

impl ContentSettings {
    fn validate(&self) -> Result<(), SettingsError> {
        if self.overview_allow_nsfw && self.mask_non_all_age_thumbnails {
            Err(SettingsError::InvalidField("content.nsfw_visibility"))
        } else {
            Ok(())
        }
    }
}

impl Default for UgoiraSettings {
    fn default() -> Self {
        Self {
            max_zip_bytes: 512 * 1024 * 1024,
            max_frames: NonZeroU32::new(3_000).unwrap(),
            max_pixels_per_frame: 64_000_000,
            decoded_frame_cache_bytes: 512 * 1024 * 1024,
        }
    }
}

impl UgoiraSettings {
    fn validate(&self) -> Result<(), SettingsError> {
        if self.max_zip_bytes == 0
            || self.max_pixels_per_frame == 0
            || self.decoded_frame_cache_bytes == 0
        {
            Err(SettingsError::InvalidField("ugoira.limits"))
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RateLimit {
    pub requests: NonZeroU16,
    pub per_seconds: u32,
}

impl RateLimit {
    fn validate(&self, field: &'static str) -> Result<(), SettingsError> {
        if self.per_seconds == 0 {
            Err(SettingsError::InvalidField(field))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("unknown setting group")]
    UnknownGroup,
    #[error("setting group mismatch")]
    WrongGroup,
    #[error("unsupported setting schema version")]
    UnsupportedSchemaVersion,
    #[error("invalid setting field: {0}")]
    InvalidField(&'static str),
    #[error("setting update batch is empty")]
    EmptyBatch,
    #[error("setting update batch contains a duplicate group")]
    DuplicateGroup,
    #[error("setting revision conflict")]
    RevisionConflict,
    #[error("settings storage failed")]
    Storage,
}

impl From<DbError> for SettingsError {
    fn from(error: DbError) -> Self {
        match error {
            DbError::RevisionConflict => Self::RevisionConflict,
            DbError::InvalidValue(_) => Self::Storage,
            _ => Self::Storage,
        }
    }
}

impl From<serde_json::Error> for SettingsError {
    fn from(_error: serde_json::Error) -> Self {
        Self::Storage
    }
}

#[cfg(test)]
mod tests {
    use super::{ContentSettings, FailureLimit, SecuritySettings, StorageSettings};
    use std::path::PathBuf;

    #[test]
    fn content_settings_do_not_allow_masked_thumbnails_in_overview_decorations() {
        assert!(ContentSettings::default().validate().is_ok());
        assert!(
            ContentSettings {
                overview_allow_nsfw: true,
                mask_non_all_age_thumbnails: true,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn security_settings_reject_unrepresentable_and_incoherent_durations() {
        assert!(
            SecuritySettings {
                session_idle_timeout_seconds: u64::MAX,
                session_absolute_timeout_seconds: u64::MAX,
                ..SecuritySettings::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            SecuritySettings {
                session_idle_timeout_seconds: 60,
                session_absolute_timeout_seconds: 120,
                last_activity_persist_interval_seconds: 61,
                ..SecuritySettings::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            FailureLimit {
                threshold: 1,
                window_seconds: u64::MAX,
                cooldown_seconds: 1,
            }
            .validate("security.test_failures")
            .is_err()
        );
    }

    #[test]
    fn storage_media_root_is_absolute_optional_and_overrides_bootstrap() {
        let mut storage = StorageSettings::default();
        assert_eq!(storage.media_root, None);
        assert_eq!(
            storage.active_media_root(PathBuf::from("/srv/pixivarchive/media")),
            PathBuf::from("/srv/pixivarchive/media")
        );

        storage.media_root = Some("relative/media".to_owned());
        assert!(storage.validate().is_err());
        storage.media_root = Some("/mnt/archive/pixiv".to_owned());
        assert!(storage.validate().is_ok());
        assert_eq!(
            storage.active_media_root(PathBuf::from("/srv/pixivarchive/media")),
            PathBuf::from("/mnt/archive/pixiv")
        );
    }

    #[test]
    fn storage_media_root_defaults_when_reading_older_settings() {
        let storage: StorageSettings = serde_json::from_value(serde_json::json!({
            "warning_threshold_bytes": 200,
            "media_write_stop_threshold_bytes": 100,
            "trash_retention_days": 30
        }))
        .unwrap();

        assert_eq!(storage.media_root, None);
    }
}
