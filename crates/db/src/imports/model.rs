use crate::DbError;
use pixivarchive_domain::{
    rule::RuleDefinitionV1,
    subscription::{ImportKind, ImportRunStatus},
};
use serde_json::{Value, json};
use sqlx::Row;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct CreateImportRun {
    pub account_id: Uuid,
    pub kind: ImportKind,
    pub target_pixiv_id: i64,
    pub forced: bool,
    pub rule_document: Option<RuleDefinitionV1>,
    pub status: ImportRunStatus,
}

#[derive(Clone, Debug)]
pub struct QueueImportRequest {
    pub account_id: Uuid,
    pub kind: ImportKind,
    pub target_pixiv_id: i64,
    pub forced: bool,
    pub rule_document: Option<RuleDefinitionV1>,
}

#[derive(Clone, Debug)]
pub struct ImportRunRecord {
    pub run_id: Uuid,
    pub account_id: Uuid,
    pub kind: ImportKind,
    pub target_pixiv_id: i64,
    pub forced: bool,
    pub status: ImportRunStatus,
    pub rule_document: Option<RuleDefinitionV1>,
    pub error_class: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportRunSummaryRecord {
    pub id: Uuid,
    pub job_id: Option<Uuid>,
    pub account_id: Uuid,
    pub kind: ImportKind,
    pub target_pixiv_id: i64,
    pub forced: bool,
    pub rule_id: Option<Uuid>,
    pub status: ImportRunStatus,
    pub discovered_count: i32,
    pub saved_count: i32,
    pub error_class: Option<String>,
    pub error_message: Option<String>,
    pub created_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
}

pub(super) fn import_params(forced: bool, rule_document: Option<&RuleDefinitionV1>) -> Value {
    json!({
        "forced": forced,
        "rule_document": rule_document,
    })
}

pub(super) fn run_from_row(row: &sqlx::postgres::PgRow) -> Result<ImportRunRecord, DbError> {
    let kind_value: String = row.get("import_kind");
    let kind = ImportKind::from_db_value(&kind_value)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown import kind {kind_value}")))?;
    let params = row.get::<sqlx::types::Json<Value>, _>("params").0;
    let status_value: String = row.get("status");
    let status = ImportRunStatus::from_db_value(&status_value)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown import status {status_value}")))?;
    let rule_document = params
        .get("rule_document")
        .filter(|value| !value.is_null())
        .cloned()
        .map(RuleDefinitionV1::parse)
        .transpose()
        .map_err(|error| DbError::InvalidValue(error.to_string()))?;
    Ok(ImportRunRecord {
        run_id: row.get("id"),
        account_id: row.get("pixiv_account_id"),
        kind,
        target_pixiv_id: row.get("target_pixiv_id"),
        forced: row.get("forced"),
        status,
        rule_document,
        error_class: row.get("error_class"),
        error_message: row.get("error_message"),
    })
}

pub(super) fn summary_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<ImportRunSummaryRecord, DbError> {
    let kind_value: String = row.get("import_kind");
    let kind = ImportKind::from_db_value(&kind_value)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown import kind {kind_value}")))?;
    let status_value: String = row.get("status");
    let status = ImportRunStatus::from_db_value(&status_value)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown import status {status_value}")))?;
    let params = row.get::<sqlx::types::Json<Value>, _>("params").0;
    let rule_id = params
        .get("rule_document")
        .filter(|value| !value.is_null())
        .cloned()
        .map(RuleDefinitionV1::parse)
        .transpose()
        .map_err(|error| DbError::InvalidValue(error.to_string()))?
        .map(|document| document.id);
    Ok(ImportRunSummaryRecord {
        id: row.get("id"),
        job_id: row.get("job_id"),
        account_id: row.get("pixiv_account_id"),
        kind,
        target_pixiv_id: row.get("target_pixiv_id"),
        forced: row.get("forced"),
        rule_id,
        status,
        discovered_count: row.get("discovered_count"),
        saved_count: row.get("saved_count"),
        error_class: row.get("error_class"),
        error_message: row.get("error_message"),
        created_at: row.get("created_at"),
        finished_at: row.get("finished_at"),
    })
}
