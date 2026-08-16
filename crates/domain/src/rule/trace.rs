use super::{
    Condition, FieldValue, GroupMode, PageQuantifier, RuleAction, RuleField, RuleOperator,
};
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RuleTrace {
    pub rule_index: usize,
    pub rule_id: Uuid,
    pub rule_name: String,
    pub action: RuleAction,
    pub group_mode: GroupMode,
    pub state: RuleTraceState,
    pub groups: Vec<GroupTrace>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum RuleTraceState {
    Skipped,
    Matched,
    NotMatched,
    StoppedBeforeEvaluation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TraceEvaluationState {
    Evaluated,
    StoppedBeforeEvaluation,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GroupTrace {
    pub group_index: usize,
    pub mode: GroupMode,
    pub result: Option<bool>,
    pub state: TraceEvaluationState,
    pub conditions: Vec<ConditionTrace>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConditionTrace {
    pub condition_index: usize,
    pub field: RuleField,
    pub operator: RuleOperator,
    pub page_quantifier: Option<PageQuantifier>,
    pub result: Option<bool>,
    pub state: TraceEvaluationState,
    pub value: Option<FieldValue>,
    pub pages: Vec<PageConditionTrace>,
    pub stopped_at_page_index: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PageConditionTrace {
    pub page_index: usize,
    pub result: bool,
    pub value: Option<FieldValue>,
}

impl ConditionTrace {
    pub fn result(
        condition_index: usize,
        condition: &Condition,
        result: bool,
        value: Option<FieldValue>,
        pages: Vec<PageConditionTrace>,
        stopped_at_page_index: Option<usize>,
    ) -> Self {
        Self {
            condition_index,
            field: condition.field,
            operator: condition.operator,
            page_quantifier: condition.page_quantifier,
            result: Some(result),
            state: TraceEvaluationState::Evaluated,
            value,
            pages,
            stopped_at_page_index,
        }
    }

    pub fn stopped(
        condition_index: usize,
        field: RuleField,
        operator: RuleOperator,
        page_quantifier: Option<PageQuantifier>,
    ) -> Self {
        Self {
            condition_index,
            field,
            operator,
            page_quantifier,
            result: None,
            state: TraceEvaluationState::StoppedBeforeEvaluation,
            value: None,
            pages: Vec::new(),
            stopped_at_page_index: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct EvaluationTrace {
    pub decision: RuleAction,
    pub matched_rule_id: Option<Uuid>,
    pub rules: Vec<RuleTrace>,
}
