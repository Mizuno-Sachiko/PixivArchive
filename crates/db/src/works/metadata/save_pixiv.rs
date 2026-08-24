use super::SavePixivWorkMetadata;
use crate::DbError;
use crate::works::model::WorkRevisionSourceInput;
use pixivarchive_domain::{
    job::{CollectionState, WorkSummary},
    pixiv::{PixivBookmarkVisibility, PixivWorkKind},
};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction, types::Json};
use uuid::Uuid;

type MetadataTransaction<'a> = Transaction<'a, Postgres>;

pub(super) struct PreparedPixivMetadata {
    page_count: i32,
    bookmark_count: i64,
    view_count: i64,
    like_count: i64,
    comment_count: i64,
    revision_basis: Value,
    metadata: Value,
}

impl PreparedPixivMetadata {
    pub(super) fn new(input: &SavePixivWorkMetadata) -> Result<Self, DbError> {
        validate_pixiv_metadata(input)?;
        let detail = &input.detail;
        let page_count = i32::try_from(input.pages.pages.len())
            .map_err(|_| DbError::InvalidValue("Pixiv page count is too large".to_owned()))?;
        let revision_basis = json!({
            "title": detail.title,
            "description": detail.description,
            "kind": detail.kind,
            "age_rating": detail.age_rating,
            "ai_classification": detail.ai_classification,
            "is_original": detail.is_original,
            "artist": detail.artist,
            "published_at": detail.published_at,
            "updated_at": detail.updated_at,
            "tags": detail.tags,
            "page_count": page_count,
            "dimensions": detail.dimensions,
            "series": detail.series,
            "pages": input.pages.pages,
            "ugoira": input.ugoira,
        });
        let metadata = json!({
            "ai_classification": detail.ai_classification,
            "is_original": detail.is_original,
            "bookmark": detail.bookmark,
            "bookmarked_by_current_account": detail.bookmarked_by_current_account,
            "series_order": detail.series.as_ref().and_then(|series| series.order),
            "pages": input.pages.pages,
            "ugoira": input.ugoira,
            "provenance": input.provenance,
            "revision_basis": revision_basis,
        });
        Ok(Self {
            page_count,
            bookmark_count: count_to_i64(detail.counts.bookmarks, "bookmark")?,
            view_count: count_to_i64(detail.counts.views, "view")?,
            like_count: count_to_i64(detail.counts.likes, "like")?,
            comment_count: count_to_i64(detail.counts.comments, "comment")?,
            revision_basis,
            metadata,
        })
    }
}

pub(super) struct StoredWork {
    id: Uuid,
    collection_state: String,
    resource_revision: i64,
    previous_basis: Option<Value>,
}

impl StoredWork {
    pub(super) fn id(&self) -> Uuid {
        self.id
    }

    pub(super) fn resource_revision(&self) -> i64 {
        self.resource_revision
    }

    pub(super) fn into_summary(self, pixiv_id: i64) -> Result<WorkSummary, DbError> {
        let collection_state =
            CollectionState::from_db_value(&self.collection_state).ok_or_else(|| {
                DbError::InvalidValue(format!("unknown work state {}", self.collection_state))
            })?;
        Ok(WorkSummary {
            id: self.id,
            pixiv_id,
            collection_state,
            resource_revision: self.resource_revision,
        })
    }
}

pub(super) async fn ensure_not_deleted(
    tx: &mut MetadataTransaction<'_>,
    pixiv_work_id: i64,
) -> Result<(), DbError> {
    let blocked: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM deletion_marker WHERE pixiv_work_id = $1)")
            .bind(pixiv_work_id)
            .fetch_one(&mut **tx)
            .await?;
    if blocked {
        return Err(DbError::Constraint(format!(
            "pixiv work {pixiv_work_id} has a deletion marker"
        )));
    }
    Ok(())
}

