use super::{
    ImportRepository,
    model::{CreateImportRun, ImportRunRecord, import_params, run_from_row},
};
use crate::{DbError, JobRepository};
use pixivarchive_domain::{job::JobLease, subscription::ImportRunStatus};
use sqlx::Row;
use uuid::Uuid;

impl ImportRepository {
    pub async fn create(&self, input: CreateImportRun) -> Result<Uuid, DbError> {
        let id = Uuid::now_v7();
        sqlx::query(
            r#"
            INSERT INTO import_run (
                id,
                pixiv_account_id,
                import_kind,
                target_pixiv_id,
                forced,
                status,
                params,
                started_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, now())
            "#,
        )
        .bind(id)
        .bind(input.account_id)
        .bind(input.kind.as_str())
        .bind(input.target_pixiv_id)
        .bind(input.forced)
        .bind(input.status.as_str())
        .bind(sqlx::types::Json(import_params(
            input.forced,
            input.rule_document.as_ref(),
        )))
        .execute(self.db.pool())
        .await?;
        Ok(id)
    }

    pub async fn finish(
        &self,
        run_id: Uuid,
        status: ImportRunStatus,
        discovered_count: i32,
        saved_count: i32,
        error_class: Option<&str>,
    ) -> Result<(), DbError> {
        let updated = sqlx::query(
            r#"
            UPDATE import_run
            SET status = $2,
                discovered_count = $3,
                saved_count = $4,
                error_class = $5,
                error_message = $5,
                finished_at = now()
            WHERE id = $1
              AND status IN ('queued', 'running')
            "#,
        )
        .bind(run_id)
        .bind(status.as_str())
        .bind(discovered_count)
        .bind(saved_count)
        .bind(error_class)
        .execute(self.db.pool())
        .await?;
        if updated.rows_affected() != 1 {
            return Err(DbError::RevisionConflict);
        }
        Ok(())
    }

