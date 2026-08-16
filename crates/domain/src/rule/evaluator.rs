use super::{
    Condition, ConditionGroup, ConditionTrace, ConditionValue, EvaluationTrace, FieldValue,
    GroupMode, GroupTrace, PageConditionTrace, PageQuantifier, RuleAction, RuleDefinitionV1,
    RuleField, RuleOperator, RuleTrace, RuleTraceState, TraceEvaluationState, schema::TagScope,
    value::TagValue,
};
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct EvaluationContext {
    pub now: OffsetDateTime,
    pub candidate: RuleCandidate,
}

#[derive(Clone, Debug)]
pub struct RuleCandidate {
    pub pixiv_work_id: i64,
    pub content_type: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub artist_id: Option<i64>,
    pub artist_name: Option<String>,
    pub published_at: Option<OffsetDateTime>,
    pub updated_at: Option<OffsetDateTime>,
    pub tags: Vec<RuleTag>,
    pub page_count: u32,
    pub age_rating: Option<String>,
    pub ai_generated: Option<bool>,
    pub original_work: Option<bool>,
    pub bookmarked_by_current_account: Option<bool>,
    pub bookmark_count: Option<u64>,
    pub view_count: Option<u64>,
    pub like_count: Option<u64>,
    pub comment_count: Option<u64>,
    pub bookmark_rate: Option<f64>,
    pub bookmarks_per_day: Option<f64>,
    pub ranking_rank: Option<u32>,
    pub ranking_date: Option<OffsetDateTime>,
    pub series_id: Option<i64>,
    pub series_title: Option<String>,
    pub series_order: Option<u32>,
    pub pages: Vec<CandidatePage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleTag {
    pub original: String,
    pub translation: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CandidatePage {
    pub metadata: Option<PageRuleMetadata>,
}

#[derive(Clone, Debug, Default)]
pub struct PageRuleMetadata {
    pub original_extension: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub aspect_ratio: Option<f64>,
    pub orientation: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EvaluationDecision {
    pub action: RuleAction,
    pub matched_rule_id: Option<Uuid>,
    pub trace: EvaluationTrace,
}

#[derive(Debug, Error)]
pub enum EvaluationError {
    #[error("page metadata is required for page {page_index}")]
    PageMetadataRequired { page_index: usize },
}

impl RuleDefinitionV1 {
    pub fn evaluate(
        &self,
        context: &EvaluationContext,
    ) -> Result<EvaluationDecision, EvaluationError> {
        if !self.enabled {
            let trace = RuleTrace {
                rule_index: 0,
                rule_id: self.id,
                rule_name: self.name.clone(),
                action: self.action,
                group_mode: self.group_mode,
                state: RuleTraceState::Skipped,
                groups: self
                    .groups
                    .iter()
                    .enumerate()
                    .map(|(group_index, group)| stopped_group(group_index, group))
                    .collect(),
            };
            return Ok(EvaluationDecision {
                action: self.default_action,
                matched_rule_id: None,
                trace: EvaluationTrace {
                    decision: self.default_action,
                    matched_rule_id: None,
                    rules: vec![trace],
                },
            });
        }

        let mut group_traces = Vec::new();
        let mut group_results = Vec::new();
        let mut stopped_groups = false;
        for (group_index, group) in self.groups.iter().enumerate() {
            if stopped_groups {
                group_traces.push(stopped_group(group_index, group));
                continue;
            }
            let group_trace = evaluate_group(group_index, group, context)?;
            let group_result = group_trace
                .result
                .expect("evaluated groups always have a boolean result");
            group_results.push(group_result);
            group_traces.push(group_trace);
            if self.group_mode == GroupMode::Any && group_result {
                stopped_groups = true;
            }
            if self.group_mode == GroupMode::All && !group_result {
                stopped_groups = true;
            }
        }
        let matched = combine(self.group_mode, &group_results);
        let action = if matched {
            self.action
        } else {
            self.default_action
        };
        let matched_rule_id = matched.then_some(self.id);
        let trace = RuleTrace {
            rule_index: 0,
            rule_id: self.id,
            rule_name: self.name.clone(),
            action: self.action,
            group_mode: self.group_mode,
            state: if matched {
                RuleTraceState::Matched
            } else {
                RuleTraceState::NotMatched
            },
            groups: group_traces,
        };
        Ok(EvaluationDecision {
            action,
            matched_rule_id,
            trace: EvaluationTrace {
                decision: action,
                matched_rule_id,
                rules: vec![trace],
            },
        })
    }
}

fn evaluate_group(
    group_index: usize,
    group: &ConditionGroup,
    context: &EvaluationContext,
) -> Result<GroupTrace, EvaluationError> {
    let mut condition_traces = Vec::new();
    let mut condition_results = Vec::new();
    let mut stopped_conditions = false;
    for (condition_index, condition) in group.conditions.iter().enumerate() {
        if stopped_conditions {
            condition_traces.push(stopped_condition(condition_index, condition));
            continue;
        }
        let trace = evaluate_condition(condition_index, condition, context)?;
        let result = trace
            .result
            .expect("evaluated conditions always have a boolean result");
        condition_results.push(result);
        condition_traces.push(trace);
        if group.mode == GroupMode::Any && result {
            stopped_conditions = true;
        }
        if group.mode == GroupMode::All && !result {
            stopped_conditions = true;
        }
    }
    Ok(GroupTrace {
        group_index,
        mode: group.mode,
        result: Some(combine(group.mode, &condition_results)),
        state: TraceEvaluationState::Evaluated,
        conditions: condition_traces,
    })
}

fn stopped_group(group_index: usize, group: &ConditionGroup) -> GroupTrace {
    GroupTrace {
        group_index,
        mode: group.mode,
        result: None,
        state: TraceEvaluationState::StoppedBeforeEvaluation,
        conditions: group
            .conditions
            .iter()
            .enumerate()
            .map(|(condition_index, condition)| stopped_condition(condition_index, condition))
            .collect(),
    }
}

fn stopped_condition(condition_index: usize, condition: &Condition) -> ConditionTrace {
    ConditionTrace::stopped(
        condition_index,
        condition.field,
        condition.operator,
        condition.page_quantifier,
    )
}

fn combine(mode: GroupMode, results: &[bool]) -> bool {
    match mode {
        GroupMode::All => !results.is_empty() && results.iter().all(|value| *value),
        GroupMode::Any => results.iter().any(|value| *value),
    }
}

fn evaluate_condition(
    condition_index: usize,
    condition: &Condition,
    context: &EvaluationContext,
) -> Result<ConditionTrace, EvaluationError> {
    if let Some(quantifier) = condition.page_quantifier {
        evaluate_page_condition(condition_index, condition, quantifier, context)
    } else {
        let value = work_value(condition.field, &context.candidate);
        let result = evaluate_value(condition, value.as_ref(), context.now);
        Ok(ConditionTrace::result(
            condition_index,
            condition,
            result,
            value,
            Vec::new(),
            None,
        ))
    }
}

fn evaluate_page_condition(
    condition_index: usize,
    condition: &Condition,
    quantifier: PageQuantifier,
    context: &EvaluationContext,
) -> Result<ConditionTrace, EvaluationError> {
    if context.candidate.pages.is_empty() {
        return Ok(ConditionTrace::result(
            condition_index,
            condition,
            false,
            None,
            Vec::new(),
            None,
        ));
    }
    let mut last_value = None;
    let mut page_traces = Vec::new();
    let mut saw_result = false;
    let mut stopped_at_page_index = None;
    for (page_index, page) in context.candidate.pages.iter().enumerate() {
        let metadata = page
            .metadata
            .as_ref()
            .ok_or(EvaluationError::PageMetadataRequired { page_index })?;
        let value = page_value(condition.field, &context.candidate, metadata);
        let result = evaluate_value(condition, value.as_ref(), context.now);
        saw_result = true;
        last_value = value.clone();
        page_traces.push(PageConditionTrace {
            page_index,
            result,
            value,
        });
        match quantifier {
            PageQuantifier::AnyPage if result => {
                stopped_at_page_index = Some(page_index);
                return Ok(ConditionTrace::result(
                    condition_index,
                    condition,
                    true,
                    last_value,
                    page_traces,
                    stopped_at_page_index,
                ));
            }
            PageQuantifier::AllPages if !result => {
                stopped_at_page_index = Some(page_index);
                return Ok(ConditionTrace::result(
                    condition_index,
                    condition,
                    false,
                    last_value,
                    page_traces,
                    stopped_at_page_index,
                ));
            }
            _ => {}
        }
    }
    let result = match quantifier {
        PageQuantifier::AnyPage => false,
        PageQuantifier::AllPages => saw_result,
    };
    Ok(ConditionTrace::result(
        condition_index,
        condition,
        result,
        last_value,
        page_traces,
        stopped_at_page_index,
    ))
}

fn evaluate_value(condition: &Condition, value: Option<&FieldValue>, now: OffsetDateTime) -> bool {
    use RuleOperator::*;
    match condition.operator {
        Exists => value.is_some(),
        Missing => value.is_none(),
        IsTrue => matches!(value, Some(FieldValue::Boolean(true))),
        IsFalse => matches!(value, Some(FieldValue::Boolean(false))),
        Equals => compare_equals(condition, value),
        NotEquals => value.is_some() && !compare_equals(condition, value),
        GreaterThan => compare_number(condition, value, |left, right| left > right),
        GreaterThanOrEqual => compare_number(condition, value, |left, right| left >= right),
        LessThan => compare_number(condition, value, |left, right| left < right),
        LessThanOrEqual => compare_number(condition, value, |left, right| left <= right),
        Between => number_between(condition, value, false) || date_between(condition, value, false),
        NotBetween => {
            number_between(condition, value, true) || date_between(condition, value, true)
        }
        Contains => compare_text(condition, value, |left, right| left.contains(right)),
        NotContains => compare_text(condition, value, |left, right| !left.contains(right)),
        StartsWith => compare_text(condition, value, |left, right| left.starts_with(right)),
        EndsWith => compare_text(condition, value, |left, right| left.ends_with(right)),
        InSet | InAny => text_in_set(condition, value, false),
        NotInSet | NotInAny => text_in_set(condition, value, true),
        ContainsAnyTag => tags_match(condition, value, |tags, expected| {
            expected.iter().any(|item| tags.contains(item))
        }),
        ContainsAllTags => tags_match(condition, value, |tags, expected| {
            expected.iter().all(|item| tags.contains(item))
        }),
        ExcludesAnyTag => tags_match(condition, value, |tags, expected| {
            expected.iter().all(|item| !tags.contains(item))
        }),
        NotContainsAllTags => tags_match(condition, value, |tags, expected| {
            !expected.iter().all(|item| tags.contains(item))
        }),
        EqualsTagSet => tags_match(condition, value, |tags, expected| tags == expected),
        TagNameContains => tag_text_match(condition, value, |name, needle| name.contains(needle)),
        TagNameNotContains => tag_text_all(condition, value, |name, needle| !name.contains(needle)),
        CountEquals => tag_count(condition, value, |left, right| left == right),
        CountGreaterThanOrEqual => tag_count(condition, value, |left, right| left >= right),
        CountLessThanOrEqual => tag_count(condition, value, |left, right| left <= right),
        Before => compare_date(condition, value, |left, right| left < right),
        After => compare_date(condition, value, |left, right| left > right),
        RecentHours => recent(condition, value, now, Duration::hours),
        RecentDays => recent(condition, value, now, Duration::days),
    }
}

fn compare_equals(condition: &Condition, value: Option<&FieldValue>) -> bool {
    match (value, &condition.value) {
        (Some(FieldValue::Number(left)), Some(ConditionValue::Number(right))) => {
            (*left - *right).abs() < f64::EPSILON
        }
        (Some(FieldValue::Text(left)), Some(ConditionValue::Text(right)))
        | (Some(FieldValue::Category(left)), Some(ConditionValue::Text(right))) => {
            normalize(left, condition.case_sensitive) == normalize(right, condition.case_sensitive)
        }
        (Some(FieldValue::Date(left)), Some(ConditionValue::Date(right))) => left == right,
        _ => false,
    }
}

fn compare_number(
    condition: &Condition,
    value: Option<&FieldValue>,
    compare: impl FnOnce(f64, f64) -> bool,
) -> bool {
    match (value, &condition.value) {
        (Some(FieldValue::Number(left)), Some(ConditionValue::Number(right))) => {
            compare(*left, *right)
        }
        _ => false,
    }
}

fn number_between(condition: &Condition, value: Option<&FieldValue>, invert: bool) -> bool {
    match (value, &condition.value) {
        (Some(FieldValue::Number(left)), Some(ConditionValue::NumberRange(range))) => {
            let contains = *left >= range.min && *left <= range.max;
            contains != invert
        }
        _ => false,
    }
}

fn date_between(condition: &Condition, value: Option<&FieldValue>, invert: bool) -> bool {
    match (value, &condition.value) {
        (Some(FieldValue::Date(left)), Some(ConditionValue::DateRange(range))) => {
            let contains = *left >= range.start && *left <= range.end;
            contains != invert
        }
        _ => false,
    }
}

fn compare_text(
    condition: &Condition,
    value: Option<&FieldValue>,
    compare: impl FnOnce(&str, &str) -> bool,
) -> bool {
    match (value, &condition.value) {
        (Some(FieldValue::Text(left)), Some(ConditionValue::Text(right)))
        | (Some(FieldValue::Category(left)), Some(ConditionValue::Text(right))) => compare(
            &normalize(left, condition.case_sensitive),
            &normalize(right, condition.case_sensitive),
        ),
        _ => false,
    }
}

fn text_in_set(condition: &Condition, value: Option<&FieldValue>, invert: bool) -> bool {
    match (value, &condition.value) {
        (Some(FieldValue::Text(left)), Some(ConditionValue::TextList(items)))
        | (Some(FieldValue::Category(left)), Some(ConditionValue::TextList(items))) => {
            let left = normalize(left, condition.case_sensitive);
            let contains = items
                .iter()
                .map(|item| normalize(item, condition.case_sensitive))
                .any(|item| item == left);
            contains != invert
        }
        _ => false,
    }
}

fn compare_date(
    condition: &Condition,
    value: Option<&FieldValue>,
    compare: impl FnOnce(OffsetDateTime, OffsetDateTime) -> bool,
) -> bool {
    match (value, &condition.value) {
        (Some(FieldValue::Date(left)), Some(ConditionValue::Date(right))) => compare(*left, *right),
        _ => false,
    }
}

fn recent(
    condition: &Condition,
    value: Option<&FieldValue>,
    now: OffsetDateTime,
    duration: impl FnOnce(i64) -> Duration,
) -> bool {
    let amount = match condition.value {
        Some(ConditionValue::DurationHours(value)) | Some(ConditionValue::DurationDays(value)) => {
            value
        }
        _ => return false,
    };
    match value {
        Some(FieldValue::Date(left)) => *left >= now - duration(amount) && *left <= now,
        _ => false,
    }
}

fn tags_match(
    condition: &Condition,
    value: Option<&FieldValue>,
    compare: impl FnOnce(&[String], &[String]) -> bool,
) -> bool {
    match (value, &condition.value) {
        (Some(FieldValue::Tags(tags)), Some(ConditionValue::TextList(expected))) => {
            let mut tags = normalized_tags(condition, tags);
            let mut expected: Vec<_> = expected
                .iter()
                .map(|item| normalize(item, condition.case_sensitive))
                .collect();
            tags.sort();
            expected.sort();
            compare(&tags, &expected)
        }
        _ => false,
    }
}

fn tag_text_match(
    condition: &Condition,
    value: Option<&FieldValue>,
    compare: impl Fn(&str, &str) -> bool,
) -> bool {
    match (value, &condition.value) {
        (Some(FieldValue::Tags(tags)), Some(ConditionValue::Text(needle))) => {
            let needle = normalize(needle, condition.case_sensitive);
            normalized_tags(condition, tags)
                .iter()
                .any(|tag| compare(tag, &needle))
        }
        _ => false,
    }
}

fn tag_text_all(
    condition: &Condition,
    value: Option<&FieldValue>,
    compare: impl Fn(&str, &str) -> bool,
) -> bool {
    match (value, &condition.value) {
        (Some(FieldValue::Tags(tags)), Some(ConditionValue::Text(needle))) => {
            let needle = normalize(needle, condition.case_sensitive);
            normalized_tags(condition, tags)
                .iter()
                .all(|tag| compare(tag, &needle))
        }
        _ => false,
    }
}

fn tag_count(
    condition: &Condition,
    value: Option<&FieldValue>,
    compare: impl FnOnce(f64, f64) -> bool,
) -> bool {
    match (value, &condition.value) {
        (Some(FieldValue::Tags(tags)), Some(ConditionValue::Number(expected))) => {
            compare(tags.len() as f64, *expected)
        }
        _ => false,
    }
}

fn normalized_tags(condition: &Condition, tags: &[TagValue]) -> Vec<String> {
    tags.iter()
        .flat_map(|tag| {
            let mut names = vec![normalize(&tag.original, condition.case_sensitive)];
            if condition.tag_scope == Some(TagScope::OriginalAndTranslation)
                && let Some(translation) = &tag.translation
            {
                names.push(normalize(translation, condition.case_sensitive));
            }
            names
        })
        .collect()
}

fn normalize(value: &str, case_sensitive: Option<bool>) -> String {
    let normalized: String = value.nfkc().collect();
    if case_sensitive.unwrap_or(false) {
        normalized
    } else {
        normalized.to_lowercase()
    }
}

fn work_value(field: RuleField, candidate: &RuleCandidate) -> Option<FieldValue> {
    match field {
        RuleField::PixivWorkId => Some(FieldValue::Number(candidate.pixiv_work_id as f64)),
        RuleField::ContentType => Some(FieldValue::Category(candidate.content_type.clone())),
        RuleField::Title => candidate.title.clone().map(FieldValue::Text),
        RuleField::Description => candidate.description.clone().map(FieldValue::Text),
        RuleField::ArtistId => candidate
            .artist_id
            .map(|value| FieldValue::Number(value as f64)),
        RuleField::ArtistName => candidate.artist_name.clone().map(FieldValue::Text),
        RuleField::PublishedAt => candidate.published_at.map(FieldValue::Date),
        RuleField::UpdatedAt => candidate.updated_at.map(FieldValue::Date),
        RuleField::Tags => Some(FieldValue::Tags(
            candidate
                .tags
                .iter()
                .map(|tag| TagValue {
                    original: tag.original.clone(),
                    translation: tag.translation.clone(),
                })
                .collect(),
        )),
        RuleField::PageCount => Some(FieldValue::Number(candidate.page_count as f64)),
        RuleField::AgeRating => candidate.age_rating.clone().map(FieldValue::Category),
        RuleField::AiGenerated => candidate.ai_generated.map(FieldValue::Boolean),
        RuleField::OriginalWork => candidate.original_work.map(FieldValue::Boolean),
        RuleField::BookmarkedByCurrentAccount => candidate
            .bookmarked_by_current_account
            .map(FieldValue::Boolean),
        RuleField::BookmarkCount => candidate
            .bookmark_count
            .map(|value| FieldValue::Number(value as f64)),
        RuleField::ViewCount => candidate
            .view_count
            .map(|value| FieldValue::Number(value as f64)),
        RuleField::LikeCount => candidate
            .like_count
            .map(|value| FieldValue::Number(value as f64)),
        RuleField::CommentCount => candidate
            .comment_count
            .map(|value| FieldValue::Number(value as f64)),
        RuleField::BookmarkRate => candidate.bookmark_rate.map(FieldValue::Number),
        RuleField::BookmarksPerDay => candidate.bookmarks_per_day.map(FieldValue::Number),
        RuleField::RankingRank => candidate
            .ranking_rank
            .map(|value| FieldValue::Number(value as f64)),
        RuleField::RankingDate => candidate.ranking_date.map(FieldValue::Date),
        RuleField::SeriesId => candidate
            .series_id
            .map(|value| FieldValue::Number(value as f64)),
        RuleField::SeriesTitle => candidate.series_title.clone().map(FieldValue::Text),
        RuleField::SeriesOrder => candidate
            .series_order
            .map(|value| FieldValue::Number(value as f64)),
        _ => None,
    }
}

fn page_value(
    field: RuleField,
    candidate: &RuleCandidate,
    page: &PageRuleMetadata,
) -> Option<FieldValue> {
    match field {
        RuleField::PageOriginalExtension => {
            page.original_extension.clone().map(FieldValue::Category)
        }
        RuleField::PageWidth => page.width.map(|value| FieldValue::Number(value as f64)),
        RuleField::PageHeight => page.height.map(|value| FieldValue::Number(value as f64)),
        RuleField::PageAspectRatio => page.aspect_ratio.map(FieldValue::Number),
        RuleField::PageOrientation => page.orientation.clone().map(FieldValue::Category),
        _ => work_value(field, candidate),
    }
}
