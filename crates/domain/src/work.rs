use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    job::CollectionState,
    media::{DerivativeFormat, MediaFormat, MediaKind},
    pixiv::{PixivAgeRating, PixivUgoiraMeta, PixivWorkKind},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum FilterMode {
    #[default]
    All,
    Any,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GallerySearch {
    #[serde(default)]
    pub group_mode: FilterMode,
    #[serde(default)]
    pub groups: Vec<GalleryFilterGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restrict_work_ids: Vec<Uuid>,
    #[serde(default)]
    pub sort_field: GallerySortField,
    #[serde(default)]
    pub sort_direction: SortDirection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<GalleryCursor>,
    #[serde(default = "default_gallery_limit")]
    pub limit: u16,
}

impl Default for GallerySearch {
    fn default() -> Self {
        Self {
            group_mode: FilterMode::All,
            groups: Vec::new(),
            restrict_work_ids: Vec::new(),
            sort_field: GallerySortField::default(),
            sort_direction: SortDirection::default(),
            cursor: None,
            limit: default_gallery_limit(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GallerySelectionExpression {
    pub search: GallerySearch,
    #[serde(default)]
    pub base_selected: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exception_work_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GallerySelectionProjection {
    pub selected_count: u64,
    pub selected_visible_work_ids: Vec<Uuid>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GalleryContextKind {
    Artist,
    Tag,
    Series,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GalleryContextSelectionExpression {
    pub kind: GalleryContextKind,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub base_selected: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exception_context_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GalleryContextSelectionProjection {
    pub selected_context_count: u64,
    pub selected_work_count: u64,
    pub selected_visible_context_ids: Vec<Uuid>,
}

const fn default_gallery_limit() -> u16 {
    50
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum WorkSourceState {
    Present,
    Missing,
    Deleted,
    Restricted,
}

impl WorkSourceState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Missing => "missing",
            Self::Deleted => "deleted",
            Self::Restricted => "restricted",
        }
    }

    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "present" => Some(Self::Present),
            "missing" => Some(Self::Missing),
            "deleted" => Some(Self::Deleted),
            "restricted" => Some(Self::Restricted),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GalleryFilterGroup {
    pub mode: FilterMode,
    pub filters: Vec<GalleryFilter>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GalleryFilter {
    WorkId {
        value: Uuid,
    },
    PixivWorkId {
        value: i64,
    },
    ArtistId {
        value: Uuid,
    },
    PixivArtistId {
        value: i64,
    },
    TagId {
        value: Uuid,
    },
    SeriesId {
        value: Uuid,
    },
    MediaRevisionId {
        value: Uuid,
    },
    Text {
        field: GalleryTextField,
        operator: GalleryTextOperator,
        value: String,
    },
    Tags {
        operator: GalleryTagOperator,
        names: Vec<String>,
        scope: GalleryTagScope,
    },
    Category {
        field: GalleryCategoryField,
        #[serde(default)]
        include: Vec<String>,
        #[serde(default)]
        exclude: Vec<String>,
    },
    Number {
        field: GalleryNumberField,
        comparison: GalleryNumberComparison,
    },
    Date {
        field: GalleryDateField,
        comparison: GalleryDateComparison,
    },
    Boolean {
        field: GalleryBooleanField,
        value: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GalleryTextField {
    Any,
    Title,
    Description,
    ArtistName,
    SeriesTitle,
    TagName,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GalleryTextOperator {
    Equals,
    Contains,
    Excludes,
    StartsWith,
    EndsWith,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GalleryTagOperator {
    Any,
    All,
    ExcludeAny,
    NotAll,
    ExactSet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GalleryTagScope {
    Original,
    OriginalAndTranslation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GalleryCategoryField {
    WorkKind,
    AgeRating,
    CollectionState,
    SourceState,
    MediaFormat,
    DerivativeFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GalleryNumberField {
    BookmarkCount,
    ViewCount,
    LikeCount,
    CommentCount,
    PageCount,
    Width,
    Height,
    MediaByteSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "operator", content = "value", rename_all = "snake_case")]
pub enum GalleryNumberComparison {
    Equals(f64),
    GreaterThan(f64),
    GreaterThanOrEqual(f64),
    LessThan(f64),
    LessThanOrEqual(f64),
    Between { min: f64, max: f64 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GalleryDateField {
    PublishedAt,
    SourceUpdatedAt,
    LocalUpdatedAt,
    TrashedAt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "operator", rename_all = "snake_case")]
pub enum GalleryDateComparison {
    Before {
        #[serde(with = "time::serde::rfc3339")]
        value: OffsetDateTime,
    },
    After {
        #[serde(with = "time::serde::rfc3339")]
        value: OffsetDateTime,
    },
    Between {
        #[serde(with = "time::serde::rfc3339")]
        start: OffsetDateTime,
        #[serde(with = "time::serde::rfc3339")]
        end: OffsetDateTime,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GalleryBooleanField {
    Ugoira,
    HasMedia,
    BookmarkedByCurrentAccount,
    AiGenerated,
    OriginalWork,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GallerySortField {
    #[default]
    PixivId,
    LocalUpdatedAt,
    PublishedAt,
    BookmarkCount,
    Title,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Ascending,
    #[default]
    Descending,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GalleryCursor {
    pub sort_field: GallerySortField,
    pub sort_direction: SortDirection,
    pub key: GalleryCursorKey,
    pub work_id: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum GalleryCursorKey {
    Null,
    Integer(i64),
    Date(#[serde(with = "time::serde::rfc3339")] OffsetDateTime),
    Text(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GallerySearchPage {
    pub items: Vec<GalleryWork>,
    pub next_cursor: Option<GalleryCursor>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GalleryWork {
    pub id: Uuid,
    pub pixiv_work_id: i64,
    pub title: String,
    pub description: Option<String>,
    pub artist_id: Uuid,
    pub pixiv_artist_id: i64,
    pub artist_name: String,
    pub series_id: Option<Uuid>,
    pub series_title: Option<String>,
    pub work_kind: PixivWorkKind,
    pub age_rating: PixivAgeRating,
    pub ai_generated: bool,
    pub page_count: u32,
    pub collection_state: CollectionState,
    pub source_state: WorkSourceState,
    pub bookmarked_by_current_account: bool,
    pub bookmark_id: Option<i64>,
    pub bookmark_count: Option<i64>,
    pub view_count: Option<i64>,
    pub like_count: Option<i64>,
    pub comment_count: Option<i64>,
    pub pixiv_published_at: Option<OffsetDateTime>,
    pub pixiv_updated_at: Option<OffsetDateTime>,
    pub local_updated_at: OffsetDateTime,
    pub cover_path: Option<String>,
    pub cover_derivative_id: Option<Uuid>,
    pub cover_width: Option<u32>,
    pub cover_height: Option<u32>,
    pub media_kind: Option<MediaKind>,
    pub tags: Vec<GalleryTag>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GalleryTag {
    pub id: Uuid,
    pub original: String,
    pub translation: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GalleryWorkDetail {
    pub work: GalleryWork,
    pub pages: Vec<GalleryPage>,
    pub ugoira: Option<PixivUgoiraMeta>,
    pub trash_capabilities: Option<TrashActionCapabilities>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GalleryPage {
    pub id: Uuid,
    pub page_index: u32,
    pub source_state: WorkSourceState,
    pub source_url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub current_media: Option<GalleryMediaRevision>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GalleryMediaRevision {
    pub id: Uuid,
    pub revision_number: u64,
    pub media_kind: MediaKind,
    pub format: MediaFormat,
    pub source_path: String,
    pub byte_size: u64,
    pub sha256: Vec<u8>,
    pub derivatives: Vec<GalleryDerivative>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GalleryDerivative {
    pub id: Uuid,
    pub kind: String,
    pub format: DerivativeFormat,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub byte_size: u64,
    pub dominant_color: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GalleryArtistDetail {
    pub id: Uuid,
    pub pixiv_artist_id: i64,
    pub name: String,
    pub account_name: Option<String>,
    pub work_count: u64,
    pub cover_derivative_id: Option<Uuid>,
    pub cover_width: Option<u32>,
    pub cover_height: Option<u32>,
    pub cover_age_rating: Option<PixivAgeRating>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GalleryTagDetail {
    pub tag: GalleryTag,
    pub work_count: u64,
    pub cover_derivative_id: Option<Uuid>,
    pub cover_width: Option<u32>,
    pub cover_height: Option<u32>,
    pub cover_age_rating: Option<PixivAgeRating>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GallerySeriesDetail {
    pub id: Uuid,
    pub pixiv_series_id: i64,
    pub pixiv_artist_id: Option<i64>,
    pub title: String,
    pub work_count: u64,
    pub cover_derivative_id: Option<Uuid>,
    pub cover_width: Option<u32>,
    pub cover_height: Option<u32>,
    pub cover_age_rating: Option<PixivAgeRating>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GalleryOverviewDecoration {
    pub pixiv_work_id: i64,
    pub title: String,
    pub age_rating: PixivAgeRating,
    pub cover_derivative_id: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GalleryContextPage<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub next_cursor: Option<GalleryContextCursor>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GalleryContextCursor {
    pub work_count: u64,
    pub normalized_name: String,
    pub identity: GalleryContextIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum GalleryContextIdentity {
    Artist(i64),
    Tag(Uuid),
    Series(i64),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrashEntry {
    pub work_id: Uuid,
    pub previous_collection_state: String,
    pub trashed_at: OffsetDateTime,
    pub scheduled_purge_at: OffsetDateTime,
    pub purge_state: String,
    pub purge_attempts: u32,
    pub failure_message: Option<String>,
    pub capabilities: TrashActionCapabilities,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum TrashActionBlockReason {
    PurgeQueued,
    PurgeStarted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TrashActionCapabilities {
    pub can_restore: bool,
    pub can_reschedule: bool,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub blocked_reason: Option<TrashActionBlockReason>,
}

impl TrashActionCapabilities {
    pub const fn available() -> Self {
        Self {
            can_restore: true,
            can_reschedule: true,
            blocked_reason: None,
        }
    }

    pub const fn blocked(reason: TrashActionBlockReason) -> Self {
        Self {
            can_restore: false,
            can_reschedule: false,
            blocked_reason: Some(reason),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrashWorkSummary {
    pub entry: TrashEntry,
    pub pixiv_work_id: i64,
    pub title: String,
    pub artist_name: String,
    pub page_count: u32,
    pub estimated_release_bytes: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TrashFilter {
    pub query: Option<String>,
    pub purge_states: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TrashSelectionExpression {
    pub filter: TrashFilter,
    #[serde(default)]
    pub base_selected: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exception_work_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TrashSelectionProjection {
    pub selected_count: u64,
    pub blocked_count: u64,
    pub selected_visible_work_ids: Vec<Uuid>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TrashSelectionMutation {
    pub selected_count: u64,
    pub blocked_count: u64,
    pub affected_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrashCursor {
    pub scheduled_purge_at: OffsetDateTime,
    pub work_id: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrashPage {
    pub items: Vec<TrashWorkSummary>,
    pub next_cursor: Option<TrashCursor>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrashCollectionSummary {
    pub total_count: u64,
    pub logical_bytes: u64,
    pub estimated_reclaimable_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DuePurge {
    pub work_id: Uuid,
    pub job_id: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkRevisionSummary {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub work_kind: PixivWorkKind,
    pub page_count: u32,
    pub captured_at: OffsetDateTime,
}
