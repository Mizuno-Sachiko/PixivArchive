use crate::DbError;
use pixivarchive_domain::work::{
    FilterMode, GalleryBooleanField, GalleryCategoryField, GalleryCursor, GalleryCursorKey,
    GalleryDateComparison, GalleryDateField, GalleryFilter, GalleryFilterGroup,
    GalleryNumberComparison, GalleryNumberField, GallerySearch, GallerySelectionExpression,
    GallerySortField, GalleryTagOperator, GalleryTagScope, GalleryTextField, GalleryTextOperator,
    SortDirection,
};
use sqlx::{Postgres, QueryBuilder};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub(super) enum GalleryScope {
    BrowsableCollection,
    AddressableDetail,
    StoredWork,
}

pub(crate) fn matching_work_query(
    select: &str,
    search: &GallerySearch,
    current_account_id: Option<Uuid>,
) -> QueryBuilder<Postgres> {
    let mut query = QueryBuilder::<Postgres>::new(select);
    push_gallery_from(&mut query);
    push_search_scope(&mut query, search, current_account_id);
    query
}

pub(crate) fn selection_projection_query(
    expression: &GallerySelectionExpression,
    visible_work_ids: &[Uuid],
    current_account_id: Option<Uuid>,
) -> QueryBuilder<Postgres> {
    let mut query = QueryBuilder::<Postgres>::new("SELECT count(*) FILTER (WHERE ");
    push_selection_state(&mut query, expression, current_account_id);
    query.push(")::bigint AS selected_count, coalesce(array_agg(work.id ORDER BY work.id) FILTER (WHERE work.id = ANY(")
        .push_bind(visible_work_ids.to_vec())
        .push("::uuid[]) AND ");
    push_selection_state(&mut query, expression, current_account_id);
    query.push("), ARRAY[]::uuid[]) AS selected_visible_work_ids");
    push_gallery_from(&mut query);
    push_search_scope(&mut query, &expression.search, current_account_id);
    query
}

fn push_gallery_from(query: &mut QueryBuilder<Postgres>) {
    query.push(
        r#"
        FROM work
        JOIN work_revision AS revision ON revision.id = work.current_revision_id
        JOIN artist ON artist.id = work.artist_id
        LEFT JOIN series ON series.id = work.series_id
        WHERE true
        "#,
    );
}

pub(crate) fn push_selection_state(
    query: &mut QueryBuilder<Postgres>,
    expression: &GallerySelectionExpression,
    _current_account_id: Option<Uuid>,
) {
    query
        .push("(")
        .push_bind(expression.base_selected)
        .push(" <> (work.id = ANY(")
        .push_bind(expression.exception_work_ids.clone())
        .push("::uuid[])))");
}

pub(super) fn push_search_scope(
    query: &mut QueryBuilder<Postgres>,
    search: &GallerySearch,
    current_account_id: Option<Uuid>,
) {
    push_search_scope_for(
        query,
        search,
        current_account_id,
        GalleryScope::BrowsableCollection,
    );
}

pub(super) fn push_search_scope_for(
    query: &mut QueryBuilder<Postgres>,
    search: &GallerySearch,
    current_account_id: Option<Uuid>,
    scope: GalleryScope,
) {
    push_collection_scope(query, scope);
    if !search.restrict_work_ids.is_empty() {
        query
            .push(" AND work.id = ANY(")
            .push_bind(search.restrict_work_ids.clone())
            .push("::uuid[])");
    }
    push_groups(query, search.group_mode, &search.groups, current_account_id);
}

pub(super) fn push_collection_scope(query: &mut QueryBuilder<Postgres>, scope: GalleryScope) {
    match scope {
        GalleryScope::BrowsableCollection => {
            query.push(
                r#"
            AND work.collection_state = 'collected'
            AND EXISTS (
                SELECT 1
                FROM work_page AS local_page
                JOIN media_revision AS local_media
                  ON local_media.id = local_page.current_media_revision_id
                WHERE local_page.work_id = work.id
                  AND local_page.source_state = 'present'
            )
            "#,
            );
        }
        GalleryScope::AddressableDetail => {
            query.push(" AND work.collection_state IN ('collected', 'trash')");
        }
        GalleryScope::StoredWork => {}
    }
}

fn push_groups(
    query: &mut QueryBuilder<Postgres>,
    mode: FilterMode,
    groups: &[GalleryFilterGroup],
    current_account_id: Option<Uuid>,
) {
    if groups.is_empty() {
        return;
    }
    query.push(" AND (");
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            query.push(mode_separator(mode));
        }
        query.push("(");
        push_group(query, group, current_account_id);
        query.push(")");
    }
    query.push(")");
}

