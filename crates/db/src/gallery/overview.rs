use super::{GalleryRepository, model::parse_enum_value};
use crate::DbError;
use pixivarchive_domain::{pixiv::PixivAgeRating, work::GalleryOverviewDecoration};
use sqlx::{Postgres, Row, Transaction};
use time::Date;
use uuid::Uuid;

const DECORATION_SLOT_COUNT: usize = 3;

impl GalleryRepository {
    pub async fn overview_decorations(
        &self,
        date: Date,
        allow_nsfw: bool,
    ) -> Result<Vec<Option<GalleryOverviewDecoration>>, DbError> {
        self.select_overview_decorations(date, allow_nsfw, false)
            .await
    }

    pub async fn shuffle_overview_decorations(
        &self,
        date: Date,
        allow_nsfw: bool,
    ) -> Result<Vec<Option<GalleryOverviewDecoration>>, DbError> {
        self.select_overview_decorations(date, allow_nsfw, true)
            .await
    }

    async fn select_overview_decorations(
        &self,
        date: Date,
        allow_nsfw: bool,
        replace: bool,
    ) -> Result<Vec<Option<GalleryOverviewDecoration>>, DbError> {
        let mut tx = self.db.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("overview_decoration_selection:{date}"))
            .execute(&mut *tx)
            .await?;

        let stored_positions: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM overview_decoration_selection WHERE selection_date = $1",
        )
        .bind(date)
        .fetch_one(&mut *tx)
        .await?;
        if replace || stored_positions != DECORATION_SLOT_COUNT as i64 {
            replace_selection(&mut tx, date, allow_nsfw).await?;
        }

        let decorations = read_selection(&mut tx, date).await?;
        tx.commit().await?;
        Ok(decorations)
    }
}

async fn replace_selection(
    tx: &mut Transaction<'_, Postgres>,
    date: Date,
    allow_nsfw: bool,
) -> Result<(), DbError> {
    let candidates = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT work.id
        FROM work
        JOIN work_revision AS revision
          ON revision.id = work.current_revision_id
        JOIN work_page AS cover_page
          ON cover_page.work_id = work.id
         AND cover_page.page_index = 0
         AND cover_page.source_state = 'present'
        JOIN media_revision AS cover_media
          ON cover_media.id = cover_page.current_media_revision_id
        JOIN LATERAL (
            SELECT derivative.id
            FROM derivative
            WHERE derivative.media_revision_id = cover_media.id
              AND derivative.derivative_kind IN ('waterfall_thumbnail', 'ugoira_cover')
            ORDER BY
                CASE derivative.derivative_kind
                    WHEN 'waterfall_thumbnail' THEN 0
                    ELSE 1
                END,
                CASE derivative.format WHEN 'avif' THEN 0 ELSE 1 END
            LIMIT 1
        ) AS cover ON true
        WHERE work.collection_state = 'collected'
          AND revision.sanity_level <> 'unknown'
          AND ($1 OR revision.sanity_level = 'all_age')
        ORDER BY random()
        LIMIT 3
        "#,
    )
    .bind(allow_nsfw)
    .fetch_all(&mut **tx)
    .await?;

    sqlx::query("DELETE FROM overview_decoration_selection WHERE selection_date = $1")
        .bind(date)
        .execute(&mut **tx)
        .await?;
    for position in 0..DECORATION_SLOT_COUNT {
        let work_id = if candidates.is_empty() {
            None
        } else {
            Some(candidates[position % candidates.len()])
        };
        sqlx::query(
            r#"
            INSERT INTO overview_decoration_selection (selection_date, position, work_id)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(date)
        .bind(position as i16)
        .bind(work_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn read_selection(
    tx: &mut Transaction<'_, Postgres>,
    date: Date,
) -> Result<Vec<Option<GalleryOverviewDecoration>>, DbError> {
    let rows = sqlx::query(
        r#"
        SELECT
            live.pixiv_work_id,
            live.title,
            live.age_rating,
            live.cover_derivative_id
        FROM overview_decoration_selection AS selection
        LEFT JOIN LATERAL (
            SELECT
                work.pixiv_work_id,
                revision.title,
                revision.sanity_level AS age_rating,
                cover.id AS cover_derivative_id
            FROM work
            JOIN work_revision AS revision
              ON revision.id = work.current_revision_id
            JOIN work_page AS cover_page
              ON cover_page.work_id = work.id
             AND cover_page.page_index = 0
             AND cover_page.source_state = 'present'
            JOIN media_revision AS cover_media
              ON cover_media.id = cover_page.current_media_revision_id
            JOIN LATERAL (
                SELECT derivative.id
                FROM derivative
                WHERE derivative.media_revision_id = cover_media.id
                  AND derivative.derivative_kind IN ('waterfall_thumbnail', 'ugoira_cover')
                ORDER BY
                    CASE derivative.derivative_kind
                        WHEN 'waterfall_thumbnail' THEN 0
                        ELSE 1
                    END,
                    CASE derivative.format WHEN 'avif' THEN 0 ELSE 1 END
                LIMIT 1
            ) AS cover ON true
            WHERE work.id = selection.work_id
              AND work.collection_state = 'collected'
              AND revision.sanity_level <> 'unknown'
        ) AS live ON true
        WHERE selection.selection_date = $1
        ORDER BY selection.position
        "#,
    )
    .bind(date)
    .fetch_all(&mut **tx)
    .await?;

    rows.into_iter()
        .map(|row| {
            let Some(pixiv_work_id) = row.get::<Option<i64>, _>("pixiv_work_id") else {
                return Ok(None);
            };
            let age_rating = parse_enum_value(
                row.get("age_rating"),
                "overview decoration age rating",
                PixivAgeRating::from_db_value,
            )?;
            Ok(Some(GalleryOverviewDecoration {
                pixiv_work_id,
                title: row.get("title"),
                age_rating,
                cover_derivative_id: row.get("cover_derivative_id"),
            }))
        })
        .collect()
}
