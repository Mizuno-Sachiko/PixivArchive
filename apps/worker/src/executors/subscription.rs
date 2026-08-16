use crate::executors::{ExecutorOutcome, JobExecutor};
use async_trait::async_trait;
use pixivarchive_application::{
    jobs::database_error_class,
    pixiv_accounts::{
        PixivAccountContextError, PixivAccountContextFactory, PixivCookieCipherError,
    },
    subscriptions::{SubscriptionExecutionService, SubscriptionUnitRequest},
};
use pixivarchive_db::{Db, JobCompletion, SubscriptionRepository};
use pixivarchive_domain::{
    job::{ClaimedJob, JobErrorClass},
    subscription::SubscriptionRunStatus,
};
use pixivarchive_pixiv::{PixivGateway, PixivRequestContext};
use std::sync::Arc;
use uuid::Uuid;

#[async_trait]
pub trait PixivContextProvider: Send + Sync {
    async fn context_for_account(
        &self,
        account_id: Uuid,
    ) -> Result<PixivRequestContext, PixivAccountContextError>;
}

#[async_trait]
impl PixivContextProvider for PixivAccountContextFactory {
    async fn context_for_account(
        &self,
        account_id: Uuid,
    ) -> Result<PixivRequestContext, PixivAccountContextError> {
        self.load(account_id).await
    }
}

#[derive(Clone)]
pub struct SubscriptionExecutor<G> {
    db: Db,
    gateway: G,
    context_provider: Arc<dyn PixivContextProvider>,
}

impl<G> SubscriptionExecutor<G>
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
impl<G> JobExecutor for SubscriptionExecutor<G>
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

impl<G> SubscriptionExecutor<G>
where
    G: PixivGateway + Clone + 'static,
{
    async fn execute_job(&self, job: ClaimedJob) -> Result<ExecutorOutcome, JobErrorClass> {
        let unit = SubscriptionRepository::new(self.db.clone())
            .load_unit_by_job(job.id)
            .await
            .map_err(|error| database_error_class(&error))?;
        if unit.state == SubscriptionRunStatus::Succeeded {
            return Ok(ExecutorOutcome::completed());
        }
        if unit.state == SubscriptionRunStatus::Failed {
            let error_class = unit
                .error_class
                .as_deref()
                .and_then(JobErrorClass::from_db_value)
                .unwrap_or(JobErrorClass::Permanent);
            return Ok(match unit.error_message {
                Some(message) => ExecutorOutcome::failed_with_message(error_class, None, message),
                None => ExecutorOutcome::failed(error_class, None),
            });
        }
        let context = self
            .context_provider
            .context_for_account(unit.pixiv_account_id)
            .await
            .map_err(context_error_class)?;
        let result = SubscriptionExecutionService::new(self.db.clone(), self.gateway.clone())
            .execute_unit_job_attempt(
                job.lease(),
                job.priority,
                SubscriptionUnitRequest {
                    context,
                    unit_id: unit.id,
                },
            )
            .await
            .map_err(|error| database_error_class(&error))?;
        if let Some(error_class) = result.result.error_class {
            let error_class = job_error_class(&error_class);
            return Ok(match result.result.error_message {
                Some(message) => ExecutorOutcome::failed_with_message(error_class, None, message),
                None => ExecutorOutcome::failed(error_class, None),
            });
        }
        let completion = result.completion.ok_or(JobErrorClass::Server)?;
        Ok(ExecutorOutcome::Completed(JobCompletion::Subscription(
            completion,
        )))
    }
}

fn job_error_class(value: &str) -> JobErrorClass {
    JobErrorClass::from_db_value(value).unwrap_or(JobErrorClass::Permanent)
}

pub(super) fn context_error_class(error: PixivAccountContextError) -> JobErrorClass {
    match error {
        PixivAccountContextError::AccountUnavailable {
            state: pixivarchive_domain::subscription::PixivAccountState::CredentialInvalid,
        }
        | PixivAccountContextError::Cipher(PixivCookieCipherError::InvalidCredential) => {
            JobErrorClass::CredentialInvalid
        }
        PixivAccountContextError::AccountUnavailable { .. }
        | PixivAccountContextError::Cipher(_) => JobErrorClass::Server,
        PixivAccountContextError::Storage(error) => database_error_class(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::context_error_class;
    use pixivarchive_application::pixiv_accounts::{
        PixivAccountContextError, PixivCookieCipherError,
    };
    use pixivarchive_domain::job::JobErrorClass;

    #[test]
    fn invalid_stored_cookie_blocks_the_account_without_hiding_internal_failures() {
        assert_eq!(
            context_error_class(PixivAccountContextError::Cipher(
                PixivCookieCipherError::InvalidCredential
            )),
            JobErrorClass::CredentialInvalid
        );
        assert_eq!(
            context_error_class(PixivAccountContextError::Cipher(
                PixivCookieCipherError::InvalidKeyring
            )),
            JobErrorClass::Server
        );
    }
}