fn push_group(
    query: &mut QueryBuilder<Postgres>,
    group: &GalleryFilterGroup,
    current_account_id: Option<Uuid>,
) {
    if group.filters.is_empty() {
        query.push(match group.mode {
            FilterMode::All => "true",
            FilterMode::Any => "false",
        });
        return;
    }
    for (index, filter) in group.filters.iter().enumerate() {
        if index > 0 {
            query.push(mode_separator(group.mode));
        }
        query.push("(");
        push_filter(query, filter, current_account_id);
        query.push(")");
    }
}

fn mode_separator(mode: FilterMode) -> &'static str {
    match mode {
        FilterMode::All => " AND ",
        FilterMode::Any => " OR ",
    }
}

fn push_filter(
    query: &mut QueryBuilder<Postgres>,
    filter: &GalleryFilter,
    current_account_id: Option<Uuid>,
) {
    match filter {
        GalleryFilter::WorkId { value } => {
            query.push("work.id = ").push_bind(*value);
        }
        GalleryFilter::PixivWorkId { value } => {
            query.push("work.pixiv_work_id = ").push_bind(*value);
        }
        GalleryFilter::ArtistId { value } => {
            query.push("artist.id = ").push_bind(*value);
        }
        GalleryFilter::PixivArtistId { value } => {
            query.push("artist.pixiv_artist_id = ").push_bind(*value);
        }
        GalleryFilter::TagId { value } => {
            query
                .push(
                    "EXISTS (
                        SELECT 1
                        FROM work_tag AS identity_work_tag
                        WHERE identity_work_tag.work_id = work.id
                          AND identity_work_tag.tag_id = ",
                )
                .push_bind(*value)
                .push(")");
        }
        GalleryFilter::SeriesId { value } => {
            query.push("work.series_id = ").push_bind(*value);
        }
        GalleryFilter::MediaRevisionId { value } => {
            query
                .push(
                    r#"EXISTS (
                        SELECT 1
                        FROM work_page AS media_page
                        WHERE media_page.work_id = work.id
                          AND media_page.current_media_revision_id =
                    "#,
                )
                .push_bind(*value)
                .push(")");
        }
        GalleryFilter::Text {
            field,
            operator,
            value,
        } => push_text_filter(query, *field, *operator, value),
        GalleryFilter::Tags {
            operator,
            names,
            scope,
        } => push_tag_filter(query, *operator, names, *scope),
        GalleryFilter::Category {
            field,
            include,
            exclude,
        } => push_category_filter(query, *field, include, exclude),
        GalleryFilter::Number { field, comparison } => {
            push_number_filter(query, *field, *comparison)
        }
        GalleryFilter::Date { field, comparison } => push_date_filter(query, *field, *comparison),
        GalleryFilter::Boolean { field, value } => {
            push_boolean_filter(query, *field, *value, current_account_id)
        }
    }
}

