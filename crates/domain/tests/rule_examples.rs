use pixivarchive_domain::rule::{
    CandidatePage, Condition, ConditionGroup, ConditionValue, EvaluationContext, FieldType,
    GroupMode, NumberRange, PageQuantifier, PageRuleMetadata, RuleAction, RuleCandidate,
    RuleDefinitionV1, RuleField, RuleOperator, RuleTag, TagScope, TimeRange, TraceEvaluationState,
};
use serde_json::json;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[test]
fn schema_v1_fixture_is_valid() {
    let value =
        serde_json::from_str(include_str!("../../../fixtures/rules/schema-v1.json")).unwrap();

    RuleDefinitionV1::parse(value).unwrap();
}

#[test]
fn matching_rule_uses_its_action_and_records_trace() {
    let first_id = Uuid::now_v7();
    let document = document(vec![Rule {
        id: first_id,
        name: "bookmark threshold".to_owned(),
        enabled: true,
        group_mode: GroupMode::All,
        groups: vec![group(
            GroupMode::All,
            vec![number_condition(
                RuleField::BookmarkCount,
                RuleOperator::GreaterThanOrEqual,
                500.0,
            )],
        )],
        action: RuleAction::MetadataOnly,
    }]);

    let decision = document.evaluate(&context()).unwrap();

    assert_eq!(decision.action, RuleAction::MetadataOnly);
    assert_eq!(decision.matched_rule_id, Some(first_id));
    assert_eq!(decision.trace.rules.len(), 1);
    assert_eq!(
        decision.trace.rules[0].state,
        pixivarchive_domain::rule::RuleTraceState::Matched
    );
}

#[test]
fn any_and_all_groups_are_evaluated_at_both_levels() {
    let matching_rule = Uuid::now_v7();
    let document = document(vec![Rule {
        id: matching_rule,
        name: "nested groups".to_owned(),
        enabled: true,
        group_mode: GroupMode::Any,
        groups: vec![
            group(
                GroupMode::All,
                vec![
                    text_condition(RuleField::AgeRating, RuleOperator::Equals, "r18"),
                    bool_condition(RuleField::AiGenerated, RuleOperator::IsTrue),
                ],
            ),
            group(
                GroupMode::Any,
                vec![
                    text_condition(RuleField::ContentType, RuleOperator::Equals, "ugoira"),
                    number_condition(RuleField::BookmarkCount, RuleOperator::GreaterThan, 600.0),
                ],
            ),
            group(
                GroupMode::All,
                vec![number_condition(
                    RuleField::ViewCount,
                    RuleOperator::GreaterThan,
                    1000.0,
                )],
            ),
        ],
        action: RuleAction::Download,
    }]);

    let decision = document.evaluate(&context()).unwrap();

    assert_eq!(decision.action, RuleAction::Download);
    assert_eq!(decision.matched_rule_id, Some(matching_rule));
    assert_eq!(decision.trace.rules[0].groups.len(), 3);
    assert_eq!(decision.trace.rules[0].groups[0].result, Some(false));
    assert_eq!(decision.trace.rules[0].groups[1].result, Some(true));
    assert_eq!(decision.trace.rules[0].groups[2].result, None);
    assert_eq!(
        decision.trace.rules[0].groups[0].conditions[1].state,
        TraceEvaluationState::StoppedBeforeEvaluation
    );
    assert_eq!(
        decision.trace.rules[0].groups[2].conditions[0].state,
        TraceEvaluationState::StoppedBeforeEvaluation
    );
}

#[test]
fn tag_matching_uses_unicode_normalization_and_translation_scope() {
    let document = document(vec![Rule {
        id: Uuid::now_v7(),
        name: "translated tag".to_owned(),
        enabled: true,
        group_mode: GroupMode::All,
        groups: vec![group(
            GroupMode::All,
            vec![Condition {
                field: RuleField::Tags,
                operator: RuleOperator::ContainsAnyTag,
                value: Some(ConditionValue::TextList(vec!["Cafe".to_owned()])),
                case_sensitive: Some(false),
                tag_scope: Some(TagScope::OriginalAndTranslation),
                page_quantifier: None,
            }],
        )],
        action: RuleAction::Download,
    }]);
    let mut context = context();
    context.candidate.tags = vec![RuleTag {
        original: "創作".to_owned(),
        translation: Some("ＣＡＦＥ".to_owned()),
    }];

    let decision = document.evaluate(&context).unwrap();

    assert_eq!(decision.action, RuleAction::Download);
}

