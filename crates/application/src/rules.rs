use crate::{
    pixiv_accounts::{PixivAccountContextError, PixivAccountContextFactory},
    pixiv_works::{PixivWorkProcessingError, rule_preview_candidate},
};
use async_trait::async_trait;
use pixivarchive_db::{
    CreateRule, Db, DbError, PixivAccountRepository, PublishRuleVersion,
    RuleDraftRecord as DbRuleDraftRecord, RuleRecord as DbRuleRecord,
    RuleVersionRecord as DbRuleVersionRecord, RulesRepository, SaveRuleDraft,
};
use pixivarchive_domain::{
    rule::{
        EvaluationContext, EvaluationDecision, EvaluationError, RuleAction, RuleDefinitionV1,
        RuleError,
    },
    subscription::PixivAccountState,
};
use pixivarchive_pixiv::{PixivErrorClass, PixivGateway};
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct RuleService {
    repository: RulesRepository,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleSummary {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub match_action: RuleAction,
    pub default_action: RuleAction,
    pub current_version_id: Option<Uuid>,
    pub current_version: Option<i64>,
    pub has_draft: bool,
    pub revision: i64,
    pub sort_order: i64,
}

impl From<DbRuleRecord> for RuleSummary {
    fn from(record: DbRuleRecord) -> Self {
        Self {
            id: record.id,
            name: record.name,
            enabled: record.enabled,
            match_action: record.match_action,
            default_action: record.default_action,
            current_version_id: record.current_version_id,
            current_version: record.current_version,
            has_draft: record.has_draft,
            revision: record.revision,
            sort_order: record.sort_order,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleDraft {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub base_version: Option<i64>,
    pub schema_version: i64,
    pub definition: Value,
    pub revision: i64,
}

impl From<DbRuleDraftRecord> for RuleDraft {
    fn from(record: DbRuleDraftRecord) -> Self {
        Self {
            id: record.id,
            rule_id: record.rule_id,
            base_version: record.base_version,
            schema_version: record.schema_version,
            definition: record.definition,
            revision: record.revision,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleVersion {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub version: i64,
    pub base_version: Option<i64>,
    pub schema_version: i64,
    pub definition: Value,
    pub created_by: Option<Uuid>,
}

impl From<DbRuleVersionRecord> for RuleVersion {
    fn from(record: DbRuleVersionRecord) -> Self {
        Self {
            id: record.id,
            rule_id: record.rule_id,
            version: record.version,
            base_version: record.base_version,
            schema_version: record.schema_version,
            definition: record.definition,
            created_by: record.created_by,
        }
    }
}

impl RuleService {
    pub fn new(db: Db) -> Self {
        Self {
            repository: RulesRepository::new(db),
        }
    }

    pub async fn create_rule(
        &self,
        name: &str,
        default_action: RuleAction,
    ) -> Result<RuleSummary, RuleServiceError> {
        let id = Uuid::now_v7();
        let definition =
            RuleDefinitionV1::match_all(id, name.trim(), RuleAction::Download, default_action);
        self.create_definition(definition).await
    }

    pub async fn copy_rule(
        &self,
        source_rule_id: Uuid,
        name: &str,
    ) -> Result<RuleSummary, RuleServiceError> {
        let mut definition = RuleDefinitionV1::parse(
            self.repository
                .copy_source_definition(source_rule_id)
                .await?,
        )?;
        definition.id = Uuid::now_v7();
        definition.name = name.trim().to_owned();
        self.create_definition(definition).await
    }

    async fn create_definition(
        &self,
        definition: RuleDefinitionV1,
    ) -> Result<RuleSummary, RuleServiceError> {
        definition.validate()?;
        Ok(self
            .repository
            .create_rule(CreateRule {
                id: definition.id,
                name: definition.name.clone(),
                enabled: definition.enabled,
                match_action: definition.action,
                default_action: definition.default_action,
                schema_version: i64::from(definition.schema_version),
                definition: serde_json::to_value(definition)?,
            })
            .await?
            .into())
    }

    pub async fn list_rules(&self) -> Result<Vec<RuleSummary>, RuleServiceError> {
        Ok(self
            .repository
            .list_rules()
            .await?
            .into_iter()
            .map(RuleSummary::from)
            .collect())
    }

    pub async fn rule(&self, rule_id: Uuid) -> Result<RuleSummary, RuleServiceError> {
        Ok(self.repository.get_rule(rule_id).await?.into())
    }

    pub async fn reorder_rules(
        &self,
        ordered_rule_ids: &[Uuid],
    ) -> Result<Vec<RuleSummary>, RuleServiceError> {
        Ok(self
            .repository
            .reorder_rules(ordered_rule_ids)
            .await?
            .into_iter()
            .map(RuleSummary::from)
            .collect())
    }

    pub async fn delete_rule(
        &self,
        rule_id: Uuid,
        expected_revision: i64,
    ) -> Result<(), RuleServiceError> {
        Ok(self
            .repository
            .delete_rule(rule_id, expected_revision)
            .await?)
    }

    pub async fn load_draft(&self, rule_id: Uuid) -> Result<Option<RuleDraft>, RuleServiceError> {
        Ok(self
            .repository
            .load_draft(rule_id)
            .await?
            .map(RuleDraft::from))
    }

    pub async fn save_draft(
        &self,
        request: SaveRuleDraftRequest,
    ) -> Result<RuleDraft, RuleServiceError> {
        let definition = RuleDefinitionV1::parse(request.definition)?;
        if definition.id != request.rule_id {
            return Err(RuleServiceError::InvalidRequest(
                "rule definition ID does not match route".to_owned(),
            ));
        }
        Ok(self
            .repository
            .save_draft(SaveRuleDraft {
                rule_id: request.rule_id,
                expected_revision: request.expected_revision,
                base_version: request.base_version,
                schema_version: definition.schema_version as i64,
                definition: serde_json::to_value(definition)?,
            })
            .await?
            .into())
    }

    pub async fn import_json(
        &self,
        rule_id: Uuid,
        expected_revision: Option<i64>,
        definition: Value,
    ) -> Result<RuleDraft, RuleServiceError> {
        let base_version = match self.repository.load_draft(rule_id).await? {
            Some(draft) => draft.base_version,
            None => self
                .repository
                .current_version(rule_id)
                .await?
                .map(|version| version.version),
        };
        self.save_draft(SaveRuleDraftRequest {
            rule_id,
            expected_revision,
            base_version,
            definition,
        })
        .await
    }

    pub async fn publish_version(
        &self,
        request: PublishRuleVersionRequest,
    ) -> Result<RuleVersion, RuleServiceError> {
        let draft = self
            .repository
            .load_draft(request.rule_id)
            .await?
            .ok_or(RuleServiceError::RevisionConflict)?;
        if draft.base_version != request.base_version
            || draft.revision != request.expected_draft_revision
        {
            return Err(RuleServiceError::RevisionConflict);
        }
        let definition = RuleDefinitionV1::parse(draft.definition)?;
        if definition.id != request.rule_id {
            return Err(RuleServiceError::InvalidRequest(
                "rule definition ID does not match route".to_owned(),
            ));
        }
        Ok(self
            .repository
            .publish_version(PublishRuleVersion {
                rule_id: request.rule_id,
                base_version: request.base_version,
                expected_draft_revision: request.expected_draft_revision,
                name: definition.name,
                enabled: definition.enabled,
                match_action: definition.action,
                default_action: definition.default_action,
                created_by: request.created_by,
            })
            .await?
            .into())
    }

    pub async fn export_json(&self, rule_id: Uuid) -> Result<Option<Value>, RuleServiceError> {
        Ok(self
            .repository
            .current_version(rule_id)
            .await?
            .map(|version| version.definition))
    }

    pub fn validate(&self, definition: Value) -> Result<RuleDefinitionV1, RuleServiceError> {
        Ok(RuleDefinitionV1::parse(definition)?)
    }

    pub fn evaluate_one(
        &self,
        definition: Value,
        context: &EvaluationContext,
    ) -> Result<EvaluationDecision, RuleServiceError> {
        Ok(RuleDefinitionV1::parse(definition)?.evaluate(context)?)
    }
}

#[derive(Clone, Debug)]
pub struct SaveRuleDraftRequest {
    pub rule_id: Uuid,
    pub expected_revision: Option<i64>,
    pub base_version: Option<i64>,
    pub definition: Value,
}

#[derive(Clone, Debug)]
pub struct PublishRuleVersionRequest {
    pub rule_id: Uuid,
    pub base_version: Option<i64>,
    pub expected_draft_revision: i64,
    pub created_by: Option<Uuid>,
}

#[derive(Clone, Debug)]
pub struct RulePreviewRequest {
    pub pixiv_work_id: i64,
    pub definition: Value,
}

#[derive(Clone, Debug)]
pub struct RulePreviewResult {
    pub pixiv_work_id: i64,
    pub title: String,
    pub artist_name: String,
    pub content_type: String,
    pub decision: EvaluationDecision,
}

#[async_trait]
pub trait RulePreviewPort: Send + Sync {
    async fn preview(
        &self,
        request: RulePreviewRequest,
    ) -> Result<RulePreviewResult, RulePreviewError>;
}

pub struct DisabledRulePreviewPort;

#[async_trait]
impl RulePreviewPort for DisabledRulePreviewPort {
    async fn preview(
        &self,
        _request: RulePreviewRequest,
    ) -> Result<RulePreviewResult, RulePreviewError> {
        Err(RulePreviewError::Unavailable)
    }
}

pub struct LiveRulePreviewPort<G> {
    accounts: PixivAccountRepository,
    gateway: Arc<G>,
    contexts: PixivAccountContextFactory,
}

impl<G> LiveRulePreviewPort<G> {
    pub fn new(db: Db, gateway: Arc<G>, contexts: PixivAccountContextFactory) -> Self {
        Self {
            accounts: PixivAccountRepository::new(db),
            gateway,
            contexts,
        }
    }
}

#[async_trait]
impl<G> RulePreviewPort for LiveRulePreviewPort<G>
where
    G: PixivGateway + 'static,
{
    async fn preview(
        &self,
        request: RulePreviewRequest,
    ) -> Result<RulePreviewResult, RulePreviewError> {
        if request.pixiv_work_id <= 0 {
            return Err(RulePreviewError::InvalidPixivWorkId);
        }
        let account = self
            .accounts
            .current()
            .await?
            .ok_or(RulePreviewError::AccountNotConfigured)?;
        if account.state != PixivAccountState::Normal {
            return Err(RulePreviewError::AccountUnavailable);
        }
        let context = self.contexts.context_for_record(&account)?;
        let detail = self
            .gateway
            .work_detail(&context, request.pixiv_work_id)
            .await
            .map_err(RulePreviewError::from_pixiv)?
            .value;
        let pages = self
            .gateway
            .work_pages(&context, request.pixiv_work_id)
            .await
            .map_err(RulePreviewError::from_pixiv)?
            .value;
        let now = OffsetDateTime::now_utc();
        let candidate = rule_preview_candidate(&detail, &pages, now)?;
        let decision = RuleDefinitionV1::parse(request.definition)?
            .evaluate(&EvaluationContext { now, candidate })?;
        Ok(RulePreviewResult {
            pixiv_work_id: detail.work_id,
            title: detail.title,
            artist_name: detail.artist.name,
            content_type: detail.kind.as_str().to_owned(),
            decision,
        })
    }
}

#[derive(Debug, Error)]
pub enum RulePreviewError {
    #[error("Pixiv work ID must be a positive integer")]
    InvalidPixivWorkId,
    #[error("Pixiv account is not configured")]
    AccountNotConfigured,
    #[error("Pixiv account is unavailable")]
    AccountUnavailable,
    #[error("Pixiv work was not found")]
    WorkNotFound,
    #[error("Pixiv credential is invalid")]
    CredentialInvalid,
    #[error("Pixiv temporarily rejected the request")]
    Temporary,
    #[error("rule preview is unavailable")]
    Unavailable,
    #[error("rule preview storage failed")]
    Storage(#[from] DbError),
    #[error("Pixiv account request context is unavailable")]
    Context(#[from] PixivAccountContextError),
    #[error("invalid rule document")]
    Rule(#[from] RuleError),
    #[error("rule evaluation failed")]
    Evaluation(#[from] EvaluationError),
    #[error("Pixiv work metadata is invalid")]
    Work(#[from] PixivWorkProcessingError),
}

impl RulePreviewError {
    fn from_pixiv(error: pixivarchive_pixiv::PixivError) -> Self {
        match error.class() {
            PixivErrorClass::HiddenOrNotFound | PixivErrorClass::AgeRestrictedDisabled => {
                Self::WorkNotFound
            }
            PixivErrorClass::CredentialInvalid | PixivErrorClass::CsrfFailed => {
                Self::CredentialInvalid
            }
            _ => Self::Temporary,
        }
    }
}

#[derive(Debug, Error)]
pub enum RuleServiceError {
    #[error("invalid rule document")]
    Rule(#[from] RuleError),
    #[error("rule evaluation failed")]
    Evaluation(#[from] EvaluationError),
    #[error("rule was not found")]
    NotFound,
    #[error("invalid rule request: {0}")]
    InvalidRequest(String),
    #[error("rule conflicts with existing data")]
    Conflict,
    #[error("rule revision conflict")]
    RevisionConflict,
    #[error("rule storage failed")]
    Storage,
}

impl From<DbError> for RuleServiceError {
    fn from(error: DbError) -> Self {
        match error {
            DbError::NotFound => Self::NotFound,
            DbError::InvalidValue(message) => Self::InvalidRequest(message),
            DbError::Constraint(_) => Self::Conflict,
            DbError::RevisionConflict => Self::RevisionConflict,
            _ => Self::Storage,
        }
    }
}

impl From<serde_json::Error> for RuleServiceError {
    fn from(_error: serde_json::Error) -> Self {
        Self::Storage
    }
}
