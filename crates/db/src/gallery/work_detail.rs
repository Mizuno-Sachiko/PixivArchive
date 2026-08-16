use super::{
    GalleryRepository,
    model::{
        non_negative_u32, optional_positive_u32, parse_enum_value, positive_u64,
        required_optional_string,
    },
    query::{GalleryScope, push_collection_scope},
};
use crate::{DbError, works::load_trash_action_capabilities};
use pixivarchive_domain::{
    job::CollectionState,
    media::{DerivativeFormat, MediaFormat, MediaKind},
    pixiv::{PixivUgoiraMeta, PixivWorkKind},
    work::{
        FilterMode, GalleryDerivative, GalleryFilter, GalleryFilterGroup, GalleryMediaRevision,
        GalleryPage, GallerySearch, GalleryWorkDetail, WorkRevisionSummary, WorkSourceState,
    },
};
use serde_json::Value;
use sqlx::{Postgres, QueryBuilder, Row};
use std::collections::HashMap;
use uuid::Uuid;

impl GalleryRepository {
    pub async fn work_detail(
        &self,
        work_id: Uuid,
        current_account_id: Option<Uuid>,
    ) -> Result<GalleryWorkDetail, DbError> {
        let mut result = self
            .search_stored_work(
                GallerySearch {
                    groups: vec![GalleryFilterGroup {
                        mode: FilterMode::All,
                        filters: vec![GalleryFilter::WorkId { value: work_id }],
                    }],
                    limit: 1,
                    ..GallerySearch::default()
                },
                current_account_id,
            )
            .await?;
        let work = result.items.pop().ok_or(DbError::NotFound)?;
        let trash_capabilities = if work.collection_state == CollectionState::Trash {
            Some(
                load_trash_action_capabilities(self.db.pool(), work_id)
                    .await?
                    .ok_or_else(|| {
                        DbError::Constraint(format!(
                            "trash work {work_id} has no matching trash entry"
                        ))
                    })?,
            )
        } else {
            None
        };
        let ugoira = sqlx::query_scalar::<_, Option<Value>>(
            r#"
            SELECT revision.metadata -> 'ugoira'
            FROM work
            JOIN work_revision AS revision ON revision.id = work.current_revision_id
            WHERE work.id = $1
            "#,
        )
        .bind(work_id)
        .fetch_one(self.db.pool())
        .await?
        .filter(|value| !value.is_null())
        .map(serde_json::from_value::<PixivUgoiraMeta>)
        .transpose()
        .map_err(|error| DbError::InvalidValue(format!("invalid Ugoira metadata: {error}")))?;
        let rows = sqlx::query(
            r#"
            SELECT
                work_page.id,
                work_page.page_index,
                work_page.source_state,
                work_page.source_url,
                work_page.width,
                work_page.height,
                media_revision.id AS media_id,
                media_revision.revision_number,
                media_revision.media_kind,
                media_revision.format,
                media_revision.source_path,
                media_revision.byte_size,
                media_revision.sha256
            FROM work_page
            LEFT JOIN media_revision
                ON media_revision.id = work_page.current_media_revision_id
            WHERE work_page.work_id = $1
            ORDER BY work_page.page_index
            "#,
        )
        .bind(work_id)
        .fetch_all(self.db.pool())
        .await?;
        let media_ids = rows
            .iter()
            .filter_map(|row| row.get::<Option<Uuid>, _>("media_id"))
            .collect::<Vec<_>>();
        let mut derivatives = self.load_derivatives(&media_ids).await?;
        let mut pages = Vec::with_capacity(rows.len());
        for row in rows {
            let media_id = row.get::<Option<Uuid>, _>("media_id");
            let current_media = media_id
                .map(|id| {
                    Ok::<_, DbError>(GalleryMediaRevision {
                        id,
                        revision_number: positive_u64(
                            row.get::<Option<i64>, _>("revision_number")
                                .ok_or_else(|| {
                                    DbError::InvalidValue(
                                        "current media revision number is missing".to_owned(),
                                    )
                                })?,
                            "media revision number",
                        )?,
                        media_kind: parse_enum_value(
                            required_optional_string(&row, "media_kind")?,
                            "media kind",
                            MediaKind::from_db_value,
                        )?,
                        format: parse_enum_value(
                            required_optional_string(&row, "format")?,
                            "media format",
                            MediaFormat::from_db_value,
                        )?,
                        source_path: required_optional_string(&row, "source_path")?,
                        byte_size: positive_u64(
                            row.get::<Option<i64>, _>("byte_size").ok_or_else(|| {
                                DbError::InvalidValue(
                                    "current media byte size is missing".to_owned(),
                                )
                            })?,
                            "media byte size",
                        )?,
                        sha256: row.get::<Option<Vec<u8>>, _>("sha256").ok_or_else(|| {
                            DbError::InvalidValue("current media SHA-256 is missing".to_owned())
                        })?,
                        derivatives: derivatives.remove(&id).unwrap_or_default(),
                    })
                })
                .transpose()?;
            pages.push(GalleryPage {
                id: row.get("id"),
                page_index: non_negative_u32(row.get("page_index"), "page index")?,
                source_state: parse_enum_value(
                    row.get("source_state"),
                    "source state",
                    WorkSourceState::from_db_value,
                )?,
                source_url: row.get("source_url"),
                width: optional_positive_u32(row.get("width"), "page width")?,
                height: optional_positive_u32(row.get("height"), "page height")?,
                current_media,
            });
        }
        Ok(GalleryWorkDetail {
            work,
            pages,
            ugoira,
            trash_capabilities,
        })
    }

