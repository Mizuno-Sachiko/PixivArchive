use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemSetting {
    pub key: String,
    pub value: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingGroupKey {
    Security,
    Storage,
    Retry,
    Queue,
    Processing,
    Derivative,
    Ugoira,
    Pixiv,
    Content,
}

impl SettingGroupKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Security => "security",
            Self::Storage => "storage",
            Self::Retry => "retry",
            Self::Queue => "queue",
            Self::Processing => "processing",
            Self::Derivative => "derivative",
            Self::Ugoira => "ugoira",
            Self::Pixiv => "pixiv",
            Self::Content => "content",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "security" => Some(Self::Security),
            "storage" => Some(Self::Storage),
            "retry" => Some(Self::Retry),
            "queue" => Some(Self::Queue),
            "processing" => Some(Self::Processing),
            "derivative" => Some(Self::Derivative),
            "ugoira" => Some(Self::Ugoira),
            "pixiv" => Some(Self::Pixiv),
            "content" => Some(Self::Content),
            _ => None,
        }
    }
}