pub(super) async fn upsert_artist(
    tx: &mut MetadataTransaction<'_>,
    input: &SavePixivWorkMetadata,
) -> Result<Uuid, DbError> {
    Ok(sqlx::query_scalar(
        r#"
        INSERT INTO artist (id, pixiv_artist_id, name, account_name)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (pixiv_artist_id)
        DO UPDATE SET name = excluded.name,
                      account_name = excluded.account_name,
                      updated_at = now(),
                      revision = artist.revision + 1
        RETURNING id
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(input.detail.artist.pixiv_id)
    .bind(&input.detail.artist.name)
    .bind(&input.detail.artist.account_name)
    .fetch_one(&mut **tx)
    .await?)
}

pub(super) async fn upsert_series(
    tx: &mut MetadataTransaction<'_>,
    input: &SavePixivWorkMetadata,
) -> Result<Option<Uuid>, DbError> {
    let Some(series) = &input.detail.series else {
        return Ok(None);
    };
    Ok(Some(
        sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO series (id, pixiv_series_id, title)
            VALUES ($1, $2, $3)
            ON CONFLICT (pixiv_series_id)
            DO UPDATE SET title = excluded.title,
                          updated_at = now()
            RETURNING id
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(series.pixiv_id)
        .bind(&series.title)
        .fetch_one(&mut **tx)
        .await?,
    ))
}

pub(super) async fn upsert_work(
    tx: &mut MetadataTransaction<'_>,
    input: &SavePixivWorkMetadata,
    prepared: &PreparedPixivMetadata,
    artist_id: Uuid,
    series_id: Option<Uuid>,
) -> Result<StoredWork, DbError> {
    let existing = sqlx::query(
        r#"
        SELECT work.id,
               work.collection_state,
               work.resource_revision,
               work_revision.metadata -> 'revision_basis' AS revision_basis
        FROM work
        LEFT JOIN work_revision ON work_revision.id = work.current_revision_id
        WHERE work.pixiv_work_id = $1
        FOR UPDATE OF work
        "#,
    )
    .bind(input.detail.work_id)
    .fetch_optional(&mut **tx)
    .await?;

    if let Some(row) = existing {
        let work_id: Uuid = row.get("id");
        let updated = sqlx::query(
            r#"
            UPDATE work
            SET artist_id = $2,
                series_id = $3,
                source_state = 'present',
                bookmark_count = $4,
                view_count = $5,
                like_count = $6,
                comment_count = $7,
                last_collected_at = now(),
                updated_at = now(),
                resource_revision = resource_revision + 1
            WHERE id = $1
            RETURNING collection_state, resource_revision
            "#,
        )
        .bind(work_id)
        .bind(artist_id)
        .bind(series_id)
        .bind(prepared.bookmark_count)
        .bind(prepared.view_count)
        .bind(prepared.like_count)
        .bind(prepared.comment_count)
        .fetch_one(&mut **tx)
        .await?;
        return Ok(StoredWork {
            id: work_id,
            collection_state: updated.get("collection_state"),
            resource_revision: updated.get("resource_revision"),
            previous_basis: row
                .get::<Option<Json<Value>>, _>("revision_basis")
                .map(|value| value.0),
        });
    }

    let work_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO work (
            id,
            pixiv_work_id,
            artist_id,
            series_id,
            collection_state,
            source_state,
            bookmark_count,
            view_count,
            like_count,
            comment_count,
            last_collected_at
        )
        VALUES (
            $1, $2, $3, $4, 'metadata_only', 'present',
            $5, $6, $7, $8, now()
        )
        "#,
    )
    .bind(work_id)
    .bind(input.detail.work_id)
    .bind(artist_id)
    .bind(series_id)
    .bind(prepared.bookmark_count)
    .bind(prepared.view_count)
    .bind(prepared.like_count)
    .bind(prepared.comment_count)
    .execute(&mut **tx)
    .await?;
    Ok(StoredWork {
        id: work_id,
        collection_state: "metadata_only".to_owned(),
        resource_revision: 1,
        previous_basis: None,
    })
}

pub(super) async fn store_current_revision(
    tx: &mut MetadataTransaction<'_>,
    input: &SavePixivWorkMetadata,
    prepared: &PreparedPixivMetadata,
    work: &StoredWork,
) -> Result<Option<Uuid>, DbError> {
    if work.previous_basis.as_ref() == Some(&prepared.revision_basis) {
        return Ok(None);
    }
    let detail = &input.detail;
    let revision_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO work_revision (
            id,
            work_id,
            title,
            caption,
            work_kind,
            page_count,
            width,
            height,
            sanity_level,
            pixiv_created_at,
            pixiv_updated_at,
            metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
    )
    .bind(revision_id)
    .bind(work.id)
    .bind(&detail.title)
    .bind(&detail.description)
    .bind(detail.kind.as_str())
    .bind(prepared.page_count)
    .bind(
        i32::try_from(detail.dimensions.width)
            .map_err(|_| DbError::InvalidValue("Pixiv work width is too large".to_owned()))?,
    )
    .bind(
        i32::try_from(detail.dimensions.height)
            .map_err(|_| DbError::InvalidValue("Pixiv work height is too large".to_owned()))?,
    )
    .bind(detail.age_rating.as_str())
    .bind(detail.published_at)
    .bind(detail.updated_at)
    .bind(Json(&prepared.metadata))
    .execute(&mut **tx)
    .await?;
    sqlx::query("UPDATE work SET current_revision_id = $2 WHERE id = $1")
        .bind(work.id)
        .bind(revision_id)
        .execute(&mut **tx)
        .await?;
    Ok(Some(revision_id))
}