#[test]
fn original_tag_scope_does_not_match_translated_names() {
    let mut condition = tag_list_condition(
        RuleOperator::ContainsAnyTag,
        vec!["Cafe"],
        Some(TagScope::Original),
    );
    condition.case_sensitive = Some(false);
    let document = single_condition_document(condition);
    let mut context = context();
    context.candidate.tags = vec![RuleTag {
        original: "創作".to_owned(),
        translation: Some("ＣＡＦＥ".to_owned()),
    }];

    let decision = document.evaluate(&context).unwrap();

    assert_eq!(decision.action, RuleAction::Ignore);
}

#[test]
fn tag_name_not_contains_requires_every_selected_tag_name_to_miss_the_needle() {
    let document = single_condition_document(Condition {
        field: RuleField::Tags,
        operator: RuleOperator::TagNameNotContains,
        value: Some(ConditionValue::Text("land".to_owned())),
        case_sensitive: Some(false),
        tag_scope: Some(TagScope::Original),
        page_quantifier: None,
    });
    let mut context = context();
    context.candidate.tags = vec![
        RuleTag {
            original: "landscape".to_owned(),
            translation: None,
        },
        RuleTag {
            original: "portrait".to_owned(),
            translation: None,
        },
    ];

    let decision = document.evaluate(&context).unwrap();

    assert_eq!(decision.action, RuleAction::Ignore);
}

#[test]
fn work_scoped_operator_families_match_expected_values() {
    let now = fixed_now();
    let published_at = now - Duration::days(2);
    let cases = vec![
        (
            "number equals",
            number_condition(RuleField::BookmarkCount, RuleOperator::Equals, 700.0),
        ),
        (
            "number not equals",
            number_condition(RuleField::BookmarkCount, RuleOperator::NotEquals, 701.0),
        ),
        (
            "number greater than",
            number_condition(RuleField::BookmarkCount, RuleOperator::GreaterThan, 699.0),
        ),
        (
            "number greater than or equal",
            number_condition(
                RuleField::BookmarkCount,
                RuleOperator::GreaterThanOrEqual,
                700.0,
            ),
        ),
        (
            "number less than",
            number_condition(RuleField::BookmarkCount, RuleOperator::LessThan, 701.0),
        ),
        (
            "number less than or equal",
            number_condition(
                RuleField::BookmarkCount,
                RuleOperator::LessThanOrEqual,
                700.0,
            ),
        ),
        (
            "number between",
            range_condition(
                RuleField::BookmarkCount,
                RuleOperator::Between,
                NumberRange {
                    min: 600.0,
                    max: 800.0,
                },
            ),
        ),
        (
            "number not between",
            range_condition(
                RuleField::BookmarkCount,
                RuleOperator::NotBetween,
                NumberRange {
                    min: 800.0,
                    max: 900.0,
                },
            ),
        ),
        (
            "text contains",
            text_condition(RuleField::Title, RuleOperator::Contains, "any"),
        ),
        (
            "text not contains",
            text_condition(RuleField::Title, RuleOperator::NotContains, "missing"),
        ),
        (
            "text starts with",
            text_condition(RuleField::Title, RuleOperator::StartsWith, "any"),
        ),
        (
            "text ends with",
            text_condition(RuleField::Title, RuleOperator::EndsWith, "thing"),
        ),
        (
            "text in set",
            text_list_condition(
                RuleField::Title,
                RuleOperator::InSet,
                vec!["other", "anything"],
            ),
        ),
        (
            "text not in set",
            text_list_condition(RuleField::Title, RuleOperator::NotInSet, vec!["other"]),
        ),
        (
            "category equals",
            text_condition(RuleField::ContentType, RuleOperator::Equals, "illustration"),
        ),
        (
            "category not equals",
            text_condition(RuleField::ContentType, RuleOperator::NotEquals, "manga"),
        ),
        (
            "category in any",
            text_list_condition(
                RuleField::ContentType,
                RuleOperator::InAny,
                vec!["manga", "illustration"],
            ),
        ),
        (
            "category not in any",
            text_list_condition(
                RuleField::ContentType,
                RuleOperator::NotInAny,
                vec!["ugoira"],
            ),
        ),
        (
            "date before",
            date_condition(
                RuleField::PublishedAt,
                RuleOperator::Before,
                published_at + Duration::days(1),
            ),
        ),
        (
            "date after",
            date_condition(
                RuleField::PublishedAt,
                RuleOperator::After,
                published_at - Duration::days(1),
            ),
        ),
        (
            "date between",
            date_range_condition(
                RuleField::PublishedAt,
                RuleOperator::Between,
                TimeRange {
                    start: published_at - Duration::hours(1),
                    end: published_at + Duration::hours(1),
                },
            ),
        ),
        (
            "recent hours",
            duration_hours_condition(RuleField::UpdatedAt, RuleOperator::RecentHours, 25),
        ),
        (
            "recent days",
            duration_days_condition(RuleField::UpdatedAt, RuleOperator::RecentDays, 2),
        ),
        (
            "boolean true",
            bool_condition(RuleField::OriginalWork, RuleOperator::IsTrue),
        ),
        (
            "boolean false",
            bool_condition(RuleField::AiGenerated, RuleOperator::IsFalse),
        ),
        (
            "exists",
            empty_condition(RuleField::ArtistName, RuleOperator::Exists),
        ),
        (
            "missing",
            empty_condition(RuleField::SeriesTitle, RuleOperator::Missing),
        ),
    ];

    for (name, condition) in cases {
        let document = single_condition_document(condition);
        let decision = document.evaluate(&context()).unwrap();
        assert_eq!(decision.action, RuleAction::Download, "{name}");
    }
}

