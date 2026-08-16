use crate::{Db, DbError, EventRepository};
use pixivarchive_domain::{
    event::{EventPayload, EventResource},
    rule::RuleAction,
};
use serde_json::Value;
use sqlx::{Row, types::Json};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Clone)]
pub struct RulesRepository {
    db: Db,
}

impl RulesRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn create_rule(&self, input: CreateRule) -> Result<RuleRecord, DbError> {
        if input.name.trim().is_empty() {
            return Err(DbError::InvalidValue("rule name is empty".to_owned()));
        }
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(
            r#"
            INSERT INTO download_rule (id, name, enabled, match_action, default_action)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, name, enabled, match_action, default_action,
                      current_version_id, revision, NULL::bigint AS current_version,
                      true AS has_draft, sort_order
            "#,
        )
        .bind(input.id)
        .bind(input.name.trim())
        .bind(input.enabled)
        .bind(input.match_action.as_str())
        .bind(input.default_action.as_str())
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"
            INSERT INTO rule_draft (id, rule_id, base_version, schema_version, definition)
            VALUES ($1, $2, NULL, $3, $4)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(input.id)
        .bind(input.schema_version)
        .bind(Json(input.definition))
        .execute(&mut *tx)
        .await?;
        let record = rule_from_row(&row)?;
        append_rule_event(&self.db, &mut tx, record.id, record.revision).await?;
        tx.commit().await?;
        Ok(record)
    }

    pub async fn get_rule(&self, id: Uuid) -> Result<RuleRecord, DbError> {
        let row = sqlx::query(
            r#"
            SELECT r.id, r.name, r.enabled, r.match_action, r.default_action,
                   r.current_version_id, r.revision, v.version AS current_version,
                   EXISTS(SELECT 1 FROM rule_draft d WHERE d.rule_id = r.id) AS has_draft,
                   r.sort_order
            FROM download_rule r
            LEFT JOIN rule_version v ON v.id = r.current_version_id
            WHERE r.id = $1
            "#,
        )
        .bind(id)
        .fetch_one(self.db.pool())
        .await?;
        rule_from_row(&row)
    }

    pub async fn list_rules(&self) -> Result<Vec<RuleRecord>, DbError> {
        let rows = sqlx::query(
            r#"
            SELECT r.id, r.name, r.enabled, r.match_action, r.default_action,
                   r.current_version_id, r.revision, v.version AS current_version,
                   EXISTS(SELECT 1 FROM rule_draft d WHERE d.rule_id = r.id) AS has_draft,
                   r.sort_order
            FROM download_rule r
            LEFT JOIN rule_version v ON v.id = r.current_version_id
            ORDER BY r.sort_order, r.id
            "#,
        )
        .fetch_all(self.db.pool())
        .await?;
        rows.iter().map(rule_from_row).collect()
    }

    pub async fn copy_source_definition(&self, rule_id: Uuid) -> Result<Value, DbError> {
        let definition = sqlx::query_scalar::<_, Option<Json<Value>>>(
            r#"
            SELECT COALESCE(d.definition, v.definition)
            FROM download_rule r
            LEFT JOIN rule_draft d ON d.rule_id = r.id
            LEFT JOIN rule_version v ON v.id = r.current_version_id
            WHERE r.id = $1
            "#,
        )
        .bind(rule_id)
        .fetch_one(self.db.pool())
        .await?;
        definition
            .map(|definition| definition.0)
            .ok_or_else(|| DbError::InvalidValue("rule has no draft or saved version".to_owned()))
    }

    pub async fn reorder_rules(
        &self,
        ordered_rule_ids: &[Uuid],
    ) -> Result<Vec<RuleRecord>, DbError> {
        let distinct_ids = ordered_rule_ids.iter().copied().collect::<HashSet<_>>();
        if distinct_ids.len() != ordered_rule_ids.len() {
            return Err(DbError::InvalidValue(
                "rule order contains duplicate IDs".to_owned(),
            ));
        }

        let mut tx = self.db.begin().await?;
        sqlx::query("LOCK TABLE download_rule IN SHARE ROW EXCLUSIVE MODE")
            .execute(&mut *tx)
            .await?;
        let current_ids = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM download_rule ORDER BY sort_order, id FOR UPDATE",
        )
        .fetch_all(&mut *tx)
        .await?;
        let current_id_set = current_ids.iter().copied().collect::<HashSet<_>>();
        if current_id_set != distinct_ids {
            return Err(DbError::RevisionConflict);
        }

        sqlx::query(
            r#"
            UPDATE download_rule AS rule
            SET sort_order = ordered_rule.sort_order
            FROM unnest($1::uuid[]) WITH ORDINALITY AS ordered_rule(id, sort_order)
            WHERE rule.id = ordered_rule.id
            "#,
        )
        .bind(ordered_rule_ids)
        .execute(&mut *tx)
        .await?;

        let rows = sqlx::query(
            r#"
            SELECT r.id, r.name, r.enabled, r.match_action, r.default_action,
                   r.current_version_id, r.revision, v.version AS current_version,
                   EXISTS(SELECT 1 FROM rule_draft d WHERE d.rule_id = r.id) AS has_draft,
                   r.sort_order
            FROM download_rule r
            LEFT JOIN rule_version v ON v.id = r.current_version_id
            ORDER BY r.sort_order, r.id
            "#,
        )
        .fetch_all(&mut *tx)
        .await?;
        let records = rows.iter().map(rule_from_row).collect::<Result<_, _>>()?;
        tx.commit().await?;
        Ok(records)
    }

    pub async fn delete_rule(&self, rule_id: Uuid, expected_revision: i64) -> Result<(), DbError> {
        let mut tx = self.db.begin().await?;
        let deleted = sqlx::query_scalar::<_, i64>(
            "DELETE FROM download_rule WHERE id = $1 AND revision = $2 RETURNING revision",
        )
        .bind(rule_id)
        .bind(expected_revision)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(revision) = deleted {
            append_rule_event(&self.db, &mut tx, rule_id, revision).await?;
            tx.commit().await?;
            return Ok(());
        }
        if sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM download_rule WHERE id = $1)")
            .bind(rule_id)
            .fetch_one(&mut *tx)
            .await?
        {
            Err(DbError::RevisionConflict)
        } else {
            Err(DbError::NotFound)
        }
    }

    pub async fn load_draft(&self, rule_id: Uuid) -> Result<Option<RuleDraftRecord>, DbError> {
        let row = sqlx::query(
            "SELECT id, rule_id, base_version, schema_version, definition, revision FROM rule_draft WHERE rule_id = $1",
        )
        .bind(rule_id)
        .fetch_optional(self.db.pool())
        .await?;
        row.map(|row| draft_from_row(&row)).transpose()
    }

    pub async fn save_draft(&self, input: SaveRuleDraft) -> Result<RuleDraftRecord, DbError> {
        let mut tx = self.db.begin().await?;
        let rule = sqlx::query(
            r#"
            SELECT v.version AS current_version
            FROM download_rule r
            LEFT JOIN rule_version v ON v.id = r.current_version_id
            WHERE r.id = $1
            FOR UPDATE OF r
            "#,
        )
        .bind(input.rule_id)
        .fetch_one(&mut *tx)
        .await?;
        let current_version: Option<i64> = rule.try_get("current_version")?;
        let existing = sqlx::query(
            "SELECT base_version, revision FROM rule_draft WHERE rule_id = $1 FOR UPDATE",
        )
        .bind(input.rule_id)
        .fetch_optional(&mut *tx)
        .await?;
        let row = if let Some(existing) = existing {
            let base_version: Option<i64> = existing.try_get("base_version")?;
            let revision: i64 = existing.try_get("revision")?;
            if input.expected_revision != Some(revision)
                || input.base_version != base_version
                || base_version != current_version
            {
                return Err(DbError::RevisionConflict);
            }
            sqlx::query(
                r#"
                UPDATE rule_draft
                SET schema_version = $2, definition = $3, updated_at = now(), revision = revision + 1
                WHERE rule_id = $1
                RETURNING id, rule_id, base_version, schema_version, definition, revision
                "#,
            )
            .bind(input.rule_id)
            .bind(input.schema_version)
            .bind(Json(input.definition))
            .fetch_one(&mut *tx)
            .await?
        } else {
            if input.expected_revision.is_some() || input.base_version != current_version {
                return Err(DbError::RevisionConflict);
            }
            sqlx::query(
                r#"
                INSERT INTO rule_draft (id, rule_id, base_version, schema_version, definition)
                VALUES ($1, $2, $3, $4, $5)
                RETURNING id, rule_id, base_version, schema_version, definition, revision
                "#,
            )
            .bind(Uuid::now_v7())
            .bind(input.rule_id)
            .bind(input.base_version)
            .bind(input.schema_version)
            .bind(Json(input.definition))
            .fetch_one(&mut *tx)
            .await?
        };
        let draft = draft_from_row(&row)?;
        let revision = sqlx::query_scalar::<_, i64>(
            "UPDATE download_rule SET updated_at = now(), revision = revision + 1 WHERE id = $1 RETURNING revision",
        )
        .bind(draft.rule_id)
        .fetch_one(&mut *tx)
        .await?;
        append_rule_event(&self.db, &mut tx, draft.rule_id, revision).await?;
        tx.commit().await?;
        Ok(draft)
    }

    pub async fn publish_version(
        &self,
        input: PublishRuleVersion,
    ) -> Result<RuleVersionRecord, DbError> {
        let mut tx = self.db.begin().await?;
        let rule = sqlx::query(
            r#"
            SELECT v.version AS current_version, r.revision
            FROM download_rule r
            LEFT JOIN rule_version v ON v.id = r.current_version_id
            WHERE r.id = $1
            FOR UPDATE OF r
            "#,
        )
        .bind(input.rule_id)
        .fetch_one(&mut *tx)
        .await?;
        let current_version: Option<i64> = rule.try_get("current_version")?;
        if input.base_version != current_version {
            return Err(DbError::RevisionConflict);
        }
        let draft = sqlx::query(
            "SELECT id, base_version, schema_version, definition, revision FROM rule_draft WHERE rule_id = $1 FOR UPDATE",
        )
        .bind(input.rule_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DbError::RevisionConflict)?;
        let draft_id: Uuid = draft.try_get("id")?;
        if draft.try_get::<Option<i64>, _>("base_version")? != input.base_version
            || draft.try_get::<i64, _>("revision")? != input.expected_draft_revision
        {
            return Err(DbError::RevisionConflict);
        }
        let version = current_version.unwrap_or(0) + 1;
        let version_id = Uuid::now_v7();
        let row = sqlx::query(
            r#"
            INSERT INTO rule_version (id, rule_id, version, base_version, schema_version, definition, created_by)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id, rule_id, version, base_version, schema_version, definition, created_by
            "#,
        )
        .bind(version_id)
        .bind(input.rule_id)
        .bind(version)
        .bind(input.base_version)
        .bind(draft.try_get::<i64, _>("schema_version")?)
        .bind(draft.try_get::<Json<Value>, _>("definition")?)
        .bind(input.created_by)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"
            UPDATE download_rule
            SET name = $2, enabled = $3, match_action = $4, default_action = $5,
                current_version_id = $6, updated_at = now(), revision = revision + 1
            WHERE id = $1
            "#,
        )
        .bind(input.rule_id)
        .bind(input.name.trim())
        .bind(input.enabled)
        .bind(input.match_action.as_str())
        .bind(input.default_action.as_str())
        .bind(version_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM rule_draft WHERE id = $1")
            .bind(draft_id)
            .execute(&mut *tx)
            .await?;
        let record = version_from_row(&row)?;
        let revision = rule.try_get::<i64, _>("revision")? + 1;
        append_rule_event(&self.db, &mut tx, input.rule_id, revision).await?;
        tx.commit().await?;
        Ok(record)
    }

    pub async fn current_version(
        &self,
        rule_id: Uuid,
    ) -> Result<Option<RuleVersionRecord>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT v.id, v.rule_id, v.version, v.base_version, v.schema_version, v.definition, v.created_by
            FROM download_rule r JOIN rule_version v ON v.id = r.current_version_id
            WHERE r.id = $1
            "#,
        )
        .bind(rule_id)
        .fetch_optional(self.db.pool())
        .await?;
        row.map(|row| version_from_row(&row)).transpose()
    }

    pub async fn version(&self, rule_id: Uuid, version: i64) -> Result<RuleVersionRecord, DbError> {
        let row = sqlx::query(
            "SELECT id, rule_id, version, base_version, schema_version, definition, created_by FROM rule_version WHERE rule_id = $1 AND version = $2",
        )
        .bind(rule_id)
        .bind(version)
        .fetch_one(self.db.pool())
        .await?;
        version_from_row(&row)
    }
}

