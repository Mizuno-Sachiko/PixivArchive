use super::{
    GalleryRepository, MAX_PAGE_SIZE,
    model::{cursor_for, non_negative_u64, work_from_row},
    query::{
        GalleryScope, matching_work_query, push_current_bookmark_exists, push_current_bookmark_id,
        push_cursor, push_order, push_search_scope_for,
    },
};
use crate::DbError;
use pixivarchive_domain::work::{GallerySearch, GallerySearchPage, GalleryTag, GalleryWork};
use sqlx::{Postgres, QueryBuilder, Row};
use std::collections::HashMap;
use uuid::Uuid;

impl GalleryRepository {
    pub async fn search(
        &self,
        search: GallerySearch,
        current_account_id: Option<Uuid>,
    ) -> Result<GallerySearchPage, DbError> {
        self.search_in_scope(
            search,
            current_account_id,
            GalleryScope::BrowsableCollection,
        )
        .await
    }

    pub(super) async fn search_stored_work(
        &self,
        search: GallerySearch,
        current_account_id: Option<Uuid>,
    ) -> Result<GallerySearchPage, DbError> {
        self.search_in_scope(search, current_account_id, GalleryScope::StoredWork)
            .await
    }

    async fn search_in_scope(
        &self,
        search: GallerySearch,
        current_account_id: Option<Uuid>,
        scope: GalleryScope,
    ) -> Result<GallerySearchPage, DbError> {
        if search.limit == 0 || search.limit > MAX_PAGE_SIZE {
            return Err(DbError::InvalidValue(format!(
                "gallery page size must be between 1 and {MAX_PAGE_SIZE}"
            )));
        }

        let mut query = QueryBuilder::<Postgres>::new(
            r#"
            SELECT
                work.id,
                work.pixiv_work_id,
                revision.title,
                revision.caption AS description,
                artist.id AS artist_id,
                artist.pixiv_artist_id,
                artist.name AS artist_name,
                series.id AS series_id,
                series.title AS series_title,
                revision.work_kind,
                revision.sanity_level AS age_rating,
                coalesce(
                    revision.metadata ->> 'ai_classification' = 'ai_generated',
                    false
                ) AS ai_generated,
                revision.page_count,
                work.collection_state,
                work.source_state,
            "#,
        );
        push_current_bookmark_exists(&mut query, current_account_id);
        query.push(" AS bookmarked_by_current_account, ");
        push_current_bookmark_id(&mut query, current_account_id);
        query.push(
            r#" AS bookmark_id,
                work.bookmark_count,
                work.view_count,
                work.like_count,
                work.comment_count,
                revision.pixiv_created_at,
                revision.pixiv_updated_at,
                work.updated_at AS local_updated_at,
                cover.id AS cover_derivative_id,
                cover.path AS cover_path,
                cover.width AS cover_width,
                cover.height AS cover_height,
                cover.media_kind
            FROM work
            JOIN work_revision AS revision ON revision.id = work.current_revision_id
            JOIN artist ON artist.id = work.artist_id
            LEFT JOIN series ON series.id = work.series_id
            LEFT JOIN LATERAL (
                SELECT
                    derivative.id,
                    derivative.path,
                    derivative.width,
                    derivative.height,
                    media_revision.media_kind
                FROM work_page
                LEFT JOIN media_revision
                    ON media_revision.id = work_page.current_media_revision_id
                LEFT JOIN derivative
                    ON derivative.media_revision_id = media_revision.id
                   AND derivative.derivative_kind IN ('waterfall_thumbnail', 'ugoira_cover')
                WHERE work_page.work_id = work.id
                  AND work_page.page_index = 0
                  AND work_page.source_state = 'present'
                ORDER BY
                    CASE derivative.derivative_kind
                        WHEN 'waterfall_thumbnail' THEN 0
                        ELSE 1
                    END,
                    CASE derivative.format
                        WHEN 'avif' THEN 0
                        ELSE 1
                    END
                LIMIT 1
            ) AS cover ON true
            WHERE true
            "#,
        );

        push_search_scope_for(&mut query, &search, current_account_id, scope);
        push_cursor(
            &mut query,
            search.sort_field,
            search.sort_direction,
            search.cursor.as_ref(),
        )?;
        push_order(&mut query, search.sort_field, search.sort_direction);
        query.push(" LIMIT ");
        query.push_bind(i64::from(search.limit) + 1);

        let rows = query.build().fetch_all(self.db.pool()).await?;
        let mut items = rows
            .into_iter()
            .map(work_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = items.len() > usize::from(search.limit);
        if has_more {
            items.pop();
        }
        self.load_tags(&mut items).await?;
        let next_cursor = has_more
            .then(|| {
                items
                    .last()
                    .map(|work| cursor_for(work, search.sort_field, search.sort_direction))
            })
            .flatten();

        Ok(GallerySearchPage { items, next_cursor })
    }

    pub async fn count(
        &self,
        search: &GallerySearch,
        current_account_id: Option<Uuid>,
    ) -> Result<u64, DbError> {
        let mut query = matching_work_query("SELECT count(*)::bigint", search, current_account_id);
        let count = query
            .build_query_scalar::<i64>()
            .fetch_one(self.db.pool())
            .await?;
        non_negative_u64(count, "gallery result count")
    }

    pub async fn work_ids(
        &self,
        search: &GallerySearch,
        excluded_work_ids: &[Uuid],
        current_account_id: Option<Uuid>,
    ) -> Result<Vec<Uuid>, DbError> {
        let mut query = matching_work_query("SELECT work.id", search, current_account_id);
        if !excluded_work_ids.is_empty() {
            query
                .push(" AND NOT (work.id = ANY(")
                .push_bind(excluded_work_ids.to_vec())
                .push("::uuid[]))");
        }
        query
            .build_query_scalar::<Uuid>()
            .fetch_all(self.db.pool())
            .await
            .map_err(DbError::from)
    }

    async fn load_tags(&self, works: &mut [GalleryWork]) -> Result<(), DbError> {
        if works.is_empty() {
            return Ok(());
        }
        let work_ids = works.iter().map(|work| work.id).collect::<Vec<_>>();
        let rows = sqlx::query(
            r#"
            SELECT work_tag.work_id, tag.id, tag.raw_name, tag.translated_name
            FROM work_tag
            JOIN tag ON tag.id = work_tag.tag_id
            WHERE work_tag.work_id = ANY($1)
            ORDER BY tag.raw_name, tag.id
            "#,
        )
        .bind(work_ids)
        .fetch_all(self.db.pool())
        .await?;
        let mut tags = HashMap::<Uuid, Vec<GalleryTag>>::new();
        for row in rows {
            tags.entry(row.try_get("work_id")?)
                .or_default()
                .push(GalleryTag {
                    id: row.try_get("id")?,
                    original: row.try_get("raw_name")?,
                    translation: row.try_get("translated_name")?,
                });
        }
        for work in works {
            work.tags = tags.remove(&work.id).unwrap_or_default();
        }
        Ok(())
    }
}