    pub async fn work_id_by_pixiv_id(&self, pixiv_work_id: i64) -> Result<Uuid, DbError> {
        super::validate_source_id(pixiv_work_id, "Pixiv work ID")?;
        let mut query =
            QueryBuilder::<Postgres>::new("SELECT work.id FROM work WHERE work.pixiv_work_id = ");
        query.push_bind(pixiv_work_id);
        push_collection_scope(&mut query, GalleryScope::AddressableDetail);
        query
            .build_query_scalar()
            .fetch_optional(self.db.pool())
            .await?
            .ok_or(DbError::NotFound)
    }

    pub async fn revisions(&self, work_id: Uuid) -> Result<Vec<WorkRevisionSummary>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT id, title, caption, work_kind, page_count, captured_at
            FROM work_revision
            WHERE work_id = $1
            ORDER BY captured_at DESC, id DESC
            "#,
        )
        .bind(work_id)
        .fetch_all(self.db.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(WorkRevisionSummary {
                    id: row.get("id"),
                    title: row.get("title"),
                    description: row.get("caption"),
                    work_kind: parse_enum_value(
                        row.get("work_kind"),
                        "work kind",
                        PixivWorkKind::from_db_value,
                    )?,
                    page_count: non_negative_u32(row.get("page_count"), "revision page count")?,
                    captured_at: row.get("captured_at"),
                })
            })
            .collect()
    }

    async fn load_derivatives(
        &self,
        media_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Vec<GalleryDerivative>>, DbError> {
        if media_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = sqlx::query(
            r#"
            SELECT
                media_revision_id,
                id,
                derivative_kind,
                format,
                path,
                width,
                height,
                byte_size,
                dominant_color
            FROM derivative
            WHERE media_revision_id = ANY($1)
            ORDER BY media_revision_id, derivative_kind, format
            "#,
        )
        .bind(media_ids)
        .fetch_all(self.db.pool())
        .await?;
        let mut derivatives = HashMap::<Uuid, Vec<GalleryDerivative>>::new();
        for row in rows {
            derivatives
                .entry(row.get("media_revision_id"))
                .or_default()
                .push(GalleryDerivative {
                    id: row.get("id"),
                    kind: row.get("derivative_kind"),
                    format: parse_enum_value(
                        row.get("format"),
                        "derivative format",
                        DerivativeFormat::from_db_value,
                    )?,
                    path: row.get("path"),
                    width: non_negative_u32(row.get("width"), "derivative width")?,
                    height: non_negative_u32(row.get("height"), "derivative height")?,
                    byte_size: positive_u64(row.get("byte_size"), "derivative byte size")?,
                    dominant_color: row.get("dominant_color"),
                });
        }
        Ok(derivatives)
    }
}
