use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PixivWorkKind {
    Illustration,
    Manga,
    Ugoira,
}

impl PixivWorkKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Illustration => "illustration",
            Self::Manga => "manga",
            Self::Ugoira => "ugoira",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "illustration" => Some(Self::Illustration),
            "manga" => Some(Self::Manga),
            "ugoira" => Some(Self::Ugoira),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PixivAgeRating {
    AllAge,
    R18,
    R18g,
    Unknown,
}

impl PixivAgeRating {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AllAge => "all_age",
            Self::R18 => "r18",
            Self::R18g => "r18g",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "all_age" => Some(Self::AllAge),
            "r18" => Some(Self::R18),
            "r18g" => Some(Self::R18g),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixivAiClassification {
    Unknown,
    NotAiGenerated,
    AiGenerated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixivBookmarkVisibility {
    Public,
    Private,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixivFollowingVisibility {
    Public,
    Private,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivTag {
    pub name: String,
    pub translated_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivArtistRef {
    pub pixiv_id: i64,
    pub name: String,
    pub account_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivSeriesRef {
    pub pixiv_id: i64,
    pub title: String,
    pub order: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivDimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivWorkCounts {
    pub bookmarks: u64,
    pub likes: u64,
    pub comments: u64,
    pub views: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivBookmarkRef {
    pub bookmark_id: i64,
    pub visibility: PixivBookmarkVisibility,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivWorkDetail {
    pub work_id: i64,
    pub title: String,
    pub description: String,
    pub kind: PixivWorkKind,
    pub age_rating: PixivAgeRating,
    pub ai_classification: PixivAiClassification,
    pub is_original: bool,
    pub artist: PixivArtistRef,
    pub published_at: Option<OffsetDateTime>,
    pub updated_at: Option<OffsetDateTime>,
    pub tags: Vec<PixivTag>,
    pub page_count: u32,
    pub dimensions: PixivDimensions,
    pub counts: PixivWorkCounts,
    pub bookmarked_by_current_account: Option<bool>,
    pub bookmark: Option<PixivBookmarkRef>,
    pub series: Option<PixivSeriesRef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixivImageFormat {
    Jpeg,
    Png,
    Gif,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivWorkPage {
    pub page_index: u32,
    pub original_url: Url,
    pub dimensions: PixivDimensions,
    pub format_hint: Option<PixivImageFormat>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivWorkPages {
    pub work_id: i64,
    pub pages: Vec<PixivWorkPage>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivUgoiraFrame {
    pub file: String,
    pub delay_ms: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivUgoiraMeta {
    pub work_id: i64,
    pub zip_url: Url,
    pub frame_mime_type: String,
    pub frames: Vec<PixivUgoiraFrame>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivDiscoveryWork {
    pub work_id: i64,
    pub title: String,
    pub kind: PixivWorkKind,
    pub age_rating: PixivAgeRating,
    pub ai_classification: PixivAiClassification,
    pub is_original: bool,
    pub artist: PixivArtistRef,
    pub tags: Vec<PixivTag>,
    pub page_count: u32,
    pub dimensions: Option<PixivDimensions>,
    pub view_count: Option<u64>,
    pub bookmarked_by_current_account: Option<bool>,
    pub bookmark: Option<PixivBookmarkRef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixivRankingMode {
    Daily,
    Weekly,
    Monthly,
    Rookie,
    Original,
    AiGenerated,
    R18,
    R18g,
    Male,
    Female,
}

impl PixivRankingMode {
    pub const fn supports_content(self, content: PixivRankingContent) -> bool {
        match self {
            Self::Daily | Self::Weekly | Self::R18 => true,
            Self::Monthly | Self::Rookie | Self::R18g => {
                !matches!(content, PixivRankingContent::Ugoira)
            }
            Self::Original | Self::AiGenerated | Self::Male | Self::Female => {
                matches!(content, PixivRankingContent::All)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixivRankingContent {
    All,
    Illustration,
    Manga,
    Ugoira,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivRankingRequest {
    pub mode: PixivRankingMode,
    pub content: PixivRankingContent,
    pub date: Option<Date>,
    pub page: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivRankingCursor {
    pub mode: PixivRankingMode,
    pub content: PixivRankingContent,
    pub date: Option<Date>,
    pub page: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivRankingEntry {
    pub work: PixivDiscoveryWork,
    pub rank: u32,
    pub previous_rank: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivRankingPage {
    pub date: Option<Date>,
    pub items: Vec<PixivRankingEntry>,
    pub next_cursor: Option<PixivRankingCursor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixivFollowLatestSource {
    Following,
    Mypixiv,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixivFollowLatestMode {
    All,
    R18,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivFollowLatestRequest {
    pub source: PixivFollowLatestSource,
    pub mode: PixivFollowLatestMode,
    pub tag: Option<String>,
    pub language: String,
    pub page: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivFollowLatestCursor {
    pub source: PixivFollowLatestSource,
    pub mode: PixivFollowLatestMode,
    pub tag: Option<String>,
    pub language: String,
    pub page: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixivBookmarksMode {
    All,
    Safe,
    R18,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivBookmarksRequest {
    pub user_id: i64,
    pub visibility: PixivBookmarkVisibility,
    pub mode: PixivBookmarksMode,
    pub tag: Option<String>,
    pub offset: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivBookmarksCursor {
    pub user_id: i64,
    pub visibility: PixivBookmarkVisibility,
    pub mode: PixivBookmarksMode,
    pub tag: Option<String>,
    pub offset: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivFollowingRequest {
    pub user_id: i64,
    pub visibility: PixivFollowingVisibility,
    pub offset: u32,
    pub limit: u32,
    pub language: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivFollowingCursor {
    pub user_id: i64,
    pub visibility: PixivFollowingVisibility,
    pub offset: u32,
    pub limit: u32,
    pub language: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivFollowedArtist {
    pub pixiv_id: i64,
    pub name: String,
    pub profile_image_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivArtistFollowState {
    pub artist_id: i64,
    pub name: String,
    pub profile_image_url: Option<String>,
    pub followed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivArtistFollowRequest {
    pub artist_id: i64,
    pub visibility: PixivFollowingVisibility,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivArtistFollowWriteResult {
    pub artist_id: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivPage<T, C> {
    pub items: Vec<T>,
    pub next_cursor: Option<C>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivArtistWorkIds {
    pub artist_id: i64,
    pub work_ids: Vec<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivBookmarkAddRequest {
    pub work_id: i64,
    pub visibility: PixivBookmarkVisibility,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivBookmarkWriteResult {
    pub bookmark_id: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PixivAccountValidation {
    pub user_id: i64,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub private_bookmarks_verified: bool,
}

#[cfg(test)]
mod tests {
    use super::{PixivAgeRating, PixivWorkKind};

    #[test]
    fn pixiv_enum_strings_match_their_serialized_contract() {
        for (kind, expected) in [
            (PixivWorkKind::Illustration, "illustration"),
            (PixivWorkKind::Manga, "manga"),
            (PixivWorkKind::Ugoira, "ugoira"),
        ] {
            assert_eq!(kind.as_str(), expected);
            assert_eq!(serde_json::to_value(kind).unwrap(), expected);
        }

        for (rating, expected) in [
            (PixivAgeRating::AllAge, "all_age"),
            (PixivAgeRating::R18, "r18"),
            (PixivAgeRating::R18g, "r18g"),
            (PixivAgeRating::Unknown, "unknown"),
        ] {
            assert_eq!(rating.as_str(), expected);
            assert_eq!(serde_json::to_value(rating).unwrap(), expected);
        }
    }
}
