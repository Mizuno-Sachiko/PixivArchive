use pixivarchive_domain::rule::{
    CandidatePage, Condition, ConditionGroup, ConditionValue, EvaluationContext, GroupMode,
    PageQuantifier, PageRuleMetadata, RuleAction, RuleCandidate, RuleDefinitionV1, RuleError,
    RuleField, RuleOperator, TraceEvaluationState,
};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

#[test]
fn page_quantifiers_stop_when_the_answer_is_known() {
    let any_page = document(vec![rule(
        Uuid::now_v7(),
        RuleAction::Download,
        page_width_condition(PageQuantifier::AnyPage, 1000.0),
    )]);
    let all_pages = document(vec![rule(
        Uuid::now_v7(),
        RuleAction::Download,
        page_width_condition(PageQuantifier::AllPages, 1000.0),
    )]);
    let mut context = context();
    context.candidate.pages = vec![page(1200), CandidatePage { metadata: None }];
    assert_eq!(
        any_page.evaluate(&context).unwrap().action,
        RuleAction::Download
    );

    context.candidate.pages = vec![page(800), CandidatePage { metadata: None }];
    assert_eq!(
        all_pages.evaluate(&context).unwrap().action,
        RuleAction::Ignore
    );
    let decision = all_pages.evaluate(&context).unwrap();
    let condition = &decision.trace.rules[0].groups[0].conditions[0];
    assert_eq!(condition.result, Some(false));
    assert_eq!(condition.pages.len(), 1);
    assert_eq!(condition.pages[0].page_index, 0);
    assert_eq!(condition.stopped_at_page_index, Some(0));
}

#[test]
fn trace_keeps_stable_positions_after_condition_and_rule_stops() {
    let matching_rule = Uuid::now_v7();
    let document = document(vec![Rule {
        id: matching_rule,
        name: "early match".to_owned(),
        enabled: true,
        group_mode: GroupMode::All,
        groups: vec![
            ConditionGroup {
                mode: GroupMode::Any,
                conditions: vec![
                    number_condition(RuleField::BookmarkCount, RuleOperator::GreaterThan, 10.0),
                    number_condition(RuleField::ViewCount, RuleOperator::GreaterThan, 10.0),
                ],
            },
            ConditionGroup {
                mode: GroupMode::All,
                conditions: vec![number_condition(
                    RuleField::LikeCount,
                    RuleOperator::GreaterThan,
                    10.0,
                )],
            },
        ],
        action: RuleAction::Download,
    }]);

    let decision = document.evaluate(&context()).unwrap();

    assert_eq!(decision.matched_rule_id, Some(matching_rule));
    assert_eq!(decision.trace.rules[0].rule_index, 0);
    assert_eq!(decision.trace.rules[0].groups[0].group_index, 0);
    assert_eq!(
        decision.trace.rules[0].groups[0].conditions[1].condition_index,
        1
    );
    assert_eq!(
        decision.trace.rules[0].groups[0].conditions[1].state,
        TraceEvaluationState::StoppedBeforeEvaluation
    );
    assert_eq!(decision.trace.rules[0].groups[1].result, Some(true));
}

#[test]
fn invalid_operator_or_value_shapes_are_rejected_with_condition_errors() {
    let cases = vec![
        (
            "number field cannot receive text equality",
            "bookmark_count",
            "equals",
            json!({ "type": "text", "value": "500" }),
        ),
        (
            "date field cannot receive number range",
            "published_at",
            "between",
            json!({ "type": "number_range", "value": { "min": 1.0, "max": 2.0 } }),
        ),
        (
            "text field cannot receive number equality",
            "title",
            "equals",
            json!({ "type": "number", "value": 1.0 }),
        ),
    ];

    for (name, field, operator, value) in cases {
        let error =
            RuleDefinitionV1::parse(rule_json(field, operator, Some(value))).expect_err(name);
        assert!(
            matches!(error, RuleError::InvalidCondition { .. }),
            "{name}"
        );
    }
}

