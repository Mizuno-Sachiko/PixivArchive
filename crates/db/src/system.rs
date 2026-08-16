use crate::{Db, DbError, JobRepository};
use pixivarchive_domain::job::{JobKind, JobPriority, NewJob};
use serde_json::json;
use sqlx::Row;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone)]
pub struct SystemRepository {
    db: Db,
}

impl SystemRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn readiness(&self) -> Result<(), DbError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(self.db.pool())
            .await?;
        Ok(())
    }

    pub async fn status(&self) -> Result<SystemDatabaseStatus, DbError> {
        let migration_version =
            sqlx::query_scalar::<_, Option<i64>>("SELECT max(version) FROM _sqlx_migrations")
                .fetch_one(self.db.pool())
                .await?
                .unwrap_or_default();
        let rows = sqlx::query(
            r#"
            SELECT priority_class, state, count(*)::bigint AS count
            FROM job
            GROUP BY priority_class, state
            ORDER BY priority_class, state
            "#,
        )
        .fetch_all(self.db.pool())
        .await?;
        let mut queue = BTreeMap::<String, BTreeMap<String, i64>>::new();
        for row in rows {
            queue
                .entry(row.get("priority_class"))
                .or_default()
                .insert(row.get("state"), row.get("count"));
        }
        Ok(SystemDatabaseStatus {
            migration_version,
            queue,
        })
    }

    pub async fn enqueue_derivative_regeneration_jobs(
        &self,
        priority: JobPriority,
    ) -> Result<Vec<Uuid>, DbError> {
        let kind = JobKind::GenerateDerivative;
        let mut tx = self.db.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind("regenerate_derivatives")
            .execute(&mut *tx)
            .await?;
        let media_revision_ids = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT media_revision.id
            FROM media_revision
            JOIN work_page
              ON work_page.current_media_revision_id = media_revision.id
            JOIN work
              ON work.id = work_page.work_id
            WHERE work.collection_state = 'collected'
              AND NOT EXISTS (
                  SELECT 1
                  FROM job
                  WHERE job.kind = $1
                    AND job.payload ->> 'media_revision_id' = media_revision.id::text
                    AND (
                        job.state IN ('queued', 'running', 'waiting_account', 'waiting_storage')
                        OR (job.state = 'failed' AND job.retryable = true)
                    )
              )
            ORDER BY media_revision.id
            "#,
        )
        .bind(kind.as_str())
        .fetch_all(&mut *tx)
        .await?;
        let jobs = JobRepository::new(self.db.clone());
        let mut job_ids = Vec::with_capacity(media_revision_ids.len());
        for media_revision_id in media_revision_ids {
            job_ids.push(
                jobs.enqueue_in_tx(
                    &mut tx,
                    NewJob::for_kind(
                        priority,
                        kind,
                        json!({
                            "media_revision_id": media_revision_id,
                            "regenerate": true
                        }),
                    ),
                )
                .await?,
            );
        }
        tx.commit().await?;
        Ok(job_ids)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemDatabaseStatus {
    pub migration_version: i64,
    pub queue: BTreeMap<String, BTreeMap<String, i64>>,
}
