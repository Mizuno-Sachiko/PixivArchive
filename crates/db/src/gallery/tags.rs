use super::{
    GalleryRepository,
    model::{
        context_cursor_count, context_limit_with_lookahead, context_page, non_negative_u64,
        optional_positive_u32, parse_enum_value, parse_optional_enum_value, validate_context_limit,
    },
};
use crate::DbError;
use pixivarchive_domain::{
    pixiv::PixivAgeRating,
    work::{
        GalleryContextCursor, GalleryContextIdentity, GalleryContextPage, GalleryTag,
        GalleryTagDetail,
    },
};
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;

struct TagCover {
    derivative_id: Uuid,
    width: i32,
    height: i32,
    age_rating: PixivAgeRating,
}

impl GalleryRepository {
    pub async fn tags(
        &self,
        limit: u16,
        cursor: Option<&GalleryContextCursor>,
        query: Option<&str>,
    ) -> Result<GalleryContextPage<GalleryTagDetail>, DbError> {
        validate_context_limit(limit)?;
        let query = query.map(str::trim).filter(|value| !value.is_empty());
        let (cursor_count, cursor_name, cursor_id) = match cursor {
            None => (None, None, None),
            Some(cursor) => {
                let GalleryContextIdentity::Tag(tag_id) = &cursor.identity else {
                    return Err(DbError::InvalidValue(
                        "gallery tag cursor has the wrong identity type".to_owned(),
                    ));
                };
                (
                    context_cursor_count(Some(cursor))?,
                    Some(cursor.normalized_name.as_str()),
                    Some(*tag_id),
                )
            }
        };
        let rows = sqlx::query(
            r#"
            WITH matching_tags AS (
                SELECT
                    tag.id,
                    tag.raw_name,
                    tag.translated_name,
                    lower(btrim(tag.raw_name)) AS normalized_name,
                    count(work.id) AS work_count,
                    count(*) OVER () AS total_count
                FROM tag
                JOIN work_tag ON work_tag.tag_id = tag.id
                JOIN work
                  ON work.id = work_tag.work_id
                 AND work.collection_state = 'collected'
                WHERE EXISTS (
                    SELECT 1
                    FROM work_page AS local_page
                    WHERE local_page.work_id = work.id
                      AND local_page.source_state = 'present'
                      AND local_page.current_media_revision_id IS NOT NULL
                 )
                  AND (
                    $2::text IS NULL
                    OR tag.raw_name ILIKE '%' || $2 || '%'
                    OR coalesce(tag.translated_name, '') ILIKE '%' || $2 || '%'
                )
                GROUP BY tag.id
            ),
            paged_tags AS (
                SELECT *
                FROM matching_tags
                WHERE $3::bigint IS NULL OR (
                    matching_tags.work_count < $3
                    OR (
                        matching_tags.work_count = $3
                        AND (
                            matching_tags.normalized_name > $4
                            OR (
                                matching_tags.normalized_name = $4
                                AND matching_tags.id > $5
                            )
                        )
                    )
                )
                ORDER BY matching_tags.work_count DESC, matching_tags.normalized_name, matching_tags.id
                LIMIT $1
            )
            SELECT
                paged_tags.id,
                paged_tags.raw_name,
                paged_tags.translated_name,
                paged_tags.normalized_name,
                paged_tags.work_count,
                paged_tags.total_count
            FROM paged_tags
            ORDER BY paged_tags.work_count DESC, paged_tags.normalized_name, paged_tags.id
            "#,
        )
        .bind(context_limit_with_lookahead(limit))
        .bind(query)
        .bind(cursor_count)
        .bind(cursor_name)
        .bind(cursor_id)
        .fetch_all(self.db.pool())
        .await?;
        let total = match rows.first() {
            Some(row) => row.get("total_count"),
            None => self.tag_count(query).await?,
        };
        let tag_ids = rows.iter().map(|row| row.get("id")).collect::<Vec<Uuid>>();
        let covers = self.tag_covers(&tag_ids).await?;
        let mut rows = rows;
        let has_more = rows.len() > usize::from(limit);
        if has_more {
            rows.truncate(usize::from(limit));
        }
        let next_cursor = if has_more {
            let row = rows.last().expect("lookahead leaves one tag");
            Some(GalleryContextCursor {
                work_count: non_negative_u64(row.get("work_count"), "tag work count")?,
                normalized_name: row.get("normalized_name"),
                identity: GalleryContextIdentity::Tag(row.get("id")),
            })
        } else {
            None
        };
        let items = rows
            .into_iter()
            .map(|row| {
                let id = row.get("id");
                let cover = covers.get(&id);
                Ok(GalleryTagDetail {
                    tag: GalleryTag {
                        id,
                        original: row.get("raw_name"),
                        translation: row.get("translated_name"),
                    },
                    work_count: non_negative_u64(row.get("work_count"), "tag work count")?,
                    cover_derivative_id: cover.map(|cover| cover.derivative_id),
                    cover_width: optional_positive_u32(
                        cover.map(|cover| cover.width),
                        "cover width",
                    )?,
                    cover_height: optional_positive_u32(
                        cover.map(|cover| cover.height),
                        "cover height",
                    )?,
                    cover_age_rating: cover.map(|cover| cover.age_rating),
                })
            })
            .collect::<Result<Vec<_>, DbError>>()?;
        context_page(items, total, next_cursor)
    }

