use crate::{
    dto::{
        AccountProfileBodyDto, AjaxEnvelope, ArtistFollowProfileBodyDto, BookmarkWriteBodyDto,
        BookmarksBodyDto, DiscoveryWorkDto, FollowLatestBodyDto, FollowedUserDto, FollowingBodyDto,
        NumberValue, PageDto, ProfileAllBodyDto, RankingDto, RankingEntryDto,
        RemoveArtistFollowBodyDto, UgoiraBodyDto, WorkDetailBody,
    },
    error::{PixivError, classify_semantic_error},
    web::{ADAPTER_VERSION, PixivEndpoint},
};
use pixivarchive_domain::pixiv::{
    PixivAgeRating, PixivAiClassification, PixivArtistFollowState, PixivArtistFollowWriteResult,
    PixivArtistRef, PixivArtistWorkIds, PixivBookmarkRef, PixivBookmarkVisibility,
    PixivBookmarkWriteResult, PixivBookmarksCursor, PixivBookmarksRequest, PixivDimensions,
    PixivDiscoveryWork, PixivFollowLatestCursor, PixivFollowLatestRequest, PixivFollowedArtist,
    PixivFollowingCursor, PixivFollowingRequest, PixivImageFormat, PixivPage, PixivRankingCursor,
    PixivRankingEntry, PixivRankingMode, PixivRankingPage, PixivRankingRequest, PixivSeriesRef,
    PixivTag, PixivUgoiraFrame, PixivUgoiraMeta, PixivWorkCounts, PixivWorkDetail, PixivWorkKind,
    PixivWorkPage, PixivWorkPages,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
use time::{
    Date, OffsetDateTime, format_description::well_known::Rfc3339, macros::format_description,
};
use url::Url;

pub(crate) fn map_work_detail(raw: &Value) -> Result<PixivWorkDetail, PixivError> {
    let body: WorkDetailBody = ajax_body(raw, PixivEndpoint::WorkDetail)?;
    let endpoint = PixivEndpoint::WorkDetail;
    let bookmark = body
        .bookmark_data
        .as_ref()
        .map(|bookmark| {
            Ok(PixivBookmarkRef {
                bookmark_id: positive_i64(&bookmark.id, endpoint)?,
                visibility: if bookmark.private {
                    PixivBookmarkVisibility::Private
                } else {
                    PixivBookmarkVisibility::Public
                },
            })
        })
        .transpose()?;
    let series = body
        .series
        .as_ref()
        .map(|series| {
            Ok(PixivSeriesRef {
                pixiv_id: positive_i64(&series.series_id, endpoint)?,
                title: series.title.clone(),
                order: series
                    .order
                    .as_ref()
                    .map(|value| required_u32(value, endpoint))
                    .transpose()?,
            })
        })
        .transpose()?;

    Ok(PixivWorkDetail {
        work_id: positive_i64(&body.illust_id, endpoint)?,
        title: body.title,
        description: body.description,
        kind: work_kind(&body.illust_type, endpoint)?,
        age_rating: age_rating(body.x_restrict.as_ref()),
        ai_classification: ai_classification(body.ai_type.as_ref()),
        is_original: body.is_original,
        artist: PixivArtistRef {
            pixiv_id: positive_i64(&body.user_id, endpoint)?,
            name: body.user_name,
            account_name: body.user_account.filter(|value| !value.is_empty()),
        },
        published_at: optional_timestamp(body.create_date.as_deref(), endpoint)?,
        updated_at: optional_timestamp(body.upload_date.as_deref(), endpoint)?,
        tags: body
            .tags
            .tags
            .into_iter()
            .map(|tag| PixivTag {
                name: tag.tag,
                translated_name: tag.translation.and_then(|translation| translation.en),
            })
            .collect(),
        page_count: positive_u32(&body.page_count, endpoint)?,
        dimensions: PixivDimensions {
            width: positive_u32(&body.width, endpoint)?,
            height: positive_u32(&body.height, endpoint)?,
        },
        counts: PixivWorkCounts {
            bookmarks: optional_u64(body.bookmark_count.as_ref()),
            likes: optional_u64(body.like_count.as_ref()),
            comments: optional_u64(body.comment_count.as_ref()),
            views: optional_u64(body.view_count.as_ref()),
        },
        bookmarked_by_current_account: Some(bookmark.is_some()),
        bookmark,
        series,
    })
}

pub(crate) fn map_work_pages(work_id: i64, raw: &Value) -> Result<PixivWorkPages, PixivError> {
    let endpoint = PixivEndpoint::WorkPages;
    require_positive_id(work_id, endpoint)?;
    let pages: Vec<PageDto> = ajax_body(raw, endpoint)?;
    let pages = pages
        .into_iter()
        .enumerate()
        .map(|(page_index, page)| {
            let original_url =
                Url::parse(&page.urls.original).map_err(|_| PixivError::invalid_json(endpoint))?;
            Ok(PixivWorkPage {
                page_index: u32::try_from(page_index)
                    .map_err(|_| PixivError::invalid_json(endpoint))?,
                format_hint: image_format_hint(&original_url),
                original_url,
                dimensions: PixivDimensions {
                    width: positive_u32(&page.width, endpoint)?,
                    height: positive_u32(&page.height, endpoint)?,
                },
            })
        })
        .collect::<Result<Vec<_>, PixivError>>()?;
    Ok(PixivWorkPages { work_id, pages })
}

pub(crate) fn map_ugoira_meta(work_id: i64, raw: &Value) -> Result<PixivUgoiraMeta, PixivError> {
    let endpoint = PixivEndpoint::UgoiraMeta;
    require_positive_id(work_id, endpoint)?;
    let body: UgoiraBodyDto = ajax_body(raw, endpoint)?;
    let zip_url = Url::parse(&body.original_src).map_err(|_| PixivError::invalid_json(endpoint))?;
    let frames = body
        .frames
        .into_iter()
        .map(|frame| {
            Ok(PixivUgoiraFrame {
                file: frame.file,
                delay_ms: required_u32(&frame.delay, endpoint)?,
            })
        })
        .collect::<Result<Vec<_>, PixivError>>()?;
    Ok(PixivUgoiraMeta {
        work_id,
        zip_url,
        frame_mime_type: body.mime_type,
        frames,
    })
}

pub(crate) fn map_ranking_page(
    request: &PixivRankingRequest,
    raw: &Value,
) -> Result<PixivRankingPage, PixivError> {
    let endpoint = PixivEndpoint::Ranking;
    require_page(request.page, endpoint)?;
    let response: RankingDto = serde_json::from_value(raw.clone()).map_err(|error| {
        tracing::warn!(error = %error, "Pixiv ranking response mapping failed");
        PixivError::invalid_json(endpoint)
    })?;
    if response.contents.len() > 50 {
        return Err(PixivError::invalid_json(endpoint));
    }
    let ranking_date = response
        .date
        .as_deref()
        .map(|date| {
            Date::parse(date, format_description!("[year][month][day]"))
                .map_err(|_| PixivError::invalid_json(endpoint))
        })
        .transpose()?
        .or(request.date);
    let items = response
        .contents
        .into_iter()
        .map(|entry| map_ranking_entry(entry, request.mode))
        .collect::<Result<Vec<_>, PixivError>>()?;
    let next_page = response
        .next
        .page()
        .map_err(|()| PixivError::invalid_json(endpoint))?;
    let next_cursor = next_page.map(|page| PixivRankingCursor {
        mode: request.mode,
        content: request.content,
        date: ranking_date,
        page,
    });
    Ok(PixivRankingPage {
        date: ranking_date,
        items,
        next_cursor,
    })
}

pub(crate) fn map_follow_latest(
    request: &PixivFollowLatestRequest,
    raw: &Value,
) -> Result<PixivPage<PixivDiscoveryWork, PixivFollowLatestCursor>, PixivError> {
    let endpoint = match request.source {
        pixivarchive_domain::pixiv::PixivFollowLatestSource::Following => {
            PixivEndpoint::FollowLatest
        }
        pixivarchive_domain::pixiv::PixivFollowLatestSource::Mypixiv => {
            PixivEndpoint::MypixivLatest
        }
    };
    require_page(request.page, endpoint)?;
    let raw_context = FollowLatestMappingContext::from_raw(endpoint, request.page, raw);
    let body: FollowLatestBodyDto = match decode_ajax_body(raw, endpoint) {
        Ok(body) => body,
        Err(AjaxBodyError::Response(error)) => return Err(error),
        Err(AjaxBodyError::Structure(error)) => {
            return Err(raw_context.reject_structure(error));
        }
    };
    let context = FollowLatestMappingContext::from_body(endpoint, request.page, &body);
    let translations = body
        .tag_translation
        .into_iter()
        .filter_map(|(tag, translation)| translation.en.map(|value| (tag, value)))
        .collect::<BTreeMap<_, _>>();
    let mut works = BTreeMap::new();
    let mut thumbnail_order = Vec::with_capacity(body.thumbnails.illust.len());
    for work in body.thumbnails.illust {
        let work_id = positive_i64(&work.id, endpoint).map_err(|error| {
            context.reject(
                "thumbnail_id",
                None,
                error,
                "thumbnails.illust contains an invalid work id",
            )
        })?;
        let mapped = map_discovery_work(work, endpoint, &translations, None).map_err(|error| {
            context.reject(
                "thumbnail_work",
                Some(work_id),
                error,
                format!("thumbnails.illust work mapping rejected; work_id={work_id}"),
            )
        })?;
        if works.insert(work_id, mapped).is_some() {
            return Err(context.reject(
                "duplicate_thumbnail_id",
                Some(work_id),
                PixivError::invalid_json(endpoint),
                format!("thumbnails.illust contains a duplicate work id; work_id={work_id}"),
            ));
        }
        thumbnail_order.push(work_id);
    }
    let mut items = Vec::new();
    if let Some(page) = &body.page {
        for id in &page.ids {
            let id = positive_i64(id, endpoint).map_err(|error| {
                context.reject(
                    "page_id",
                    None,
                    error,
                    "page.ids contains an invalid work id",
                )
            })?;
            if let Some(work) = works.remove(&id) {
                items.push(work);
            }
        }
    } else {
        for id in thumbnail_order {
            let work = works.remove(&id).ok_or_else(|| {
                context.reject(
                    "thumbnail_order",
                    Some(id),
                    PixivError::invalid_json(endpoint),
                    format!("thumbnail order references an absent work; work_id={id}"),
                )
            })?;
            items.push(work);
        }
    }
    let next_cursor = if items.is_empty() {
        None
    } else {
        Some(PixivFollowLatestCursor {
            source: request.source,
            mode: request.mode,
            tag: request.tag.clone(),
            language: request.language.clone(),
            page: request.page.checked_add(1).ok_or_else(|| {
                context.reject(
                    "next_page",
                    None,
                    PixivError::invalid_json(endpoint),
                    "follow-latest page cursor overflowed",
                )
            })?,
        })
    };
    Ok(PixivPage { items, next_cursor })
}

#[derive(Clone, Copy)]
struct FollowLatestMappingContext {
    endpoint: PixivEndpoint,
    page: u32,
    page_id_count: Option<usize>,
    thumbnail_count: Option<usize>,
}

impl FollowLatestMappingContext {
    fn from_raw(endpoint: PixivEndpoint, page: u32, raw: &Value) -> Self {
        Self {
            endpoint,
            page,
            page_id_count: raw
                .pointer("/body/page/ids")
                .and_then(Value::as_array)
                .map(Vec::len),
            thumbnail_count: raw
                .pointer("/body/thumbnails/illust")
                .and_then(Value::as_array)
                .map(Vec::len),
        }
    }

    fn from_body(endpoint: PixivEndpoint, page: u32, body: &FollowLatestBodyDto) -> Self {
        Self {
            endpoint,
            page,
            page_id_count: body.page.as_ref().map(|page| page.ids.len()),
            thumbnail_count: Some(body.thumbnails.illust.len()),
        }
    }

    fn reject(
        self,
        stage: &'static str,
        work_id: Option<i64>,
        error: PixivError,
        message: impl AsRef<str>,
    ) -> PixivError {
        self.log_rejection(stage, work_id, &error);
        error.with_message(message.as_ref())
    }

    fn reject_structure(self, error: serde_json::Error) -> PixivError {
        self.log_rejection("response_structure", None, &error);
        PixivError::invalid_json(self.endpoint).with_message(&format!(
            "{} response structure rejected",
            self.endpoint.as_str()
        ))
    }

    fn log_rejection(self, stage: &'static str, work_id: Option<i64>, error: &dyn fmt::Display) {
        tracing::warn!(
            adapter_version = ADAPTER_VERSION,
            endpoint = self.endpoint.as_str(),
            page = self.page,
            page_id_count = ?self.page_id_count,
            thumbnail_count = ?self.thumbnail_count,
            stage,
            work_id = ?work_id,
            error = %error,
            "Pixiv follow-latest response mapping failed"
        );
    }
}

pub(crate) fn map_bookmarks(
    request: &PixivBookmarksRequest,
    raw: &Value,
) -> Result<PixivPage<i64, PixivBookmarksCursor>, PixivError> {
    let endpoint = PixivEndpoint::Bookmarks;
    require_positive_id(request.user_id, endpoint)?;
    let body: BookmarksBodyDto = ajax_body(raw, endpoint)?;
    let total = required_u32(&body.total, endpoint)?;
    let items = body
        .works
        .into_iter()
        .enumerate()
        .map(|(index, work)| {
            positive_i64(&work.id, endpoint).map_err(|error| {
                error.with_message(&format!(
                    "bookmark item {index} id must be a positive integer"
                ))
            })
        })
        .collect::<Result<Vec<_>, PixivError>>()?;
    let returned = u32::try_from(items.len()).map_err(|_| PixivError::invalid_json(endpoint))?;
    let next_offset = request
        .offset
        .checked_add(returned)
        .ok_or_else(|| PixivError::invalid_json(endpoint))?;
    let next_cursor = (returned > 0 && next_offset < total).then(|| PixivBookmarksCursor {
        user_id: request.user_id,
        visibility: request.visibility,
        mode: request.mode,
        tag: request.tag.clone(),
        offset: next_offset,
    });
    Ok(PixivPage { items, next_cursor })
}

pub(crate) fn map_followed_artists(
    request: &PixivFollowingRequest,
    raw: &Value,
) -> Result<PixivPage<PixivFollowedArtist, PixivFollowingCursor>, PixivError> {
    let endpoint = PixivEndpoint::Following;
    validate_following_request(request, endpoint)?;
    let body: FollowingBodyDto = ajax_body(raw, endpoint)?;
    let total = required_u32(&body.total, endpoint)?;
    let items = body
        .users
        .into_iter()
        .map(map_followed_user)
        .collect::<Result<Vec<_>, PixivError>>()?;
    let returned = u32::try_from(items.len()).map_err(|_| PixivError::invalid_json(endpoint))?;
    let next_offset = request
        .offset
        .checked_add(returned)
        .ok_or_else(|| PixivError::invalid_json(endpoint))?;
    let next_cursor = (returned > 0 && next_offset < total).then(|| PixivFollowingCursor {
        user_id: request.user_id,
        visibility: request.visibility,
        offset: next_offset,
        limit: request.limit,
        language: request.language.clone(),
    });
    Ok(PixivPage { items, next_cursor })
}

pub(crate) fn map_artist_work_ids(
    artist_id: i64,
    raw: &Value,
) -> Result<PixivArtistWorkIds, PixivError> {
    let endpoint = PixivEndpoint::ProfileAll;
    require_positive_id(artist_id, endpoint)?;
    let body: ProfileAllBodyDto = ajax_body(raw, endpoint)?;
    let mut work_ids = BTreeSet::new();
    collect_positive_object_keys(&body.illusts, &mut work_ids);
    collect_positive_object_keys(&body.manga, &mut work_ids);
    Ok(PixivArtistWorkIds {
        artist_id,
        work_ids: work_ids.into_iter().collect(),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MappedAccountProfile {
    pub display_name: String,
    pub avatar_url: Option<String>,
}

pub(crate) fn map_account_profile(
    user_id: i64,
    raw: &Value,
) -> Result<MappedAccountProfile, PixivError> {
    let endpoint = PixivEndpoint::Profile;
    require_positive_id(user_id, endpoint)?;
    let body: AccountProfileBodyDto = ajax_body(raw, endpoint)?;
    if positive_i64(&body.user_id, endpoint)? != user_id || body.name.trim().is_empty() {
        return Err(PixivError::invalid_json(endpoint));
    }
    Ok(MappedAccountProfile {
        display_name: body.name.trim().to_owned(),
        avatar_url: body
            .image_big
            .or(body.image)
            .and_then(|value| valid_avatar_url(&value)),
    })
}

fn valid_avatar_url(value: &str) -> Option<String> {
    let parsed = Url::parse(value.trim()).ok()?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return None;
    }
    Some(parsed.to_string())
}

pub(crate) fn map_artist_follow_state(
    artist_id: i64,
    raw: &Value,
) -> Result<PixivArtistFollowState, PixivError> {
    let endpoint = PixivEndpoint::ArtistFollowState;
    require_positive_id(artist_id, endpoint)?;
    let body: ArtistFollowProfileBodyDto = ajax_body(raw, endpoint)?;
    if positive_i64(&body.user_id, endpoint)? != artist_id || body.name.trim().is_empty() {
        return Err(PixivError::invalid_json(endpoint));
    }
    Ok(PixivArtistFollowState {
        artist_id,
        name: body.name.trim().to_owned(),
        profile_image_url: body.image.filter(|value| !value.trim().is_empty()),
        followed: body.is_followed,
    })
}

pub(crate) fn map_add_artist_follow_result(
    artist_id: i64,
    raw: &Value,
) -> Result<PixivArtistFollowWriteResult, PixivError> {
    let endpoint = PixivEndpoint::AddArtistFollow;
    require_positive_id(artist_id, endpoint)?;
    if raw.as_array().is_some_and(Vec::is_empty) {
        return Ok(PixivArtistFollowWriteResult { artist_id });
    }
    Err(write_response_error(raw, endpoint))
}

pub(crate) fn map_remove_artist_follow_result(
    artist_id: i64,
    raw: &Value,
) -> Result<PixivArtistFollowWriteResult, PixivError> {
    let endpoint = PixivEndpoint::RemoveArtistFollow;
    require_positive_id(artist_id, endpoint)?;
    let body: RemoveArtistFollowBodyDto =
        serde_json::from_value(raw.clone()).map_err(|_| write_response_error(raw, endpoint))?;
    if positive_i64(&body.user_id, endpoint)? != artist_id {
        return Err(PixivError::invalid_json(endpoint));
    }
    Ok(PixivArtistFollowWriteResult { artist_id })
}

pub(crate) fn map_bookmark_write_result(
    raw: &Value,
    endpoint: PixivEndpoint,
) -> Result<PixivBookmarkWriteResult, PixivError> {
    let body: BookmarkWriteBodyDto = ajax_body(raw, endpoint)?;
    let bookmark_id = body
        .last_bookmark_id
        .as_ref()
        .map(|value| positive_i64(value, endpoint))
        .transpose()?;
    Ok(PixivBookmarkWriteResult { bookmark_id })
}

fn write_response_error(raw: &Value, endpoint: PixivEndpoint) -> PixivError {
    raw.get("message")
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .map(|message| classify_semantic_error(endpoint, message))
        .unwrap_or_else(|| PixivError::invalid_json(endpoint))
}

pub(crate) fn has_private_bookmark_evidence(raw: &Value) -> bool {
    raw.get("body")
        .and_then(Value::as_object)
        .and_then(|body| body.get("bookmarkTags"))
        .is_some_and(|tags| tags.is_object() || tags.is_array())
}

fn map_ranking_entry(
    entry: RankingEntryDto,
    mode: PixivRankingMode,
) -> Result<PixivRankingEntry, PixivError> {
    let endpoint = PixivEndpoint::Ranking;
    let age_rating = match mode {
        PixivRankingMode::R18 => PixivAgeRating::R18,
        PixivRankingMode::R18g => PixivAgeRating::R18g,
        _ => PixivAgeRating::AllAge,
    };
    let ai_classification = if mode == PixivRankingMode::AiGenerated {
        PixivAiClassification::AiGenerated
    } else {
        PixivAiClassification::Unknown
    };
    let dimensions = match (&entry.width, &entry.height) {
        (Some(width), Some(height)) => Some(PixivDimensions {
            width: positive_u32(width, endpoint)?,
            height: positive_u32(height, endpoint)?,
        }),
        _ => None,
    };
    Ok(PixivRankingEntry {
        work: PixivDiscoveryWork {
            work_id: positive_i64(&entry.illust_id, endpoint)?,
            title: entry.title,
            kind: work_kind(&entry.illust_type, endpoint)?,
            age_rating,
            ai_classification,
            is_original: entry.attr == "original",
            artist: PixivArtistRef {
                pixiv_id: positive_i64(&entry.user_id, endpoint)?,
                name: entry.user_name,
                account_name: None,
            },
            tags: entry
                .tags
                .into_iter()
                .map(|name| PixivTag {
                    name,
                    translated_name: None,
                })
                .collect(),
            page_count: positive_u32(&entry.illust_page_count, endpoint)?,
            dimensions,
            view_count: entry.view_count.as_ref().and_then(NumberValue::as_u64),
            bookmarked_by_current_account: Some(entry.is_bookmarked),
            bookmark: None,
        },
        rank: positive_u32(&entry.rank, endpoint)?,
        previous_rank: entry
            .yes_rank
            .as_ref()
            .map(|value| value.page())
            .transpose()
            .map_err(|()| PixivError::invalid_json(endpoint))?
            .flatten(),
    })
}

fn map_discovery_work(
    work: DiscoveryWorkDto,
    endpoint: PixivEndpoint,
    translations: &BTreeMap<String, String>,
    bookmarked_override: Option<bool>,
) -> Result<PixivDiscoveryWork, PixivError> {
    let bookmark = work
        .bookmark_data
        .as_ref()
        .map(|bookmark| {
            Ok(PixivBookmarkRef {
                bookmark_id: positive_i64(&bookmark.id, endpoint)?,
                visibility: if bookmark.private {
                    PixivBookmarkVisibility::Private
                } else {
                    PixivBookmarkVisibility::Public
                },
            })
        })
        .transpose()?;
    let dimensions = match (&work.width, &work.height) {
        (Some(width), Some(height)) => Some(PixivDimensions {
            width: positive_u32(width, endpoint)?,
            height: positive_u32(height, endpoint)?,
        }),
        _ => None,
    };
    Ok(PixivDiscoveryWork {
        work_id: positive_i64(&work.id, endpoint)?,
        title: work.title,
        kind: work_kind(&work.illust_type, endpoint)?,
        age_rating: age_rating(work.x_restrict.as_ref()),
        ai_classification: ai_classification(work.ai_type.as_ref()),
        is_original: work.is_original,
        artist: PixivArtistRef {
            pixiv_id: positive_i64(&work.user_id, endpoint)?,
            name: work.user_name,
            account_name: None,
        },
        tags: work
            .tags
            .into_iter()
            .map(|name| PixivTag {
                translated_name: translations.get(&name).cloned(),
                name,
            })
            .collect(),
        page_count: positive_u32(&work.page_count, endpoint)?,
        dimensions,
        view_count: work.view_count.as_ref().and_then(NumberValue::as_u64),
        bookmarked_by_current_account: bookmarked_override
            .or_else(|| bookmark.as_ref().map(|_| true)),
        bookmark,
    })
}

fn map_followed_user(user: FollowedUserDto) -> Result<PixivFollowedArtist, PixivError> {
    let endpoint = PixivEndpoint::Following;
    Ok(PixivFollowedArtist {
        pixiv_id: positive_i64(&user.user_id, endpoint)?,
        name: user.user_name,
        profile_image_url: user.profile_image_url.filter(|value| !value.is_empty()),
    })
}

enum AjaxBodyError {
    Response(PixivError),
    Structure(serde_json::Error),
}

fn decode_ajax_body<T: DeserializeOwned>(
    raw: &Value,
    endpoint: PixivEndpoint,
) -> Result<T, AjaxBodyError> {
    if raw.get("error").and_then(Value::as_bool) == Some(true) {
        let message = raw
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Pixiv returned an error");
        return Err(AjaxBodyError::Response(classify_semantic_error(
            endpoint, message,
        )));
    }
    let envelope: AjaxEnvelope<T> =
        serde_json::from_value(raw.clone()).map_err(AjaxBodyError::Structure)?;
    if envelope.error {
        return Err(AjaxBodyError::Response(classify_semantic_error(
            endpoint,
            &envelope.message,
        )));
    }
    Ok(envelope.body)
}

fn ajax_body<T: DeserializeOwned>(raw: &Value, endpoint: PixivEndpoint) -> Result<T, PixivError> {
    match decode_ajax_body(raw, endpoint) {
        Ok(body) => Ok(body),
        Err(AjaxBodyError::Response(error)) => Err(error),
        Err(AjaxBodyError::Structure(error)) => {
            tracing::warn!(
                adapter_version = ADAPTER_VERSION,
                endpoint = endpoint.as_str(),
                error = %error,
                "Pixiv AJAX response mapping failed"
            );
            Err(PixivError::invalid_json(endpoint))
        }
    }
}

fn work_kind(value: &NumberValue, endpoint: PixivEndpoint) -> Result<PixivWorkKind, PixivError> {
    match value.as_i64() {
        Some(0) => Ok(PixivWorkKind::Illustration),
        Some(1) => Ok(PixivWorkKind::Manga),
        Some(2) => Ok(PixivWorkKind::Ugoira),
        _ => Err(PixivError::invalid_json(endpoint)),
    }
}

fn age_rating(value: Option<&NumberValue>) -> PixivAgeRating {
    match value.and_then(NumberValue::as_i64) {
        Some(0) => PixivAgeRating::AllAge,
        Some(1) => PixivAgeRating::R18,
        Some(2) => PixivAgeRating::R18g,
        _ => PixivAgeRating::Unknown,
    }
}

fn ai_classification(value: Option<&NumberValue>) -> PixivAiClassification {
    match value.and_then(NumberValue::as_i64) {
        Some(1) => PixivAiClassification::NotAiGenerated,
        Some(2) => PixivAiClassification::AiGenerated,
        _ => PixivAiClassification::Unknown,
    }
}

fn required_u32(value: &NumberValue, endpoint: PixivEndpoint) -> Result<u32, PixivError> {
    value
        .as_u32()
        .ok_or_else(|| PixivError::invalid_json(endpoint))
}

fn positive_u32(value: &NumberValue, endpoint: PixivEndpoint) -> Result<u32, PixivError> {
    value
        .as_u32()
        .filter(|value| *value > 0)
        .ok_or_else(|| PixivError::invalid_json(endpoint))
}

fn positive_i64(value: &NumberValue, endpoint: PixivEndpoint) -> Result<i64, PixivError> {
    value
        .as_i64()
        .filter(|value| *value > 0)
        .ok_or_else(|| PixivError::invalid_json(endpoint))
}

fn optional_u64(value: Option<&NumberValue>) -> u64 {
    value.and_then(NumberValue::as_u64).unwrap_or(0)
}

fn optional_timestamp(
    value: Option<&str>,
    endpoint: PixivEndpoint,
) -> Result<Option<OffsetDateTime>, PixivError> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| {
            OffsetDateTime::parse(value, &Rfc3339).map_err(|_| PixivError::invalid_json(endpoint))
        })
        .transpose()
}

fn image_format_hint(url: &Url) -> Option<PixivImageFormat> {
    match url
        .path_segments()
        .and_then(Iterator::last)
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg" | "jpeg") => Some(PixivImageFormat::Jpeg),
        Some("png") => Some(PixivImageFormat::Png),
        Some("gif") => Some(PixivImageFormat::Gif),
        _ => None,
    }
}

