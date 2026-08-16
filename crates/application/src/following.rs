use crate::{
    pixiv_accounts::{PixivAccountContextError, PixivAccountContextFactory},
    subscriptions::{
        ALLOWED_SYNC_INTERVAL_MINUTES, ScheduledSubscriptionRun, SubscriptionRunStartError,
        SubscriptionService, SubscriptionView,
    },
};
use async_trait::async_trait;
use pixivarchive_db::{
    Db, DbError, FollowingAuthorRecord as DbFollowingAuthorRecord, FollowingAuthorSnapshot,
    FollowingRepository, PixivAccountRepository, SubscriptionRepository, SyncFollowingAuthors,
};
use pixivarchive_domain::job::JobLease;
use pixivarchive_domain::pixiv::{
    PixivArtistFollowRequest, PixivArtistFollowState, PixivFollowingRequest,
    PixivFollowingVisibility,
};
use pixivarchive_pixiv::{
    PixivAssetGateway, PixivError, PixivGateway, PixivMediaResponse, PixivRequestContext,
};
use std::{collections::BTreeMap, sync::Arc};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

const FOLLOWING_PAGE_SIZE: u32 = 100;

#[derive(Clone)]
pub struct FollowingAdminService {
    accounts: PixivAccountRepository,
    authors: FollowingRepository,
    subscriptions: SubscriptionRepository,
    subscription_runs: SubscriptionService,
}

impl FollowingAdminService {
    pub fn new(db: Db) -> Self {
        Self {
            accounts: PixivAccountRepository::new(db.clone()),
            authors: FollowingRepository::new(db.clone()),
            subscriptions: SubscriptionRepository::new(db.clone()),
            subscription_runs: SubscriptionService::new(db),
        }
    }

    pub async fn current(&self) -> Result<FollowingAdminState, DbError> {
        let account_id = self.current_account_id().await?;
        self.state_for_account(account_id).await
    }

    pub async fn current_for(
        &self,
        expected_account_id: Uuid,
    ) -> Result<FollowingAdminState, DbError> {
        self.accounts.require_current(expected_account_id).await?;
        self.state_for_account(expected_account_id).await
    }

    async fn state_for_account(&self, account_id: Uuid) -> Result<FollowingAdminState, DbError> {
        let subscription = self
            .subscriptions
            .following_subscription(account_id)
            .await?;
        Ok(FollowingAdminState {
            last_full_reconciled_at: self
                .subscriptions
                .last_successful_backfill_at(subscription.id)
                .await?,
            subscription: self.subscription_runs.get(subscription.id).await?,
            authors: self
                .authors
                .list(account_id)
                .await?
                .into_iter()
                .map(FollowingAuthorView::from)
                .collect(),
        })
    }

    pub async fn configure_subscription(
        &self,
        expected_account_id: Uuid,
        expected_revision: i64,
        enabled: bool,
        interval_minutes: i64,
    ) -> Result<SubscriptionView, DbError> {
        if !ALLOWED_SYNC_INTERVAL_MINUTES.contains(&interval_minutes) {
            return Err(DbError::InvalidValue(
                "unsupported following synchronization interval".to_owned(),
            ));
        }
        self.accounts.require_current(expected_account_id).await?;
        let subscription = self
            .subscriptions
            .configure_following_subscription(
                expected_account_id,
                expected_revision,
                enabled,
                interval_minutes,
                OffsetDateTime::now_utc(),
            )
            .await?;
        self.subscription_runs.get(subscription.id).await
    }

    pub async fn set_author_enabled(
        &self,
        expected_account_id: Uuid,
        pixiv_artist_id: i64,
        enabled: bool,
    ) -> Result<FollowingAuthorView, DbError> {
        let account_id = self.accounts.require_current(expected_account_id).await?.id;
        self.authors
            .set_enabled(account_id, pixiv_artist_id, enabled)
            .await?;
        self.authors
            .list(account_id)
            .await?
            .into_iter()
            .find(|author| author.pixiv_artist_id == pixiv_artist_id)
            .map(FollowingAuthorView::from)
            .ok_or(DbError::NotFound)
    }

    pub async fn set_authors_enabled(
        &self,
        expected_account_id: Uuid,
        pixiv_artist_ids: Vec<i64>,
        enabled: bool,
    ) -> Result<FollowingAdminState, DbError> {
        let account_id = self.accounts.require_current(expected_account_id).await?.id;
        self.authors
            .set_enabled_many(account_id, &pixiv_artist_ids, enabled)
            .await?;
        self.state_for_account(account_id).await
    }

