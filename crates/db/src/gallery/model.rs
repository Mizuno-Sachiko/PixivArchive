use crate::DbError;
use pixivarchive_domain::{
    job::CollectionState,
    media::MediaKind,
    pixiv::{PixivAgeRating, PixivWorkKind},
    work::{
        GalleryContextCursor, GalleryContextPage, GalleryCursor, GalleryCursorKey,
        GallerySortField, GalleryWork, SortDirection, WorkSourceState,
    },
};
use sqlx::Row;

use super::MAX_PAGE_SIZE;

pub(super) fn work_from_row(row: sqlx::postgres::PgRow) -> Result<GalleryWork, DbError> {
    Ok(GalleryWork {
        id: row.try_get("id")?,
        pixiv_work_id: row.try_get("pixiv_work_id")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        artist_id: row.try_get("artist_id")?,
        pixiv_artist_id: row.try_get("pixiv_artist_id")?,
        artist_name: row.try_get("artist_name")?,
        series_id: row.try_get("series_id")?,
        series_title: row.try_get("series_title")?,
        work_kind: parse_enum_value(
            row.try_get("work_kind")?,
            "work kind",
            PixivWorkKind::from_db_value,
        )?,
        age_rating: parse_enum_value(
            row.try_get("age_rating")?,
            "age rating",
            PixivAgeRating::from_db_value,
        )?,
        ai_generated: row.try_get("ai_generated")?,
        page_count: u32::try_from(row.try_get::<i32, _>("page_count")?)
            .map_err(|_| DbError::InvalidValue("negative work page count".to_owned()))?,
        collection_state: parse_enum_value(
            row.try_get("collection_state")?,
            "collection state",
            CollectionState::from_db_value,
        )?,
        source_state: parse_enum_value(
            row.try_get("source_state")?,
            "source state",
            WorkSourceState::from_db_value,
        )?,
        bookmarked_by_current_account: row.try_get("bookmarked_by_current_account")?,
        bookmark_id: row.try_get("bookmark_id")?,
        bookmark_count: row.try_get("bookmark_count")?,
        view_count: row.try_get("view_count")?,
        like_count: row.try_get("like_count")?,
        comment_count: row.try_get("comment_count")?,
        pixiv_published_at: row.try_get("pixiv_created_at")?,
        pixiv_updated_at: row.try_get("pixiv_updated_at")?,
        local_updated_at: row.try_get("local_updated_at")?,
        cover_path: row.try_get("cover_path")?,
        cover_derivative_id: row.try_get("cover_derivative_id")?,
        cover_width: optional_positive_u32(row.try_get("cover_width")?, "cover width")?,
        cover_height: optional_positive_u32(row.try_get("cover_height")?, "cover height")?,
        media_kind: parse_optional_enum_value(
            row.try_get("media_kind")?,
            "media kind",
            MediaKind::from_db_value,
        )?,
        tags: Vec::new(),
    })
}

pub(super) fn validate_context_limit(limit: u16) -> Result<(), DbError> {
    if limit == 0 || limit > MAX_PAGE_SIZE {
        return Err(DbError::InvalidValue(format!(
            "gallery context limit must be between 1 and {MAX_PAGE_SIZE}"
        )));
    }
    Ok(())
}

pub(super) fn context_page<T>(
    items: Vec<T>,
    total: i64,
    next_cursor: Option<GalleryContextCursor>,
) -> Result<GalleryContextPage<T>, DbError> {
    let total = non_negative_u64(total, "gallery context total")?;
    Ok(GalleryContextPage {
        items,
        total,
        next_cursor,
    })
}

pub(super) fn context_limit_with_lookahead(limit: u16) -> i64 {
    i64::from(limit) + 1
}

pub(super) fn context_cursor_count(
    cursor: Option<&GalleryContextCursor>,
) -> Result<Option<i64>, DbError> {
    cursor
        .map(|cursor| {
            i64::try_from(cursor.work_count).map_err(|_| {
                DbError::InvalidValue("gallery context cursor count is too large".to_owned())
            })
        })
        .transpose()
}

pub(super) fn cursor_for(
    work: &GalleryWork,
    sort_field: GallerySortField,
    sort_direction: SortDirection,
) -> GalleryCursor {
    let key = match sort_field {
        GallerySortField::PixivId => GalleryCursorKey::Integer(work.pixiv_work_id),
        GallerySortField::LocalUpdatedAt => GalleryCursorKey::Date(work.local_updated_at),
        GallerySortField::PublishedAt => work
            .pixiv_published_at
            .map(GalleryCursorKey::Date)
            .unwrap_or(GalleryCursorKey::Null),
        GallerySortField::BookmarkCount => work
            .bookmark_count
            .map(GalleryCursorKey::Integer)
            .unwrap_or(GalleryCursorKey::Null),
        GallerySortField::Title => GalleryCursorKey::Text(work.title.to_lowercase()),
    };
    GalleryCursor {
        sort_field,
        sort_direction,
        key,
        work_id: work.id,
    }
}

pub(super) fn required_optional_string(
    row: &sqlx::postgres::PgRow,
    column: &str,
) -> Result<String, DbError> {
    row.try_get::<Option<String>, _>(column)?
        .ok_or_else(|| DbError::InvalidValue(format!("current media {column} is missing")))
}

pub(super) fn parse_enum_value<T>(
    value: String,
    name: &str,
    parse: impl FnOnce(&str) -> Option<T>,
) -> Result<T, DbError> {
    parse(&value).ok_or_else(|| DbError::InvalidValue(format!("invalid {name} {value}")))
}

pub(super) fn parse_optional_enum_value<T>(
    value: Option<String>,
    name: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Option<T>, DbError> {
    value
        .map(|value| parse_enum_value(value, name, &parse))
        .transpose()
}

pub(super) fn positive_u64(value: i64, name: &str) -> Result<u64, DbError> {
    let value = u64::try_from(value)
        .map_err(|_| DbError::InvalidValue(format!("{name} cannot be negative")))?;
    if value == 0 {
        return Err(DbError::InvalidValue(format!("{name} must be positive")));
    }
    Ok(value)
}

pub(super) fn non_negative_u64(value: i64, name: &str) -> Result<u64, DbError> {
    u64::try_from(value).map_err(|_| DbError::InvalidValue(format!("{name} cannot be negative")))
}

pub(super) fn non_negative_u32(value: i32, name: &str) -> Result<u32, DbError> {
    u32::try_from(value).map_err(|_| DbError::InvalidValue(format!("{name} cannot be negative")))
}

pub(super) fn optional_positive_u32(
    value: Option<i32>,
    name: &str,
) -> Result<Option<u32>, DbError> {
    value
        .map(|value| {
            let value = non_negative_u32(value, name)?;
            if value == 0 {
                return Err(DbError::InvalidValue(format!("{name} must be positive")));
            }
            Ok(value)
        })
        .transpose()
}
