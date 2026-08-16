use super::{
    ImportRepository,
    model::{ImportRunSummaryRecord, QueueImportRequest, import_params, summary_from_row},
};
use crate::{DbError, JobRepository};
use pixivarchive_domain::{
    job::{JobKind, JobPriority, NewJob},
    subscription::ImportKind,
};
use serde_json::json;
use uuid::Uuid;

impl ImportRepository {
    pub async fn queue(
        &self,
        request: QueueImportRequest,
        priority: JobPriority,
    ) -> Result<ImportRunSummaryRecord, DbError> {
        let mut tx = self.db.begin().await?;
        let run_id = Uuid::now_v7();
        let job_kind = match request.kind {
            ImportKind::Artist => JobKind::ImportArtist,
            ImportKind::Work => JobKind::ImportWork,
        };
        let mut job = NewJob::for_kind(
            priority,
            job_kind,
            json!({
                "import_run_id": run_id.to_string(),
                "import_kind": request.kind,
                "target_pixiv_id": request.target_pixiv_id,
            }),
        );
        job.pixiv_account_id = Some(request.account_id);
        let job_id = JobRepository::new(self.db.clone())
            .enqueue_in_tx(&mut tx, job)
            .await?;
        let row = sqlx::query(
            r#"
            INSERT INTO import_run (
                id,
                job_id,
                pixiv_account_id,
                import_kind,
                target_pixiv_id,
                forced,
                status,
                params
            )
            VALUES ($1, $2, $3, $4, $5, $6, 'queued', $7)
            RETURNING id,
                      job_id,
                      pixiv_account_id,
                      import_kind,
                      target_pixiv_id,
                      forced,
                      params,
                      status,
                      discovered_count,
                      saved_count,
                      error_class,
                      error_message,
                      created_at,
                      finished_at
            "#,
        )
        .bind(run_id)
        .bind(job_id)
        .bind(request.account_id)
        .bind(request.kind.as_str())
        .bind(request.target_pixiv_id)
        .bind(request.forced)
        .bind(sqlx::types::Json(import_params(
            request.forced,
            request.rule_document.as_ref(),
        )))
        .fetch_one(&mut *tx)
        .await?;
        let queued = summary_from_row(&row)?;
        tx.commit().await?;
        Ok(queued)
    }

    pub async fn list(&self, limit: u16) -> Result<Vec<ImportRunSummaryRecord>, DbError> {
        if limit == 0 || limit > 500 {
            return Err(DbError::InvalidValue(
                "import run limit must be between 1 and 500".to_owned(),
            ));
        }
        let rows = sqlx::query(
            r#"
            SELECT id,
                   job_id,
                   pixiv_account_id,
                   import_kind,
                   target_pixiv_id,
                   forced,
                   params,
                   status,
                   discovered_count,
                   saved_count,
                   error_class,
                   error_message,
                   created_at,
                   finished_at
            FROM import_run
            ORDER BY created_at DESC, id DESC
            LIMIT $1
            "#,
        )
        .bind(i64::from(limit))
        .fetch_all(self.db.pool())
        .await?;
        rows.iter().map(summary_from_row).collect()
    }
}
