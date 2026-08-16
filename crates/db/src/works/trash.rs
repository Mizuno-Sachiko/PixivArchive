use std::collections::HashSet;

use super::WorkRepository;
use crate::{
    DbError, EventRepository,
    gallery::{context_selected_work_query, matching_work_query, push_selection_state},
};
use pixivarchive_domain::{
    event::{EventPayload, EventResource},
    work::{
        GalleryContextSelectionExpression, GallerySelectionExpression, TrashActionBlockReason,
        TrashActionCapabilities, TrashCollectionSummary, TrashCursor, TrashEntry, TrashFilter,
        TrashPage, TrashSelectionExpression, TrashSelectionMutation, TrashSelectionProjection,
        TrashWorkSummary,
    },
};
use sqlx::{PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

impl WorkRepository {
    pub async fn move_selection_to_trash(
        &self,
        expression: &GallerySelectionExpression,
        current_account_id: Option<Uuid>,
        scheduled_purge_at: OffsetDateTime,
    ) -> Result<u64, DbError> {
        let mut query = matching_work_query(
            r#"
            WITH target AS (
                SELECT work.id,
                       work.collection_state AS previous_collection_state
            "#,
            &expression.search,
            current_account_id,
        );
        query.push(" AND (");
        push_selection_state(&mut query, expression, current_account_id);
        query.push(") FOR UPDATE OF work)");
        self.execute_bulk_trash_transition(query, scheduled_purge_at)
            .await
    }

    pub async fn move_context_selection_to_trash(
        &self,
        expression: &GalleryContextSelectionExpression,
        scheduled_purge_at: OffsetDateTime,
    ) -> Result<u64, DbError> {
        let mut query = context_selected_work_query(
            r#"
            , target AS (
                SELECT work.id,
                       work.collection_state AS previous_collection_state
            "#,
            expression,
        );
        query.push(" FOR UPDATE OF work)");
        self.execute_bulk_trash_transition(query, scheduled_purge_at)
            .await
    }

    async fn execute_bulk_trash_transition(
        &self,
        mut query: sqlx::QueryBuilder<Postgres>,
        scheduled_purge_at: OffsetDateTime,
    ) -> Result<u64, DbError> {
        let mut tx = self.db.begin().await?;
        query.push(
            r#"
            ,
            updated AS (
                UPDATE work AS selected_work
                SET collection_state = 'trash',
                    trashed_at = now(),
                    updated_at = now(),
                    resource_revision = selected_work.resource_revision + 1
                FROM target
                WHERE selected_work.id = target.id
                RETURNING selected_work.id,
                          target.previous_collection_state,
                          selected_work.resource_revision
            ),
            upserted_trash AS (
                INSERT INTO trash_entry (
                    work_id,
                    previous_collection_state,
                    scheduled_purge_at,
                    purge_state
                )
                SELECT updated.id,
                       updated.previous_collection_state,
            "#,
        );
        query.push_bind(scheduled_purge_at);
        query.push(
            r#",
                       'pending'
                FROM updated
                WHERE true
                ON CONFLICT (work_id)
                DO UPDATE SET scheduled_purge_at = excluded.scheduled_purge_at,
                              purge_state = 'pending',
                              failure_message = NULL,
                              failure_details = '[]'::jsonb
                RETURNING work_id
            ),
            inserted_events AS (
                INSERT INTO app_event (resource, resource_id, payload)
                SELECT 'work',
                       updated.id,
                       jsonb_build_object(
                           'type', 'work_changed',
                           'revision', updated.resource_revision
                       )
                FROM updated
                JOIN upserted_trash ON upserted_trash.work_id = updated.id
                RETURNING id
            )
            SELECT count(*)::bigint AS affected_count,
                   max(id) AS latest_event_id
            FROM inserted_events
            "#,
        );

        let row = query.build().fetch_one(&mut *tx).await?;
        let affected_count = row.try_get::<i64, _>("affected_count")?;
        let latest_event_id = row.try_get::<Option<i64>, _>("latest_event_id")?;
        if let Some(latest_event_id) = latest_event_id {
            sqlx::query("SELECT pg_notify('pixivarchive_events', $1)")
                .bind(latest_event_id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;

        u64::try_from(affected_count)
            .map_err(|_| DbError::Constraint("negative gallery trash count".to_owned()))
    }

    pub async fn move_to_trash(
        &self,
        work_id: Uuid,
        scheduled_purge_at: OffsetDateTime,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        let current = sqlx::query(
            r#"
            SELECT
                work.collection_state,
                work.resource_revision,
                trash_entry.previous_collection_state
            FROM work
            LEFT JOIN trash_entry ON trash_entry.work_id = work.id
            WHERE work.id = $1
            FOR UPDATE OF work
            "#,
        )
        .bind(work_id)
        .fetch_one(&mut *tx)
        .await?;
        let collection_state: String = current.try_get("collection_state")?;
        let resource_revision: i64 = current.try_get("resource_revision")?;
        let previous_collection_state = if collection_state == "trash" {
            current
                .try_get::<Option<String>, _>("previous_collection_state")?
                .ok_or_else(|| {
                    DbError::Constraint(format!("trash work {work_id} has no matching trash entry"))
                })?
        } else {
            collection_state.clone()
        };

        if previous_collection_state != "collected" && previous_collection_state != "metadata_only"
        {
            return Err(DbError::Constraint(format!(
                "work {work_id} cannot enter trash from {previous_collection_state}"
            )));
        }

        let transitioned = collection_state != "trash";
        if transitioned {
            sqlx::query(
                r#"
                UPDATE work
                SET collection_state = 'trash',
                    trashed_at = now(),
                    updated_at = now(),
                    resource_revision = resource_revision + 1
                WHERE id = $1
                "#,
            )
            .bind(work_id)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            r#"
            INSERT INTO trash_entry (work_id, previous_collection_state, scheduled_purge_at, purge_state)
            VALUES ($1, $2, $3, 'pending')
            ON CONFLICT (work_id)
            DO UPDATE SET scheduled_purge_at = excluded.scheduled_purge_at,
                          purge_state = 'pending',
                          failure_message = NULL,
                          failure_details = '[]'::jsonb
            "#,
        )
        .bind(work_id)
        .bind(previous_collection_state)
        .bind(scheduled_purge_at)
        .execute(&mut *tx)
        .await?;
        if transitioned {
            EventRepository::new(self.db.clone())
                .append_in_tx(
                    &mut tx,
                    EventResource::Work,
                    work_id,
                    EventPayload::WorkChanged {
                        revision: resource_revision + 1,
                    },
                )
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn trash_entry(&self, work_id: Uuid) -> Result<TrashEntry, DbError> {
        let row = sqlx::query(
            r#"
            SELECT
                work_id,
                previous_collection_state,
                trashed_at,
                scheduled_purge_at,
                purge_state,
                purge_attempts,
                failure_message,
                EXISTS (
                    SELECT 1
                    FROM job
                    WHERE kind = 'purge_trash'
                      AND (payload ->> 'work_id')::uuid = trash_entry.work_id
                      AND (
                        state IN ('queued', 'running', 'waiting_account', 'waiting_storage')
                        OR (state = 'failed' AND retryable = true)
                      )
                ) AS purge_job_active
            FROM trash_entry
            WHERE work_id = $1
            "#,
        )
        .bind(work_id)
        .fetch_one(self.db.pool())
        .await?;
        trash_entry_from_row(&row)
    }

    pub async fn trash_page(
        &self,
        filter: &TrashFilter,
        cursor: Option<&TrashCursor>,
        limit: u16,
    ) -> Result<TrashPage, DbError> {
        if limit == 0 || limit > 200 {
            return Err(DbError::InvalidValue(
                "trash page size must be between 1 and 200".to_owned(),
            ));
        }
        let TrashFilterValues {
            query_pattern,
            pixiv_work_id,
            purge_states,
        } = trash_filter_values(filter)?;
        let cursor_time = cursor.map(|cursor| cursor.scheduled_purge_at);
        let cursor_work_id = cursor.map(|cursor| cursor.work_id);
        let mut rows = sqlx::query(
            r#"
            SELECT
                trash_entry.work_id,
                trash_entry.previous_collection_state,
                trash_entry.trashed_at,
                trash_entry.scheduled_purge_at,
                trash_entry.purge_state,
                trash_entry.purge_attempts,
                trash_entry.failure_message,
                EXISTS (
                    SELECT 1
                    FROM job
                    WHERE kind = 'purge_trash'
                      AND (payload ->> 'work_id')::uuid = trash_entry.work_id
                      AND (
                        state IN ('queued', 'running', 'waiting_account', 'waiting_storage')
                        OR (state = 'failed' AND retryable = true)
                      )
                ) AS purge_job_active,
                work.pixiv_work_id,
                revision.title,
                artist.name AS artist_name,
                revision.page_count,
                (
                    SELECT coalesce(sum(media_revision.byte_size), 0)::bigint
                    FROM work_page
                    JOIN media_revision ON media_revision.work_page_id = work_page.id
                    WHERE work_page.work_id = work.id
                ) + (
                    SELECT coalesce(sum(derivative.byte_size), 0)::bigint
                    FROM work_page
                    JOIN media_revision ON media_revision.work_page_id = work_page.id
                    JOIN derivative ON derivative.media_revision_id = media_revision.id
                    WHERE work_page.work_id = work.id
                ) AS estimated_release_bytes
            FROM trash_entry
            JOIN work ON work.id = trash_entry.work_id
            JOIN work_revision AS revision ON revision.id = work.current_revision_id
            JOIN artist ON artist.id = work.artist_id
            WHERE (
                    $1::text IS NULL
                    OR revision.title ILIKE $1 ESCAPE '\'
                    OR artist.name ILIKE $1 ESCAPE '\'
                    OR work.pixiv_work_id = $2
                  )
              AND (
                    coalesce(cardinality($3::text[]), 0) = 0
                    OR trash_entry.purge_state = ANY($3)
                  )
              AND (
                    $4::timestamptz IS NULL
                    OR (trash_entry.scheduled_purge_at, trash_entry.work_id) > ($4, $5::uuid)
                  )
            ORDER BY trash_entry.scheduled_purge_at, trash_entry.work_id
            LIMIT $6
            "#,
        )
        .bind(query_pattern)
        .bind(pixiv_work_id)
        .bind(purge_states)
        .bind(cursor_time)
        .bind(cursor_work_id)
        .bind(i64::from(limit) + 1)
        .fetch_all(self.db.pool())
        .await?;
        let has_more = rows.len() > usize::from(limit);
        if has_more {
            rows.pop();
        }
        let next_cursor = has_more
            .then(|| rows.last())
            .flatten()
            .map(|row| TrashCursor {
                scheduled_purge_at: row.get("scheduled_purge_at"),
                work_id: row.get("work_id"),
            });
        let items = rows
            .into_iter()
            .map(|row| {
                Ok::<TrashWorkSummary, DbError>(TrashWorkSummary {
                    entry: trash_entry_from_row(&row)?,
                    pixiv_work_id: row.get("pixiv_work_id"),
                    title: row.get("title"),
                    artist_name: row.get("artist_name"),
                    page_count: u32::try_from(row.get::<i32, _>("page_count")).map_err(|_| {
                        DbError::InvalidValue("trash work page count cannot be negative".to_owned())
                    })?,
                    estimated_release_bytes: u64::try_from(
                        row.get::<i64, _>("estimated_release_bytes"),
                    )
                    .map_err(|_| {
                        DbError::InvalidValue(
                            "trash release estimate cannot be negative".to_owned(),
                        )
                    })?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(TrashPage { items, next_cursor })
    }

    pub async fn trash_summary(
        &self,
        filter: &TrashFilter,
    ) -> Result<TrashCollectionSummary, DbError> {
        let TrashFilterValues {
            query_pattern,
            pixiv_work_id,
            purge_states,
        } = trash_filter_values(filter)?;
        let row = sqlx::query(
            r#"
            WITH matching_works AS (
                SELECT work.id
                FROM trash_entry
                JOIN work ON work.id = trash_entry.work_id
                JOIN work_revision AS revision ON revision.id = work.current_revision_id
                JOIN artist ON artist.id = work.artist_id
                WHERE (
                        $1::text IS NULL
                        OR revision.title ILIKE $1 ESCAPE '\'
                        OR artist.name ILIKE $1 ESCAPE '\'
                        OR work.pixiv_work_id = $2
                      )
                  AND (
                        coalesce(cardinality($3::text[]), 0) = 0
                        OR trash_entry.purge_state = ANY($3)
                      )
            ),
            source_totals AS (
                SELECT coalesce(sum(media_revision.byte_size), 0)::bigint AS logical_bytes
                FROM matching_works
                JOIN work_page ON work_page.work_id = matching_works.id
                JOIN media_revision ON media_revision.work_page_id = work_page.id
            ),
            derivative_totals AS (
                SELECT coalesce(sum(derivative.byte_size), 0)::bigint AS logical_bytes
                FROM matching_works
                JOIN work_page ON work_page.work_id = matching_works.id
                JOIN media_revision ON media_revision.work_page_id = work_page.id
                JOIN derivative ON derivative.media_revision_id = media_revision.id
            ),
            matching_source_groups AS (
                SELECT DISTINCT media_revision.byte_size, media_revision.sha256
                FROM matching_works
                JOIN work_page ON work_page.work_id = matching_works.id
                JOIN media_revision ON media_revision.work_page_id = work_page.id
            ),
            reclaimable_source_totals AS (
                SELECT coalesce(sum(source_group.byte_size), 0)::bigint AS logical_bytes
                FROM matching_source_groups AS source_group
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM media_revision AS other_revision
                    JOIN work_page AS other_page
                      ON other_page.id = other_revision.work_page_id
                    WHERE other_revision.byte_size = source_group.byte_size
                      AND other_revision.sha256 = source_group.sha256
                      AND NOT EXISTS (
                          SELECT 1
                          FROM matching_works
                          WHERE matching_works.id = other_page.work_id
                      )
                )
            )
            SELECT
                (SELECT count(*)::bigint FROM matching_works) AS total_count,
                source_totals.logical_bytes + derivative_totals.logical_bytes AS logical_bytes,
                reclaimable_source_totals.logical_bytes + derivative_totals.logical_bytes
                    AS estimated_reclaimable_bytes
            FROM source_totals, derivative_totals, reclaimable_source_totals
            "#,
        )
        .bind(query_pattern)
        .bind(pixiv_work_id)
        .bind(purge_states)
        .fetch_one(self.db.pool())
        .await?;
        Ok(TrashCollectionSummary {
            total_count: nonnegative_u64(row.get("total_count"), "trash work count")?,
            logical_bytes: nonnegative_u64(row.get("logical_bytes"), "trash logical bytes")?,
            estimated_reclaimable_bytes: nonnegative_u64(
                row.get("estimated_reclaimable_bytes"),
                "trash reclaim estimate",
            )?,
        })
    }

    pub async fn project_trash_selection(
        &self,
        expression: &TrashSelectionExpression,
        visible_work_ids: &[Uuid],
    ) -> Result<TrashSelectionProjection, DbError> {
        let mut query = trash_selection_ctes(expression)?;
        query.push(
            r#"
            SELECT count(*)::bigint AS selected_count,
                   count(*) FILTER (
                       WHERE trash_entry.purge_attempts > 0
                          OR trash_entry.purge_state = 'running'
                          OR EXISTS (
                              SELECT 1
                              FROM job
                              WHERE kind = 'purge_trash'
                                AND (payload ->> 'work_id')::uuid = trash_entry.work_id
                                AND (
                                  state IN ('queued', 'running', 'waiting_account', 'waiting_storage')
                                  OR (state = 'failed' AND retryable = true)
                                )
                          )
                   )::bigint AS blocked_count,
                   coalesce(
                       array_agg(work_id ORDER BY work_id)
                           FILTER (WHERE work_id = ANY(
            "#,
        );
        query.push_bind(visible_work_ids.to_vec()).push(
            r#"::uuid[])),
                       ARRAY[]::uuid[]
                   ) AS selected_visible_work_ids
            FROM selected_trash
            JOIN trash_entry USING (work_id)
            "#,
        );
        let row = query.build().fetch_one(self.db.pool()).await?;
        Ok(TrashSelectionProjection {
            selected_count: nonnegative_u64(
                row.try_get("selected_count")?,
                "trash selection count",
            )?,
            blocked_count: nonnegative_u64(
                row.try_get("blocked_count")?,
                "blocked trash selection count",
            )?,
            selected_visible_work_ids: row.try_get("selected_visible_work_ids")?,
        })
    }

    pub async fn reschedule_trash(
        &self,
        work_id: Uuid,
        scheduled_purge_at: OffsetDateTime,
    ) -> Result<(), DbError> {
        self.reschedule_trash_many(&[work_id], scheduled_purge_at)
            .await
    }

    pub async fn reschedule_trash_many(
        &self,
        work_ids: &[Uuid],
        scheduled_purge_at: OffsetDateTime,
    ) -> Result<(), DbError> {
        let work_ids = validated_trash_batch_ids(work_ids)?;
        let mut tx = self.db.begin().await?;
        for work_id in work_ids {
            reschedule_trash_in_tx(&mut tx, work_id, scheduled_purge_at).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn restore_many(&self, work_ids: &[Uuid]) -> Result<(), DbError> {
        let work_ids = validated_trash_batch_ids(work_ids)?;
        let mut tx = self.db.begin().await?;
        for work_id in work_ids {
            restore_in_tx(&self.db, &mut tx, work_id).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn restore(&self, work_id: Uuid) -> Result<(), DbError> {
        self.restore_many(&[work_id]).await
    }

    pub async fn reschedule_trash_selection(
        &self,
        expression: &TrashSelectionExpression,
        scheduled_purge_at: OffsetDateTime,
    ) -> Result<TrashSelectionMutation, DbError> {
        let mut tx = self.db.begin().await?;
        let selection = lock_trash_selection(&mut tx, expression).await?;
        if selection
            .entries
            .iter()
            .any(|entry| scheduled_purge_at < entry.trashed_at)
        {
            return Err(DbError::InvalidValue(
                "trash purge date must not precede the deletion date".to_owned(),
            ));
        }
        if selection.blocked_count > 0 {
            let mutation = selection.mutation(0);
            tx.commit().await?;
            return Ok(mutation);
        }

        let work_ids = selection.work_ids();
        let updated = sqlx::query(
            r#"
            UPDATE trash_entry
            SET scheduled_purge_at = $2,
                purge_state = 'pending',
                failure_message = NULL,
                failure_details = '[]'::jsonb
            WHERE work_id = ANY($1)
            "#,
        )
        .bind(&work_ids)
        .bind(scheduled_purge_at)
        .execute(&mut *tx)
        .await?;
        let affected_count = updated.rows_affected();
        let mutation = selection.mutation(affected_count);
        tx.commit().await?;
        Ok(mutation)
    }

    pub async fn restore_trash_selection(
        &self,
        expression: &TrashSelectionExpression,
    ) -> Result<TrashSelectionMutation, DbError> {
        let mut tx = self.db.begin().await?;
        let selection = lock_trash_selection(&mut tx, expression).await?;
        if selection.blocked_count > 0 {
            let mutation = selection.mutation(0);
            tx.commit().await?;
            return Ok(mutation);
        }

        let row = sqlx::query(
            r#"
            WITH deleted AS (
                DELETE FROM trash_entry
                WHERE work_id = ANY($1)
                RETURNING trash_entry.work_id, trash_entry.previous_collection_state
            ),
            updated AS (
                UPDATE work
                SET collection_state = deleted.previous_collection_state,
                    trashed_at = NULL,
                    updated_at = now(),
                    resource_revision = work.resource_revision + 1
                FROM deleted
                WHERE work.id = deleted.work_id
                RETURNING work.id, work.resource_revision
            ),
            inserted_events AS (
                INSERT INTO app_event (resource, resource_id, payload)
                SELECT 'work',
                       updated.id,
                       jsonb_build_object(
                           'type', 'work_changed',
                           'revision', updated.resource_revision
                       )
                FROM updated
                RETURNING id
            )
            SELECT count(inserted_events.id)::bigint AS affected_count,
                   max(inserted_events.id) AS latest_event_id
            FROM inserted_events
            "#,
        )
        .bind(selection.work_ids())
        .fetch_one(&mut *tx)
        .await?;
        let affected_count =
            nonnegative_u64(row.try_get("affected_count")?, "restored trash count")?;
        let mutation = selection.mutation(affected_count);
        if let Some(event_id) = row.try_get::<Option<i64>, _>("latest_event_id")? {
            sqlx::query("SELECT pg_notify('pixivarchive_events', $1)")
                .bind(event_id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(mutation)
    }
}

pub(crate) fn trash_selection_ctes(
    expression: &TrashSelectionExpression,
) -> Result<sqlx::QueryBuilder<Postgres>, DbError> {
    let TrashFilterValues {
        query_pattern,
        pixiv_work_id,
        purge_states,
    } = trash_filter_values(&expression.filter)?;
    let mut query = sqlx::QueryBuilder::new(
        r#"
        WITH matching_trash AS (
            SELECT trash_entry.work_id
            FROM trash_entry
            JOIN work ON work.id = trash_entry.work_id
            JOIN work_revision AS revision ON revision.id = work.current_revision_id
            JOIN artist ON artist.id = work.artist_id
            WHERE (
                    "#,
    );
    query
        .push_bind(query_pattern.clone())
        .push("::text IS NULL OR revision.title ILIKE ")
        .push_bind(query_pattern.clone())
        .push(" ESCAPE '\\' OR artist.name ILIKE ")
        .push_bind(query_pattern)
        .push(" ESCAPE '\\' OR work.pixiv_work_id = ")
        .push_bind(pixiv_work_id)
        .push(") AND (coalesce(cardinality(")
        .push_bind(purge_states.clone())
        .push("::text[]), 0) = 0 OR trash_entry.purge_state = ANY(")
        .push_bind(purge_states)
        .push(
            r#"::text[]))
        ),
        selected_trash AS (
            SELECT work_id
            FROM matching_trash
            WHERE "#,
        )
        .push_bind(expression.base_selected)
        .push(" <> (work_id = ANY(")
        .push_bind(expression.exception_work_ids.clone())
        .push("::uuid[])) ) ");
    Ok(query)
}

struct LockedTrashSelectionEntry {
    work_id: Uuid,
    trashed_at: OffsetDateTime,
    purge_state: String,
    purge_attempts: u32,
}

struct LockedTrashSelection {
    entries: Vec<LockedTrashSelectionEntry>,
    selected_count: u64,
    blocked_count: u64,
}

impl LockedTrashSelection {
    fn work_ids(&self) -> Vec<Uuid> {
        self.entries.iter().map(|entry| entry.work_id).collect()
    }

    fn mutation(&self, affected_count: u64) -> TrashSelectionMutation {
        TrashSelectionMutation {
            selected_count: self.selected_count,
            blocked_count: self.blocked_count,
            affected_count,
        }
    }
}

async fn lock_trash_selection(
    tx: &mut Transaction<'_, Postgres>,
    expression: &TrashSelectionExpression,
) -> Result<LockedTrashSelection, DbError> {
    let mut query = trash_selection_ctes(expression)?;
    query.push(
        r#"
        SELECT trash_entry.work_id,
               trash_entry.trashed_at,
               trash_entry.purge_state,
               trash_entry.purge_attempts
        FROM selected_trash
        JOIN trash_entry USING (work_id)
        ORDER BY trash_entry.work_id
        FOR UPDATE OF trash_entry
        "#,
    );
    let rows = query.build().fetch_all(&mut **tx).await?;
    let entries = rows
        .into_iter()
        .map(|row| {
            Ok(LockedTrashSelectionEntry {
                work_id: row.try_get("work_id")?,
                trashed_at: row.try_get("trashed_at")?,
                purge_state: row.try_get("purge_state")?,
                purge_attempts: nonnegative_u32(
                    row.try_get("purge_attempts")?,
                    "negative purge attempt count",
                )?,
            })
        })
        .collect::<Result<Vec<_>, DbError>>()?;
    let work_ids = entries
        .iter()
        .map(|entry| entry.work_id)
        .collect::<Vec<_>>();

    // Purge enqueueing holds the same rows. A second statement observes a job
    // committed by a transaction that this lock had to wait for.
    let active_purges = active_purge_work_ids_in_tx(tx, &work_ids).await?;
    let selected_count = u64::try_from(entries.len())
        .map_err(|_| DbError::InvalidValue("trash selection is too large".to_owned()))?;
    let blocked_count = u64::try_from(
        entries
            .iter()
            .filter(|entry| {
                !trash_action_capabilities(
                    &entry.purge_state,
                    entry.purge_attempts,
                    active_purges.contains(&entry.work_id),
                )
                .can_restore
            })
            .count(),
    )
    .map_err(|_| DbError::InvalidValue("trash selection is too large".to_owned()))?;
    Ok(LockedTrashSelection {
        entries,
        selected_count,
        blocked_count,
    })
}

async fn active_purge_work_ids_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    work_ids: &[Uuid],
) -> Result<HashSet<Uuid>, DbError> {
    if work_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let work_ids = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT DISTINCT (payload ->> 'work_id')::uuid
        FROM job
        WHERE kind = 'purge_trash'
          AND (payload ->> 'work_id')::uuid = ANY($1)
          AND (
            state IN ('queued', 'running', 'waiting_account', 'waiting_storage')
            OR (state = 'failed' AND retryable = true)
          )
        "#,
    )
    .bind(work_ids)
    .fetch_all(&mut **tx)
    .await?;
    Ok(work_ids.into_iter().collect())
}

async fn reschedule_trash_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    work_id: Uuid,
    scheduled_purge_at: OffsetDateTime,
) -> Result<(), DbError> {
    let entry = lock_trash_action(tx, work_id).await?;
    if !entry.capabilities.can_reschedule {
        return Err(DbError::RevisionConflict);
    }
    if scheduled_purge_at < entry.trashed_at {
        return Err(DbError::InvalidValue(
            "trash purge date must not precede the deletion date".to_owned(),
        ));
    }
    sqlx::query(
        r#"
        UPDATE trash_entry
        SET scheduled_purge_at = $2,
            purge_state = 'pending',
            failure_message = NULL,
            failure_details = '[]'::jsonb
        WHERE work_id = $1
        "#,
    )
    .bind(work_id)
    .bind(scheduled_purge_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn restore_in_tx(
    db: &crate::Db,
    tx: &mut Transaction<'_, Postgres>,
    work_id: Uuid,
) -> Result<(), DbError> {
    let entry = lock_trash_action(tx, work_id).await?;
    if !entry.capabilities.can_restore {
        return Err(DbError::RevisionConflict);
    }
    sqlx::query("DELETE FROM trash_entry WHERE work_id = $1")
        .bind(work_id)
        .execute(&mut **tx)
        .await?;
    let updated = sqlx::query(
        r#"
        UPDATE work
        SET collection_state = $2,
            trashed_at = NULL,
            updated_at = now(),
            resource_revision = resource_revision + 1
        WHERE id = $1
        RETURNING resource_revision
        "#,
    )
    .bind(work_id)
    .bind(entry.previous_collection_state)
    .fetch_one(&mut **tx)
    .await?;
    let resource_revision: i64 = updated.try_get("resource_revision")?;
    EventRepository::new(db.clone())
        .append_in_tx(
            tx,
            EventResource::Work,
            work_id,
            EventPayload::WorkChanged {
                revision: resource_revision,
            },
        )
        .await?;
    Ok(())
}

struct LockedTrashAction {
    previous_collection_state: String,
    trashed_at: OffsetDateTime,
    capabilities: TrashActionCapabilities,
}

async fn lock_trash_action(
    tx: &mut Transaction<'_, Postgres>,
    work_id: Uuid,
) -> Result<LockedTrashAction, DbError> {
    let row = sqlx::query(
        r#"
        SELECT trash_entry.previous_collection_state,
               trash_entry.trashed_at,
               trash_entry.purge_state,
               trash_entry.purge_attempts
        FROM trash_entry
        WHERE trash_entry.work_id = $1
        FOR UPDATE OF trash_entry
        "#,
    )
    .bind(work_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or(DbError::NotFound)?;
    let purge_state = row.try_get::<String, _>("purge_state")?;
    let purge_attempts = nonnegative_u32(
        row.try_get("purge_attempts")?,
        "negative purge attempt count",
    )?;
    let active_purges = active_purge_work_ids_in_tx(tx, &[work_id]).await?;
    Ok(LockedTrashAction {
        previous_collection_state: row.try_get("previous_collection_state")?,
        trashed_at: row.try_get("trashed_at")?,
        capabilities: trash_action_capabilities(
            &purge_state,
            purge_attempts,
            active_purges.contains(&work_id),
        ),
    })
}

pub(crate) async fn load_trash_action_capabilities(
    pool: &PgPool,
    work_id: Uuid,
) -> Result<Option<TrashActionCapabilities>, DbError> {
    let row = sqlx::query(
        r#"
        SELECT trash_entry.purge_state,
               trash_entry.purge_attempts,
               EXISTS (
                   SELECT 1
                   FROM job
                   WHERE kind = 'purge_trash'
                     AND (payload ->> 'work_id')::uuid = trash_entry.work_id
                     AND (
                       state IN ('queued', 'running', 'waiting_account', 'waiting_storage')
                       OR (state = 'failed' AND retryable = true)
                     )
               ) AS purge_job_active
        FROM trash_entry
        WHERE trash_entry.work_id = $1
        "#,
    )
    .bind(work_id)
    .fetch_optional(pool)
    .await?;
    row.as_ref()
        .map(trash_action_capabilities_from_row)
        .transpose()
}

fn trash_entry_from_row(row: &sqlx::postgres::PgRow) -> Result<TrashEntry, DbError> {
    let purge_state = row.try_get::<String, _>("purge_state")?;
    let purge_attempts = nonnegative_u32(
        row.try_get("purge_attempts")?,
        "negative purge attempt count",
    )?;
    Ok(TrashEntry {
        work_id: row.try_get("work_id")?,
        previous_collection_state: row.try_get("previous_collection_state")?,
        trashed_at: row.try_get("trashed_at")?,
        scheduled_purge_at: row.try_get("scheduled_purge_at")?,
        purge_state: purge_state.clone(),
        purge_attempts,
        failure_message: row.try_get("failure_message")?,
        capabilities: trash_action_capabilities(
            &purge_state,
            purge_attempts,
            row.try_get("purge_job_active")?,
        ),
    })
}

fn trash_action_capabilities_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<TrashActionCapabilities, DbError> {
    let purge_state = row.try_get::<String, _>("purge_state")?;
    let purge_attempts = nonnegative_u32(
        row.try_get("purge_attempts")?,
        "negative purge attempt count",
    )?;
    Ok(trash_action_capabilities(
        &purge_state,
        purge_attempts,
        row.try_get("purge_job_active")?,
    ))
}

fn trash_action_capabilities(
    purge_state: &str,
    purge_attempts: u32,
    purge_job_active: bool,
) -> TrashActionCapabilities {
    if purge_attempts > 0 || purge_state == "running" {
        TrashActionCapabilities::blocked(TrashActionBlockReason::PurgeStarted)
    } else if purge_job_active {
        TrashActionCapabilities::blocked(TrashActionBlockReason::PurgeQueued)
    } else {
        TrashActionCapabilities::available()
    }
}

fn nonnegative_u32(value: i32, message: &str) -> Result<u32, DbError> {
    u32::try_from(value).map_err(|_| DbError::InvalidValue(message.to_owned()))
}

struct TrashFilterValues {
    query_pattern: Option<String>,
    pixiv_work_id: Option<i64>,
    purge_states: Vec<String>,
}

fn trash_filter_values(filter: &TrashFilter) -> Result<TrashFilterValues, DbError> {
    let query = filter
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty());
    if query.is_some_and(|query| query.chars().count() > 200) {
        return Err(DbError::InvalidValue(
            "trash query must not exceed 200 characters".to_owned(),
        ));
    }
    let mut purge_states = filter
        .purge_states
        .iter()
        .map(|state| state.trim())
        .filter(|state| !state.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if purge_states
        .iter()
        .any(|state| !matches!(state.as_str(), "pending" | "running" | "failed"))
    {
        return Err(DbError::InvalidValue(
            "trash purge state is invalid".to_owned(),
        ));
    }
    purge_states.sort_unstable();
    purge_states.dedup();
    let pixiv_work_id = query
        .and_then(|query| query.parse::<i64>().ok())
        .filter(|value| *value > 0);
    let query_pattern = query.map(|query| {
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        format!("%{escaped}%")
    });
    Ok(TrashFilterValues {
        query_pattern,
        pixiv_work_id,
        purge_states,
    })
}

fn nonnegative_u64(value: i64, name: &str) -> Result<u64, DbError> {
    u64::try_from(value).map_err(|_| DbError::InvalidValue(format!("{name} cannot be negative")))
}

pub(crate) fn validated_trash_batch_ids(work_ids: &[Uuid]) -> Result<Vec<Uuid>, DbError> {
    if work_ids.is_empty() {
        return Err(DbError::InvalidValue(
            "trash batch must contain at least one work".to_owned(),
        ));
    }
    let mut work_ids = work_ids.to_vec();
    work_ids.sort_unstable();
    if work_ids.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(DbError::InvalidValue(
            "trash batch contains duplicate works".to_owned(),
        ));
    }
    Ok(work_ids)
}
