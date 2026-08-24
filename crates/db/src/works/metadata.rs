use super::{SavePixivWorkMetadata, WorkRepository};
use crate::{DbError, EventRepository};
use pixivarchive_domain::{
    event::{EventPayload, EventResource},
    job::{CollectionState, JobLease, WorkSummary},
    work::WorkSourceState,
};
use sqlx::Row;
use uuid::Uuid;

mod save_pixiv;

use save_pixiv::{
    PreparedPixivMetadata, ensure_not_deleted, insert_work_revision_source, replace_tags,
    store_current_revision, sync_pages, update_bookmark, upsert_artist, upsert_series, upsert_work,
};

impl WorkRepository {
    pub async fn create_metadata_only(
        &self,
        pixiv_work_id: i64,
        pixiv_artist_id: i64,
        title: &str,
    ) -> Result<WorkSummary, DbError> {
        let mut tx = self.db.begin().await?;
        lock_pixiv_work(&mut tx, pixiv_work_id).await?;
        let blocked = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM deletion_marker WHERE pixiv_work_id = $1)",
            pixiv_work_id
        )
        .fetch_one(&mut *tx)
        .await?;
        if blocked.unwrap_or(false) {
            return Err(DbError::Constraint(format!(
                "pixiv work {pixiv_work_id} has a deletion marker"
            )));
        }

        let artist = sqlx::query!(
            r#"
            INSERT INTO artist (id, pixiv_artist_id, name)
            VALUES ($1, $2, $3)
            ON CONFLICT (pixiv_artist_id)
            DO UPDATE SET updated_at = now()
            RETURNING id
            "#,
            Uuid::now_v7(),
            pixiv_artist_id,
            format!("pixiv:{pixiv_artist_id}")
        )
        .fetch_one(&mut *tx)
        .await?;

        let work_id = Uuid::now_v7();
        let revision_id = Uuid::now_v7();
        sqlx::query!(
            r#"
            INSERT INTO work (
                id, pixiv_work_id, artist_id, collection_state, source_state, last_collected_at
            )
            VALUES ($1, $2, $3, 'metadata_only', 'present', now())
            "#,
            work_id,
            pixiv_work_id,
            artist.id
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO work_revision (id, work_id, title, work_kind, page_count, sanity_level)
            VALUES ($1, $2, $3, 'illustration', 1, 'unknown')
            "#,
            revision_id,
            work_id,
            title
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            "UPDATE work SET current_revision_id = $2 WHERE id = $1",
            work_id,
            revision_id
        )
        .execute(&mut *tx)
        .await?;
        EventRepository::new(self.db.clone())
            .append_in_tx(
                &mut tx,
                EventResource::Work,
                work_id,
                EventPayload::WorkChanged { revision: 1 },
            )
            .await?;
        tx.commit().await?;

        Ok(WorkSummary {
            id: work_id,
            pixiv_id: pixiv_work_id,
            collection_state: CollectionState::MetadataOnly,
            resource_revision: 1,
        })
    }

    pub async fn mark_source_state(
        &self,
        pixiv_work_id: i64,
        source_state: WorkSourceState,
    ) -> Result<bool, DbError> {
        self.mark_source_state_with_lease(None, pixiv_work_id, source_state)
            .await
    }

    pub async fn mark_source_state_for_job(
        &self,
        lease: JobLease,
        pixiv_work_id: i64,
        source_state: WorkSourceState,
    ) -> Result<bool, DbError> {
        self.mark_source_state_with_lease(Some(lease), pixiv_work_id, source_state)
            .await
    }

    async fn mark_source_state_with_lease(
        &self,
        lease: Option<JobLease>,
        pixiv_work_id: i64,
        source_state: WorkSourceState,
    ) -> Result<bool, DbError> {
        let mut tx = self.db.begin().await?;
        if let Some(lease) = lease {
            crate::JobRepository::new(self.db.clone())
                .lock_active_lease_in_tx(&mut tx, lease)
                .await?;
        }
        lock_pixiv_work(&mut tx, pixiv_work_id).await?;
        let updated = sqlx::query(
            r#"
            UPDATE work
            SET source_state = $2,
                updated_at = now(),
                resource_revision = resource_revision + 1
            WHERE pixiv_work_id = $1
              AND source_state <> $2
            RETURNING id, resource_revision
            "#,
        )
        .bind(pixiv_work_id)
        .bind(source_state.as_str())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(updated) = updated else {
            tx.commit().await?;
            return Ok(false);
        };
        EventRepository::new(self.db.clone())
            .append_in_tx(
                &mut tx,
                EventResource::Work,
                updated.try_get("id")?,
                EventPayload::WorkChanged {
                    revision: updated.try_get("resource_revision")?,
                },
            )
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn save_pixiv_metadata(
        &self,
        input: SavePixivWorkMetadata,
    ) -> Result<WorkSummary, DbError> {
        self.save_pixiv_metadata_with_lease(None, input, DeletionMarkerPolicy::Reject)
            .await
    }

    pub async fn save_pixiv_metadata_for_job(
        &self,
        lease: JobLease,
        input: SavePixivWorkMetadata,
    ) -> Result<WorkSummary, DbError> {
        self.save_pixiv_metadata_with_lease(Some(lease), input, DeletionMarkerPolicy::Reject)
            .await
    }

    pub async fn save_reimported_pixiv_metadata(
        &self,
        input: SavePixivWorkMetadata,
    ) -> Result<WorkSummary, DbError> {
        self.save_pixiv_metadata_with_lease(None, input, DeletionMarkerPolicy::Remove)
            .await
    }

    pub async fn save_reimported_pixiv_metadata_for_job(
        &self,
        lease: JobLease,
        input: SavePixivWorkMetadata,
    ) -> Result<WorkSummary, DbError> {
        self.save_pixiv_metadata_with_lease(Some(lease), input, DeletionMarkerPolicy::Remove)
            .await
    }

    async fn save_pixiv_metadata_with_lease(
        &self,
        lease: Option<JobLease>,
        input: SavePixivWorkMetadata,
        deletion_marker_policy: DeletionMarkerPolicy,
    ) -> Result<WorkSummary, DbError> {
        let prepared = PreparedPixivMetadata::new(&input)?;
        let mut tx = self.db.begin().await?;
        if let Some(lease) = lease {
            crate::JobRepository::new(self.db.clone())
                .lock_active_lease_in_tx(&mut tx, lease)
                .await?;
        }
        lock_pixiv_work(&mut tx, input.detail.work_id).await?;
        match deletion_marker_policy {
            DeletionMarkerPolicy::Reject => {
                ensure_not_deleted(&mut tx, input.detail.work_id).await?;
            }
            DeletionMarkerPolicy::Remove => {
                self.remove_deletion_marker_in_tx(&mut tx, input.detail.work_id)
                    .await?;
            }
        }
        let artist_id = upsert_artist(&mut tx, &input).await?;
        let series_id = upsert_series(&mut tx, &input).await?;
        let work = upsert_work(&mut tx, &input, &prepared, artist_id, series_id).await?;
        if let Some(revision_id) = store_current_revision(&mut tx, &input, &prepared, &work).await?
            && let Some(source) = input.revision_source.as_ref()
        {
            insert_work_revision_source(&mut tx, revision_id, source).await?;
        }
        replace_tags(&mut tx, &input, work.id()).await?;
        update_bookmark(&mut tx, &input, work.id()).await?;
        sync_pages(&mut tx, &input, &prepared, work.id()).await?;
        EventRepository::new(self.db.clone())
            .append_in_tx(
                &mut tx,
                EventResource::Work,
                work.id(),
                EventPayload::WorkChanged {
                    revision: work.resource_revision(),
                },
            )
            .await?;
        tx.commit().await?;
        work.into_summary(input.detail.work_id)
    }

    pub async fn find_by_pixiv_id(
        &self,
        pixiv_work_id: i64,
    ) -> Result<Option<WorkSummary>, DbError> {
        let row = sqlx::query!(
            r#"
            SELECT id, pixiv_work_id, collection_state, resource_revision
            FROM work
            WHERE pixiv_work_id = $1
            "#,
            pixiv_work_id
        )
        .fetch_optional(self.db.pool())
        .await?;

        row.map(|row| {
            work_from_row(
                row.id,
                row.pixiv_work_id,
                row.collection_state,
                row.resource_revision,
            )
        })
        .transpose()
    }
}

#[derive(Clone, Copy)]
enum DeletionMarkerPolicy {
    Reject,
    Remove,
}

pub(super) async fn lock_pixiv_work(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    pixiv_work_id: i64,
) -> Result<(), DbError> {
    sqlx::query!("SELECT pg_advisory_xact_lock($1)", pixiv_work_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn work_from_row(
    id: Uuid,
    pixiv_id: i64,
    collection_state: String,
    resource_revision: i64,
) -> Result<WorkSummary, DbError> {
    let collection_state = CollectionState::from_db_value(&collection_state)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown work state {collection_state}")))?;
    Ok(WorkSummary {
        id,
        pixiv_id,
        collection_state,
        resource_revision,
    })
}