    async fn tag_covers(&self, tag_ids: &[Uuid]) -> Result<HashMap<Uuid, TagCover>, DbError> {
        if tag_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query(
            r#"
            SELECT
                requested.tag_id,
                cover.id AS cover_derivative_id,
                cover.width AS cover_width,
                cover.height AS cover_height,
                cover.age_rating AS cover_age_rating
            FROM unnest($1::uuid[]) AS requested(tag_id)
            JOIN LATERAL (
                SELECT
                    derivative.id,
                    derivative.width,
                    derivative.height,
                    cover_revision.sanity_level AS age_rating
                FROM LATERAL (
                    SELECT
                        cover_page.current_media_revision_id,
                        cover_work.current_revision_id
                    FROM (
                        -- Keep the partial index order across the correlated tag test so the
                        -- scan stops as soon as the most recent usable work is found.
                        SELECT id, current_revision_id, last_collected_at, updated_at
                        FROM work
                        WHERE collection_state = 'collected'
                        ORDER BY last_collected_at DESC NULLS LAST, updated_at DESC, id DESC
                        OFFSET 0
                    ) AS cover_work
                    JOIN work_page AS cover_page
                      ON cover_page.work_id = cover_work.id
                     AND cover_page.page_index = 0
                     AND cover_page.source_state = 'present'
                    WHERE EXISTS (
                        SELECT 1
                        FROM work_tag AS cover_work_tag
                        WHERE cover_work_tag.work_id = cover_work.id
                          AND cover_work_tag.tag_id = requested.tag_id
                    )
                      AND EXISTS (
                        SELECT 1
                        FROM derivative AS available_derivative
                        WHERE available_derivative.media_revision_id = cover_page.current_media_revision_id
                          AND available_derivative.derivative_kind IN ('waterfall_thumbnail', 'ugoira_cover')
                    )
                    ORDER BY
                        cover_work.last_collected_at DESC NULLS LAST,
                        cover_work.updated_at DESC,
                        cover_work.id DESC
                    LIMIT 1
                ) AS cover_media
                JOIN work_revision AS cover_revision
                  ON cover_revision.id = cover_media.current_revision_id
                JOIN derivative
                  ON derivative.media_revision_id = cover_media.current_media_revision_id
                 AND derivative.derivative_kind IN ('waterfall_thumbnail', 'ugoira_cover')
                ORDER BY
                    CASE derivative.derivative_kind
                        WHEN 'waterfall_thumbnail' THEN 0
                        ELSE 1
                    END,
                    CASE derivative.format WHEN 'avif' THEN 0 ELSE 1 END
                LIMIT 1
            ) AS cover ON true
            "#,
        )
        .bind(tag_ids)
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok((
                    row.get("tag_id"),
                    TagCover {
                        derivative_id: row.get("cover_derivative_id"),
                        width: row.get("cover_width"),
                        height: row.get("cover_height"),
                        age_rating: parse_enum_value(
                            row.get("cover_age_rating"),
                            "cover age rating",
                            PixivAgeRating::from_db_value,
                        )?,
                    },
                ))
            })
            .collect::<Result<HashMap<_, _>, DbError>>()
    }

    async fn tag_count(&self, query: Option<&str>) -> Result<i64, DbError> {
        sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM (
                SELECT tag.id
                FROM tag
                JOIN work_tag ON work_tag.tag_id = tag.id
                JOIN work
                  ON work.id = work_tag.work_id
                 AND work.collection_state = 'collected'
                WHERE EXISTS (
                    SELECT 1
                    FROM work_page
                    WHERE work_page.work_id = work.id
                      AND work_page.source_state = 'present'
                      AND work_page.current_media_revision_id IS NOT NULL
                )
                  AND (
                    $1::text IS NULL
                    OR tag.raw_name ILIKE '%' || $1 || '%'
                    OR coalesce(tag.translated_name, '') ILIKE '%' || $1 || '%'
                  )
                GROUP BY tag.id
            ) AS matching_tags
            "#,
        )
        .bind(query)
        .fetch_one(self.db.pool())
        .await
        .map_err(Into::into)
    }

    pub async fn tag_detail(&self, tag_name: &str) -> Result<GalleryTagDetail, DbError> {
        let tag_name = tag_name.trim();
        if tag_name.is_empty() {
            return Err(DbError::InvalidValue(
                "Pixiv tag name cannot be empty".to_owned(),
            ));
        }
        let row = sqlx::query(
            r#"
            SELECT
                tag.id,
                tag.raw_name,
                tag.translated_name,
                count(work.id) AS work_count,
                cover.id AS cover_derivative_id,
                cover.width AS cover_width,
                cover.height AS cover_height,
                cover.age_rating AS cover_age_rating
            FROM tag
            LEFT JOIN work_tag ON work_tag.tag_id = tag.id
            LEFT JOIN work
              ON work.id = work_tag.work_id
             AND work.collection_state = 'collected'
             AND EXISTS (
                SELECT 1
                FROM work_page AS local_page
                WHERE local_page.work_id = work.id
                  AND local_page.source_state = 'present'
                  AND local_page.current_media_revision_id IS NOT NULL
             )
            LEFT JOIN LATERAL (
                SELECT
                    derivative.id,
                    derivative.width,
                    derivative.height,
                    cover_revision.sanity_level AS age_rating
                FROM work_tag AS cover_work_tag
                JOIN work AS cover_work
                  ON cover_work.id = cover_work_tag.work_id
                 AND cover_work.collection_state = 'collected'
                JOIN work_revision AS cover_revision
                  ON cover_revision.id = cover_work.current_revision_id
                JOIN work_page AS cover_page
                  ON cover_page.work_id = cover_work.id
                 AND cover_page.page_index = 0
                 AND cover_page.source_state = 'present'
                JOIN media_revision AS cover_media
                  ON cover_media.id = cover_page.current_media_revision_id
                JOIN derivative
                  ON derivative.media_revision_id = cover_media.id
                 AND derivative.derivative_kind IN ('waterfall_thumbnail', 'ugoira_cover')
                WHERE cover_work_tag.tag_id = tag.id
                ORDER BY
                    cover_work.last_collected_at DESC NULLS LAST,
                    cover_work.updated_at DESC,
                    cover_work.id DESC,
                    CASE derivative.derivative_kind
                        WHEN 'waterfall_thumbnail' THEN 0
                        ELSE 1
                    END,
                    CASE derivative.format WHEN 'avif' THEN 0 ELSE 1 END
                LIMIT 1
            ) AS cover ON true
            WHERE lower(btrim(tag.raw_name)) = lower(btrim($1))
            GROUP BY tag.id, cover.id, cover.width, cover.height, cover.age_rating
            "#,
        )
        .bind(tag_name)
        .fetch_one(self.db.pool())
        .await?;
        Ok(GalleryTagDetail {
            tag: GalleryTag {
                id: row.get("id"),
                original: row.get("raw_name"),
                translation: row.get("translated_name"),
            },
            work_count: non_negative_u64(row.get("work_count"), "tag work count")?,
            cover_derivative_id: row.get("cover_derivative_id"),
            cover_width: optional_positive_u32(row.get("cover_width"), "cover width")?,
            cover_height: optional_positive_u32(row.get("cover_height"), "cover height")?,
            cover_age_rating: parse_optional_enum_value(
                row.get("cover_age_rating"),
                "cover age rating",
                PixivAgeRating::from_db_value,
            )?,
        })
    }
}
