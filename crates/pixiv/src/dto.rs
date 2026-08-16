use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct AjaxEnvelope<T> {
    pub error: bool,
    #[serde(default)]
    pub message: String,
    pub body: T,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum NumberValue {
    Signed(i64),
    Unsigned(u64),
    String(String),
}

impl NumberValue {
    pub(crate) fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Signed(value) => Some(*value),
            Self::Unsigned(value) => i64::try_from(*value).ok(),
            Self::String(value) => value.parse().ok(),
        }
    }

    pub(crate) fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Signed(value) => u64::try_from(*value).ok(),
            Self::Unsigned(value) => Some(*value),
            Self::String(value) => value.parse().ok(),
        }
    }

    pub(crate) fn as_u32(&self) -> Option<u32> {
        self.as_u64().and_then(|value| u32::try_from(value).ok())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct WorkDetailBody {
    #[serde(rename = "illustId")]
    pub illust_id: NumberValue,
    #[serde(rename = "illustTitle")]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "illustType")]
    pub illust_type: NumberValue,
    #[serde(rename = "createDate", default)]
    pub create_date: Option<String>,
    #[serde(rename = "uploadDate", default)]
    pub upload_date: Option<String>,
    #[serde(rename = "xRestrict", default)]
    pub x_restrict: Option<NumberValue>,
    #[serde(rename = "aiType", default)]
    pub ai_type: Option<NumberValue>,
    #[serde(rename = "isOriginal", default)]
    pub is_original: bool,
    #[serde(rename = "userId")]
    pub user_id: NumberValue,
    #[serde(rename = "userName")]
    pub user_name: String,
    #[serde(rename = "userAccount", default)]
    pub user_account: Option<String>,
    #[serde(rename = "pageCount")]
    pub page_count: NumberValue,
    pub width: NumberValue,
    pub height: NumberValue,
    #[serde(rename = "bookmarkCount", default)]
    pub bookmark_count: Option<NumberValue>,
    #[serde(rename = "likeCount", default)]
    pub like_count: Option<NumberValue>,
    #[serde(rename = "commentCount", default)]
    pub comment_count: Option<NumberValue>,
    #[serde(rename = "viewCount", default)]
    pub view_count: Option<NumberValue>,
    #[serde(default)]
    pub tags: TagsDto,
    #[serde(rename = "bookmarkData", default)]
    pub bookmark_data: Option<BookmarkDataDto>,
    #[serde(rename = "seriesNavData", default)]
    pub series: Option<SeriesDto>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct TagsDto {
    #[serde(default)]
    pub tags: Vec<TagDto>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct TagDto {
    pub tag: String,
    #[serde(default)]
    pub translation: Option<TagTranslationDto>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct TagTranslationDto {
    #[serde(default)]
    pub en: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct BookmarkDataDto {
    pub id: NumberValue,
    #[serde(default)]
    pub private: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct SeriesDto {
    #[serde(rename = "seriesId")]
    pub series_id: NumberValue,
    pub title: String,
    #[serde(default)]
    pub order: Option<NumberValue>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct PageDto {
    pub urls: PageUrlsDto,
    pub width: NumberValue,
    pub height: NumberValue,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct PageUrlsDto {
    pub original: String,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct UgoiraBodyDto {
    #[serde(rename = "originalSrc")]
    pub original_src: String,
    #[serde(rename = "mime_type")]
    pub mime_type: String,
    pub frames: Vec<UgoiraFrameDto>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct UgoiraFrameDto {
    pub file: String,
    pub delay: NumberValue,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RankingDto {
    pub contents: Vec<RankingEntryDto>,
    pub next: PageOrFalse,
    #[serde(default)]
    pub date: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RankingEntryDto {
    pub illust_id: NumberValue,
    pub title: String,
    pub illust_type: NumberValue,
    pub illust_page_count: NumberValue,
    pub user_id: NumberValue,
    pub user_name: String,
    pub rank: NumberValue,
    #[serde(default)]
    pub yes_rank: Option<PageOrFalse>,
    #[serde(default)]
    pub width: Option<NumberValue>,
    #[serde(default)]
    pub height: Option<NumberValue>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub attr: String,
    #[serde(default)]
    pub is_bookmarked: bool,
    #[serde(default)]
    pub view_count: Option<NumberValue>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum PageOrFalse {
    Number(NumberValue),
    Boolean(bool),
}

impl PageOrFalse {
    pub(crate) fn page(&self) -> Result<Option<u32>, ()> {
        match self {
            Self::Number(value) => value
                .as_u32()
                .map(|page| if page == 0 { None } else { Some(page) })
                .ok_or(()),
            Self::Boolean(false) => Ok(None),
            Self::Boolean(true) => Err(()),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FollowLatestBodyDto {
    #[serde(default)]
    pub page: Option<FollowPageDto>,
    #[serde(rename = "tagTranslation", default)]
    pub tag_translation: BTreeMap<String, TagTranslationDto>,
    pub thumbnails: ThumbnailsDto,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FollowPageDto {
    #[serde(default)]
    pub ids: Vec<NumberValue>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ThumbnailsDto {
    #[serde(default)]
    pub illust: Vec<DiscoveryWorkDto>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct DiscoveryWorkDto {
    pub id: NumberValue,
    pub title: String,
    #[serde(rename = "illustType")]
    pub illust_type: NumberValue,
    #[serde(rename = "xRestrict", default)]
    pub x_restrict: Option<NumberValue>,
    #[serde(rename = "aiType", default)]
    pub ai_type: Option<NumberValue>,
    #[serde(rename = "userId")]
    pub user_id: NumberValue,
    #[serde(rename = "userName")]
    pub user_name: String,
    #[serde(rename = "pageCount")]
    pub page_count: NumberValue,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(rename = "isOriginal", default)]
    pub is_original: bool,
    #[serde(rename = "bookmarkData", default)]
    pub bookmark_data: Option<BookmarkDataDto>,
    #[serde(default)]
    pub width: Option<NumberValue>,
    #[serde(default)]
    pub height: Option<NumberValue>,
    #[serde(rename = "viewCount", default)]
    pub view_count: Option<NumberValue>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct BookmarkWorkDto {
    pub id: NumberValue,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct BookmarksBodyDto {
    #[serde(default)]
    pub works: Vec<BookmarkWorkDto>,
    pub total: NumberValue,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FollowingBodyDto {
    #[serde(default)]
    pub users: Vec<FollowedUserDto>,
    pub total: NumberValue,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct FollowedUserDto {
    #[serde(rename = "userId")]
    pub user_id: NumberValue,
    #[serde(rename = "userName")]
    pub user_name: String,
    #[serde(rename = "profileImageUrl", default)]
    pub profile_image_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ProfileAllBodyDto {
    #[serde(default)]
    pub illusts: Value,
    #[serde(default)]
    pub manga: Value,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct AccountProfileBodyDto {
    #[serde(rename = "userId")]
    pub user_id: NumberValue,
    pub name: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(rename = "imageBig", default)]
    pub image_big: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ArtistFollowProfileBodyDto {
    #[serde(rename = "userId")]
    pub user_id: NumberValue,
    pub name: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(rename = "isFollowed")]
    pub is_followed: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RemoveArtistFollowBodyDto {
    pub user_id: NumberValue,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct BookmarkWriteBodyDto {
    #[serde(rename = "last_bookmark_id", default)]
    pub last_bookmark_id: Option<NumberValue>,
}
