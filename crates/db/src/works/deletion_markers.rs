use super::{WorkRepository, metadata::lock_pixiv_work};
use crate::{DbError, EventRepository};
use pixivarchive_domain::event::{EventPayload, EventResource};
use uuid::Uuid;

impl WorkRepository {
    pub async fn mark_physically_deleted(
        &self,
        pixiv_work_id: i64,
        deletion_method: &str,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        lock_pixiv_work(&mut tx, pixiv_work_id).await?;
        let deleted = sqlx::query!(
            "DELETE FROM work WHERE pixiv_work_id = $1 RETURNING id, resource_revision",
            pixiv_work_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        let marker = sqlx::query!(
            r#"
            INSERT INTO deletion_marker (id, pixiv_work_id, deletion_method)
            VALUES ($1, $2, $3)
            ON CONFLICT (pixiv_work_id)
            DO UPDATE SET deleted_at = now(),
                          deletion_method = excluded.deletion_method
            RETURNING id
            "#,
            Uuid::now_v7(),
            pixiv_work_id,
            deletion_method
        )
        .fetch_one(&mut *tx)
        .await?;
        if let Some(deleted) = deleted {
            EventRepository::new(self.db.clone())
                .append_in_tx(
                    &mut tx,
                    EventResource::Work,
                    deleted.id,
                    EventPayload::WorkDeleted {
                        revision: deleted.resource_revision + 1,
                    },
                )
                .await?;
        }
        EventRepository::new(self.db.clone())
            .append_in_tx(
                &mut tx,
                EventResource::DeletionMarker,
                marker.id,
                EventPayload::DeletionMarkerCreated {
                    pixiv_work_id,
                    deletion_method: deletion_method.to_owned(),
                },
            )
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn remove_deletion_marker_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        pixiv_work_id: i64,
    ) -> Result<bool, DbError> {
        let marker = sqlx::query!(
            "DELETE FROM deletion_marker WHERE pixiv_work_id = $1 RETURNING id",
            pixiv_work_id
        )
        .fetch_optional(&mut **tx)
        .await?;
        let Some(marker) = marker else {
            return Ok(false);
        };

        EventRepository::new(self.db.clone())
            .append_in_tx(
                tx,
                EventResource::DeletionMarker,
                marker.id,
                EventPayload::DeletionMarkerRemoved { pixiv_work_id },
            )
            .await?;
        Ok(true)
    }

    pub async fn deletion_marker_exists(&self, pixiv_work_id: i64) -> Result<bool, DbError> {
        let exists = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM deletion_marker WHERE pixiv_work_id = $1)",
            pixiv_work_id
        )
        .fetch_one(self.db.pool())
        .await?;
        Ok(exists.unwrap_or(false))
    }
}
