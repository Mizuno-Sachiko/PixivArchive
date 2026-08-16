use crate::subscription::SubscriptionKind;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;
use std::{fmt, str::FromStr};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobPriority {
    Immediate,
    ManualImport,
    ScheduledCollection,
    BackgroundMaintenance,
}

impl JobPriority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::ManualImport => "manual_import",
            Self::ScheduledCollection => "scheduled_collection",
            Self::BackgroundMaintenance => "background_maintenance",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "immediate" => Some(Self::Immediate),
            "manual_import" => Some(Self::ManualImport),
            "scheduled_collection" => Some(Self::ScheduledCollection),
            "background_maintenance" => Some(Self::BackgroundMaintenance),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobState {
    Queued,
    Running,
    WaitingAccount,
    WaitingStorage,
    Completed,
    Failed,
    Cancelled,
}

impl JobState {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::WaitingAccount => "waiting_account",
            Self::WaitingStorage => "waiting_storage",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "waiting_account" => Some(Self::WaitingAccount),
            "waiting_storage" => Some(Self::WaitingStorage),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum JobKind {
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

impl JobKind {
    pub const ALL: [Self; 9] = [
        Self::ScheduledCollection,
        Self::RankingCollection,
        Self::FollowingCollection,
        Self::BookmarksCollection,
        Self::ImportArtist,
        Self::ImportWork,
        Self::DownloadMedia,
        Self::GenerateDerivative,
        Self::PurgeTrash,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ScheduledCollection => "scheduled_collection",
            Self::RankingCollection => "ranking_collection",
            Self::FollowingCollection => "following_collection",
            Self::BookmarksCollection => "bookmarks_collection",
            Self::ImportArtist => "import_artist",
            Self::ImportWork => "import_work",
            Self::DownloadMedia => "download_media",
            Self::GenerateDerivative => "generate_derivative",
            Self::PurgeTrash => "purge_trash",
        }
    }

    pub fn parse(value: &str) -> Result<Self, JobKindParseError> {
        Self::from_str(value)
    }

    pub const fn for_subscription(kind: SubscriptionKind) -> Self {
        match kind {
            SubscriptionKind::Ranking => Self::RankingCollection,
            SubscriptionKind::Following => Self::FollowingCollection,
            SubscriptionKind::Bookmarks => Self::BookmarksCollection,
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::ScheduledCollection => 0,
            Self::RankingCollection => 1,
            Self::FollowingCollection => 2,
            Self::BookmarksCollection => 3,
            Self::ImportArtist => 4,
            Self::ImportWork => 5,
            Self::DownloadMedia => 6,
            Self::GenerateDerivative => 7,
            Self::PurgeTrash => 8,
        }
    }
}

impl FromStr for JobKind {
    type Err = JobKindParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "scheduled_collection" => Ok(Self::ScheduledCollection),
            "ranking_collection" => Ok(Self::RankingCollection),
            "following_collection" => Ok(Self::FollowingCollection),
            "bookmarks_collection" => Ok(Self::BookmarksCollection),
            "import_artist" => Ok(Self::ImportArtist),
            "import_work" => Ok(Self::ImportWork),
            "download_media" => Ok(Self::DownloadMedia),
            "generate_derivative" => Ok(Self::GenerateDerivative),
            "purge_trash" => Ok(Self::PurgeTrash),
            _ => Err(JobKindParseError {
                value: value.to_owned(),
            }),
        }
    }
}

impl fmt::Display for JobKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Serialize for JobKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for JobKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(de::Error::custom)
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("unknown job kind: {value}")]
pub struct JobKindParseError {
    value: String,
}