#[test]
fn tag_operator_family_matches_expected_values() {
    let cases = vec![
        (
            "contains any tag",
            tag_list_condition(
                RuleOperator::ContainsAnyTag,
                vec!["創作"],
                Some(TagScope::Original),
            ),
        ),
        (
            "contains all tags",
            tag_list_condition(
                RuleOperator::ContainsAllTags,
                vec!["original", "創作"],
                Some(TagScope::OriginalAndTranslation),
            ),
        ),
        (
            "excludes any tag",
            tag_list_condition(
                RuleOperator::ExcludesAnyTag,
                vec!["AI生成"],
                Some(TagScope::Original),
            ),
        ),
        (
            "not contains all tags",
            tag_list_condition(
                RuleOperator::NotContainsAllTags,
                vec!["創作", "AI生成"],
                Some(TagScope::Original),
            ),
        ),
        (
            "equals tag set",
            tag_list_condition(
                RuleOperator::EqualsTagSet,
                vec!["original", "創作"],
                Some(TagScope::OriginalAndTranslation),
            ),
        ),
        (
            "tag name contains",
            Condition {
                field: RuleField::Tags,
                operator: RuleOperator::TagNameContains,
                value: Some(ConditionValue::Text("作".to_owned())),
                case_sensitive: Some(false),
                tag_scope: Some(TagScope::Original),
                page_quantifier: None,
            },
        ),
        (
            "tag name not contains",
            Condition {
                field: RuleField::Tags,
                operator: RuleOperator::TagNameNotContains,
                value: Some(ConditionValue::Text("AI".to_owned())),
                case_sensitive: Some(false),
                tag_scope: Some(TagScope::Original),
                page_quantifier: None,
            },
        ),
        (
            "count equals",
            tag_count_condition(RuleOperator::CountEquals, 1.0),
        ),
        (
            "count greater than or equal",
            tag_count_condition(RuleOperator::CountGreaterThanOrEqual, 1.0),
        ),
        (
            "count less than or equal",
            tag_count_condition(RuleOperator::CountLessThanOrEqual, 1.0),
        ),
    ];

    for (name, condition) in cases {
        let document = single_condition_document(condition);
        let decision = document.evaluate(&context()).unwrap();
        assert_eq!(decision.action, RuleAction::Download, "{name}");
    }
}