fn push_text_filter(
    query: &mut QueryBuilder<Postgres>,
    field: GalleryTextField,
    operator: GalleryTextOperator,
    value: &str,
) {
    let negated = operator == GalleryTextOperator::Excludes;
    if negated {
        query.push("NOT (");
    }
    let columns = match field {
        GalleryTextField::Any => &[
            "revision.title",
            "coalesce(revision.caption, '')",
            "artist.name",
            "coalesce(series.title, '')",
        ][..],
        GalleryTextField::Title => &["revision.title"][..],
        GalleryTextField::Description => &["coalesce(revision.caption, '')"][..],
        GalleryTextField::ArtistName => &["artist.name"][..],
        GalleryTextField::SeriesTitle => &["coalesce(series.title, '')"][..],
        GalleryTextField::TagName => &[][..],
    };
    query.push("(");
    let mut wrote = false;
    for column in columns {
        if wrote {
            query.push(" OR ");
        }
        push_text_predicate(query, column, operator, value);
        wrote = true;
    }
    if matches!(field, GalleryTextField::Any | GalleryTextField::TagName) {
        if wrote {
            query.push(" OR ");
        }
        query.push(
            r#"EXISTS (
                SELECT 1
                FROM work_tag AS text_work_tag
                JOIN tag AS text_tag ON text_tag.id = text_work_tag.tag_id
                WHERE text_work_tag.work_id = work.id
                  AND (
            "#,
        );
        push_text_predicate(query, "text_tag.raw_name", operator, value);
        query.push(" OR ");
        push_text_predicate(
            query,
            "coalesce(text_tag.translated_name, '')",
            operator,
            value,
        );
        query.push("))");
    }
    query.push(")");
    if negated {
        query.push(")");
    }
}

fn push_text_predicate(
    query: &mut QueryBuilder<Postgres>,
    column: &str,
    operator: GalleryTextOperator,
    value: &str,
) {
    match operator {
        GalleryTextOperator::Equals => {
            query
                .push("lower(")
                .push(column)
                .push(") = lower(")
                .push_bind(value.to_owned())
                .push(")");
        }
        GalleryTextOperator::Contains | GalleryTextOperator::Excludes => {
            query
                .push(column)
                .push(" ILIKE ")
                .push_bind(like_pattern(value, true, true))
                .push(" ESCAPE '\\'");
        }
        GalleryTextOperator::StartsWith => {
            query
                .push(column)
                .push(" ILIKE ")
                .push_bind(like_pattern(value, false, true))
                .push(" ESCAPE '\\'");
        }
        GalleryTextOperator::EndsWith => {
            query
                .push(column)
                .push(" ILIKE ")
                .push_bind(like_pattern(value, true, false))
                .push(" ESCAPE '\\'");
        }
    }
}

fn like_pattern(value: &str, prefix: bool, suffix: bool) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!(
        "{}{escaped}{}",
        if prefix { "%" } else { "" },
        if suffix { "%" } else { "" }
    )
}

