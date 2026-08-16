use super::{GalleryRepository, selection_projection_query};
use crate::DbError;
use pixivarchive_domain::work::{
    GalleryContextKind, GalleryContextSelectionExpression, GalleryContextSelectionProjection,
    GallerySelectionExpression, GallerySelectionProjection,
};
use sqlx::{Postgres, QueryBuilder, Row};
use uuid::Uuid;

impl GalleryRepository {
    pub async fn selection_projection(
        &self,
        expression: &GallerySelectionExpression,
        visible_work_ids: &[Uuid],
        current_account_id: Option<Uuid>,
    ) -> Result<GallerySelectionProjection, DbError> {
        let mut query =
            selection_projection_query(expression, visible_work_ids, current_account_id);
        let row = query.build().fetch_one(self.db.pool()).await?;
        Ok(GallerySelectionProjection {
            selected_count: u64::try_from(row.try_get::<i64, _>("selected_count")?)
                .map_err(|_| DbError::Constraint("negative gallery selection count".to_owned()))?,
            selected_visible_work_ids: row.try_get("selected_visible_work_ids")?,
        })
    }

    pub async fn context_selection_projection(
        &self,
        expression: &GalleryContextSelectionExpression,
        visible_context_ids: &[Uuid],
    ) -> Result<GalleryContextSelectionProjection, DbError> {
        let mut query = context_selection_projection_query(expression, visible_context_ids);
        let row = query.build().fetch_one(self.db.pool()).await?;
        Ok(GalleryContextSelectionProjection {
            selected_context_count: non_negative_count(
                row.try_get("selected_context_count")?,
                "gallery context selection count",
            )?,
            selected_work_count: non_negative_count(
                row.try_get("selected_work_count")?,
                "gallery context selected work count",
            )?,
            selected_visible_context_ids: row.try_get("selected_visible_context_ids")?,
        })
    }
}

pub(crate) fn context_selected_work_query(
    select: &str,
    expression: &GalleryContextSelectionExpression,
) -> QueryBuilder<Postgres> {
    let mut query = context_selection_ctes(expression);
    query
        .push(select)
        .push(" FROM selected_works JOIN work ON work.id = selected_works.id WHERE true");
    query
}

fn context_selection_ctes(
    expression: &GalleryContextSelectionExpression,
) -> QueryBuilder<Postgres> {
    let mut query = QueryBuilder::new("WITH context_items AS (");
    push_context_items(&mut query, expression.kind);
    query.push("), selected_contexts AS (SELECT context_id FROM context_items WHERE ");
    push_context_search_predicate(&mut query, &expression.query);
    query.push(" AND ");
    push_context_selection_state(&mut query, expression);
    query.push("), selected_works AS (SELECT DISTINCT work.id FROM selected_contexts");
    push_context_work_join(&mut query, expression.kind);
    push_browsable_work_condition(&mut query);
    query.push(") ");
    query
}

fn context_selection_projection_query(
    expression: &GalleryContextSelectionExpression,
    visible_context_ids: &[Uuid],
) -> QueryBuilder<Postgres> {
    let mut query = context_selection_ctes(expression);
    query.push(
        "SELECT (SELECT count(*)::bigint FROM selected_contexts) AS selected_context_count, (SELECT count(*)::bigint FROM selected_works) AS selected_work_count, (SELECT coalesce(array_agg(context_id ORDER BY context_id) FILTER (WHERE context_id = ANY(",
    );
    query.push_bind(visible_context_ids.to_vec()).push(
        "::uuid[])), ARRAY[]::uuid[]) FROM selected_contexts) AS selected_visible_context_ids",
    );
    query
}

fn push_context_items(query: &mut QueryBuilder<Postgres>, kind: GalleryContextKind) {
    match kind {
        GalleryContextKind::Artist => query.push(
            r#"
            SELECT artist.id AS context_id,
                   artist.name AS title,
                   coalesce(artist.account_name, '') AS secondary,
                   artist.pixiv_artist_id AS source_id
            FROM artist
            WHERE EXISTS (
                SELECT 1
                FROM work
                WHERE work.artist_id = artist.id
            "#,
        ),
        GalleryContextKind::Tag => query.push(
            r#"
            SELECT tag.id AS context_id,
                   tag.raw_name AS title,
                   coalesce(tag.translated_name, '') AS secondary,
                   NULL::bigint AS source_id
            FROM tag
            WHERE EXISTS (
                SELECT 1
                FROM work_tag
                JOIN work ON work.id = work_tag.work_id
                WHERE work_tag.tag_id = tag.id
            "#,
        ),
        GalleryContextKind::Series => query.push(
            r#"
            SELECT series.id AS context_id,
                   series.title AS title,
                   ''::text AS secondary,
                   series.pixiv_series_id AS source_id
            FROM series
            WHERE EXISTS (
                SELECT 1
                FROM work
                WHERE work.series_id = series.id
            "#,
        ),
    };
    push_browsable_work_condition(query);
    query.push(")");
}

fn push_context_work_join(query: &mut QueryBuilder<Postgres>, kind: GalleryContextKind) {
    match kind {
        GalleryContextKind::Artist => {
            query.push(" JOIN work ON work.artist_id = selected_contexts.context_id WHERE true")
        }
        GalleryContextKind::Tag => query.push(
            " JOIN work_tag ON work_tag.tag_id = selected_contexts.context_id JOIN work ON work.id = work_tag.work_id WHERE true",
        ),
        GalleryContextKind::Series => {
            query.push(" JOIN work ON work.series_id = selected_contexts.context_id WHERE true")
        }
    };
}

fn push_browsable_work_condition(query: &mut QueryBuilder<Postgres>) {
    query.push(
        r#"
        AND work.collection_state = 'collected'
        AND EXISTS (
            SELECT 1
            FROM work_page AS context_page
            WHERE context_page.work_id = work.id
              AND context_page.source_state = 'present'
              AND context_page.current_media_revision_id IS NOT NULL
        )
        "#,
    );
}

fn push_context_selection_state(
    query: &mut QueryBuilder<Postgres>,
    expression: &GalleryContextSelectionExpression,
) {
    query
        .push("(")
        .push_bind(expression.base_selected)
        .push(" <> (context_items.context_id = ANY(")
        .push_bind(expression.exception_context_ids.clone())
        .push("::uuid[])))");
}

fn push_context_search_predicate(query: &mut QueryBuilder<Postgres>, search: &str) {
    let search = search.trim();
    if search.is_empty() {
        query.push("true");
        return;
    }
    query
        .push("(context_items.title ILIKE '%' || ")
        .push_bind(search.to_owned())
        .push(" || '%' OR context_items.secondary ILIKE '%' || ")
        .push_bind(search.to_owned())
        .push(" || '%' OR coalesce(context_items.source_id::text = ")
        .push_bind(search.to_owned())
        .push(", false))");
}

fn non_negative_count(value: i64, name: &str) -> Result<u64, DbError> {
    u64::try_from(value).map_err(|_| DbError::Constraint(format!("negative {name}")))
}