#[test]
fn page_operator_families_match_expected_values() {
    let cases = vec![
        page_text_condition(
            RuleField::PageOriginalExtension,
            RuleOperator::Equals,
            "jpg",
        ),
        page_number_condition(
            RuleField::PageWidth,
            RuleOperator::GreaterThanOrEqual,
            1200.0,
        ),
        page_number_condition(RuleField::PageHeight, RuleOperator::LessThanOrEqual, 900.0),
        page_number_condition(RuleField::PageAspectRatio, RuleOperator::Between, 1.0),
        page_text_condition(
            RuleField::PageOrientation,
            RuleOperator::Equals,
            "landscape",
        ),
    ];
    let mut context = context();
    context.candidate.pages = vec![CandidatePage {
        metadata: Some(PageRuleMetadata {
            original_extension: Some("jpg".to_owned()),
            width: Some(1200),
            height: Some(800),
            aspect_ratio: Some(1.5),
            orientation: Some("landscape".to_owned()),
        }),
    }];

    for condition in cases {
        let document = single_condition_document(condition);
        let decision = document.evaluate(&context).unwrap();
        assert_eq!(decision.action, RuleAction::Download);
    }
}

#[test]
fn text_matching_uses_unicode_normalization() {
    let document = single_condition_document(text_condition(
        RuleField::Title,
        RuleOperator::Contains,
        "pixiv",
    ));
    let mut context = context();
    context.candidate.title = Some("ＰＩＸＩＶ作品".to_owned());

    let decision = document.evaluate(&context).unwrap();

    assert_eq!(decision.action, RuleAction::Download);
}

#[test]
fn page_quantifiers_short_circuit_probe_conditions() {
    let document = document(vec![Rule {
        id: Uuid::now_v7(),
        name: "wide page".to_owned(),
        enabled: true,
        group_mode: GroupMode::All,
        groups: vec![group(
            GroupMode::All,
            vec![Condition {
                field: RuleField::PageWidth,
                operator: RuleOperator::GreaterThanOrEqual,
                value: Some(ConditionValue::Number(1600.0)),
                case_sensitive: None,
                tag_scope: None,
                page_quantifier: Some(PageQuantifier::AnyPage),
            }],
        )],
        action: RuleAction::Download,
    }]);
    let mut context = context();
    context.candidate.pages = vec![
        CandidatePage {
            metadata: Some(PageRuleMetadata {
                width: Some(800),
                ..PageRuleMetadata::default()
            }),
        },
        CandidatePage {
            metadata: Some(PageRuleMetadata {
                width: Some(1800),
                ..PageRuleMetadata::default()
            }),
        },
        CandidatePage { metadata: None },
    ];

    let decision = document.evaluate(&context).unwrap();

    assert_eq!(decision.action, RuleAction::Download);
    let condition = &decision.trace.rules[0].groups[0].conditions[0];
    assert_eq!(condition.result, Some(true));
    assert_eq!(condition.page_quantifier, Some(PageQuantifier::AnyPage));
    assert_eq!(condition.pages.len(), 2);
    assert_eq!(condition.pages[0].page_index, 0);
    assert!(!condition.pages[0].result);
    assert_eq!(condition.pages[1].page_index, 1);
    assert!(condition.pages[1].result);
    assert_eq!(condition.stopped_at_page_index, Some(1));
}