fn collect_positive_object_keys(value: &Value, output: &mut BTreeSet<i64>) {
    if let Some(object) = value.as_object() {
        for key in object.keys() {
            if let Ok(id) = key.parse::<i64>()
                && id > 0
            {
                output.insert(id);
            }
        }
    }
}

fn require_positive_id(value: i64, endpoint: PixivEndpoint) -> Result<(), PixivError> {
    if value > 0 {
        Ok(())
    } else {
        Err(PixivError::hidden_or_invalid(
            endpoint,
            "identifier must be positive",
        ))
    }
}

fn require_page(value: u32, endpoint: PixivEndpoint) -> Result<(), PixivError> {
    if value > 0 {
        Ok(())
    } else {
        Err(PixivError::hidden_or_invalid(
            endpoint,
            "page must start at one",
        ))
    }
}

pub(crate) fn validate_following_request(
    request: &PixivFollowingRequest,
    endpoint: PixivEndpoint,
) -> Result<(), PixivError> {
    require_positive_id(request.user_id, endpoint)?;
    if request.limit == 0 || request.limit > 100 {
        return Err(PixivError::hidden_or_invalid(
            endpoint,
            "following page limit must be between 1 and 100",
        ));
    }
    request
        .offset
        .checked_add(request.limit)
        .ok_or_else(|| PixivError::hidden_or_invalid(endpoint, "following offset overflow"))?;
    Ok(())
}
