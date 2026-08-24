pub mod auth;
pub mod bookmarks;
pub mod events;
pub mod following;
pub mod gallery;
pub mod imports;
pub mod jobs;
pub mod media;
pub mod pixiv;
pub mod rules;
pub mod settings;
pub mod subscriptions;
pub mod system;
pub mod trash;
pub mod worker;
pub mod works;

use sqlx::{PgPool, Postgres, Transaction, postgres::PgPoolOptions};
use thiserror::Error;

pub use auth::AuthRepository;
pub use bookmarks::*;
pub use events::{EventNotificationListener, EventRepository};
pub use following::{
    FollowingAuthorRecord, FollowingAuthorSnapshot, FollowingRepository, SyncFollowingAuthors,
};
pub use gallery::GalleryRepository;
pub use imports::{
    CreateImportRun, ImportRepository, ImportRunRecord, ImportRunSummaryRecord, QueueImportRequest,
};
pub use jobs::{
    ImportJobCompletion, JobAttemptRecord, JobCompletion, JobRecord, JobRepository, JobStats,
};
pub use media::{
    CurrentMediaRevision, DerivativeKind, MediaArtifactIntent, MediaDownloadItem,
    MediaDownloadPage, MediaDownloadPlan, MediaRepository, ProcessingMedia, SaveDerivative,
    SaveSourceMediaRevision, SavedSourceMediaRevision, SourceMediaFile,
};
pub use pixiv::{
    ActivatePixivAccount, BookmarkWritebackRecord, PixivAccountRecord, PixivAccountRepository,
    PixivAccountStatus, PixivCredentialEnvelope, RecordBookmarkWriteback, SavePixivAccount,
};
pub use rules::{
    CreateRule, PublishRuleVersion, RuleDraftRecord, RuleRecord, RuleVersionRecord,
    RulesRepository, SaveRuleDraft,
};
pub use settings::{SettingWrite, SettingsRepository};
pub use subscriptions::{
    CreateSubscription, DueSubscription, FinishSubscriptionRun, FinishSubscriptionRunResult,
    FinishSubscriptionRunUnit, FinishSubscriptionRunUnitResult, RecordRankingUnitEntry,
    ScheduleDueSubscription, ScheduleDueSubscriptionResult, ScheduledSubscriptionRun,
    SubscriptionCursorRecord, SubscriptionCursorUpdate, SubscriptionRecord, SubscriptionRepository,
    SubscriptionRunRecord, SubscriptionRunSummaryRecord, SubscriptionRunUnitRecord,
    UpdateSubscription,
};
pub use system::{SystemDatabaseStatus, SystemRepository};
pub use trash::{TrashPurgeFailure, TrashPurgePlan, TrashRepository};
pub use worker::{WorkerHeartbeatRecord, WorkerHeartbeatRepository, WorkerHeartbeatUpdate};
pub use works::{SavePixivWorkMetadata, WorkRepository, WorkRevisionSourceInput};

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub async fn connect(database_url: &str) -> Result<Self, DbError> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(database_url)
            .await
            .map_err(DbError::Connection)?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn begin(&self) -> Result<Transaction<'_, Postgres>, DbError> {
        self.pool.begin().await.map_err(DbError::from)
    }
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database connection failed")]
    Connection(#[source] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[source] sqlx::migrate::MigrateError),
    #[error("database constraint failed: {0}")]
    Constraint(String),
    #[error("record not found")]
    NotFound,
    #[error("resource revision conflict")]
    RevisionConflict,
    #[error("job lease conflict")]
    LeaseConflict,
    #[error("rate limit exceeded")]
    RateLimited { retry_after_seconds: u64 },
    #[error("database query failed")]
    Query(#[source] sqlx::Error),
    #[error("database boundary value is invalid: {0}")]
    InvalidValue(String),
}

impl From<sqlx::Error> for DbError {
    fn from(error: sqlx::Error) -> Self {
        match &error {
            sqlx::Error::Database(database_error) => {
                if database_error.is_unique_violation()
                    || database_error.is_check_violation()
                    || database_error.is_foreign_key_violation()
                {
                    return Self::Constraint(database_error.message().to_owned());
                }
                Self::Query(error)
            }
            sqlx::Error::RowNotFound => Self::NotFound,
            _ => Self::Query(error),
        }
    }
}

impl From<sqlx::migrate::MigrateError> for DbError {
    fn from(error: sqlx::migrate::MigrateError) -> Self {
        Self::Migration(error)
    }
}
