use crate::{Db, DbError};
use pixivarchive_domain::event::{AppEvent, EventPayload, EventReplayWindow, EventResource};
use sqlx::{Postgres, Row, Transaction, postgres::PgListener, types::Json};
use uuid::Uuid;

const MAX_REPLAY_LIMIT: i64 = 1_000;

pub struct EventNotificationListener {
    listener: PgListener,
}

impl EventNotificationListener {
    pub async fn connect(database_url: &str) -> Result<Self, DbError> {
        let mut listener = PgListener::connect(database_url).await?;
        listener.listen("pixivarchive_events").await?;
        Ok(Self { listener })
    }

    pub async fn recv(&mut self) -> Result<(), DbError> {
        self.listener
            .recv()
            .await
            .map(|_| ())
            .map_err(DbError::from)
    }
}

#[derive(Clone)]
pub struct EventRepository {
    db: Db,
}

impl EventRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn append_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        resource: EventResource,
        resource_id: Uuid,
        payload: EventPayload,
    ) -> Result<(), DbError> {
        let payload_json = serde_json::to_value(&payload)
            .map_err(|error| DbError::InvalidValue(error.to_string()))?;
        sqlx::query(
            r#"
            INSERT INTO app_event (resource, resource_id, payload)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(resource.as_str())
        .bind(resource_id)
        .bind(payload_json)
        .execute(&mut **tx)
        .await?;

        sqlx::query("SELECT pg_notify('pixivarchive_events', '')")
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    pub async fn list_after(&self, event_id: i64, limit: i64) -> Result<Vec<AppEvent>, DbError> {
        self.publish_pending().await?;
        let rows = sqlx::query(
            r#"
            SELECT published_id, resource, resource_id, payload
            FROM app_event
            WHERE published_id > $1
            ORDER BY published_id
            LIMIT $2
            "#,
        )
        .bind(event_id)
        .bind(limit)
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter()
            .map(|row| {
                event_from_row(
                    row.get("published_id"),
                    row.get("resource"),
                    row.get("resource_id"),
                    row.get::<Json<serde_json::Value>, _>("payload").0,
                )
            })
            .collect()
    }

    pub async fn replay_window(
        &self,
        last_event_id: Option<i64>,
        limit: i64,
    ) -> Result<EventReplayWindow, DbError> {
        if limit <= 0 {
            return Err(DbError::InvalidValue(
                "replay limit must be positive".to_owned(),
            ));
        }

        self.publish_pending().await?;
        let boundary = sqlx::query(
            r#"
            SELECT min(published_id) AS oldest_event_id,
                   max(published_id) AS latest_event_id
            FROM app_event
            WHERE published_id IS NOT NULL
            "#,
        )
        .fetch_one(self.db.pool())
        .await?;
        let oldest_event_id: Option<i64> = boundary.try_get("oldest_event_id")?;
        let latest_event_id: Option<i64> = boundary.try_get("latest_event_id")?;
        let Some(oldest) = oldest_event_id else {
            return Ok(EventReplayWindow {
                events: Vec::new(),
                oldest_event_id,
                latest_event_id,
                snapshot_refresh: false,
                has_more: false,
            });
        };

        let Some(requested_id) = last_event_id.filter(|id| *id > 0) else {
            return Ok(EventReplayWindow {
                events: Vec::new(),
                oldest_event_id,
                latest_event_id,
                snapshot_refresh: false,
                has_more: false,
            });
        };

        let latest = latest_event_id.expect("non-empty app_event has a latest id");
        if requested_id < oldest - 1 || requested_id > latest {
            return Ok(EventReplayWindow {
                events: Vec::new(),
                oldest_event_id,
                latest_event_id,
                snapshot_refresh: true,
                has_more: false,
            });
        }

        let effective_limit = limit.min(MAX_REPLAY_LIMIT);
        let rows = sqlx::query(
            r#"
            SELECT published_id, resource, resource_id, payload
            FROM app_event
            WHERE published_id > $1
            ORDER BY published_id
            LIMIT $2
            "#,
        )
        .bind(requested_id)
        .bind(effective_limit + 1)
        .fetch_all(self.db.pool())
        .await?;
        let has_more = rows.len() as i64 > effective_limit;
        let events = rows
            .into_iter()
            .take(effective_limit as usize)
            .map(|row| {
                event_from_row(
                    row.get("published_id"),
                    row.get("resource"),
                    row.get("resource_id"),
                    row.get::<Json<serde_json::Value>, _>("payload").0,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(EventReplayWindow {
            events,
            oldest_event_id,
            latest_event_id,
            snapshot_refresh: false,
            has_more,
        })
    }

    async fn publish_pending(&self) -> Result<(), DbError> {
        let has_pending: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app_event WHERE published_id IS NULL)")
                .fetch_one(self.db.pool())
                .await?;
        if !has_pending {
            return Ok(());
        }

        let mut tx = self.db.begin().await?;
        let mut published_id: i64 = sqlx::query_scalar(
            r#"
            SELECT last_published_id
            FROM app_event_publication_cursor
            WHERE singleton = true
            FOR UPDATE
            "#,
        )
        .fetch_one(&mut *tx)
        .await?;
        let event_ids = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT id
            FROM app_event
            WHERE published_id IS NULL
            ORDER BY id
            FOR UPDATE
            "#,
        )
        .fetch_all(&mut *tx)
        .await?;
        if event_ids.is_empty() {
            tx.commit().await?;
            return Ok(());
        }
        for event_id in event_ids {
            published_id = published_id
                .checked_add(1)
                .ok_or_else(|| DbError::InvalidValue("event publication id overflow".to_owned()))?;
            sqlx::query("UPDATE app_event SET published_id = $2 WHERE id = $1")
                .bind(event_id)
                .bind(published_id)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query(
            r#"
            UPDATE app_event_publication_cursor
            SET last_published_id = $1
            WHERE singleton = true
            "#,
        )
        .bind(published_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("SELECT pg_notify('pixivarchive_events', $1)")
            .bind(published_id.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
}

fn event_from_row(
    id: i64,
    resource: String,
    resource_id: Uuid,
    payload: serde_json::Value,
) -> Result<AppEvent, DbError> {
    let resource = EventResource::from_db_value(&resource)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown event resource {resource}")))?;
    let payload = serde_json::from_value(payload)
        .map_err(|error| DbError::InvalidValue(error.to_string()))?;
    Ok(AppEvent {
        id,
        resource,
        resource_id,
        payload,
    })
}
