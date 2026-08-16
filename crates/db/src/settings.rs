use crate::{Db, DbError, EventRepository};
use pixivarchive_domain::{
    event::{EventPayload, EventResource},
    settings::SettingGroupKey,
};
use serde_json::Value;
use sqlx::{Row, types::Json};
use uuid::Uuid;

#[derive(Clone)]
pub struct SettingsRepository {
    db: Db,
}

impl SettingsRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn get(&self, group: SettingGroupKey) -> Result<Option<StoredSetting>, DbError> {
        let row = sqlx::query("SELECT id, key, value, revision FROM system_setting WHERE key = $1")
            .bind(group.as_str())
            .fetch_optional(self.db.pool())
            .await?;
        row.map(|row| stored_from_row(&row)).transpose()
    }

    pub async fn list(&self) -> Result<Vec<StoredSetting>, DbError> {
        let rows = sqlx::query("SELECT id, key, value, revision FROM system_setting ORDER BY key")
            .fetch_all(self.db.pool())
            .await?;
        rows.iter().map(stored_from_row).collect()
    }

    pub async fn update(
        &self,
        group: SettingGroupKey,
        expected_revision: Option<i64>,
        value: Value,
    ) -> Result<StoredSetting, DbError> {
        self.update_many(vec![SettingWrite {
            group,
            expected_revision,
            value,
        }])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| DbError::InvalidValue("setting update returned no row".to_owned()))
    }

    pub async fn update_many(
        &self,
        mut writes: Vec<SettingWrite>,
    ) -> Result<Vec<StoredSetting>, DbError> {
        writes.sort_by_key(|write| setting_lock_order(write.group));
        if writes.windows(2).any(|pair| pair[0].group == pair[1].group) {
            return Err(DbError::InvalidValue(
                "setting update contains duplicate groups".to_owned(),
            ));
        }

        let mut tx = self.db.begin().await?;
        for write in &writes {
            sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
                .bind(SETTING_LOCK_NAMESPACE)
                .bind(setting_lock_order(write.group))
                .execute(&mut *tx)
                .await?;
        }

        for write in &writes {
            let current_revision = sqlx::query_scalar::<_, i64>(
                "SELECT revision FROM system_setting WHERE key = $1 FOR UPDATE",
            )
            .bind(write.group.as_str())
            .fetch_optional(&mut *tx)
            .await?;
            match current_revision {
                Some(current_revision) if write.expected_revision != Some(current_revision) => {
                    return Err(DbError::RevisionConflict);
                }
                None if write.expected_revision.is_some() => {
                    return Err(DbError::RevisionConflict);
                }
                _ => {}
            }
        }

        let events = EventRepository::new(self.db.clone());
        let mut saved = Vec::with_capacity(writes.len());
        for write in writes {
            let key = write.group.as_str();
            let row = if write.expected_revision.is_some() {
                sqlx::query(
                    r#"
                    UPDATE system_setting
                    SET value = $2, revision = revision + 1, updated_at = now()
                    WHERE key = $1
                    RETURNING id, key, value, revision
                    "#,
                )
                .bind(key)
                .bind(Json(write.value))
                .fetch_one(&mut *tx)
                .await?
            } else {
                sqlx::query(
                    r#"
                    INSERT INTO system_setting (id, key, value)
                    VALUES ($1, $2, $3)
                    RETURNING id, key, value, revision
                    "#,
                )
                .bind(Uuid::now_v7())
                .bind(key)
                .bind(Json(write.value))
                .fetch_one(&mut *tx)
                .await?
            };
            let stored = stored_from_row(&row)?;
            events
                .append_in_tx(
                    &mut tx,
                    EventResource::SystemSetting,
                    stored.id,
                    EventPayload::SystemSettingChanged {
                        group: stored.group,
                        revision: stored.revision,
                    },
                )
                .await?;
            saved.push(stored);
        }
        tx.commit().await?;
        Ok(saved)
    }
}

const SETTING_LOCK_NAMESPACE: i32 = 0x5041_5345;

fn setting_lock_order(group: SettingGroupKey) -> i32 {
    match group {
        SettingGroupKey::Security => 0,
        SettingGroupKey::Storage => 1,
        SettingGroupKey::Retry => 2,
        SettingGroupKey::Queue => 3,
        SettingGroupKey::Processing => 4,
        SettingGroupKey::Derivative => 5,
        SettingGroupKey::Ugoira => 6,
        SettingGroupKey::Pixiv => 7,
        SettingGroupKey::Content => 8,
    }
}

#[derive(Clone, Debug)]
pub struct SettingWrite {
    pub group: SettingGroupKey,
    pub expected_revision: Option<i64>,
    pub value: Value,
}

#[derive(Clone, Debug)]
pub struct StoredSetting {
    pub id: Uuid,
    pub group: SettingGroupKey,
    pub value: Value,
    pub revision: i64,
}

fn stored_from_row(row: &sqlx::postgres::PgRow) -> Result<StoredSetting, DbError> {
    let value: Json<Value> = row.try_get("value")?;
    let key: String = row.try_get("key")?;
    let group = SettingGroupKey::from_db_value(&key)
        .ok_or_else(|| DbError::InvalidValue(format!("unknown setting group {key}")))?;
    Ok(StoredSetting {
        id: row.try_get("id")?,
        group,
        value: value.0,
        revision: row.try_get("revision")?,
    })
}
