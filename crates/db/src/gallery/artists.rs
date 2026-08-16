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
    work::{GalleryArtistDetail, GalleryContextCursor, GalleryContextIdentity, GalleryContextPage},
};
use sqlx::Row;

impl GalleryRepository {
    pub async fn artists(
        &self,
        limit: u16,
        cursor: Option<&GalleryContextCursor>,
        query: Option<&str>,
    ) -> Result<GalleryContextPage<GalleryArtistDetail>, DbError> {
        validate_context_limit(limit)?;
        let query = query.map(str::trim).filter(|value| !value.is_empty());
        let (cursor_count, cursor_name, cursor_id) = match cursor {
            None => (None, None, None),
            Some(cursor) => {
                let GalleryContextIdentity::Artist(source_id) = &cursor.identity else {
                    return Err(DbError::InvalidValue(
                        "gallery artist cursor has the wrong identity type".to_owned(),
                    ));
                };
                validate_source_id(*source_id, "gallery artist cursor source ID")?;
                (
                    context_cursor_count(Some(cursor))?,
                    Some(cursor.normalized_name.as_str()),
                    Some(*source_id),
                )
            }
        };
        let rows = sqlx::query(
            r#"
            WITH matching_artists AS (
                SELECT
                    artist.id,
                    artist.pixiv_artist_id,
                    artist.name,
                    artist.account_name,
                    lower(artist.name) AS normalized_name,
                    count(work.id) AS work_count,
                    count(*) OVER () AS total_count
                FROM artist
                JOIN work
                  ON work.artist_id = artist.id
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
                    OR artist.name ILIKE '%' || $2 || '%'
                    OR coalesce(artist.account_name, '') ILIKE '%' || $2 || '%'
                    OR artist.pixiv_artist_id::text = $2
                  )
                GROUP BY artist.id
            ),
            paged_artists AS (
                SELECT *
                FROM matching_artists
                WHERE $3::bigint IS NULL OR (
                    matching_artists.work_count < $3
                    OR (
                        matching_artists.work_count = $3
                        AND (
                            matching_artists.normalized_name > $4
                            OR (
                                matching_artists.normalized_name = $4
                                AND matching_artists.pixiv_artist_id > $5
                            )
                        )
                    )
                )
                ORDER BY
                    matching_artists.work_count DESC,
                    matching_artists.normalized_name,
                    matching_artists.pixiv_artist_id
                LIMIT $1
            )
            SELECT
                paged_artists.id,
                paged_artists.pixiv_artist_id,
                paged_artists.name,
                paged_artists.account_name,
                paged_artists.normalized_name,
                paged_artists.work_count,
                paged_artists.total_count,
                cover.id AS cover_derivative_id,
                cover.width AS cover_width,
                cover.height AS cover_height,
                cover.age_rating AS cover_age_rating
            FROM paged_artists
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
                WHERE cover_work.artist_id = paged_artists.id
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
                paged_artists.work_count DESC,
                paged_artists.normalized_name,
                paged_artists.pixiv_artist_id
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
            None => self.artist_count(query).await?,
        };
        let mut rows = rows;
        let has_more = rows.len() > usize::from(limit);
        if has_more {
            rows.truncate(usize::from(limit));
        }
        let next_cursor = if has_more {
            let row = rows.last().expect("lookahead leaves one artist");
            Some(GalleryContextCursor {
                work_count: non_negative_u64(row.get("work_count"), "artist work count")?,
                normalized_name: row.get("normalized_name"),
                identity: GalleryContextIdentity::Artist(row.get("pixiv_artist_id")),
            })
        } else {
            None
        };
        let items = rows
            .into_iter()
            .map(|row| {
                Ok(GalleryArtistDetail {
                    id: row.get("id"),
                    pixiv_artist_id: row.get("pixiv_artist_id"),
                    name: row.get("name"),
                    account_name: row.get("account_name"),
                    work_count: non_negative_u64(row.get("work_count"), "artist work count")?,
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

    async fn artist_count(&self, query: Option<&str>) -> Result<i64, DbError> {
        sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM (
                SELECT artist.id
                FROM artist
                JOIN work
                  ON work.artist_id = artist.id
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
                    OR artist.name ILIKE '%' || $1 || '%'
                    OR coalesce(artist.account_name, '') ILIKE '%' || $1 || '%'
                    OR artist.pixiv_artist_id::text = $1
                  )
                GROUP BY artist.id
            ) AS matching_artists
            "#,
        )
        .bind(query)
        .fetch_one(self.db.pool())
        .await
        .map_err(Into::into)
    }

    pub async fn artist_detail(
        &self,
        pixiv_artist_id: i64,
    ) -> Result<GalleryArtistDetail, DbError> {
        validate_source_id(pixiv_artist_id, "Pixiv artist ID")?;
        let row = sqlx::query(
            r#"
            SELECT
                artist.id,
                artist.pixiv_artist_id,
                artist.name,
                artist.account_name,
                count(work.id) AS work_count,
                cover.id AS cover_derivative_id,
                cover.width AS cover_width,
                cover.height AS cover_height,
                cover.age_rating AS cover_age_rating
            FROM artist
            LEFT JOIN work
              ON work.artist_id = artist.id
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
                WHERE cover_work.artist_id = artist.id
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
            WHERE artist.pixiv_artist_id = $1
            GROUP BY artist.id, cover.id, cover.width, cover.height, cover.age_rating
            "#,
        )
        .bind(pixiv_artist_id)
        .fetch_one(self.db.pool())
        .await?;
        Ok(GalleryArtistDetail {
            id: row.get("id"),
            pixiv_artist_id: row.get("pixiv_artist_id"),
            name: row.get("name"),
            account_name: row.get("account_name"),
            work_count: non_negative_u64(row.get("work_count"), "artist work count")?,
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
