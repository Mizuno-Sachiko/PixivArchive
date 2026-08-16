use super::RuleField;
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RuleValueOption {
    pub value: &'static str,
    pub label: &'static str,
}

const CONTENT_TYPES: &[RuleValueOption] = &[
    RuleValueOption {
        value: "illustration",
        label: "插画",
    },
    RuleValueOption {
        value: "manga",
        label: "漫画",
    },
    RuleValueOption {
        value: "ugoira",
        label: "动图",
    },
];

const AGE_RATINGS: &[RuleValueOption] = &[
    RuleValueOption {
        value: "all_age",
        label: "全年龄",
    },
    RuleValueOption {
        value: "r18",
        label: "R-18",
    },
    RuleValueOption {
        value: "r18g",
        label: "R-18G",
    },
];

const ORIGINAL_EXTENSIONS: &[RuleValueOption] = &[
    RuleValueOption {
        value: "jpg",
        label: "JPEG",
    },
    RuleValueOption {
        value: "png",
        label: "PNG",
    },
    RuleValueOption {
        value: "gif",
        label: "GIF",
    },
];

const PAGE_ORIENTATIONS: &[RuleValueOption] = &[
    RuleValueOption {
        value: "portrait",
        label: "竖图",
    },
    RuleValueOption {
        value: "landscape",
        label: "横图",
    },
    RuleValueOption {
        value: "square",
        label: "方图",
    },
];

pub(super) fn value_options(field: RuleField) -> &'static [RuleValueOption] {
    match field {
        RuleField::ContentType => CONTENT_TYPES,
        RuleField::AgeRating => AGE_RATINGS,
        RuleField::PageOriginalExtension => ORIGINAL_EXTENSIONS,
        RuleField::PageOrientation => PAGE_ORIENTATIONS,
        _ => &[],
    }
}
