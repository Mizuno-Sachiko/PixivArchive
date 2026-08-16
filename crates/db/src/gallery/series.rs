use super::{
    GalleryRepository,
    model::{
        context_cursor_count, context_limit_with_lookahead, context_page, non_negative_u64,
        optional_positive_u32, parse_optional_enum_value, validate_context_limit,
    },
    validate_source_id,
};
use crate::DbError;
use pixivarchive_domain::{
    pixiv::PixivAgeRating,
    work::{GalleryContextCursor, GalleryContextIdentity, GalleryContextPage, GallerySeriesDetail},
};
use sqlx::Row;

impl GalleryRepository {
    pub async fn series(
        &self,
        limit: u16,
        cursor: Option<&GalleryContextCursor>,
        query: Option<&str>,
    ) -> Result<GalleryContextPage<GallerySeriesDetail>, DbError> {
        validate_context_limit(limit)?;
        let query = query.map(str::trim).filter(|value| !value.is_empty());
        let (cursor_count, cursor_name, cursor_id) = match cursor {
            None => (None, None, None),
            Some(cursor) => {
                let GalleryContextIdentity::Series(source_id) = &cursor.identity else {
                    return Err(DbError::InvalidValue(
                        "gallery series cursor has the wrong identity type".to_owned(),
                    ));
                };
                validate_source_id(*source_id, "gallery series cursor source ID")?;
                (
                    context_cursor_count(Some(cursor))?,
                    Some(cursor.normalized_name.as_str()),
                    Some(*source_id),
                )
            }
        };
        let rows = sqlx::query(
            r#"
            WITH matching_series AS (
                SELECT
                    series.id,
                    series.pixiv_series_id,
                    series.title,
                    lower(series.title) AS normalized_name,
                    min(artist.pixiv_artist_id) AS pixiv_artist_id,
                    count(work.id) AS work_count,
                    count(*) OVER () AS total_count
                FROM series
                JOIN work
                  ON work.series_id = series.id
                 AND work.collection_state = 'collected'
                JOIN artist ON artist.id = work.artist_id
                WHERE EXISTS (
                    SELECT 1
                    FROM work_page AS local_page
                    WHERE local_page.work_id = work.id
                      AND local_page.source_state = 'present'
                      AND local_page.current_media_revision_id IS NOT NULL
                )
                  AND (
                    $2::text IS NULL
                    OR series.title ILIKE '%' || $2 || '%'
                    OR series.pixiv_series_id::text = $2
                  )
                GROUP BY series.id
            ),
            paged_series AS (
                SELECT *
                FROM matching_series
                WHERE $3::bigint IS NULL OR (
                    matching_series.work_count < $3
                    OR (
                        matching_series.work_count = $3
                        AND (
                            matching_series.normalized_name > $4
                            OR (
                                matching_series.normalized_name = $4
                                AND matching_series.pixiv_series_id > $5
                            )
                        )
                    )
                )
                ORDER BY
                    matching_series.work_count DESC,
                    matching_series.normalized_name,
                    matching_series.pixiv_series_id
                LIMIT $1
            )
            SELECT
                paged_series.id,
                paged_series.pixiv_series_id,
                paged_series.title,
                paged_series.normalized_name,
                paged_series.pixiv_artist_id,
                paged_series.work_count,
                paged_series.total_count,
                cover.id AS cover_derivative_id,
                cover.width AS cover_width,
                cover.height AS cover_height,
                cover.age_rating AS cover_age_rating
            FROM paged_series
            LEFT JOIN LATERAL (
                SELECT
                    derivative.id,
                    derivative.width,
                    derivative.height,
                    cover_revision.sanity_level AS age_rating
                FROM work AS cover_work
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
                WHERE cover_work.series_id = paged_series.id
                  AND cover_work.collection_state = 'collected'
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
            ORDER BY
                paged_series.work_count DESC,
                paged_series.normalized_name,
                paged_series.pixiv_series_id
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
            None => self.series_count(query).await?,
        };
        let mut rows = rows;
        let has_more = rows.len() > usize::from(limit);
        if has_more {
            rows.truncate(usize::from(limit));
        }
        let next_cursor = if has_more {
            let row = rows.last().expect("lookahead leaves one series");
            Some(GalleryContextCursor {
                work_count: non_negative_u64(row.get("work_count"), "series work count")?,
                normalized_name: row.get("normalized_name"),
                identity: GalleryContextIdentity::Series(row.get("pixiv_series_id")),
            })
        } else {
            None
        };
        let items = rows
            .into_iter()
            .map(|row| {
                Ok(GallerySeriesDetail {
                    id: row.get("id"),
                    pixiv_series_id: row.get("pixiv_series_id"),
                    pixiv_artist_id: row.get("pixiv_artist_id"),
                    title: row.get("title"),
                    work_count: non_negative_u64(row.get("work_count"), "series work count")?,
                    cover_derivative_id: row.get("cover_derivative_id"),
                    cover_width: optional_positive_u32(row.get("cover_width"), "cover width")?,
                    cover_height: optional_positive_u32(row.get("cover_height"), "cover height")?,
                    cover_age_rating: parse_optional_enum_value(
                        row.get("cover_age_rating"),
                        "cover age rating",
                        PixivAgeRating::from_db_value,
                    )?,
                })
            })
            .collect::<Result<Vec<_>, DbError>>()?;
        context_page(items, total, next_cursor)
    }

    async fn series_count(&self, query: Option<&str>) -> Result<i64, DbError> {
        sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM (
                SELECT series.id
                FROM series
                JOIN work
                  ON work.series_id = series.id
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
                    OR series.title ILIKE '%' || $1 || '%'
                    OR series.pixiv_series_id::text = $1
                  )
                GROUP BY series.id
            ) AS matching_series
            "#,
        )
        .bind(query)
        .fetch_one(self.db.pool())
        .await
        .map_err(Into::into)
    }

    pub async fn series_detail(
        &self,
        pixiv_series_id: i64,
    ) -> Result<GallerySeriesDetail, DbError> {
        validate_source_id(pixiv_series_id, "Pixiv series ID")?;
        let row = sqlx::query(
            r#"
            SELECT
                series.id,
                series.pixiv_series_id,
                series.title,
                min(artist.pixiv_artist_id) AS pixiv_artist_id,
                count(work.id) AS work_count,
                cover.id AS cover_derivative_id,
                cover.width AS cover_width,
                cover.height AS cover_height,
                cover.age_rating AS cover_age_rating
            FROM series
            LEFT JOIN work
              ON work.series_id = series.id
             AND work.collection_state = 'collected'
             AND EXISTS (
                SELECT 1
                FROM work_page AS local_page
                WHERE local_page.work_id = work.id
                  AND local_page.source_state = 'present'
                  AND local_page.current_media_revision_id IS NOT NULL
             )
            LEFT JOIN artist ON artist.id = work.artist_id
            LEFT JOIN LATERAL (
                SELECT
                    derivative.id,
                    derivative.width,
                    derivative.height,
                    cover_revision.sanity_level AS age_rating
                FROM work AS cover_work
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
                WHERE cover_work.series_id = series.id
                  AND cover_work.collection_state = 'collected'
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
            WHERE series.pixiv_series_id = $1
            GROUP BY series.id, cover.id, cover.width, cover.height, cover.age_rating
            "#,
        )
        .bind(pixiv_series_id)
        .fetch_one(self.db.pool())
        .await?;
        Ok(GallerySeriesDetail {
            id: row.get("id"),
            pixiv_series_id: row.get("pixiv_series_id"),
            pixiv_artist_id: row.get("pixiv_artist_id"),
            title: row.get("title"),
            work_count: non_negative_u64(row.get("work_count"), "series work count")?,
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
