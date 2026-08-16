use crate::{Db, DbError};
use sqlx::Row;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[derive(Clone)]
pub struct WorkerHeartbeatRepository {
    db: Db,
}

impl WorkerHeartbeatRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn update(&self, input: WorkerHeartbeatUpdate) -> Result<(), DbError> {
        if input.version.trim().is_empty() || input.seen_at < input.started_at {
            return Err(DbError::InvalidValue(
                "worker heartbeat values are invalid".to_owned(),
            ));
        }
        sqlx::query(
            r#"
            INSERT INTO worker_heartbeat (
                singleton,
                worker_id,
                version,
                git_commit,
                started_at,
                last_seen_at
            )
            VALUES (TRUE, $1, $2, $3, $4, $5)
            ON CONFLICT (singleton)
            DO UPDATE SET worker_id = excluded.worker_id,
                          version = excluded.version,
                          git_commit = excluded.git_commit,
                          started_at = excluded.started_at,
                          last_seen_at = excluded.last_seen_at
            "#,
        )
        .bind(input.worker_id)
        .bind(input.version.trim())
        .bind(input.git_commit)
        .bind(input.started_at)
        .bind(input.seen_at)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    pub async fn current(&self) -> Result<Option<WorkerHeartbeatRecord>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT worker_id, version, git_commit, started_at, last_seen_at
            FROM worker_heartbeat
            WHERE singleton = TRUE
            "#,
        )
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row.map(|row| WorkerHeartbeatRecord {
            worker_id: row.get("worker_id"),
            version: row.get("version"),
            git_commit: row.get("git_commit"),
            started_at: row.get("started_at"),
            last_seen_at: row.get("last_seen_at"),
        }))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerHeartbeatUpdate {
    pub worker_id: Uuid,
    pub version: String,
    pub git_commit: Option<String>,
    pub started_at: OffsetDateTime,
    pub seen_at: OffsetDateTime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerHeartbeatRecord {
    pub worker_id: Uuid,
    pub version: String,
    pub git_commit: Option<String>,
    pub started_at: OffsetDateTime,
    pub last_seen_at: OffsetDateTime,
}

impl WorkerHeartbeatRecord {
    pub fn is_online(&self, now: OffsetDateTime, stale_after: Duration) -> bool {
        stale_after > Duration::ZERO
            && self.last_seen_at <= now
            && now - self.last_seen_at <= stale_after
    }
}
