use super::FieldType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    Download,
    MetadataOnly,
    Ignore,
}

impl RuleAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Download => "download",
            Self::MetadataOnly => "metadata_only",
            Self::Ignore => "ignore",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "download" => Some(Self::Download),
            "metadata_only" => Some(Self::MetadataOnly),
            "ignore" => Some(Self::Ignore),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GroupMode {
    All,
    Any,
}

macro_rules! rule_operators {
    ($($variant:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
        #[serde(rename_all = "snake_case")]
        pub enum RuleOperator {
            $($variant),+
        }

        impl RuleOperator {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
        }
    };
}

rule_operators!(
    Equals,
    NotEquals,
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Between,
    NotBetween,
    Contains,
    NotContains,
    StartsWith,
    EndsWith,
    InSet,
    NotInSet,
    InAny,
    NotInAny,
    ContainsAnyTag,
    ContainsAllTags,
    ExcludesAnyTag,
    NotContainsAllTags,
    EqualsTagSet,
    TagNameContains,
    TagNameNotContains,
    CountEquals,
    CountGreaterThanOrEqual,
    CountLessThanOrEqual,
    Before,
    After,
    RecentHours,
    RecentDays,
    IsTrue,
    IsFalse,
    Exists,
    Missing,
);

impl RuleOperator {
    pub fn supports(self, field_type: FieldType) -> bool {
        Self::for_type(field_type).contains(&self)
    }

    pub fn requires_value(self) -> bool {
        !matches!(
            self,
            Self::Exists | Self::Missing | Self::IsTrue | Self::IsFalse
        )
    }

    pub fn for_type(field_type: FieldType) -> &'static [Self] {
        use RuleOperator::*;
        match field_type {
            FieldType::Number => &[
                Equals,
                NotEquals,
                GreaterThan,
                GreaterThanOrEqual,
                LessThan,
                LessThanOrEqual,
                Between,
                NotBetween,
                Exists,
                Missing,
            ],
            FieldType::Text => &[
                Equals,
                NotEquals,
                Contains,
                NotContains,
                StartsWith,
                EndsWith,
                InSet,
                NotInSet,
                Exists,
                Missing,
            ],
            FieldType::Category => &[Equals, NotEquals, InAny, NotInAny, Exists, Missing],
            FieldType::Tags => &[
                ContainsAnyTag,
                ContainsAllTags,
                ExcludesAnyTag,
                NotContainsAllTags,
                EqualsTagSet,
                TagNameContains,
                TagNameNotContains,
                CountEquals,
                CountGreaterThanOrEqual,
                CountLessThanOrEqual,
                Exists,
                Missing,
            ],
            FieldType::Date => &[
                Before,
                After,
                Between,
                RecentHours,
                RecentDays,
                Exists,
                Missing,
            ],
            FieldType::Boolean => &[IsTrue, IsFalse, Exists, Missing],
        }
    }
}