fn push_tag_filter(
    query: &mut QueryBuilder<Postgres>,
    operator: GalleryTagOperator,
    names: &[String],
    scope: GalleryTagScope,
) {
    let names = normalized_values(names);
    if names.is_empty() {
        query.push(match operator {
            GalleryTagOperator::Any => "false",
            GalleryTagOperator::All
            | GalleryTagOperator::ExcludeAny
            | GalleryTagOperator::ExactSet => "true",
            GalleryTagOperator::NotAll => "false",
        });
        return;
    }

    match operator {
        GalleryTagOperator::Any | GalleryTagOperator::ExcludeAny => {
            if operator == GalleryTagOperator::ExcludeAny {
                query.push("NOT ");
            }
            query.push(
                r#"EXISTS (
                    SELECT 1
                    FROM work_tag AS selected_work_tag
                    JOIN tag AS selected_tag ON selected_tag.id = selected_work_tag.tag_id
                    WHERE selected_work_tag.work_id = work.id
                      AND
                "#,
            );
            push_tag_matches_array(query, "selected_tag", scope, names);
            query.push(")");
        }
        GalleryTagOperator::All | GalleryTagOperator::NotAll => {
            if operator == GalleryTagOperator::All {
                query.push("NOT ");
            }
            query.push("EXISTS (SELECT 1 FROM unnest(");
            query.push_bind(names);
            query.push(
                r#"::text[]) AS wanted(name)
                    WHERE NOT EXISTS (
                        SELECT 1
                        FROM work_tag AS required_work_tag
                        JOIN tag AS required_tag ON required_tag.id = required_work_tag.tag_id
                        WHERE required_work_tag.work_id = work.id
                          AND
                "#,
            );
            push_tag_matches_name(query, "required_tag", scope, "wanted.name");
            query.push("))");
        }
        GalleryTagOperator::ExactSet => {
            query.push(
                r#"NOT EXISTS (
                    SELECT 1
                    FROM work_tag AS extra_work_tag
                    JOIN tag AS extra_tag ON extra_tag.id = extra_work_tag.tag_id
                    WHERE extra_work_tag.work_id = work.id
                      AND NOT
                "#,
            );
            push_tag_matches_array(query, "extra_tag", scope, names.clone());
            query.push(") AND NOT EXISTS (SELECT 1 FROM unnest(");
            query.push_bind(names);
            query.push(
                r#"::text[]) AS wanted(name)
                    WHERE NOT EXISTS (
                        SELECT 1
                        FROM work_tag AS exact_work_tag
                        JOIN tag AS exact_tag ON exact_tag.id = exact_work_tag.tag_id
                        WHERE exact_work_tag.work_id = work.id
                          AND
                "#,
            );
            push_tag_matches_name(query, "exact_tag", scope, "wanted.name");
            query.push("))");
        }
    }
}

fn push_tag_matches_array(
    query: &mut QueryBuilder<Postgres>,
    alias: &str,
    scope: GalleryTagScope,
    names: Vec<String>,
) {
    query
        .push("(")
        .push("lower(")
        .push(alias)
        .push(".raw_name) = ANY(");
    query.push_bind(names.clone()).push("::text[])");
    if scope == GalleryTagScope::OriginalAndTranslation {
        query
            .push(" OR lower(coalesce(")
            .push(alias)
            .push(".translated_name, '')) = ANY(")
            .push_bind(names)
            .push("::text[])");
    }
    query.push(")");
}

fn push_tag_matches_name(
    query: &mut QueryBuilder<Postgres>,
    alias: &str,
    scope: GalleryTagScope,
    name_expression: &str,
) {
    query
        .push("(lower(")
        .push(alias)
        .push(".raw_name) = ")
        .push(name_expression);
    if scope == GalleryTagScope::OriginalAndTranslation {
        query
            .push(" OR lower(coalesce(")
            .push(alias)
            .push(".translated_name, '')) = ")
            .push(name_expression);
    }
    query.push(")");
}

fn normalized_values(values: &[String]) -> Vec<String> {
    let mut normalized = values
        .iter()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn push_category_filter(
    query: &mut QueryBuilder<Postgres>,
    field: GalleryCategoryField,
    include: &[String],
    exclude: &[String],
) {
    let include = normalized_values(include);
    let exclude = normalized_values(exclude);
    let column = match field {
        GalleryCategoryField::WorkKind => Some("revision.work_kind"),
        GalleryCategoryField::AgeRating => Some("revision.sanity_level"),
        GalleryCategoryField::CollectionState => Some("work.collection_state"),
        GalleryCategoryField::SourceState => Some("work.source_state"),
        GalleryCategoryField::MediaFormat | GalleryCategoryField::DerivativeFormat => None,
    };
    if include.is_empty() && exclude.is_empty() {
        query.push("true");
        return;
    }
    if let Some(column) = column {
        if !include.is_empty() {
            query
                .push("lower(")
                .push(column)
                .push(") = ANY(")
                .push_bind(include.clone())
                .push("::text[])");
        }
        if !include.is_empty() && !exclude.is_empty() {
            query.push(" AND ");
        }
        if !exclude.is_empty() {
            query
                .push("NOT (lower(")
                .push(column)
                .push(") = ANY(")
                .push_bind(exclude)
                .push("::text[]))");
        }
        return;
    }

    let (table, join, format_column) = match field {
        GalleryCategoryField::MediaFormat => (
            "media_revision AS category_media",
            "category_media.work_page_id = category_page.id",
            "category_media.format",
        ),
        GalleryCategoryField::DerivativeFormat => (
            "media_revision AS category_media JOIN derivative AS category_derivative ON category_derivative.media_revision_id = category_media.id",
            "category_media.work_page_id = category_page.id",
            "category_derivative.format",
        ),
        _ => unreachable!(),
    };
    if !include.is_empty() {
        query
            .push("EXISTS (SELECT 1 FROM work_page AS category_page JOIN ")
            .push(table)
            .push(" ON ")
            .push(join)
            .push(" WHERE category_page.work_id = work.id AND lower(")
            .push(format_column)
            .push(") = ANY(")
            .push_bind(include.clone())
            .push("::text[]))");
    }
    if !include.is_empty() && !exclude.is_empty() {
        query.push(" AND ");
    }
    if !exclude.is_empty() {
        query
            .push("NOT EXISTS (SELECT 1 FROM work_page AS category_page JOIN ")
            .push(table)
            .push(" ON ")
            .push(join)
            .push(" WHERE category_page.work_id = work.id AND lower(")
            .push(format_column)
            .push(") = ANY(")
            .push_bind(exclude)
            .push("::text[]))");
    }
}

fn push_number_filter(
    query: &mut QueryBuilder<Postgres>,
    field: GalleryNumberField,
    comparison: GalleryNumberComparison,
) {
    let expression = match field {
        GalleryNumberField::BookmarkCount => "work.bookmark_count::double precision",
        GalleryNumberField::ViewCount => "work.view_count::double precision",
        GalleryNumberField::LikeCount => "work.like_count::double precision",
        GalleryNumberField::CommentCount => "work.comment_count::double precision",
        GalleryNumberField::PageCount => "revision.page_count::double precision",
        GalleryNumberField::Width => "revision.width::double precision",
        GalleryNumberField::Height => "revision.height::double precision",
        GalleryNumberField::MediaByteSize => {
            "(SELECT coalesce(sum(media_size.byte_size), 0)::double precision FROM work_page AS size_page JOIN media_revision AS media_size ON media_size.work_page_id = size_page.id WHERE size_page.work_id = work.id)"
        }
    };
    query.push(expression);
    match comparison {
        GalleryNumberComparison::Equals(value) => {
            query.push(" = ").push_bind(value);
        }
        GalleryNumberComparison::GreaterThan(value) => {
            query.push(" > ").push_bind(value);
        }
        GalleryNumberComparison::GreaterThanOrEqual(value) => {
            query.push(" >= ").push_bind(value);
        }
        GalleryNumberComparison::LessThan(value) => {
            query.push(" < ").push_bind(value);
        }
        GalleryNumberComparison::LessThanOrEqual(value) => {
            query.push(" <= ").push_bind(value);
        }
        GalleryNumberComparison::Between { min, max } => {
            query
                .push(" BETWEEN ")
                .push_bind(min)
                .push(" AND ")
                .push_bind(max);
        }
    }
}

fn push_date_filter(
    query: &mut QueryBuilder<Postgres>,
    field: GalleryDateField,
    comparison: GalleryDateComparison,
) {
    let expression = match field {
        GalleryDateField::PublishedAt => "revision.pixiv_created_at",
        GalleryDateField::SourceUpdatedAt => "revision.pixiv_updated_at",
        GalleryDateField::LocalUpdatedAt => "work.updated_at",
        GalleryDateField::TrashedAt => "work.trashed_at",
    };
    query.push(expression);
    match comparison {
        GalleryDateComparison::Before { value } => {
            query.push(" < ").push_bind(value);
        }
        GalleryDateComparison::After { value } => {
            query.push(" > ").push_bind(value);
        }
        GalleryDateComparison::Between { start, end } => {
            query
                .push(" BETWEEN ")
                .push_bind(start)
                .push(" AND ")
                .push_bind(end);
        }
    }
}

fn push_boolean_filter(
    query: &mut QueryBuilder<Postgres>,
    field: GalleryBooleanField,
    value: bool,
    current_account_id: Option<Uuid>,
) {
    if !value {
        query.push("NOT (");
    }
    match field {
        GalleryBooleanField::Ugoira => {
            query.push("revision.work_kind = 'ugoira'");
        }
        GalleryBooleanField::HasMedia => {
            query.push(
                "EXISTS (SELECT 1 FROM work_page AS available_page WHERE available_page.work_id = work.id AND available_page.current_media_revision_id IS NOT NULL)",
            );
        }
        GalleryBooleanField::BookmarkedByCurrentAccount => {
            push_current_bookmark_exists(query, current_account_id);
        }
        GalleryBooleanField::AiGenerated => {
            query.push(
                "coalesce(revision.metadata ->> 'ai_classification' = 'ai_generated', false)",
            );
        }
        GalleryBooleanField::OriginalWork => {
            query.push("coalesce((revision.metadata ->> 'is_original')::boolean, false)");
        }
    }
    if !value {
        query.push(")");
    }
}

pub(super) fn push_current_bookmark_exists(
    query: &mut QueryBuilder<Postgres>,
    current_account_id: Option<Uuid>,
) {
    let Some(account_id) = current_account_id else {
        query.push("false");
        return;
    };
    query
        .push(
            "EXISTS (SELECT 1 FROM pixiv_work_bookmark AS current_bookmark WHERE current_bookmark.work_id = work.id AND current_bookmark.pixiv_account_id = ",
        )
        .push_bind(account_id)
        .push(" AND current_bookmark.active = true)");
}

pub(super) fn push_current_bookmark_id(
    query: &mut QueryBuilder<Postgres>,
    current_account_id: Option<Uuid>,
) {
    let Some(account_id) = current_account_id else {
        query.push("NULL::bigint");
        return;
    };
    query
        .push(
            "(SELECT current_bookmark.pixiv_bookmark_id FROM pixiv_work_bookmark AS current_bookmark WHERE current_bookmark.work_id = work.id AND current_bookmark.pixiv_account_id = ",
        )
        .push_bind(account_id)
        .push(" AND current_bookmark.active = true LIMIT 1)");
}

pub(super) fn push_cursor(
    query: &mut QueryBuilder<Postgres>,
    sort_field: GallerySortField,
    sort_direction: SortDirection,
    cursor: Option<&GalleryCursor>,
) -> Result<(), DbError> {
    let Some(cursor) = cursor else {
        return Ok(());
    };
    if cursor.sort_field != sort_field || cursor.sort_direction != sort_direction {
        return Err(DbError::InvalidValue(
            "gallery cursor does not match the selected sort".to_owned(),
        ));
    }
    query.push(" AND ");
    match (sort_field, &cursor.key) {
        (GallerySortField::PixivId, GalleryCursorKey::Integer(value)) => push_value_cursor(
            query,
            "work.pixiv_work_id",
            *value,
            cursor.work_id,
            sort_direction,
        ),
        (GallerySortField::LocalUpdatedAt, GalleryCursorKey::Date(value)) => push_value_cursor(
            query,
            "work.updated_at",
            *value,
            cursor.work_id,
            sort_direction,
        ),
        (GallerySortField::PublishedAt, GalleryCursorKey::Date(value)) => push_nullable_cursor(
            query,
            "revision.pixiv_created_at",
            Some(*value),
            cursor.work_id,
            sort_direction,
        ),
        (GallerySortField::PublishedAt, GalleryCursorKey::Null) => {
            push_nullable_cursor::<OffsetDateTime>(
                query,
                "revision.pixiv_created_at",
                None,
                cursor.work_id,
                sort_direction,
            )
        }
        (GallerySortField::BookmarkCount, GalleryCursorKey::Integer(value)) => {
            push_nullable_cursor(
                query,
                "work.bookmark_count",
                Some(*value),
                cursor.work_id,
                sort_direction,
            )
        }
        (GallerySortField::BookmarkCount, GalleryCursorKey::Null) => push_nullable_cursor::<i64>(
            query,
            "work.bookmark_count",
            None,
            cursor.work_id,
            sort_direction,
        ),
        (GallerySortField::Title, GalleryCursorKey::Text(value)) => push_value_cursor(
            query,
            "lower(revision.title)",
            value.clone(),
            cursor.work_id,
            sort_direction,
        ),
        _ => {
            return Err(DbError::InvalidValue(
                "gallery cursor key does not match the selected sort field".to_owned(),
            ));
        }
    }
    Ok(())
}

fn push_value_cursor<T>(
    query: &mut QueryBuilder<Postgres>,
    expression: &str,
    value: T,
    work_id: Uuid,
    direction: SortDirection,
) where
    T: Send + Sync + for<'q> sqlx::Encode<'q, Postgres> + sqlx::Type<Postgres> + 'static,
{
    query
        .push("(")
        .push(expression)
        .push(", work.id) ")
        .push(comparison_operator(direction))
        .push(" (")
        .push_bind(value)
        .push(", ")
        .push_bind(work_id)
        .push(")");
}

fn push_nullable_cursor<T>(
    query: &mut QueryBuilder<Postgres>,
    expression: &str,
    value: Option<T>,
    work_id: Uuid,
    direction: SortDirection,
) where
    T: Send + Sync + for<'q> sqlx::Encode<'q, Postgres> + sqlx::Type<Postgres> + 'static,
{
    match value {
        Some(value) => {
            query.push("((");
            push_value_cursor(query, expression, value, work_id, direction);
            query.push(") OR ").push(expression).push(" IS NULL)");
        }
        None => {
            query
                .push("(")
                .push(expression)
                .push(" IS NULL AND work.id ")
                .push(comparison_operator(direction))
                .push(" ")
                .push_bind(work_id)
                .push(")");
        }
    }
}

const fn comparison_operator(direction: SortDirection) -> &'static str {
    match direction {
        SortDirection::Ascending => ">",
        SortDirection::Descending => "<",
    }
}

const fn order_direction(direction: SortDirection) -> &'static str {
    match direction {
        SortDirection::Ascending => "ASC",
        SortDirection::Descending => "DESC",
    }
}

pub(super) fn push_order(
    query: &mut QueryBuilder<Postgres>,
    sort_field: GallerySortField,
    sort_direction: SortDirection,
) {
    query.push(" ORDER BY ");
    let expression = match sort_field {
        GallerySortField::PixivId => "work.pixiv_work_id",
        GallerySortField::LocalUpdatedAt => "work.updated_at",
        GallerySortField::PublishedAt => "revision.pixiv_created_at",
        GallerySortField::BookmarkCount => "work.bookmark_count",
        GallerySortField::Title => "lower(revision.title)",
    };
    query
        .push(expression)
        .push(" ")
        .push(order_direction(sort_direction));
    if matches!(
        sort_field,
        GallerySortField::PublishedAt | GallerySortField::BookmarkCount
    ) {
        query.push(" NULLS LAST");
    }
    query
        .push(", work.id ")
        .push(order_direction(sort_direction));
}
