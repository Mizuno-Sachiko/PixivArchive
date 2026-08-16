use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ConditionValue {
    Number(f64),
    NumberRange(NumberRange),
    Text(String),
    TextList(Vec<String>),
    Date(#[serde(with = "time::serde::rfc3339")] OffsetDateTime),
    DateRange(TimeRange),
    DurationHours(i64),
    DurationDays(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NumberRange {
    pub min: f64,
    pub max: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TimeRange {
    #[serde(with = "time::serde::rfc3339")]
    pub start: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    pub end: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum FieldValue {
    Number(f64),
    Text(String),
    Category(String),
    Tags(Vec<TagValue>),
    Date(#[serde(with = "time::serde::rfc3339")] OffsetDateTime),
    Boolean(bool),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TagValue {
    pub original: String,
    pub translation: Option<String>,
}
