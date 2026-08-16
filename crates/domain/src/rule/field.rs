use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum FieldScope {
    Work,
    Page,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Number,
    Text,
    Category,
    Tags,
    Date,
    Boolean,
}

macro_rules! rule_fields {
    ($($variant:ident),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
        #[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
        #[serde(rename_all = "snake_case")]
        pub enum RuleField {
            $($variant),+
        }

        impl RuleField {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
        }
    };
}

rule_fields!(
    PixivWorkId,
    ContentType,
    Title,
    Description,
    ArtistId,
    ArtistName,
    PublishedAt,
    UpdatedAt,
    Tags,
    PageCount,
    AgeRating,
    AiGenerated,
    OriginalWork,
    BookmarkedByCurrentAccount,
    BookmarkCount,
    ViewCount,
    LikeCount,
    CommentCount,
    BookmarkRate,
    BookmarksPerDay,
    RankingRank,
    RankingDate,
    SeriesId,
    SeriesTitle,
    SeriesOrder,
    PageOriginalExtension,
    PageWidth,
    PageHeight,
    PageAspectRatio,
    PageOrientation,
);

impl RuleField {
    pub fn scope(self) -> FieldScope {
        match self {
            Self::PageOriginalExtension
            | Self::PageWidth
            | Self::PageHeight
            | Self::PageAspectRatio
            | Self::PageOrientation => FieldScope::Page,
            _ => FieldScope::Work,
        }
    }

    pub fn value_type(self) -> FieldType {
        match self {
            Self::PixivWorkId
            | Self::ArtistId
            | Self::PageCount
            | Self::BookmarkCount
            | Self::ViewCount
            | Self::LikeCount
            | Self::CommentCount
            | Self::BookmarkRate
            | Self::BookmarksPerDay
            | Self::RankingRank
            | Self::SeriesId
            | Self::SeriesOrder
            | Self::PageWidth
            | Self::PageHeight
            | Self::PageAspectRatio => FieldType::Number,
            Self::Title | Self::Description | Self::ArtistName | Self::SeriesTitle => {
                FieldType::Text
            }
            Self::ContentType
            | Self::AgeRating
            | Self::PageOriginalExtension
            | Self::PageOrientation => FieldType::Category,
            Self::Tags => FieldType::Tags,
            Self::PublishedAt | Self::UpdatedAt | Self::RankingDate => FieldType::Date,
            Self::AiGenerated | Self::OriginalWork | Self::BookmarkedByCurrentAccount => {
                FieldType::Boolean
            }
        }
    }
}