impl JobKindParseError {
    pub fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JobPriorityMapping {
    pub job_kind: JobKind,
    pub priority: JobPriority,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobPriorityPolicy {
    priorities: [JobPriority; JobKind::ALL.len()],
}

impl JobPriorityPolicy {
    pub fn from_mappings(mappings: &[JobPriorityMapping]) -> Result<Self, JobPriorityPolicyError> {
        let mut priorities = [None; JobKind::ALL.len()];
        for mapping in mappings {
            let slot = &mut priorities[mapping.job_kind.index()];
            if slot.replace(mapping.priority).is_some() {
                return Err(JobPriorityPolicyError::Duplicate(mapping.job_kind));
            }
        }
        for kind in JobKind::ALL {
            if priorities[kind.index()].is_none() {
                return Err(JobPriorityPolicyError::Missing(kind));
            }
        }
        Ok(Self {
            priorities: priorities.map(|priority| priority.expect("every job kind was checked")),
        })
    }

    pub fn priority_for(&self, kind: JobKind) -> JobPriority {
        self.priorities[kind.index()]
    }

    pub fn mappings(&self) -> Vec<JobPriorityMapping> {
        JobKind::ALL
            .into_iter()
            .map(|job_kind| JobPriorityMapping {
                job_kind,
                priority: self.priority_for(job_kind),
            })
            .collect()
    }
}

impl Default for JobPriorityPolicy {
    fn default() -> Self {
        Self {
            priorities: [
                JobPriority::ScheduledCollection,
                JobPriority::ScheduledCollection,
                JobPriority::ScheduledCollection,
                JobPriority::ScheduledCollection,
                JobPriority::ManualImport,
                JobPriority::ManualImport,
                JobPriority::BackgroundMaintenance,
                JobPriority::BackgroundMaintenance,
                JobPriority::BackgroundMaintenance,
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum JobPriorityPolicyError {
    #[error("job priority mapping is missing {0}")]
    Missing(JobKind),
    #[error("job priority mapping contains {0} more than once")]
    Duplicate(JobKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobErrorClass {
    Network,
    Server,
    RateLimit,
    CredentialInvalid,
    Permanent,
}

impl JobErrorClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Server => "server",
            Self::RateLimit => "rate_limit",
            Self::CredentialInvalid => "credential_invalid",
            Self::Permanent => "permanent",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "network" => Some(Self::Network),
            "server" => Some(Self::Server),
            "rate_limit" => Some(Self::RateLimit),
            "credential_invalid" => Some(Self::CredentialInvalid),
            "permanent" => Some(Self::Permanent),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct NewJob {
    pub priority: JobPriority,
    pub kind: String,
    pub payload: Value,
    pub pixiv_account_id: Option<Uuid>,
    pub available_at: OffsetDateTime,
}

impl NewJob {
    pub fn new(priority: JobPriority, kind: impl Into<String>, payload: Value) -> Self {
        Self {
            priority,
            kind: kind.into(),
            payload,
            pixiv_account_id: None,
            available_at: OffsetDateTime::now_utc(),
        }
    }

    pub fn for_kind(priority: JobPriority, kind: JobKind, payload: Value) -> Self {
        Self::new(priority, kind.as_str(), payload)
    }
}

#[derive(Clone, Debug)]
pub struct ClaimedJob {
    pub id: Uuid,
    pub priority: JobPriority,
    pub kind: String,
    pub payload: Value,
    pub state: JobState,
    pub attempt_number: i32,
    pub lease_owner: Uuid,
    pub lease_expires_at: OffsetDateTime,
    pub resource_revision: i64,
}

impl ClaimedJob {
    pub fn lease(&self) -> JobLease {
        JobLease {
            job_id: self.id,
            resource_revision: self.resource_revision,
            lease_owner: self.lease_owner,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobLease {
    pub job_id: Uuid,
    pub resource_revision: i64,
    pub lease_owner: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobQuotaSelection {
    priorities: Vec<JobPriority>,
    job_kinds: Option<Vec<JobKind>>,
}

impl JobQuotaSelection {
    pub fn new(priorities: Vec<JobPriority>) -> Self {
        Self {
            priorities,
            job_kinds: None,
        }
    }

    pub fn with_fallback(primary: JobPriority) -> Self {
        let mut priorities = vec![primary];
        for priority in [
            JobPriority::ManualImport,
            JobPriority::Immediate,
            JobPriority::ScheduledCollection,
            JobPriority::BackgroundMaintenance,
        ] {
            if priority != primary {
                priorities.push(priority);
            }
        }
        Self {
            priorities,
            job_kinds: None,
        }
    }

    pub fn restricted_to(mut self, job_kinds: impl IntoIterator<Item = JobKind>) -> Self {
        self.job_kinds = Some(job_kinds.into_iter().collect());
        self
    }

    pub fn priority_values(&self) -> Vec<String> {
        self.priorities
            .iter()
            .map(|priority| priority.as_str().to_owned())
            .collect()
    }

    pub fn kind_values(&self) -> Vec<String> {
        self.job_kinds
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|kind| kind.as_str().to_owned())
            .collect()
    }

    pub fn has_kind_restriction(&self) -> bool {
        self.job_kinds.is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.priorities.is_empty()
            || self
                .job_kinds
                .as_ref()
                .is_some_and(|job_kinds| job_kinds.is_empty())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CollectionState {
    Collected,
    MetadataOnly,
    Trash,
}

impl CollectionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Collected => "collected",
            Self::MetadataOnly => "metadata_only",
            Self::Trash => "trash",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "collected" => Some(Self::Collected),
            "metadata_only" => Some(Self::MetadataOnly),
            "trash" => Some(Self::Trash),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkSummary {
    pub id: Uuid,
    pub pixiv_id: i64,
    pub collection_state: CollectionState,
    pub resource_revision: i64,
}
