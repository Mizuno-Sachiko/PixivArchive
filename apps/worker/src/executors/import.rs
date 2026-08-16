use crate::executors::{ExecutorOutcome, JobExecutor, subscription::PixivContextProvider};
use async_trait::async_trait;
use pixivarchive_application::{
    imports::{ImportService, ImportServiceError},
    jobs::database_error_class,
};
use pixivarchive_db::{Db, ImportJobCompletion, JobCompletion};
use pixivarchive_domain::{
    job::{ClaimedJob, JobErrorClass},
    subscription::ImportRunStatus,
};
use pixivarchive_pixiv::PixivGateway;
use std::sync::Arc;

#[derive(Clone)]
pub struct ImportExecutor<G> {
    db: Db,
    gateway: G,
    context_provider: Arc<dyn PixivContextProvider>,
}

impl<G> ImportExecutor<G>
where
    G: PixivGateway + Clone + 'static,
{
    pub fn new(db: Db, gateway: G, context_provider: Arc<dyn PixivContextProvider>) -> Self {
        Self {
            db,
            gateway,
            context_provider,
        }
    }
}

#[async_trait]
impl<G> JobExecutor for ImportExecutor<G>
where
    G: PixivGateway + Clone + 'static,
{
    async fn execute(&self, job: ClaimedJob) -> ExecutorOutcome {
        let result = self.execute_job(job).await;
        match result {
            Ok(outcome) => outcome,
            Err(error_class) => ExecutorOutcome::failed(error_class, None),
        }
    }
}

impl<G> ImportExecutor<G>
where
    G: PixivGateway + Clone + 'static,
{
    async fn execute_job(&self, job: ClaimedJob) -> Result<ExecutorOutcome, JobErrorClass> {
        let import = ImportService::new(self.db.clone(), self.gateway.clone())
            .load_run_by_job(job.id)
            .await
            .map_err(|error| database_error_class(&error))?;
        if import.status.is_successful_terminal() {
            return Ok(ExecutorOutcome::completed());
        }
        if import.status == ImportRunStatus::Cancelled {
            return Ok(ExecutorOutcome::completed());
        }
        if import.status == ImportRunStatus::Failed {
            let error_class = import
                .error_class
                .as_deref()
                .and_then(JobErrorClass::from_db_value)
                .unwrap_or(JobErrorClass::Permanent);
            return Ok(match import.error_message {
                Some(message) => ExecutorOutcome::failed_with_message(error_class, None, message),
                None => ExecutorOutcome::failed(error_class, None),
            });
        }
        let context = self
            .context_provider
            .context_for_account(import.account_id)
            .await
            .map_err(super::subscription::context_error_class)?;
        let result = ImportService::new(self.db.clone(), self.gateway.clone())
            .execute_queued_job_attempt(job.lease(), job.priority, import.run_id, context)
            .await
            .map_err(import_error_class)?;
        if result.result.status == ImportRunStatus::Failed {
            return Ok(ExecutorOutcome::failed(
                result.error_class.unwrap_or(JobErrorClass::Permanent),
                None,
            ));
        }
        Ok(ExecutorOutcome::Completed(JobCompletion::Import(
            ImportJobCompletion {
                status: result.result.status,
                discovered_count: result.result.discovered_count,
                saved_count: result.result.saved_count,
            },
        )))
    }
}

fn import_error_class(error: ImportServiceError) -> JobErrorClass {
    error.error_class()
}