    pub async fn author(&self, pixiv_artist_id: i64) -> Result<FollowingAuthorView, DbError> {
        self.authors
            .get(self.current_account_id().await?, pixiv_artist_id)
            .await
            .map(FollowingAuthorView::from)
    }

    pub async fn start_manual_run(
        &self,
        expected_account_id: Uuid,
        backfill: bool,
    ) -> Result<ScheduledSubscriptionRun, SubscriptionRunStartError> {
        self.accounts.require_current(expected_account_id).await?;
        let subscription = self
            .subscriptions
            .following_subscription(expected_account_id)
            .await?;
        self.subscription_runs
            .start_manual_run(subscription.id, backfill)
            .await
    }

    async fn current_account_id(&self) -> Result<Uuid, DbError> {
        self.accounts
            .current()
            .await?
            .map(|account| account.id)
            .ok_or(DbError::NotFound)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FollowingAdminState {
    pub subscription: SubscriptionView,
    pub authors: Vec<FollowingAuthorView>,
    pub last_full_reconciled_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FollowingAuthorView {
    pub pixiv_artist_id: i64,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub visibility: PixivFollowingVisibility,
    pub enabled: bool,
    pub refreshed_at: OffsetDateTime,
    pub last_collected_at: Option<OffsetDateTime>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArtistFollowStateView {
    pub pixiv_artist_id: i64,
    pub followed: bool,
}

#[async_trait]
pub trait ArtistFollowCommandPort: Send + Sync {
    async fn status(
        &self,
        expected_account_id: Uuid,
        pixiv_artist_id: i64,
    ) -> Result<ArtistFollowStateView, ArtistFollowCommandError>;

    async fn set_followed(
        &self,
        expected_account_id: Uuid,
        pixiv_artist_id: i64,
        followed: bool,
    ) -> Result<ArtistFollowStateView, ArtistFollowCommandError>;
}

#[derive(Clone, Default)]
pub struct DisabledArtistFollowCommandPort;

#[async_trait]
impl ArtistFollowCommandPort for DisabledArtistFollowCommandPort {
    async fn status(
        &self,
        _expected_account_id: Uuid,
        _pixiv_artist_id: i64,
    ) -> Result<ArtistFollowStateView, ArtistFollowCommandError> {
        Err(ArtistFollowCommandError::Unavailable)
    }

    async fn set_followed(
        &self,
        _expected_account_id: Uuid,
        _pixiv_artist_id: i64,
        _followed: bool,
    ) -> Result<ArtistFollowStateView, ArtistFollowCommandError> {
        Err(ArtistFollowCommandError::Unavailable)
    }
}

#[derive(Clone)]
pub struct LiveArtistFollowCommandPort<G> {
    accounts: PixivAccountRepository,
    authors: FollowingRepository,
    gateway: Arc<G>,
    contexts: PixivAccountContextFactory,
}

impl<G> LiveArtistFollowCommandPort<G>
where
    G: PixivGateway + 'static,
{
    pub fn new(db: Db, gateway: G, contexts: PixivAccountContextFactory) -> Self {
        Self {
            accounts: PixivAccountRepository::new(db.clone()),
            authors: FollowingRepository::new(db),
            gateway: Arc::new(gateway),
            contexts,
        }
    }

    async fn account_context(
        &self,
        expected_account_id: Uuid,
    ) -> Result<(pixivarchive_db::PixivAccountRecord, PixivRequestContext), ArtistFollowCommandError>
    {
        let account = self.accounts.require_current(expected_account_id).await?;
        let context = self.contexts.context_for_record(&account)?;
        Ok((account, context))
    }

    async fn pixiv_state(
        &self,
        context: &PixivRequestContext,
        pixiv_artist_id: i64,
    ) -> Result<PixivArtistFollowState, ArtistFollowCommandError> {
        Ok(self
            .gateway
            .artist_follow_state(context, pixiv_artist_id)
            .await?
            .value)
    }

    async fn sync_local_projection(
        &self,
        account_id: Uuid,
        verified_state: &PixivArtistFollowState,
    ) -> Result<(), ArtistFollowCommandError> {
        if verified_state.followed {
            self.authors
                .upsert_author(
                    account_id,
                    OffsetDateTime::now_utc(),
                    FollowingAuthorSnapshot {
                        pixiv_artist_id: verified_state.artist_id,
                        display_name: verified_state.name.clone(),
                        avatar_url: verified_state.profile_image_url.clone(),
                        visibility: PixivFollowingVisibility::Public,
                    },
                )
                .await?;
        } else {
            self.authors
                .remove_author(account_id, verified_state.artist_id)
                .await?;
        }
        Ok(())
    }
}

#[async_trait]
impl<G> ArtistFollowCommandPort for LiveArtistFollowCommandPort<G>
where
    G: PixivGateway + 'static,
{
    async fn status(
        &self,
        expected_account_id: Uuid,
        pixiv_artist_id: i64,
    ) -> Result<ArtistFollowStateView, ArtistFollowCommandError> {
        let (_, context) = self.account_context(expected_account_id).await?;
        let state = self.pixiv_state(&context, pixiv_artist_id).await?;
        Ok(ArtistFollowStateView {
            pixiv_artist_id: state.artist_id,
            followed: state.followed,
        })
    }

    async fn set_followed(
        &self,
        expected_account_id: Uuid,
        pixiv_artist_id: i64,
        followed: bool,
    ) -> Result<ArtistFollowStateView, ArtistFollowCommandError> {
        let (account, context) = self.account_context(expected_account_id).await?;
        let current = self.pixiv_state(&context, pixiv_artist_id).await?;
        if current.followed == followed {
            self.sync_local_projection(account.id, &current).await?;
            return Ok(ArtistFollowStateView {
                pixiv_artist_id: current.artist_id,
                followed: current.followed,
            });
        }

        if followed {
            self.gateway
                .add_artist_follow(
                    &context,
                    PixivArtistFollowRequest {
                        artist_id: pixiv_artist_id,
                        visibility: PixivFollowingVisibility::Public,
                    },
                )
                .await?;
        } else {
            self.gateway
                .remove_artist_follow(&context, pixiv_artist_id)
                .await?;
        }

        let verified = self.pixiv_state(&context, pixiv_artist_id).await?;
        if verified.followed != followed {
            return Err(ArtistFollowCommandError::StateMismatch);
        }
        self.sync_local_projection(account.id, &verified).await?;
        Ok(ArtistFollowStateView {
            pixiv_artist_id: verified.artist_id,
            followed: verified.followed,
        })
    }
}

impl From<DbFollowingAuthorRecord> for FollowingAuthorView {
    fn from(author: DbFollowingAuthorRecord) -> Self {
        Self {
            pixiv_artist_id: author.pixiv_artist_id,
            display_name: author.display_name,
            avatar_url: author.avatar_url,
            visibility: author.visibility,
            enabled: author.enabled,
            refreshed_at: author.refreshed_at,
            last_collected_at: author.last_collected_at,
        }
    }
}

#[async_trait]
pub trait FollowingRefreshPort: Send + Sync {
    async fn refresh(
        &self,
        expected_account_id: Uuid,
    ) -> Result<Vec<FollowingAuthorView>, FollowingRefreshError>;
}

#[async_trait]
pub trait FollowingAvatarPort: Send + Sync {
    async fn fetch(&self, source: String) -> Result<PixivMediaResponse, FollowingAvatarError>;

    async fn fetch_for_account(
        &self,
        _account_id: Uuid,
        source: String,
    ) -> Result<PixivMediaResponse, FollowingAvatarError> {
        self.fetch(source).await
    }
}

#[derive(Clone, Default)]
pub struct DisabledFollowingAvatarPort;

#[async_trait]
impl FollowingAvatarPort for DisabledFollowingAvatarPort {
    async fn fetch(&self, _source: String) -> Result<PixivMediaResponse, FollowingAvatarError> {
        Err(FollowingAvatarError::Unavailable)
    }
}

#[derive(Clone)]
pub struct LiveFollowingAvatarPort<G> {
    accounts: PixivAccountRepository,
    gateway: G,
    contexts: PixivAccountContextFactory,
}

impl<G> LiveFollowingAvatarPort<G>
where
    G: PixivAssetGateway + 'static,
{
    pub fn new(db: Db, gateway: G, contexts: PixivAccountContextFactory) -> Self {
        Self {
            accounts: PixivAccountRepository::new(db),
            gateway,
            contexts,
        }
    }
}

#[async_trait]
impl<G> FollowingAvatarPort for LiveFollowingAvatarPort<G>
where
    G: PixivAssetGateway + 'static,
{
    async fn fetch(&self, source: String) -> Result<PixivMediaResponse, FollowingAvatarError> {
        let account = self
            .accounts
            .current()
            .await?
            .ok_or(FollowingAvatarError::NotConfigured)?;
        let context = self.contexts.context_for_record(&account)?;
        self.gateway
            .asset(&context, source)
            .await
            .map_err(FollowingAvatarError::from)
    }

    async fn fetch_for_account(
        &self,
        account_id: Uuid,
        source: String,
    ) -> Result<PixivMediaResponse, FollowingAvatarError> {
        let account = self.accounts.get(account_id).await?;
        let context = self.contexts.context_for_record(&account)?;
        self.gateway
            .asset(&context, source)
            .await
            .map_err(FollowingAvatarError::from)
    }
}

#[derive(Clone, Default)]
pub struct DisabledFollowingRefreshPort;

#[async_trait]
impl FollowingRefreshPort for DisabledFollowingRefreshPort {
    async fn refresh(
        &self,
        _expected_account_id: Uuid,
    ) -> Result<Vec<FollowingAuthorView>, FollowingRefreshError> {
        Err(FollowingRefreshError::Unavailable)
    }
}

#[derive(Clone)]
pub struct LiveFollowingRefreshPort<G> {
    accounts: PixivAccountRepository,
    following: FollowingService<G>,
    contexts: PixivAccountContextFactory,
}

impl<G> LiveFollowingRefreshPort<G>
where
    G: PixivGateway + 'static,
{
    pub fn new(db: Db, gateway: G, contexts: PixivAccountContextFactory) -> Self {
        Self {
            accounts: PixivAccountRepository::new(db.clone()),
            following: FollowingService::new(db, gateway),
            contexts,
        }
    }
}

#[async_trait]
impl<G> FollowingRefreshPort for LiveFollowingRefreshPort<G>
where
    G: PixivGateway + 'static,
{
    async fn refresh(
        &self,
        expected_account_id: Uuid,
    ) -> Result<Vec<FollowingAuthorView>, FollowingRefreshError> {
        let account = self.accounts.require_current(expected_account_id).await?;
        let context = self.contexts.context_for_record(&account)?;
        Ok(self.following.refresh(account.id, &context).await?)
    }
}

#[derive(Clone)]
pub struct FollowingService<G> {
    accounts: PixivAccountRepository,
    authors: FollowingRepository,
    subscriptions: SubscriptionRepository,
    subscription_views: SubscriptionService,
    gateway: Arc<G>,
}

impl<G> FollowingService<G>
where
    G: PixivGateway + 'static,
{
    pub fn new(db: Db, gateway: G) -> Self {
        Self::from_shared(db, Arc::new(gateway))
    }

    pub(crate) fn from_shared(db: Db, gateway: Arc<G>) -> Self {
        Self {
            accounts: PixivAccountRepository::new(db.clone()),
            authors: FollowingRepository::new(db.clone()),
            subscriptions: SubscriptionRepository::new(db.clone()),
            subscription_views: SubscriptionService::new(db),
            gateway,
        }
    }

    pub async fn ensure_subscription(&self, account_id: Uuid) -> Result<SubscriptionView, DbError> {
        let subscription = self
            .subscriptions
            .ensure_following_subscription(account_id, OffsetDateTime::now_utc())
            .await?;
        self.subscription_views.get(subscription.id).await
    }

    pub async fn list(&self, account_id: Uuid) -> Result<Vec<FollowingAuthorView>, DbError> {
        Ok(self
            .authors
            .list(account_id)
            .await?
            .into_iter()
            .map(FollowingAuthorView::from)
            .collect())
    }

    pub async fn set_enabled(
        &self,
        account_id: Uuid,
        pixiv_artist_id: i64,
        enabled: bool,
    ) -> Result<(), DbError> {
        self.authors
            .set_enabled(account_id, pixiv_artist_id, enabled)
            .await
    }

    pub async fn mark_enabled_collected(
        &self,
        account_id: Uuid,
        collected_at: OffsetDateTime,
    ) -> Result<u64, DbError> {
        self.authors
            .mark_enabled_collected(account_id, collected_at)
            .await
    }

    pub async fn mark_enabled_collected_for_job(
        &self,
        lease: JobLease,
        account_id: Uuid,
        collected_at: OffsetDateTime,
    ) -> Result<u64, DbError> {
        self.authors
            .mark_enabled_collected_for_job(lease, account_id, collected_at)
            .await
    }

    pub async fn refresh(
        &self,
        account_id: Uuid,
        context: &PixivRequestContext,
    ) -> Result<Vec<FollowingAuthorView>, FollowingServiceError> {
        self.refresh_with_lease(None, account_id, context).await
    }

    pub async fn refresh_for_job(
        &self,
        lease: JobLease,
        account_id: Uuid,
        context: &PixivRequestContext,
    ) -> Result<Vec<FollowingAuthorView>, FollowingServiceError> {
        self.refresh_with_lease(Some(lease), account_id, context)
            .await
    }

    async fn refresh_with_lease(
        &self,
        lease: Option<JobLease>,
        account_id: Uuid,
        context: &PixivRequestContext,
    ) -> Result<Vec<FollowingAuthorView>, FollowingServiceError> {
        let account = self.accounts.get(account_id).await?;
        if account.pixiv_user_id != context.user_id() {
            return Err(DbError::InvalidValue(
                "Pixiv account identity does not match the saved credential".to_owned(),
            )
            .into());
        }

        let mut authors = BTreeMap::new();
        for visibility in [
            PixivFollowingVisibility::Public,
            PixivFollowingVisibility::Private,
        ] {
            let mut request = PixivFollowingRequest {
                user_id: context.user_id(),
                visibility,
                offset: 0,
                limit: FOLLOWING_PAGE_SIZE,
                language: "zh".to_owned(),
            };
            loop {
                let response = self.gateway.following_page(context, request).await?;
                for artist in response.value.items {
                    authors.insert(
                        artist.pixiv_id,
                        FollowingAuthorSnapshot {
                            pixiv_artist_id: artist.pixiv_id,
                            display_name: artist.name,
                            avatar_url: artist.profile_image_url,
                            visibility,
                        },
                    );
                }
                let Some(cursor) = response.value.next_cursor else {
                    break;
                };
                request = PixivFollowingRequest {
                    user_id: cursor.user_id,
                    visibility: cursor.visibility,
                    offset: cursor.offset,
                    limit: cursor.limit,
                    language: cursor.language,
                };
            }
        }

        let input = SyncFollowingAuthors {
            account_id,
            refreshed_at: OffsetDateTime::now_utc(),
            authors: authors.into_values().collect(),
        };
        match lease {
            Some(lease) => self.authors.sync_authors_for_job(lease, input).await?,
            None => self.authors.sync_authors(input).await?,
        }
        Ok(self
            .authors
            .list(account_id)
            .await?
            .into_iter()
            .map(FollowingAuthorView::from)
            .collect())
    }
}

#[derive(Debug, Error)]
pub enum FollowingServiceError {
    #[error("following author storage failed")]
    Storage(#[from] DbError),
    #[error("Pixiv following request failed")]
    Pixiv(#[from] PixivError),
}

#[derive(Debug, Error)]
pub enum FollowingRefreshError {
    #[error("Pixiv account is not configured")]
    NotConfigured,
    #[error("following refresh is unavailable")]
    Unavailable,
    #[error("following storage failed")]
    Storage(#[from] DbError),
    #[error("Pixiv account request context is unavailable")]
    Context(#[from] PixivAccountContextError),
    #[error("following refresh failed")]
    Refresh(#[from] FollowingServiceError),
}

#[derive(Debug, Error)]
pub enum FollowingAvatarError {
    #[error("Pixiv account is not configured")]
    NotConfigured,
    #[error("following avatar loading is unavailable")]
    Unavailable,
    #[error("following avatar storage failed")]
    Storage(#[from] DbError),
    #[error("Pixiv account request context is unavailable")]
    Context(#[from] PixivAccountContextError),
    #[error("Pixiv avatar request failed")]
    Pixiv(#[from] PixivError),
}

#[derive(Debug, Error)]
pub enum ArtistFollowCommandError {
    #[error("Pixiv account is not configured")]
    NotConfigured,
    #[error("artist follow commands are unavailable")]
    Unavailable,
    #[error("following author storage failed")]
    Storage(#[from] DbError),
    #[error("Pixiv account request context is unavailable")]
    Context(#[from] PixivAccountContextError),
    #[error("Pixiv author follow request failed")]
    Pixiv(#[from] PixivError),
    #[error("Pixiv returned an unexpected author follow state")]
    StateMismatch,
}