pub(super) async fn insert_work_revision_source(
    tx: &mut MetadataTransaction<'_>,
    revision_id: Uuid,
    source: &WorkRevisionSourceInput,
) -> Result<(), DbError> {
    sqlx::query(
        r#"
        INSERT INTO work_revision_source (
            id,
            work_revision_id,
            subscription_id,
            subscription_run_id,
            subscription_name,
            pixiv_user_id
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (work_revision_id, subscription_run_id) DO NOTHING
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(revision_id)
    .bind(source.subscription_id)
    .bind(source.subscription_run_id)
    .bind(&source.subscription_name)
    .bind(source.pixiv_user_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(super) async fn replace_tags(
    tx: &mut MetadataTransaction<'_>,
    input: &SavePixivWorkMetadata,
    work_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query("DELETE FROM work_tag WHERE work_id = $1")
        .bind(work_id)
        .execute(&mut **tx)
        .await?;
    for source_tag in &input.detail.tags {
        let tag_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO tag (id, raw_name, translated_name)
            VALUES ($1, $2, $3)
            ON CONFLICT (lower(btrim(raw_name)))
            DO UPDATE SET translated_name =
                COALESCE(excluded.translated_name, tag.translated_name)
            RETURNING id
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(source_tag.name.trim())
        .bind(&source_tag.translated_name)
        .fetch_one(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO work_tag (work_id, tag_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(work_id)
        .bind(tag_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub(super) async fn update_bookmark(
    tx: &mut MetadataTransaction<'_>,
    input: &SavePixivWorkMetadata,
    work_id: Uuid,
) -> Result<(), DbError> {
    let (Some(account_id), Some(bookmarked)) =
        (input.account_id, input.detail.bookmarked_by_current_account)
    else {
        return Ok(());
    };
    if bookmarked {
        let bookmark_id = input
            .detail
            .bookmark
            .as_ref()
            .map(|bookmark| bookmark.bookmark_id);
        let visibility = input
            .detail
            .bookmark
            .as_ref()
            .map(|bookmark| bookmark.visibility)
            .unwrap_or(PixivBookmarkVisibility::Public);
        sqlx::query(
            r#"
            INSERT INTO pixiv_work_bookmark (
                pixiv_account_id, work_id, pixiv_bookmark_id, visibility, active
            )
            VALUES ($1, $2, $3, $4, true)
            ON CONFLICT (pixiv_account_id, work_id)
            DO UPDATE SET pixiv_bookmark_id = excluded.pixiv_bookmark_id,
                          visibility = excluded.visibility,
                          active = true,
                          last_seen_at = now(),
                          updated_at = now()
            "#,
        )
        .bind(account_id)
        .bind(work_id)
        .bind(bookmark_id)
        .bind(match visibility {
            PixivBookmarkVisibility::Public => "public",
            PixivBookmarkVisibility::Private => "private",
        })
        .execute(&mut **tx)
        .await?;
    } else {
        sqlx::query(
            r#"
            UPDATE pixiv_work_bookmark
            SET active = false,
                last_seen_at = now(),
                updated_at = now()
            WHERE pixiv_account_id = $1
              AND work_id = $2
            "#,
        )
        .bind(account_id)
        .bind(work_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub(super) async fn sync_pages(
    tx: &mut MetadataTransaction<'_>,
    input: &SavePixivWorkMetadata,
    prepared: &PreparedPixivMetadata,
    work_id: Uuid,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE work_page SET source_state = 'deleted' WHERE work_id = $1 AND page_index >= $2",
    )
    .bind(work_id)
    .bind(prepared.page_count)
    .execute(&mut **tx)
    .await?;
    for page in &input.pages.pages {
        sqlx::query(
            r#"
            INSERT INTO work_page (
                id, work_id, page_index, source_url, width, height, source_state
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'present')
            ON CONFLICT (work_id, page_index)
            DO UPDATE SET source_url = excluded.source_url,
                          width = excluded.width,
                          height = excluded.height,
                          source_state = 'present'
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(work_id)
        .bind(
            i32::try_from(page.page_index)
                .map_err(|_| DbError::InvalidValue("Pixiv page index is too large".to_owned()))?,
        )
        .bind(page.original_url.as_str())
        .bind(
            i32::try_from(page.dimensions.width)
                .map_err(|_| DbError::InvalidValue("Pixiv page width is too large".to_owned()))?,
        )
        .bind(
            i32::try_from(page.dimensions.height)
                .map_err(|_| DbError::InvalidValue("Pixiv page height is too large".to_owned()))?,
        )
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn validate_pixiv_metadata(input: &SavePixivWorkMetadata) -> Result<(), DbError> {
    if input.detail.work_id <= 0
        || input.pages.work_id != input.detail.work_id
        || input.pages.pages.is_empty()
        || input
            .ugoira
            .as_ref()
            .is_some_and(|ugoira| ugoira.work_id != input.detail.work_id)
    {
        return Err(DbError::InvalidValue(
            "Pixiv work metadata identifiers or pages are invalid".to_owned(),
        ));
    }
    if input.detail.kind == PixivWorkKind::Ugoira && input.ugoira.is_none() {
        return Err(DbError::InvalidValue(
            "Pixiv ugoira metadata is missing".to_owned(),
        ));
    }
    Ok(())
}

fn count_to_i64(value: u64, name: &str) -> Result<i64, DbError> {
    i64::try_from(value)
        .map_err(|_| DbError::InvalidValue(format!("Pixiv {name} count is too large")))
}
