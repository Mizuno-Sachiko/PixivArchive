use super::options::value_options;
use super::{
    FieldScope, FieldType, GroupMode, RuleAction, RuleField, RuleOperator, value::ConditionValue,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use uuid::Uuid;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct RuleDefinitionV1 {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub group_mode: GroupMode,
    pub groups: Vec<ConditionGroup>,
    pub action: RuleAction,
    pub default_action: RuleAction,
}

impl RuleDefinitionV1 {
    pub fn match_all(
        id: Uuid,
        name: impl Into<String>,
        action: RuleAction,
        default_action: RuleAction,
    ) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            id,
            name: name.into(),
            enabled: true,
            group_mode: GroupMode::All,
            groups: vec![ConditionGroup {
                mode: GroupMode::All,
                conditions: vec![Condition {
                    field: RuleField::PixivWorkId,
                    operator: RuleOperator::GreaterThan,
                    value: Some(ConditionValue::Number(0.0)),
                    case_sensitive: None,
                    tag_scope: None,
                    page_quantifier: None,
                }],
            }],
            action,
            default_action,
        }
    }

    pub fn parse(value: serde_json::Value) -> Result<Self, RuleError> {
        let document: Self = serde_json::from_value(value)?;
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<(), RuleError> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(RuleError::UnsupportedSchemaVersion(self.schema_version));
        }
        if self.name.trim().is_empty() {
            return Err(RuleError::InvalidRule {
                reason: "rule name is empty",
            });
        }
        if self.groups.is_empty() {
            return Err(RuleError::InvalidRule {
                reason: "rule has no condition groups",
            });
        }
        for (group_index, group) in self.groups.iter().enumerate() {
            group.validate(group_index)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct ConditionGroup {
    pub mode: GroupMode,
    pub conditions: Vec<Condition>,
}

impl ConditionGroup {
    fn validate(&self, group_index: usize) -> Result<(), RuleError> {
        if self.conditions.is_empty() {
            return Err(RuleError::InvalidGroup {
                group_index,
                reason: "condition group is empty",
            });
        }
        for (condition_index, condition) in self.conditions.iter().enumerate() {
            condition.validate(group_index, condition_index)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub field: RuleField,
    pub operator: RuleOperator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ConditionValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_sensitive: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_scope: Option<TagScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_quantifier: Option<PageQuantifier>,
}

impl Condition {
    fn validate(&self, group_index: usize, condition_index: usize) -> Result<(), RuleError> {
        let path = ConditionPath {
            group_index,
            condition_index,
        };
        if self.field.scope() == FieldScope::Page && self.page_quantifier.is_none() {
            return Err(RuleError::InvalidCondition {
                path,
                reason: "page-scoped condition requires a page quantifier",
            });
        }
        if self.field.scope() == FieldScope::Work && self.page_quantifier.is_some() {
            return Err(RuleError::InvalidCondition {
                path,
                reason: "work-scoped condition cannot have a page quantifier",
            });
        }
        if self.field != RuleField::Tags && self.tag_scope.is_some() {
            return Err(RuleError::InvalidCondition {
                path,
                reason: "tag scope is only valid for tag conditions",
            });
        }
        if !matches!(self.field.value_type(), FieldType::Text | FieldType::Tags)
            && self.case_sensitive.is_some()
        {
            return Err(RuleError::InvalidCondition {
                path,
                reason: "case sensitivity is only valid for text and tag conditions",
            });
        }
        if !self.operator.supports(self.field.value_type()) {
            return Err(RuleError::InvalidCondition {
                path,
                reason: "operator is not valid for field type",
            });
        }
        if !value_matches_condition(self.field.value_type(), self.operator, self.value.as_ref()) {
            return Err(RuleError::InvalidCondition {
                path,
                reason: "condition value does not match field and operator",
            });
        }
        if !value_matches_options(self.field, self.value.as_ref()) {
            return Err(RuleError::InvalidCondition {
                path,
                reason: "condition value is not available for field",
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TagScope {
    Original,
    OriginalAndTranslation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PageQuantifier {
    AnyPage,
    AllPages,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConditionPath {
    pub group_index: usize,
    pub condition_index: usize,
}

impl fmt::Display for ConditionPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "groups[{}].conditions[{}]",
            self.group_index, self.condition_index
        )
    }
}

#[derive(Debug, Error)]
pub enum RuleError {
    #[error("invalid rule JSON")]
    Json(#[from] serde_json::Error),
    #[error("unsupported rule schema version {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("invalid rule: {reason}")]
    InvalidRule { reason: &'static str },
    #[error("invalid condition group at groups[{group_index}]: {reason}")]
    InvalidGroup {
        group_index: usize,
        reason: &'static str,
    },
    #[error("invalid condition at {path}: {reason}")]
    InvalidCondition {
        path: ConditionPath,
        reason: &'static str,
    },
}

fn value_matches_condition(
    field_type: FieldType,
    operator: RuleOperator,
    value: Option<&ConditionValue>,
) -> bool {
    use ConditionValue::*;
    use RuleOperator::*;
    let value_shape_matches = match (field_type, operator) {
        (_, Exists | Missing) | (FieldType::Boolean, IsTrue | IsFalse) => value.is_none(),
        (
            FieldType::Number,
            Equals | NotEquals | GreaterThan | GreaterThanOrEqual | LessThan | LessThanOrEqual,
        ) => matches!(value, Some(Number(number)) if number.is_finite()),
        (FieldType::Number, Between | NotBetween) => matches!(value, Some(NumberRange(_))),
        (FieldType::Text, Equals | NotEquals | Contains | NotContains | StartsWith | EndsWith)
        | (FieldType::Category, Equals | NotEquals) => matches!(value, Some(Text(_))),
        (FieldType::Text, InSet | NotInSet) | (FieldType::Category, InAny | NotInAny) => {
            matches!(value, Some(TextList(items)) if !items.is_empty())
        }
        (
            FieldType::Tags,
            ContainsAnyTag | ContainsAllTags | ExcludesAnyTag | NotContainsAllTags | EqualsTagSet,
        ) => matches!(value, Some(TextList(items)) if !items.is_empty()),
        (FieldType::Tags, TagNameContains | TagNameNotContains) => {
            matches!(value, Some(Text(_)))
        }
        (FieldType::Tags, CountEquals | CountGreaterThanOrEqual | CountLessThanOrEqual) => {
            matches!(value, Some(Number(number)) if number.is_finite())
        }
        (FieldType::Date, Before | After) => matches!(value, Some(Date(_))),
        (FieldType::Date, Between) => matches!(value, Some(DateRange(_))),
        (FieldType::Date, RecentHours) => {
            matches!(value, Some(DurationHours(hours)) if *hours > 0)
        }
        (FieldType::Date, RecentDays) => {
            matches!(value, Some(DurationDays(days)) if *days > 0)
        }
        _ => false,
    };
    value_shape_matches && range_is_ordered(value)
}

fn range_is_ordered(value: Option<&ConditionValue>) -> bool {
    match value {
        Some(ConditionValue::NumberRange(range)) => {
            range.min.is_finite() && range.max.is_finite() && range.min <= range.max
        }
        Some(ConditionValue::DateRange(range)) => range.start <= range.end,
        _ => true,
    }
}

fn value_matches_options(field: RuleField, value: Option<&ConditionValue>) -> bool {
    let options = value_options(field);
    if options.is_empty() {
        return true;
    }
    let is_available = |candidate: &str| options.iter().any(|option| option.value == candidate);
    match value {
        Some(ConditionValue::Text(value)) => is_available(value),
        Some(ConditionValue::TextList(values)) => values.iter().all(|value| is_available(value)),
        None => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_value_fields_reject_values_outside_the_catalog() {
        let invalid_single = Condition {
            field: RuleField::ContentType,
            operator: RuleOperator::Equals,
            value: Some(ConditionValue::Text("novel".into())),
            case_sensitive: None,
            tag_scope: None,
            page_quantifier: None,
        };
        assert!(invalid_single.validate(0, 0).is_err());

        let invalid_list = Condition {
            field: RuleField::AgeRating,
            operator: RuleOperator::InAny,
            value: Some(ConditionValue::TextList(vec![
                "r18".into(),
                "unknown".into(),
            ])),
            case_sensitive: None,
            tag_scope: None,
            page_quantifier: None,
        };
        assert!(invalid_list.validate(0, 1).is_err());
    }
}
