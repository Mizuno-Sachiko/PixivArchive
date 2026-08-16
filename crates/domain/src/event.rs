use crate::settings::SettingGroupKey;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventResource {
    Job,
    Rule,
    Work,
    PixivBookmark,
    DeletionMarker,
    Subscription,
    PixivAccount,
    SystemSetting,
    Administrator,
}

impl EventResource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Job => "job",
            Self::Rule => "rule",
            Self::Work => "work",
            Self::PixivBookmark => "pixiv_bookmark",
            Self::DeletionMarker => "deletion_marker",
            Self::Subscription => "subscription",
            Self::PixivAccount => "pixiv_account",
            Self::SystemSetting => "system_setting",
            Self::Administrator => "administrator",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "job" => Some(Self::Job),
            "rule" => Some(Self::Rule),
            "work" => Some(Self::Work),
            "pixiv_bookmark" => Some(Self::PixivBookmark),
            "deletion_marker" => Some(Self::DeletionMarker),
            "subscription" => Some(Self::Subscription),
            "pixiv_account" => Some(Self::PixivAccount),
            "system_setting" => Some(Self::SystemSetting),
            "administrator" => Some(Self::Administrator),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventPayload {
    JobQueued {
        revision: i64,
    },
    JobClaimed {
        revision: i64,
    },
    JobCompleted {
        revision: i64,
    },
    JobFailed {
        revision: i64,
    },
    JobWaitingAccount {
        revision: i64,
    },
    JobReleasedFromAccountWait {
        revision: i64,
    },
    JobWaitingStorage {
        revision: i64,
    },
    JobReleasedFromStorageWait {
        revision: i64,
    },
    JobCancelled {
        revision: i64,
    },
    AdministratorChanged {
        revision: i64,
    },
    SystemSettingChanged {
        group: SettingGroupKey,
        revision: i64,
    },
    RuleChanged {
        revision: i64,
    },
    WorkChanged {
        revision: i64,
    },
    PixivBookmarkChanged {
        revision: i64,
    },
    WorkDeleted {
        revision: i64,
    },
    SubscriptionChanged {
        revision: i64,
    },
    PixivAccountChanged {
        revision: i64,
    },
    DeletionMarkerCreated {
        pixiv_work_id: i64,
        deletion_method: String,
    },
    DeletionMarkerRemoved {
        pixiv_work_id: i64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppEvent {
    pub id: i64,
    pub resource: EventResource,
    pub resource_id: Uuid,
    pub payload: EventPayload,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EventReplayWindow {
    pub events: Vec<AppEvent>,
    pub oldest_event_id: Option<i64>,
    pub latest_event_id: Option<i64>,
    pub snapshot_refresh: bool,
    pub has_more: bool,
}
