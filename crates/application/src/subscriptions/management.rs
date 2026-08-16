use pixivarchive_db::{
    CreateSubscription, Db, DbError, PixivAccountRecord, PixivAccountRepository,
    ScheduledSubscriptionRun as DbScheduledSubscriptionRun, SubscriptionCursorRecord,
    SubscriptionRecord, SubscriptionRepository, SubscriptionRunSummaryRecord, UpdateSubscription,
};
use pixivarchive_domain::{
    job::JobKind,
    pixiv::{PixivRankingContent, PixivRankingMode},
    subscription::{PixivAccountState, SubscriptionKind, SubscriptionRecentState},
};
use serde_json::{Value, json};
use std::collections::HashMap;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq)]
pub struct SubscriptionView {
    pub id: Uuid,
    pub account_id: Uuid,
    pub account_pixiv_user_id: i64,
    pub account_avatar_url: Option<String>,
    pub account_revision: i64,
    pub account_state: PixivAccountState,
    pub rule_id: Option<Uuid>,
    pub name: String,
    pub kind: SubscriptionKind,
    pub enabled: bool,
    pub schedule: Value,
    pub params: Value,
    pub next_run_at: Option<OffsetDateTime>,
    pub pending_run: bool,
    pub recent_state: SubscriptionRecentState,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubscriptionRunView {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub trigger_kind: String,
    pub state: pixivarchive_domain::subscription::SubscriptionRunStatus,
    pub cursor_kind: String,
    pub discovered_count: i32,
    pub ignored_count: i32,
    pub error_class: Option<String>,
    pub trace_id: Option<Uuid>,
    pub started_at: Option<OffsetDateTime>,
    pub finished_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

impl From<SubscriptionRunSummaryRecord> for SubscriptionRunView {
    fn from(record: SubscriptionRunSummaryRecord) -> Self {
        Self {
            id: record.id,
            subscription_id: record.subscription_id,
            trigger_kind: record.trigger_kind,
            state: record.state,
            cursor_kind: record.cursor_kind,
            discovered_count: record.discovered_count,
            ignored_count: record.ignored_count,
            error_class: record.error_class,
            trace_id: record.trace_id,
            started_at: record.started_at,
            finished_at: record.finished_at,
            created_at: record.created_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubscriptionCursorView {
    pub cursor_kind: String,
    pub source_key: String,
    pub value: Value,
}

impl From<SubscriptionCursorRecord> for SubscriptionCursorView {
    fn from(record: SubscriptionCursorRecord) -> Self {
        Self {
            cursor_kind: record.cursor_kind,
            source_key: record.source_key,
            value: record.value,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledSubscriptionRun {
    pub subscription_id: Uuid,
    pub run_id: Uuid,
    pub job_id: Uuid,
    pub trigger_kind: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SubscriptionRunStartError {
    #[error("Pixiv account state {state:?} blocks subscription execution")]
    AccountUnavailable { state: PixivAccountState },
    #[error(transparent)]
    Storage(#[from] DbError),
}

impl From<DbScheduledSubscriptionRun> for ScheduledSubscriptionRun {
    fn from(run: DbScheduledSubscriptionRun) -> Self {
        Self {
            subscription_id: run.subscription_id,
            run_id: run.run_id,
            job_id: run.job_id,
            trigger_kind: run.trigger_kind,
        }
    }
}

#[derive(Clone)]
pub struct SubscriptionService {
    db: Db,
    repository: SubscriptionRepository,
    accounts: PixivAccountRepository,
}

impl SubscriptionService {
    pub fn new(db: Db) -> Self {
        Self {
            repository: SubscriptionRepository::new(db.clone()),
            accounts: PixivAccountRepository::new(db.clone()),
            db,
        }
    }

    pub async fn list(&self) -> Result<Vec<SubscriptionView>, DbError> {
        self.views(self.repository.list_subscriptions().await?)
            .await
    }

    pub async fn get(&self, subscription_id: Uuid) -> Result<SubscriptionView, DbError> {
        self.view(self.repository.subscription(subscription_id).await?)
            .await
    }

    pub async fn runs(
        &self,
        subscription_id: Uuid,
        limit: u16,
    ) -> Result<Vec<SubscriptionRunView>, DbError> {
        self.repository.subscription(subscription_id).await?;
        Ok(self
            .repository
            .list_runs(subscription_id, limit)
            .await?
            .into_iter()
            .map(SubscriptionRunView::from)
            .collect())
    }

    pub async fn cursors(
        &self,
        subscription_id: Uuid,
    ) -> Result<Vec<SubscriptionCursorView>, DbError> {
        self.repository.subscription(subscription_id).await?;
        Ok(self
            .repository
            .list_cursors(subscription_id)
            .await?
            .into_iter()
            .map(SubscriptionCursorView::from)
            .collect())
    }

    pub async fn create(
        &self,
        request: SubscriptionMutationRequest,
    ) -> Result<SubscriptionView, DbError> {
        reject_fixed_subscription(request.kind)?;
        self.accounts.require_current(request.account_id).await?;
        let record = self
            .repository
            .create_subscription(CreateSubscription {
                pixiv_account_id: request.account_id,
                rule_id: request.rule_id,
                name: request.name,
                kind: request.kind,
                interval_minutes: request.interval_minutes,
                lookback_pages: request.lookback_pages,
                params: request.params,
                next_run_at: request.next_run_at,
            })
            .await?;
        self.view(record).await
    }

    pub async fn update(
        &self,
        subscription_id: Uuid,
        expected_revision: i64,
        enabled: bool,
        request: SubscriptionUpdateRequest,
    ) -> Result<SubscriptionView, DbError> {
        reject_fixed_subscription(self.repository.subscription(subscription_id).await?.kind)?;
        let record = self
            .repository
            .update_subscription(UpdateSubscription {
                id: subscription_id,
                expected_revision,
                pixiv_account_id: request.account_id,
                rule_id: request.rule_id,
                name: request.name,
                enabled,
                interval_minutes: request.interval_minutes,
                lookback_pages: request.lookback_pages,
                params: request.params,
                next_run_at: request.next_run_at,
            })
            .await?;
        self.view(record).await
    }

    pub async fn set_enabled(
        &self,
        subscription_id: Uuid,
        expected_revision: i64,
        enabled: bool,
    ) -> Result<SubscriptionView, DbError> {
        let record = self
            .repository
            .set_subscription_enabled(subscription_id, expected_revision, enabled)
            .await?;
        self.view(record).await
    }

    pub async fn delete(
        &self,
        subscription_id: Uuid,
        expected_revision: i64,
    ) -> Result<(), DbError> {
        reject_fixed_subscription(self.repository.subscription(subscription_id).await?.kind)?;
        self.repository
            .delete_subscription(subscription_id, expected_revision)
            .await
    }

    pub async fn create_ranking(
        &self,
        request: RankingSubscriptionRequest,
    ) -> Result<SubscriptionView, DbError> {
        self.accounts.require_current(request.account_id).await?;
        let params = json!({
            "modes": request.modes,
            "contents": request.contents,
        });
        let record = self
            .repository
            .create_subscription(CreateSubscription {
                pixiv_account_id: request.account_id,
                rule_id: request.rule_id,
                name: request.name,
                kind: SubscriptionKind::Ranking,
                interval_minutes: request.interval_minutes,
                lookback_pages: request.lookback_pages,
                params,
                next_run_at: request.next_run_at,
            })
            .await?;
        self.view(record).await
    }

    pub async fn start_manual_run(
        &self,
        subscription_id: Uuid,
        backfill: bool,
    ) -> Result<ScheduledSubscriptionRun, SubscriptionRunStartError> {
        let subscription = self.repository.subscription(subscription_id).await?;
        let account = self.accounts.get(subscription.pixiv_account_id).await?;
        if account.state.blocks_subscription_runs() {
            return Err(SubscriptionRunStartError::AccountUnavailable {
                state: account.state,
            });
        }
        let priority = crate::settings::effective_job_priority_policy(&self.db)
            .await?
            .priority_for(JobKind::for_subscription(subscription.kind));
        Ok(self
            .repository
            .start_manual_run_with_priority(subscription_id, backfill, priority)
            .await
            .map(ScheduledSubscriptionRun::from)?)
    }

    pub async fn stop_active_run(
        &self,
        subscription_id: Uuid,
    ) -> Result<SubscriptionView, DbError> {
        self.repository.stop_active_run(subscription_id).await?;
        self.get(subscription_id).await
    }

    async fn view(&self, record: SubscriptionRecord) -> Result<SubscriptionView, DbError> {
        let account = self.accounts.get(record.pixiv_account_id).await?;
        Ok(subscription_view(record, &account))
    }

    async fn views(
        &self,
        records: Vec<SubscriptionRecord>,
    ) -> Result<Vec<SubscriptionView>, DbError> {
        let mut account_ids = records
            .iter()
            .map(|record| record.pixiv_account_id)
            .collect::<Vec<_>>();
        account_ids.sort_unstable();
        account_ids.dedup();
        let accounts = self
            .accounts
            .get_many(&account_ids)
            .await?
            .into_iter()
            .map(|account| (account.id, account))
            .collect::<HashMap<_, _>>();
        records
            .into_iter()
            .map(|record| {
                let account = accounts
                    .get(&record.pixiv_account_id)
                    .ok_or(DbError::NotFound)?;
                Ok(subscription_view(record, account))
            })
            .collect()
    }
}

fn subscription_view(record: SubscriptionRecord, account: &PixivAccountRecord) -> SubscriptionView {
    SubscriptionView {
        id: record.id,
        account_id: record.pixiv_account_id,
        account_pixiv_user_id: account.pixiv_user_id,
        account_avatar_url: account.avatar_url.clone(),
        account_revision: account.revision,
        account_state: account.state,
        rule_id: record.rule_id,
        name: record.name,
        kind: record.kind,
        enabled: record.enabled,
        schedule: record.schedule,
        params: record.params,
        next_run_at: record.next_run_at,
        pending_run: record.pending_run,
        recent_state: record.recent_state,
        revision: record.revision,
    }
}

#[derive(Clone, Debug)]
pub struct RankingSubscriptionRequest {
    pub account_id: Uuid,
    pub name: String,
    pub modes: Vec<PixivRankingMode>,
    pub contents: Vec<PixivRankingContent>,
    pub interval_minutes: i64,
    pub lookback_pages: i64,
    pub rule_id: Option<Uuid>,
    pub next_run_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug)]
pub struct SubscriptionMutationRequest {
    pub account_id: Uuid,
    pub rule_id: Option<Uuid>,
    pub name: String,
    pub kind: SubscriptionKind,
    pub interval_minutes: i64,
    pub lookback_pages: i64,
    pub params: Value,
    pub next_run_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug)]
pub struct SubscriptionUpdateRequest {
    pub account_id: Uuid,
    pub rule_id: Option<Uuid>,
    pub name: String,
    pub interval_minutes: i64,
    pub lookback_pages: i64,
    pub params: Value,
    pub next_run_at: Option<OffsetDateTime>,
}

fn reject_fixed_subscription(kind: SubscriptionKind) -> Result<(), DbError> {
    if matches!(
        kind,
        SubscriptionKind::Following | SubscriptionKind::Bookmarks
    ) {
        return Err(DbError::InvalidValue(
            "the fixed subscription is managed from its dedicated page".to_owned(),
        ));
    }
    Ok(())
}
