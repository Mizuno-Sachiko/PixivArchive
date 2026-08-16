use crate::pixiv_works::PixivWorkProcessingError;
use pixivarchive_db::{
    DbError, ImportRunRecord as DbImportRunRecord,
    ImportRunSummaryRecord as DbImportRunSummaryRecord,
};
use pixivarchive_domain::{
    job::JobErrorClass,
    rule::{RuleDefinitionV1, RuleError},
    subscription::{ImportKind, ImportRunStatus},
};
use pixivarchive_pixiv::{PixivErrorClass, PixivRequestContext};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct QueueImportRequest {
    pub account_id: Uuid,
    pub kind: ImportKind,
    pub target_pixiv_id: i64,
    pub strategy: ImportStrategy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ImportStrategy {
    Default,
    Rule { rule_id: Uuid },
    Forced,
}

#[derive(Clone, Debug)]
pub struct ImportRun {
    pub run_id: Uuid,
    pub account_id: Uuid,
    pub kind: ImportKind,
    pub target_pixiv_id: i64,
    pub forced: bool,
    pub status: ImportRunStatus,
    pub error_class: Option<String>,
    pub error_message: Option<String>,
}

impl From<DbImportRunRecord> for ImportRun {
    fn from(run: DbImportRunRecord) -> Self {
        Self {
            run_id: run.run_id,
            account_id: run.account_id,
            kind: run.kind,
            target_pixiv_id: run.target_pixiv_id,
            forced: run.forced,
            status: run.status,
            error_class: run.error_class,
            error_message: run.error_message,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportRunSummary {
    pub run_id: Uuid,
    pub job_id: Option<Uuid>,
    pub account_id: Uuid,
    pub kind: ImportKind,
    pub target_pixiv_id: i64,
    pub strategy: ImportStrategy,
    pub status: ImportRunStatus,
    pub discovered_count: i32,
    pub saved_count: i32,
    pub error_class: Option<String>,
    pub error_message: Option<String>,
    pub created_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
}

impl From<DbImportRunSummaryRecord> for ImportRunSummary {
    fn from(run: DbImportRunSummaryRecord) -> Self {
        let strategy = if run.forced {
            ImportStrategy::Forced
        } else if let Some(rule_id) = run.rule_id {
            ImportStrategy::Rule { rule_id }
        } else {
            ImportStrategy::Default
        };
        Self {
            run_id: run.id,
            job_id: run.job_id,
            account_id: run.account_id,
            kind: run.kind,
            target_pixiv_id: run.target_pixiv_id,
            strategy,
            status: run.status,
            discovered_count: run.discovered_count,
            saved_count: run.saved_count,
            error_class: run.error_class,
            error_message: run.error_message,
            created_at: run.created_at,
            finished_at: run.finished_at,
        }
    }
}

pub struct ImportRequest {
    pub account_id: Uuid,
    pub context: PixivRequestContext,
    pub kind: ImportKind,
    pub target_pixiv_id: i64,
    pub forced: bool,
    pub rule_document: Option<RuleDefinitionV1>,
}

impl ImportRequest {
    pub fn work(account_id: Uuid, context: PixivRequestContext, work_id: i64) -> Self {
        Self {
            account_id,
            context,
            kind: ImportKind::Work,
            target_pixiv_id: work_id,
            forced: false,
            rule_document: None,
        }
    }

    pub fn artist(account_id: Uuid, context: PixivRequestContext, artist_id: i64) -> Self {
        Self {
            account_id,
            context,
            kind: ImportKind::Artist,
            target_pixiv_id: artist_id,
            forced: false,
            rule_document: None,
        }
    }

    pub fn forced(mut self) -> Self {
        self.forced = true;
        self
    }

    pub fn with_rule_document(mut self, document: RuleDefinitionV1) -> Self {
        self.rule_document = Some(document);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportResult {
    pub id: Uuid,
    pub kind: ImportKind,
    pub status: ImportRunStatus,
    pub discovered_count: i32,
    pub saved_count: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportAttemptResult {
    pub result: ImportResult,
    pub error_class: Option<JobErrorClass>,
}

#[derive(Debug, Error)]
pub enum ImportServiceError {
    #[error("import storage failed")]
    Storage(#[from] DbError),
    #[error("pixiv request failed")]
    Pixiv(JobErrorClass),
    #[error("Pixiv work processing failed")]
    Processing(#[from] PixivWorkProcessingError),
    #[error("stored import rule is invalid")]
    RuleDocument(#[from] RuleError),
}

#[derive(Debug, Error)]
pub enum ImportQueueError {
    #[error("import storage failed")]
    Storage(#[from] DbError),
    #[error("the selected rule has no published version")]
    RuleUnavailable,
    #[error("the selected rule snapshot is invalid")]
    RuleDocument(#[from] RuleError),
}

impl ImportServiceError {
    pub fn error_class(&self) -> JobErrorClass {
        match self {
            Self::Pixiv(error_class) => *error_class,
            Self::Processing(error) => error.error_class(),
            Self::Storage(error) => crate::jobs::database_error_class(error),
            Self::RuleDocument(_) => JobErrorClass::Permanent,
        }
    }
}

pub(super) fn pixiv_error_class(class: PixivErrorClass) -> JobErrorClass {
    crate::jobs::pixiv_error_class(class)
}
