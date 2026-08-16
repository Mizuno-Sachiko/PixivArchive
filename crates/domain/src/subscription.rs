use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fmt, str::FromStr};
use thiserror::Error;
use time::{Duration, OffsetDateTime};

pub const MIN_SUBSCRIPTION_INTERVAL_MINUTES: i64 = 15;
pub const MAX_SUBSCRIPTION_INTERVAL_MINUTES: i64 = 43_200;
pub const MAX_SUBSCRIPTION_LOOKBACK_PAGES: u32 = 7;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixivAccountState {
    Unconfigured,
    Validating,
    Normal,
    Restricted,
    CredentialInvalid,
}

impl PixivAccountState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unconfigured => "unconfigured",
            Self::Validating => "validating",
            Self::Normal => "normal",
            Self::Restricted => "restricted",
            Self::CredentialInvalid => "credential_invalid",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "unconfigured" => Some(Self::Unconfigured),
            "validating" => Some(Self::Validating),
            "normal" => Some(Self::Normal),
            "restricted" => Some(Self::Restricted),
            "credential_invalid" => Some(Self::CredentialInvalid),
            _ => None,
        }
    }

    pub fn blocks_subscription_runs(self) -> bool {
        matches!(
            self,
            Self::Unconfigured | Self::Validating | Self::CredentialInvalid
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionKind {
    Ranking,
    Following,
    Bookmarks,
}

impl SubscriptionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ranking => "ranking",
            Self::Following => "following",
            Self::Bookmarks => "bookmarks",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "ranking" => Some(Self::Ranking),
            "following" => Some(Self::Following),
            "bookmarks" => Some(Self::Bookmarks),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubscriptionSchedule {
    pub interval_minutes: i64,
    pub lookback_pages: u32,
}

impl SubscriptionSchedule {
    pub fn new(
        interval_minutes: i64,
        lookback_pages: i64,
    ) -> Result<Self, SubscriptionScheduleError> {
        if !(MIN_SUBSCRIPTION_INTERVAL_MINUTES..=MAX_SUBSCRIPTION_INTERVAL_MINUTES)
            .contains(&interval_minutes)
        {
            return Err(SubscriptionScheduleError::IntervalMinutes);
        }
        let lookback_pages =
            u32::try_from(lookback_pages).map_err(|_| SubscriptionScheduleError::LookbackPages)?;
        if lookback_pages > MAX_SUBSCRIPTION_LOOKBACK_PAGES {
            return Err(SubscriptionScheduleError::LookbackPages);
        }
        Ok(Self {
            interval_minutes,
            lookback_pages,
        })
    }

    pub fn parse(value: &Value) -> Result<Self, SubscriptionScheduleError> {
        let interval_minutes = value
            .get("interval_minutes")
            .and_then(Value::as_i64)
            .ok_or(SubscriptionScheduleError::Malformed)?;
        let lookback_pages = value
            .get("lookback_pages")
            .and_then(Value::as_i64)
            .ok_or(SubscriptionScheduleError::Malformed)?;
        Self::new(interval_minutes, lookback_pages)
    }

    pub fn to_value(self) -> Value {
        json!({
            "interval_minutes": self.interval_minutes,
            "lookback_pages": self.lookback_pages,
        })
    }

    pub fn next_run_after(
        &self,
        scheduled_for: OffsetDateTime,
        now: OffsetDateTime,
    ) -> Result<OffsetDateTime, SubscriptionScheduleError> {
        if scheduled_for > now {
            return Ok(scheduled_for);
        }

        let interval_seconds = self
            .interval_minutes
            .checked_mul(60)
            .ok_or(SubscriptionScheduleError::OutOfRange)?;
        let occurrences = (now - scheduled_for)
            .whole_seconds()
            .checked_div(interval_seconds)
            .and_then(|elapsed| elapsed.checked_add(1))
            .ok_or(SubscriptionScheduleError::OutOfRange)?;
        let advance_seconds = interval_seconds
            .checked_mul(occurrences)
            .ok_or(SubscriptionScheduleError::OutOfRange)?;

        scheduled_for
            .checked_add(Duration::seconds(advance_seconds))
            .ok_or(SubscriptionScheduleError::OutOfRange)
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SubscriptionScheduleError {
    #[error("subscription interval must be between 15 and 43200 minutes")]
    IntervalMinutes,
    #[error("subscription lookback pages must be between 0 and 7")]
    LookbackPages,
    #[error("subscription schedule is malformed")]
    Malformed,
    #[error("next subscription run is out of range")]
    OutOfRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl SubscriptionRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn recent_state(self) -> Option<SubscriptionRecentState> {
        match self {
            Self::Queued | Self::Running => None,
            Self::Succeeded => Some(SubscriptionRecentState::Succeeded),
            Self::Failed => Some(SubscriptionRecentState::Failed),
            Self::Cancelled => Some(SubscriptionRecentState::Paused),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionRecentState {
    NeverRun,
    Running,
    Succeeded,
    Failed,
    Paused,
}

impl SubscriptionRecentState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NeverRun => "never_run",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Paused => "paused",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "never_run" => Some(Self::NeverRun),
            "running" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "paused" => Some(Self::Paused),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    Artist,
    Work,
}

impl ImportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Artist => "artist",
            Self::Work => "work",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "artist" => Some(Self::Artist),
            "work" => Some(Self::Work),
            _ => None,
        }
    }
}

impl fmt::Display for ImportKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportRunStatus {
    Queued,
    Running,
    MetadataSaved,
    DownloadQueued,
    Ignored,
    BlockedByDeletionMarker,
    Failed,
    Cancelled,
}

impl ImportRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::MetadataSaved => "metadata_saved",
            Self::DownloadQueued => "download_queued",
            Self::Ignored => "ignored",
            Self::BlockedByDeletionMarker => "blocked_by_deletion_marker",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "metadata_saved" => Some(Self::MetadataSaved),
            "download_queued" => Some(Self::DownloadQueued),
            "ignored" => Some(Self::Ignored),
            "blocked_by_deletion_marker" => Some(Self::BlockedByDeletionMarker),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn is_successful_terminal(self) -> bool {
        matches!(
            self,
            Self::MetadataSaved
                | Self::DownloadQueued
                | Self::Ignored
                | Self::BlockedByDeletionMarker
        )
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("unknown subscription value: {value}")]
pub struct SubscriptionValueParseError {
    value: &'static str,
}

impl FromStr for SubscriptionKind {
    type Err = SubscriptionValueParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_db_value(value).ok_or(SubscriptionValueParseError {
            value: "subscription_kind",
        })
    }
}
