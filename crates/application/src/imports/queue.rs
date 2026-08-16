use super::model::{ImportQueueError, ImportRunSummary, ImportStrategy, QueueImportRequest};
use crate::settings::effective_job_priority_policy;
use pixivarchive_db::{
    Db, DbError, ImportRepository, PixivAccountRepository,
    QueueImportRequest as DbQueueImportRequest, RulesRepository,
};
use pixivarchive_domain::job::JobKind;
use pixivarchive_domain::rule::RuleDefinitionV1;
use pixivarchive_domain::subscription::ImportKind;
use uuid::Uuid;

#[derive(Clone)]
pub struct ImportQueueService {
    db: Db,
    repository: ImportRepository,
    accounts: PixivAccountRepository,
    rules: RulesRepository,
}

impl ImportQueueService {
    pub fn new(db: Db) -> Self {
        Self {
            repository: ImportRepository::new(db.clone()),
            accounts: PixivAccountRepository::new(db.clone()),
            rules: RulesRepository::new(db.clone()),
            db,
        }
    }

    pub async fn queue(
        &self,
        request: QueueImportRequest,
    ) -> Result<ImportRunSummary, ImportQueueError> {
        self.accounts.require_current(request.account_id).await?;
        let (forced, rule_document) = match request.strategy {
            ImportStrategy::Default => (false, None),
            ImportStrategy::Forced => (true, None),
            ImportStrategy::Rule { rule_id } => {
                let version = self
                    .rules
                    .current_version(rule_id)
                    .await?
                    .ok_or(ImportQueueError::RuleUnavailable)?;
                (false, Some(RuleDefinitionV1::parse(version.definition)?))
            }
        };
        self.queue_resolved(
            request.account_id,
            request.kind,
            request.target_pixiv_id,
            forced,
            rule_document,
        )
        .await
        .map_err(ImportQueueError::from)
    }

    pub(super) async fn queue_resolved(
        &self,
        account_id: Uuid,
        import_kind: ImportKind,
        target_pixiv_id: i64,
        forced: bool,
        rule_document: Option<RuleDefinitionV1>,
    ) -> Result<ImportRunSummary, DbError> {
        let job_kind = match import_kind {
            ImportKind::Artist => JobKind::ImportArtist,
            ImportKind::Work => JobKind::ImportWork,
        };
        let priority = effective_job_priority_policy(&self.db)
            .await?
            .priority_for(job_kind);
        self.repository
            .queue(
                DbQueueImportRequest {
                    account_id,
                    kind: import_kind,
                    target_pixiv_id,
                    forced,
                    rule_document,
                },
                priority,
            )
            .await
            .map(ImportRunSummary::from)
    }

    pub async fn list(&self, limit: u16) -> Result<Vec<ImportRunSummary>, DbError> {
        Ok(self
            .repository
            .list(limit)
            .await?
            .into_iter()
            .map(ImportRunSummary::from)
            .collect())
    }
}