#[derive(Clone, Debug)]
pub struct CreateRule {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub match_action: RuleAction,
    pub default_action: RuleAction,
    pub schema_version: i64,
    pub definition: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleRecord {
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

#[derive(Clone, Debug, PartialEq)]
pub struct RuleDraftRecord {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub base_version: Option<i64>,
    pub schema_version: i64,
    pub definition: Value,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleVersionRecord {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub version: i64,
    pub base_version: Option<i64>,
    pub schema_version: i64,
    pub definition: Value,
    pub created_by: Option<Uuid>,
}

#[derive(Clone, Debug)]
pub struct SaveRuleDraft {
    pub rule_id: Uuid,
    pub expected_revision: Option<i64>,
    pub base_version: Option<i64>,
    pub schema_version: i64,
    pub definition: Value,
}

#[derive(Clone, Debug)]
pub struct PublishRuleVersion {
    pub rule_id: Uuid,
    pub base_version: Option<i64>,
    pub expected_draft_revision: i64,
    pub name: String,
    pub enabled: bool,
    pub match_action: RuleAction,
    pub default_action: RuleAction,
    pub created_by: Option<Uuid>,
}

async fn append_rule_event(
    db: &Db,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rule_id: Uuid,
    revision: i64,
) -> Result<(), DbError> {
    EventRepository::new(db.clone())
        .append_in_tx(
            tx,
            EventResource::Rule,
            rule_id,
            EventPayload::RuleChanged { revision },
        )
        .await
        .map(|_| ())
}

fn rule_from_row(row: &sqlx::postgres::PgRow) -> Result<RuleRecord, DbError> {
    let match_action = action_from_row(row, "match_action")?;
    let default_action = action_from_row(row, "default_action")?;
    Ok(RuleRecord {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        enabled: row.try_get("enabled")?,
        match_action,
        default_action,
        current_version_id: row.try_get("current_version_id")?,
        current_version: row.try_get("current_version")?,
        has_draft: row.try_get("has_draft")?,
        revision: row.try_get("revision")?,
        sort_order: row.try_get("sort_order")?,
    })
}

fn draft_from_row(row: &sqlx::postgres::PgRow) -> Result<RuleDraftRecord, DbError> {
    Ok(RuleDraftRecord {
        id: row.try_get("id")?,
        rule_id: row.try_get("rule_id")?,
        base_version: row.try_get("base_version")?,
        schema_version: row.try_get("schema_version")?,
        definition: row.try_get::<Json<Value>, _>("definition")?.0,
        revision: row.try_get("revision")?,
    })
}

fn version_from_row(row: &sqlx::postgres::PgRow) -> Result<RuleVersionRecord, DbError> {
    Ok(RuleVersionRecord {
        id: row.try_get("id")?,
        rule_id: row.try_get("rule_id")?,
        version: row.try_get("version")?,
        base_version: row.try_get("base_version")?,
        schema_version: row.try_get("schema_version")?,
        definition: row.try_get::<Json<Value>, _>("definition")?.0,
        created_by: row.try_get("created_by")?,
    })
}

fn action_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<RuleAction, DbError> {
    let value: String = row.try_get(column)?;
    RuleAction::from_db_value(&value)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown rule action {value}")))
}
