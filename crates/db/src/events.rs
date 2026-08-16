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
    ) -> Result<AppEvent, DbError> {
        let payload_json = serde_json::to_value(&payload)
            .map_err(|error| DbError::InvalidValue(error.to_string()))?;
        let row = sqlx::query!(
            r#"
            INSERT INTO app_event (resource, resource_id, payload)
            VALUES ($1, $2, $3)
            RETURNING id, resource, resource_id, payload as "payload: Json<serde_json::Value>"
            "#,
            resource.as_str(),
            resource_id,
            payload_json
        )
        .fetch_one(&mut **tx)
        .await?;

        let event_id = row.id;
        sqlx::query!(
            "SELECT pg_notify('pixivarchive_events', $1)",
            event_id.to_string()
        )
        .execute(&mut **tx)
        .await?;

        event_from_row(row.id, row.resource, row.resource_id, row.payload.0)
    }

    pub async fn list_after(&self, event_id: i64, limit: i64) -> Result<Vec<AppEvent>, DbError> {
        let rows = sqlx::query!(
            r#"
            SELECT id, resource, resource_id, payload as "payload: Json<serde_json::Value>"
            FROM app_event
            WHERE id > $1
            ORDER BY id
            LIMIT $2
            "#,
            event_id,
            limit
        )
        .fetch_all(self.db.pool())
        .await?;

        rows.into_iter()
            .map(|row| event_from_row(row.id, row.resource, row.resource_id, row.payload.0))
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

        let boundary = sqlx::query(
            r#"
            SELECT min(id) AS oldest_event_id, max(id) AS latest_event_id
            FROM app_event
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
        let rows = sqlx::query!(
            r#"
            SELECT id, resource, resource_id, payload as "payload: Json<serde_json::Value>"
            FROM app_event
            WHERE id > $1
            ORDER BY id
            LIMIT $2
            "#,
            requested_id,
            effective_limit + 1
        )
        .fetch_all(self.db.pool())
        .await?;
        let has_more = rows.len() as i64 > effective_limit;
        let events = rows
            .into_iter()
            .take(effective_limit as usize)
            .map(|row| event_from_row(row.id, row.resource, row.resource_id, row.payload.0))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(EventReplayWindow {
            events,
            oldest_event_id,
            latest_event_id,
            snapshot_refresh: false,
            has_more,
        })
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
