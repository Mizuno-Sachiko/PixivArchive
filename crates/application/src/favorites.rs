use crate::subscriptions::{
    ALLOWED_SYNC_INTERVAL_MINUTES, ScheduledSubscriptionRun, SubscriptionRunStartError,
    SubscriptionService, SubscriptionView,
};
use pixivarchive_db::{
    BookmarkRepository, Db, DbError, PixivAccountRepository, SubscriptionRepository,
};
use pixivarchive_domain::job::JobKind;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct FavoritesAdminService {
    db: Db,
    accounts: PixivAccountRepository,
    bookmarks: BookmarkRepository,
    subscriptions: SubscriptionRepository,
    subscription_runs: SubscriptionService,
}

impl FavoritesAdminService {
    pub fn new(db: Db) -> Self {
        Self {
            db: db.clone(),
            accounts: PixivAccountRepository::new(db.clone()),
            bookmarks: BookmarkRepository::new(db.clone()),
            subscriptions: SubscriptionRepository::new(db.clone()),
            subscription_runs: SubscriptionService::new(db),
        }
    }

    pub async fn current(&self) -> Result<FavoritesAdminState, DbError> {
        let account = self.accounts.current().await?.ok_or(DbError::NotFound)?;
        self.for_account(account.id).await
    }

    async fn for_account(&self, account_id: Uuid) -> Result<FavoritesAdminState, DbError> {
        let subscription = self
            .subscriptions
            .ensure_bookmarks_subscription(account_id, OffsetDateTime::now_utc())
            .await?;
        Ok(FavoritesAdminState {
            last_full_reconciled_at: self.bookmarks.last_full_reconciled_at(account_id).await?,
            subscription: self.subscription_runs.get(subscription.id).await?,
        })
    }

    pub async fn update(
        &self,
        expected_account_id: Uuid,
        expected_revision: i64,
        enabled: bool,
        interval_minutes: i64,
    ) -> Result<FavoritesAdminState, DbError> {
        if !ALLOWED_SYNC_INTERVAL_MINUTES.contains(&interval_minutes) {
            return Err(DbError::InvalidValue(
                "unsupported bookmark synchronization interval".to_owned(),
            ));
        }
        let account = self.accounts.require_current(expected_account_id).await?;
        if account.state.blocks_subscription_runs() {
            return Err(DbError::RevisionConflict);
        }
        let account_id = account.id;
        let current = self.for_account(account_id).await?;
        let now = OffsetDateTime::now_utc();
        let initial_backfill_priority = if enabled
            && !current.subscription.enabled
            && !account.state.blocks_subscription_runs()
        {
            Some(
                crate::settings::effective_job_priority_policy(&self.db)
                    .await?
                    .priority_for(JobKind::BookmarksCollection),
            )
        } else {
            None
        };
        let subscription = self
            .subscriptions
            .configure_bookmarks_subscription(
                account_id,
                expected_revision,
                enabled,
                interval_minutes,
                now,
                initial_backfill_priority,
            )
            .await?;
        Ok(FavoritesAdminState {
            last_full_reconciled_at: self.bookmarks.last_full_reconciled_at(account_id).await?,
            subscription: self.subscription_runs.get(subscription.id).await?,
        })
    }

    pub async fn start_manual_run(
        &self,
        expected_account_id: Uuid,
    ) -> Result<ScheduledSubscriptionRun, SubscriptionRunStartError> {
        let account_id = self.accounts.require_current(expected_account_id).await?.id;
        let subscription = self.for_account(account_id).await?.subscription;
        self.subscription_runs
            .start_manual_run(subscription.id, true)
            .await
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FavoritesAdminState {
    pub subscription: SubscriptionView,
    pub last_full_reconciled_at: Option<OffsetDateTime>,
}