    pub async fn load(&self, run_id: Uuid) -> Result<ImportRunRecord, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id,
                   pixiv_account_id,
                   import_kind,
                   target_pixiv_id,
                   forced,
                   status,
                   params,
                   error_class,
                   error_message
            FROM import_run
            WHERE id = $1
            "#,
        )
        .bind(run_id)
        .fetch_one(self.db.pool())
        .await?;
        run_from_row(&row)
    }

    pub async fn load_by_job(&self, job_id: Uuid) -> Result<ImportRunRecord, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id,
                   pixiv_account_id,
                   import_kind,
                   target_pixiv_id,
                   forced,
                   status,
                   params,
                   error_class,
                   error_message
            FROM import_run
            WHERE job_id = $1
            "#,
        )
        .bind(job_id)
        .fetch_one(self.db.pool())
        .await?;
        run_from_row(&row)
    }

    pub async fn mark_running(&self, run_id: Uuid) -> Result<(), DbError> {
        let updated = sqlx::query(
            r#"
            UPDATE import_run
            SET status = 'running',
                started_at = COALESCE(started_at, now()),
                error_class = NULL,
                error_message = NULL,
                finished_at = NULL
            WHERE id = $1
              AND status IN ('queued', 'running')
            "#,
        )
        .bind(run_id)
        .execute(self.db.pool())
        .await?;
        if updated.rows_affected() != 1 {
            return Err(DbError::RevisionConflict);
        }
        Ok(())
    }

    pub async fn mark_running_for_job(
        &self,
        lease: JobLease,
        run_id: Uuid,
    ) -> Result<ImportRunRecord, DbError> {
        let mut tx = self.db.begin().await?;
        JobRepository::new(self.db.clone())
            .lock_active_lease_in_tx(&mut tx, lease)
            .await?;
        let row = sqlx::query(
            r#"
            SELECT id,
                   job_id,
                   pixiv_account_id,
                   import_kind,
                   target_pixiv_id,
                   forced,
                   status,
                   params,
                   error_class,
                   error_message
            FROM import_run
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(run_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DbError::NotFound)?;
        if row.get::<Option<Uuid>, _>("job_id") != Some(lease.job_id) {
            return Err(DbError::LeaseConflict);
        }
        let run = run_from_row(&row)?;
        let updated = sqlx::query(
            r#"
            UPDATE import_run
            SET status = 'running',
                started_at = COALESCE(started_at, now()),
                error_class = NULL,
                error_message = NULL,
                finished_at = NULL
            WHERE id = $1
              AND job_id = $2
              AND status IN ('queued', 'running')
            "#,
        )
        .bind(run_id)
        .bind(lease.job_id)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(DbError::RevisionConflict);
        }
        tx.commit().await?;
        Ok(run)
    }

    pub async fn record_attempt_failure(
        &self,
        run_id: Uuid,
        error_class: &str,
    ) -> Result<(), DbError> {
        let updated = sqlx::query(
            r#"
            UPDATE import_run
            SET status = 'queued',
                discovered_count = 0,
                saved_count = 0,
                error_class = $2,
                error_message = $2,
                finished_at = NULL
            WHERE id = $1
              AND status = 'running'
            "#,
        )
        .bind(run_id)
        .bind(error_class)
        .execute(self.db.pool())
        .await?;
        if updated.rows_affected() != 1 {
            return Err(DbError::RevisionConflict);
        }
        Ok(())
    }

    pub async fn record_job_attempt_failure(
        &self,
        lease: JobLease,
        run_id: Uuid,
        error_class: &str,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        JobRepository::new(self.db.clone())
            .lock_active_lease_in_tx(&mut tx, lease)
            .await?;
        let updated = sqlx::query(
            r#"
            UPDATE import_run
            SET status = 'queued',
                discovered_count = 0,
                saved_count = 0,
                error_class = $3,
                error_message = $3,
                finished_at = NULL
            WHERE id = $1
              AND job_id = $2
              AND status = 'running'
            "#,
        )
        .bind(run_id)
        .bind(lease.job_id)
        .bind(error_class)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(DbError::RevisionConflict);
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn record_candidate(
        &self,
        run_id: Uuid,
        work_id: i64,
        action: &str,
    ) -> Result<(), DbError> {
        insert_candidate(self.db.pool(), run_id, work_id, action).await
    }

    pub async fn record_candidate_for_job(
        &self,
        lease: JobLease,
        run_id: Uuid,
        work_id: i64,
        action: &str,
    ) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        JobRepository::new(self.db.clone())
            .lock_active_lease_in_tx(&mut tx, lease)
            .await?;
        let job_id: Option<Uuid> =
            sqlx::query_scalar("SELECT job_id FROM import_run WHERE id = $1 FOR UPDATE")
                .bind(run_id)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
        if job_id != Some(lease.job_id) {
            return Err(DbError::LeaseConflict);
        }
        insert_candidate(&mut *tx, run_id, work_id, action).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn finalize_failure(&self, run_id: Uuid, error_class: &str) -> Result<(), DbError> {
        let updated = sqlx::query(
            r#"
            UPDATE import_run
            SET status = 'failed',
                error_class = $2,
                error_message = $2,
                finished_at = now()
            WHERE id = $1
              AND status IN ('queued', 'running')
            "#,
        )
        .bind(run_id)
        .bind(error_class)
        .execute(self.db.pool())
        .await?;
        if updated.rows_affected() == 1 {
            return Ok(());
        }
        let status: Option<String> =
            sqlx::query_scalar("SELECT status FROM import_run WHERE id = $1")
                .bind(run_id)
                .fetch_optional(self.db.pool())
                .await?;
        if status.as_deref() == Some("failed") {
            return Ok(());
        }
        Err(DbError::RevisionConflict)
    }
}

async fn insert_candidate<'e, E>(
    executor: E,
    run_id: Uuid,
    work_id: i64,
    action: &str,
) -> Result<(), DbError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        r#"
        INSERT INTO import_candidate (id, import_run_id, pixiv_work_id, action)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (import_run_id, pixiv_work_id) DO NOTHING
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(run_id)
    .bind(work_id)
    .bind(action)
    .execute(executor)
    .await?;
    Ok(())
}