#[test]
fn missing_page_quantifier_and_regex_operator_are_rejected() {
    let missing_quantifier = json!({
        "schema_version": 1,
        "default_action": "ignore",
        "rules": [{
            "id": Uuid::now_v7(),
            "name": "bad page condition",
            "enabled": true,
            "group_mode": "all",
            "action": "download",
            "groups": [{
                "mode": "all",
                "conditions": [{
                    "field": "page_width",
                    "operator": "greater_than_or_equal",
                    "value": { "type": "number", "value": 1000.0 }
                }]
            }]
        }]
    });
    assert!(RuleDefinitionV1::parse(missing_quantifier).is_err());

    let regex_operator = json!({
        "schema_version": 1,
        "default_action": "ignore",
        "rules": [{
            "id": Uuid::now_v7(),
            "name": "regex",
            "enabled": true,
            "group_mode": "all",
            "action": "download",
            "groups": [{
                "mode": "all",
                "conditions": [{
                    "field": "title",
                    "operator": "regex",
                    "value": { "type": "text", "value": ".*" }
                }]
            }]
        }]
    });
    assert!(RuleDefinitionV1::parse(regex_operator).is_err());
}

#[test]
fn missing_optional_fields_can_be_tested_explicitly() {
    let document = document(vec![Rule {
        id: Uuid::now_v7(),
        name: "no series".to_owned(),
        enabled: true,
        group_mode: GroupMode::All,
        groups: vec![group(
            GroupMode::All,
            vec![Condition {
                field: RuleField::SeriesTitle,
                operator: RuleOperator::Missing,
                value: None,
                case_sensitive: None,
                tag_scope: None,
                page_quantifier: None,
            }],
        )],
        action: RuleAction::MetadataOnly,
    }]);

    let decision = document.evaluate(&context()).unwrap();

    assert_eq!(decision.action, RuleAction::MetadataOnly);
    assert_eq!(decision.trace.rules[0].groups[0].conditions[0].value, None);
}

fn document(mut rules: Vec<Rule>) -> RuleDefinitionV1 {
    assert_eq!(
        rules.len(),
        1,
        "a rule definition has one rule and its condition groups"
    );
    let rule = rules.remove(0);
    let document = RuleDefinitionV1 {
        schema_version: 1,
        id: rule.id,
        name: rule.name,
        enabled: rule.enabled,
        group_mode: rule.group_mode,
        groups: rule.groups,
        action: rule.action,
        default_action: RuleAction::Ignore,
    };
    document.validate().unwrap();
    document
}

fn single_condition_document(condition: Condition) -> RuleDefinitionV1 {
    document(vec![Rule {
        id: Uuid::now_v7(),
        name: "single condition".to_owned(),
        enabled: true,
        group_mode: GroupMode::All,
        groups: vec![group(GroupMode::All, vec![condition])],
        action: RuleAction::Download,
    }])
}

struct Rule {
    id: Uuid,
    name: String,
    enabled: bool,
    group_mode: GroupMode,
    groups: Vec<ConditionGroup>,
    action: RuleAction,
}

