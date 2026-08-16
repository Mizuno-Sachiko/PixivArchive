use super::{
    FieldScope, FieldType, NumberRange, PageQuantifier, RuleField, RuleOperator, TagScope,
    options::{RuleValueOption, value_options},
    schema::CURRENT_SCHEMA_VERSION,
};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RuleCatalog {
    pub schema_version: u32,
    pub fields: Vec<RuleFieldCatalog>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RuleFieldCatalog {
    pub value: RuleField,
    #[serde(rename = "type")]
    pub value_type: FieldType,
    pub scope: FieldScope,
    pub value_example: &'static str,
    pub help: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<RuleValueOption>,
    pub operators: Vec<RuleOperatorCatalog>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub case_sensitive: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_scope: Option<TagScope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_quantifier: Option<PageQuantifier>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RuleOperatorCatalog {
    pub value: RuleOperator,
    pub requires_value: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_value: Option<RuleInitialValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum RuleInitialValue {
    Number(f64),
    NumberRange(NumberRange),
    Text(String),
    TextList(Vec<String>),
    CurrentDateTime,
    CurrentDateTimeRange,
    DurationHours(i64),
    DurationDays(i64),
}

pub fn rule_catalog() -> RuleCatalog {
    RuleCatalog {
        schema_version: CURRENT_SCHEMA_VERSION,
        fields: RuleField::ALL.iter().copied().map(field_catalog).collect(),
    }
}

fn field_catalog(field: RuleField) -> RuleFieldCatalog {
    let value_type = field.value_type();
    RuleFieldCatalog {
        value: field,
        value_type,
        scope: field.scope(),
        value_example: value_example(field),
        help: help(field),
        options: value_options(field).to_vec(),
        operators: RuleOperator::for_type(value_type)
            .iter()
            .copied()
            .map(|operator| RuleOperatorCatalog {
                value: operator,
                requires_value: operator.requires_value(),
                initial_value: initial_value(field, value_type, operator),
            })
            .collect(),
        case_sensitive: matches!(value_type, FieldType::Text | FieldType::Tags).then_some(false),
        tag_scope: (value_type == FieldType::Tags).then_some(TagScope::OriginalAndTranslation),
        page_quantifier: (field.scope() == FieldScope::Page).then_some(PageQuantifier::AnyPage),
    }
}

fn value_example(field: RuleField) -> &'static str {
    match field {
        RuleField::PixivWorkId => "123456789",
        RuleField::ContentType => "illustration",
        RuleField::Title => "夏日",
        RuleField::Description => "海边",
        RuleField::ArtistId => "12345",
        RuleField::ArtistName => "作者名",
        RuleField::PublishedAt | RuleField::UpdatedAt => "2026-08-01T12:30:00Z",
        RuleField::Tags => "猫, original",
        RuleField::PageCount => "2",
        RuleField::AgeRating => "all_age",
        RuleField::AiGenerated
        | RuleField::OriginalWork
        | RuleField::BookmarkedByCurrentAccount => "true",
        RuleField::BookmarkCount => "1000",
        RuleField::ViewCount => "5000",
        RuleField::LikeCount => "300",
        RuleField::CommentCount => "20",
        RuleField::BookmarkRate => "0.2",
        RuleField::BookmarksPerDay => "10",
        RuleField::RankingRank => "1",
        RuleField::RankingDate => "2026-08-01",
        RuleField::SeriesId => "456",
        RuleField::SeriesTitle => "系列名",
        RuleField::SeriesOrder => "1",
        RuleField::PageOriginalExtension => "png",
        RuleField::PageWidth => "2048",
        RuleField::PageHeight => "3072",
        RuleField::PageAspectRatio => "0.6667",
        RuleField::PageOrientation => "portrait",
    }
}

fn help(field: RuleField) -> &'static str {
    match field {
        RuleField::PixivWorkId => "Pixiv作品的数字ID",
        RuleField::ContentType => "作品在Pixiv中的内容类型",
        RuleField::Title => "作品标题",
        RuleField::Description => "作品简介中的文本",
        RuleField::ArtistId => "Pixiv作者的数字ID",
        RuleField::ArtistName => "作者显示名称",
        RuleField::PublishedAt => "作品在Pixiv的发布时间",
        RuleField::UpdatedAt => "作品元数据的最近更新时间",
        RuleField::Tags => "多个标签使用逗号分隔",
        RuleField::PageCount => "作品包含的页面数量",
        RuleField::AgeRating => "Pixiv标注的年龄分级",
        RuleField::AiGenerated => "Pixiv标记的AI生成状态",
        RuleField::OriginalWork => "作品是否标记为原创",
        RuleField::BookmarkedByCurrentAccount => "当前Pixiv账户是否已收藏",
        RuleField::BookmarkCount => "Pixiv收藏数量",
        RuleField::ViewCount => "Pixiv浏览数量",
        RuleField::LikeCount => "Pixiv点赞数量",
        RuleField::CommentCount => "Pixiv评论数量",
        RuleField::BookmarkRate => "收藏数除以浏览数",
        RuleField::BookmarksPerDay => "发布后平均每天新增的收藏数",
        RuleField::RankingRank => "作品在榜单中的名次",
        RuleField::RankingDate => "榜单日期",
        RuleField::SeriesId => "Pixiv系列的数字ID",
        RuleField::SeriesTitle => "Pixiv系列标题",
        RuleField::SeriesOrder => "作品在系列中的顺序",
        RuleField::PageOriginalExtension => "原图文件扩展名",
        RuleField::PageWidth => "页面宽度，单位为像素",
        RuleField::PageHeight => "页面高度，单位为像素",
        RuleField::PageAspectRatio => "页面宽度除以高度",
        RuleField::PageOrientation => "根据页面宽高计算的画面方向",
    }
}

fn initial_value(
    field: RuleField,
    field_type: FieldType,
    operator: RuleOperator,
) -> Option<RuleInitialValue> {
    use RuleInitialValue::*;
    use RuleOperator::*;
    if let Some(option) = value_options(field).first() {
        match operator {
            Equals | NotEquals => return Some(Text(option.value.to_owned())),
            InAny | NotInAny => return Some(TextList(vec![option.value.to_owned()])),
            _ => {}
        }
    }
    match (field_type, operator) {
        (_, Exists | Missing) | (FieldType::Boolean, IsTrue | IsFalse) => None,
        (FieldType::Date, Between) => Some(CurrentDateTimeRange),
        (_, Between | NotBetween) => Some(NumberRange(super::NumberRange { min: 0.0, max: 0.0 })),
        (FieldType::Date, RecentHours) => Some(DurationHours(24)),
        (FieldType::Date, RecentDays) => Some(DurationDays(7)),
        (
            _,
            InSet | NotInSet | InAny | NotInAny | ContainsAnyTag | ContainsAllTags | ExcludesAnyTag
            | NotContainsAllTags | EqualsTagSet,
        ) => Some(TextList(vec![String::new()])),
        (
            FieldType::Number,
            Equals | NotEquals | GreaterThan | GreaterThanOrEqual | LessThan | LessThanOrEqual,
        )
        | (FieldType::Tags, CountEquals | CountGreaterThanOrEqual | CountLessThanOrEqual) => {
            Some(Number(0.0))
        }
        (FieldType::Date, Before | After) => Some(CurrentDateTime),
        (FieldType::Text | FieldType::Category | FieldType::Tags, _) => Some(Text(String::new())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn catalog_covers_every_field_and_supported_operator() {
        let catalog = rule_catalog();
        assert_eq!(catalog.fields.len(), RuleField::ALL.len());
        for field in catalog.fields {
            assert!(!field.operators.is_empty());
            assert!(
                field
                    .operators
                    .iter()
                    .all(|operator| operator.value.supports(field.value_type))
            );
        }
    }

    #[test]
    fn public_catalog_contains_only_supported_fields_with_filling_guidance() {
        let catalog = serde_json::to_value(rule_catalog()).unwrap();
        let fields = catalog["fields"].as_array().unwrap();
        let names: Vec<_> = fields
            .iter()
            .filter_map(|field| field["value"].as_str())
            .collect();

        for removed in [
            "subscription_id",
            "subscription_type",
            "discovery_method",
            "illust_kind",
            "first_seen_in_ranking",
            "ranking_type",
            "page_file_size_bytes",
            "work_total_file_size_bytes",
            "page_mime_type",
            "page_color_mode",
            "page_is_ugoira",
            "page_sha256",
        ] {
            assert!(
                !names.contains(&removed),
                "removed field {removed} is public"
            );
        }
        assert!(names.contains(&"ranking_rank"));
        assert!(names.contains(&"ranking_date"));

        for field in fields {
            assert!(non_empty_string(&field["value_example"]));
            assert!(non_empty_string(&field["help"]));
            for operator in field["operators"].as_array().unwrap() {
                let name = operator["value"].as_str().unwrap();
                let requires_value = operator["requires_value"].as_bool().unwrap();
                assert_eq!(
                    requires_value,
                    !matches!(name, "exists" | "missing" | "is_true" | "is_false")
                );
            }
        }

        let tags = field(fields, "tags");
        assert_eq!(tags["value_example"], Value::String("猫, original".into()));
        let ranking_date = field(fields, "ranking_date");
        assert_eq!(
            ranking_date["value_example"],
            Value::String("2026-08-01".into())
        );
    }

    #[test]
    fn finite_value_fields_publish_their_complete_option_sets() {
        let catalog = serde_json::to_value(rule_catalog()).unwrap();
        let fields = catalog["fields"].as_array().unwrap();
        let expected = [
            (
                "content_type",
                vec![
                    ("illustration", "插画"),
                    ("manga", "漫画"),
                    ("ugoira", "动图"),
                ],
            ),
            (
                "age_rating",
                vec![("all_age", "全年龄"), ("r18", "R-18"), ("r18g", "R-18G")],
            ),
            (
                "page_original_extension",
                vec![("jpg", "JPEG"), ("png", "PNG"), ("gif", "GIF")],
            ),
            (
                "page_orientation",
                vec![
                    ("portrait", "竖图"),
                    ("landscape", "横图"),
                    ("square", "方图"),
                ],
            ),
        ];

        for (name, options) in &expected {
            let descriptor = field(fields, name);
            assert_eq!(descriptor["type"], "category");
            let actual: Vec<_> = descriptor["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|option| {
                    (
                        option["value"].as_str().unwrap(),
                        option["label"].as_str().unwrap(),
                    )
                })
                .collect();
            assert_eq!(&actual, options);
        }

        for descriptor in fields {
            let name = descriptor["value"].as_str().unwrap();
            if !expected.iter().any(|(expected, _)| *expected == name) {
                assert!(descriptor.get("options").is_none());
            }
        }
    }

    fn field<'a>(fields: &'a [Value], name: &str) -> &'a Value {
        fields.iter().find(|field| field["value"] == name).unwrap()
    }

    fn non_empty_string(value: &Value) -> bool {
        value.as_str().is_some_and(|value| !value.is_empty())
    }
}