#[test]
fn regex_operator_is_not_part_of_the_rule_language() {
    let error = RuleDefinitionV1::parse(rule_json(
        "title",
        "regex",
        Some(json!({ "type": "text", "value": ".*" })),
    ))
    .expect_err("regex operator should fail deserialization");

    assert!(matches!(error, RuleError::Json(_)));
}

#[test]
fn date_values_round_trip_as_rfc3339_strings() {
    let input = rule_json(
        "published_at",
        "before",
        Some(json!({
            "type": "date",
            "value": "2026-07-30T01:00:00Z"
        })),
    );

    let serialized = serde_json::to_value(RuleDefinitionV1::parse(input).unwrap()).unwrap();

    assert_eq!(
        serialized["groups"][0]["conditions"][0]["value"]["value"],
        "2026-07-30T01:00:00Z"
    );
}

fn document(mut rules: Vec<Rule>) -> RuleDefinitionV1 {
    assert_eq!(rules.len(), 1);
    let rule = rules.remove(0);
    RuleDefinitionV1 {
        schema_version: 1,
        id: rule.id,
        name: rule.name,
        enabled: rule.enabled,
        group_mode: rule.group_mode,
        groups: rule.groups,
        action: rule.action,
        default_action: RuleAction::Ignore,
    }
}

fn rule_json(field: &str, operator: &str, value: Option<serde_json::Value>) -> serde_json::Value {
    let mut condition = json!({
        "field": field,
        "operator": operator
    });
    if let Some(value) = value {
        condition
            .as_object_mut()
            .unwrap()
            .insert("value".to_owned(), value);
    }
    json!({
        "schema_version": 1,
        "id": Uuid::now_v7(),
        "name": "invalid shape",
        "enabled": true,
        "group_mode": "all",
        "action": "download",
        "default_action": "ignore",
        "groups": [{
            "mode": "all",
            "conditions": [condition]
        }]
    })
}

fn rule(id: Uuid, action: RuleAction, condition: Condition) -> Rule {
    Rule {
        id,
        name: id.to_string(),
        enabled: true,
        group_mode: GroupMode::All,
        groups: vec![ConditionGroup {
            mode: GroupMode::All,
            conditions: vec![condition],
        }],
        action,
    }
}

#[derive(Clone)]
struct Rule {
    id: Uuid,
    name: String,
    enabled: bool,
    group_mode: GroupMode,
    groups: Vec<ConditionGroup>,
    action: RuleAction,
}

fn number_condition(field: RuleField, operator: RuleOperator, value: f64) -> Condition {
    Condition {
        field,
        operator,
        value: Some(ConditionValue::Number(value)),
        case_sensitive: None,
        tag_scope: None,
        page_quantifier: None,
    }
}

fn page_width_condition(quantifier: PageQuantifier, value: f64) -> Condition {
    Condition {
        field: RuleField::PageWidth,
        operator: RuleOperator::GreaterThan,
        value: Some(ConditionValue::Number(value)),
        case_sensitive: None,
        tag_scope: None,
        page_quantifier: Some(quantifier),
    }
}

fn page(width: u32) -> CandidatePage {
    CandidatePage {
        metadata: Some(PageRuleMetadata {
            width: Some(width),
            ..PageRuleMetadata::default()
        }),
    }
}

fn context() -> EvaluationContext {
    EvaluationContext {
        now: OffsetDateTime::UNIX_EPOCH,
        candidate: RuleCandidate {
            pixiv_work_id: 1,
            content_type: "illustration".to_owned(),
            title: None,
            description: None,
            artist_id: None,
            artist_name: None,
            published_at: None,
            updated_at: None,
            tags: vec![],
            page_count: 1,
            age_rating: None,
            ai_generated: None,
            original_work: None,
            bookmarked_by_current_account: None,
            bookmark_count: Some(100),
            view_count: Some(100),
            like_count: Some(100),
            comment_count: None,
            bookmark_rate: None,
            bookmarks_per_day: None,
            ranking_rank: None,
            ranking_date: None,
            series_id: None,
            series_title: None,
            series_order: None,
            pages: vec![],
        },
    }
}