fn group(mode: GroupMode, conditions: Vec<Condition>) -> ConditionGroup {
    ConditionGroup { mode, conditions }
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

fn text_condition(field: RuleField, operator: RuleOperator, value: &str) -> Condition {
    Condition {
        field,
        operator,
        value: Some(ConditionValue::Text(value.to_owned())),
        case_sensitive: case_sensitive_for(field),
        tag_scope: None,
        page_quantifier: None,
    }
}

fn text_list_condition(field: RuleField, operator: RuleOperator, values: Vec<&str>) -> Condition {
    Condition {
        field,
        operator,
        value: Some(ConditionValue::TextList(
            values.into_iter().map(str::to_owned).collect(),
        )),
        case_sensitive: case_sensitive_for(field),
        tag_scope: None,
        page_quantifier: None,
    }
}

fn range_condition(field: RuleField, operator: RuleOperator, value: NumberRange) -> Condition {
    Condition {
        field,
        operator,
        value: Some(ConditionValue::NumberRange(value)),
        case_sensitive: None,
        tag_scope: None,
        page_quantifier: None,
    }
}

fn date_condition(field: RuleField, operator: RuleOperator, value: OffsetDateTime) -> Condition {
    Condition {
        field,
        operator,
        value: Some(ConditionValue::Date(value)),
        case_sensitive: None,
        tag_scope: None,
        page_quantifier: None,
    }
}

fn date_range_condition(field: RuleField, operator: RuleOperator, value: TimeRange) -> Condition {
    Condition {
        field,
        operator,
        value: Some(ConditionValue::DateRange(value)),
        case_sensitive: None,
        tag_scope: None,
        page_quantifier: None,
    }
}

fn duration_hours_condition(field: RuleField, operator: RuleOperator, value: i64) -> Condition {
    Condition {
        field,
        operator,
        value: Some(ConditionValue::DurationHours(value)),
        case_sensitive: None,
        tag_scope: None,
        page_quantifier: None,
    }
}

fn duration_days_condition(field: RuleField, operator: RuleOperator, value: i64) -> Condition {
    Condition {
        field,
        operator,
        value: Some(ConditionValue::DurationDays(value)),
        case_sensitive: None,
        tag_scope: None,
        page_quantifier: None,
    }
}

fn bool_condition(field: RuleField, operator: RuleOperator) -> Condition {
    Condition {
        field,
        operator,
        value: None,
        case_sensitive: None,
        tag_scope: None,
        page_quantifier: None,
    }
}

fn empty_condition(field: RuleField, operator: RuleOperator) -> Condition {
    Condition {
        field,
        operator,
        value: None,
        case_sensitive: None,
        tag_scope: None,
        page_quantifier: None,
    }
}

fn tag_list_condition(
    operator: RuleOperator,
    values: Vec<&str>,
    tag_scope: Option<TagScope>,
) -> Condition {
    Condition {
        field: RuleField::Tags,
        operator,
        value: Some(ConditionValue::TextList(
            values.into_iter().map(str::to_owned).collect(),
        )),
        case_sensitive: Some(false),
        tag_scope,
        page_quantifier: None,
    }
}

fn tag_count_condition(operator: RuleOperator, value: f64) -> Condition {
    Condition {
        field: RuleField::Tags,
        operator,
        value: Some(ConditionValue::Number(value)),
        case_sensitive: None,
        tag_scope: None,
        page_quantifier: None,
    }
}

fn page_text_condition(field: RuleField, operator: RuleOperator, value: &str) -> Condition {
    Condition {
        field,
        operator,
        value: Some(ConditionValue::Text(value.to_owned())),
        case_sensitive: case_sensitive_for(field),
        tag_scope: None,
        page_quantifier: Some(PageQuantifier::AnyPage),
    }
}

fn page_number_condition(field: RuleField, operator: RuleOperator, value: f64) -> Condition {
    let value = if operator == RuleOperator::Between {
        ConditionValue::NumberRange(NumberRange {
            min: value,
            max: value + 1.0,
        })
    } else {
        ConditionValue::Number(value)
    };
    Condition {
        field,
        operator,
        value: Some(value),
        case_sensitive: None,
        tag_scope: None,
        page_quantifier: Some(PageQuantifier::AnyPage),
    }
}

fn fixed_now() -> OffsetDateTime {
    OffsetDateTime::UNIX_EPOCH + Duration::days(20_000)
}

fn case_sensitive_for(field: RuleField) -> Option<bool> {
    (field.value_type() == FieldType::Text).then_some(false)
}

fn context() -> EvaluationContext {
    let now = fixed_now();
    EvaluationContext {
        now,
        candidate: RuleCandidate {
            pixiv_work_id: 123456,
            content_type: "illustration".to_owned(),
            title: Some("anything".to_owned()),
            description: None,
            artist_id: Some(44),
            artist_name: Some("artist".to_owned()),
            published_at: Some(now - Duration::days(2)),
            updated_at: Some(now - Duration::days(1)),
            tags: vec![RuleTag {
                original: "創作".to_owned(),
                translation: Some("original".to_owned()),
            }],
            page_count: 2,
            age_rating: Some("all_age".to_owned()),
            ai_generated: Some(false),
            original_work: Some(true),
            bookmarked_by_current_account: Some(false),
            bookmark_count: Some(700),
            view_count: Some(3000),
            like_count: Some(900),
            comment_count: Some(3),
            bookmark_rate: Some(0.23),
            bookmarks_per_day: Some(18.0),
            ranking_rank: Some(20),
            ranking_date: Some(now),
            series_id: None,
            series_title: None,
            series_order: None,
            pages: vec![],
        },
    }
}
