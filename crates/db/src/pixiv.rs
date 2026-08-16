use crate::events::EventRepository;
use crate::{Db, DbError};
use pixivarchive_domain::event::{EventPayload, EventResource};
use pixivarchive_domain::subscription::PixivAccountState;
use serde_json::Value;
use sqlx::{Postgres, Row, Transaction, types::Json};
use time::OffsetDateTime;
use uuid::Uuid;

pub const MODULE_NAME: &str = "pixiv";

#[derive(Clone)]
pub struct PixivAccountRepository {
    db: Db,
}

impl PixivAccountRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn status(&self) -> Result<PixivAccountStatus, DbError> {
        match self.current().await? {
            Some(account) => Ok(PixivAccountStatus {
                account_id: Some(account.id),
                state: account.state,
                bookmark_writeback_enabled: account.bookmark_writeback_enabled,
            }),
            None => Ok(PixivAccountStatus {
                account_id: None,
                state: PixivAccountState::Unconfigured,
                bookmark_writeback_enabled: false,
            }),
        }
    }

    pub async fn current(&self) -> Result<Option<PixivAccountRecord>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id,
                   pixiv_user_id,
                   display_name,
                   avatar_url,
                   state,
                   cookie_key_id,
                   cookie_nonce,
                   cookie_ciphertext,
                   user_agent,
                   bookmark_writeback_enabled,
                   last_validated_at,
                   revision
            FROM pixiv_account
            WHERE is_current = true
            LIMIT 1
            "#,
        )
        .fetch_optional(self.db.pool())
        .await?;
        row.map(|row| account_from_row(&row)).transpose()
    }

    pub async fn require_current(&self, account_id: Uuid) -> Result<PixivAccountRecord, DbError> {
        match self.current().await? {
            Some(account) if account.id == account_id => Ok(account),
            Some(_) => Err(DbError::RevisionConflict),
            None => {
                self.get(account_id).await?;
                Err(DbError::RevisionConflict)
            }
        }
    }

    pub async fn find_by_pixiv_user_id(
        &self,
        pixiv_user_id: i64,
    ) -> Result<Option<PixivAccountRecord>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id,
                   pixiv_user_id,
                   display_name,
                   avatar_url,
                   state,
                   cookie_key_id,
                   cookie_nonce,
                   cookie_ciphertext,
                   user_agent,
                   bookmark_writeback_enabled,
                   last_validated_at,
                   revision
            FROM pixiv_account
            WHERE pixiv_user_id = $1
            "#,
        )
        .bind(pixiv_user_id)
        .fetch_optional(self.db.pool())
        .await?;
        row.map(|row| account_from_row(&row)).transpose()
    }

    pub async fn get(&self, id: Uuid) -> Result<PixivAccountRecord, DbError> {
        let row = sqlx::query(
            r#"
            SELECT id,
                   pixiv_user_id,
                   display_name,
                   avatar_url,
                   state,
                   cookie_key_id,
                   cookie_nonce,
                   cookie_ciphertext,
                   user_agent,
                   bookmark_writeback_enabled,
                   last_validated_at,
                   revision
            FROM pixiv_account
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_one(self.db.pool())
        .await?;
        account_from_row(&row)
    }

    pub async fn get_many(&self, ids: &[Uuid]) -> Result<Vec<PixivAccountRecord>, DbError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query(
            r#"
            SELECT id,
                   pixiv_user_id,
                   display_name,
                   avatar_url,
                   state,
                   cookie_key_id,
                   cookie_nonce,
                   cookie_ciphertext,
                   user_agent,
                   bookmark_writeback_enabled,
                   last_validated_at,
                   revision
            FROM pixiv_account
            WHERE id = ANY($1)
            ORDER BY id
            "#,
        )
        .bind(ids)
        .fetch_all(self.db.pool())
        .await?;
        rows.iter().map(account_from_row).collect()
    }

    pub async fn save_validating(
        &self,
        input: SavePixivAccount,
    ) -> Result<PixivAccountRecord, DbError> {
        validate_credential_envelope(
            input.pixiv_user_id,
            &input.display_name,
            &input.cookie_key_id,
            &input.cookie_nonce,
            &input.cookie_ciphertext,
            &input.user_agent,
        )?;

        let mut tx = self.db.begin().await?;
        let row = sqlx::query(
            r#"
            INSERT INTO pixiv_account (
                id,
                pixiv_user_id,
                display_name,
                state,
                cookie_key_id,
                cookie_nonce,
                cookie_ciphertext,
                user_agent
            )
            VALUES ($1, $2, $3, 'validating', $4, $5, $6, $7)
            ON CONFLICT (pixiv_user_id)
            DO UPDATE SET display_name = excluded.display_name,
                          state = 'validating',
                          cookie_key_id = excluded.cookie_key_id,
                          cookie_nonce = excluded.cookie_nonce,
                          cookie_ciphertext = excluded.cookie_ciphertext,
                          user_agent = excluded.user_agent,
                          updated_at = now(),
                          revision = pixiv_account.revision + 1
            RETURNING id,
                      pixiv_user_id,
                      display_name,
                      avatar_url,
                      state,
                      cookie_key_id,
                      cookie_nonce,
                      cookie_ciphertext,
                      user_agent,
                      bookmark_writeback_enabled,
                      last_validated_at,
                      revision
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(input.pixiv_user_id)
        .bind(input.display_name.trim())
        .bind(input.cookie_key_id.trim())
        .bind(input.cookie_nonce)
        .bind(input.cookie_ciphertext)
        .bind(input.user_agent.trim())
        .fetch_one(&mut *tx)
        .await?;
        let account = account_from_row(&row)?;
        append_account_event(&self.db, &mut tx, account.id, account.revision).await?;
        tx.commit().await?;
        Ok(account)
    }

    pub(crate) async fn activate_validated_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        input: ActivatePixivAccount,
    ) -> Result<PixivAccountRecord, DbError> {
        validate_credential_envelope(
            input.pixiv_user_id,
            &input.display_name,
            &input.cookie_key_id,
            &input.cookie_nonce,
            &input.cookie_ciphertext,
            &input.user_agent,
        )?;
        if input.state == PixivAccountState::Unconfigured {
            return Err(DbError::InvalidValue(
                "unconfigured requires clearing the saved credential".to_owned(),
            ));
        }
        let avatar_url = normalize_avatar_url(input.avatar_url.as_deref())?;

        let deactivated = sqlx::query(
            r#"
            UPDATE pixiv_account
            SET is_current = false,
                updated_at = now(),
                revision = revision + 1
            WHERE is_current = true
              AND pixiv_user_id <> $1
            RETURNING id, revision
            "#,
        )
        .bind(input.pixiv_user_id)
        .fetch_all(&mut **tx)
        .await?;
        append_account_row_events(&self.db, tx, deactivated).await?;

        let row = sqlx::query(
            r#"
            INSERT INTO pixiv_account (
                id,
                pixiv_user_id,
                display_name,
                avatar_url,
                state,
                cookie_key_id,
                cookie_nonce,
                cookie_ciphertext,
                user_agent,
                last_validated_at,
                is_current
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, true)
            ON CONFLICT (pixiv_user_id)
            DO UPDATE SET display_name = excluded.display_name,
                          avatar_url = excluded.avatar_url,
                          state = excluded.state,
                          cookie_key_id = excluded.cookie_key_id,
                          cookie_nonce = excluded.cookie_nonce,
                          cookie_ciphertext = excluded.cookie_ciphertext,
                          user_agent = excluded.user_agent,
                          last_validated_at = COALESCE(
                              excluded.last_validated_at,
                              pixiv_account.last_validated_at
                          ),
                          is_current = true,
                          updated_at = now(),
                          revision = pixiv_account.revision + 1
            RETURNING id,
                      pixiv_user_id,
                      display_name,
                      avatar_url,
                      state,
                      cookie_key_id,
                      cookie_nonce,
                      cookie_ciphertext,
                      user_agent,
                      bookmark_writeback_enabled,
                      last_validated_at,
                      revision
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(input.pixiv_user_id)
        .bind(input.display_name.trim())
        .bind(avatar_url)
        .bind(input.state.as_str())
        .bind(input.cookie_key_id.trim())
        .bind(input.cookie_nonce)
        .bind(input.cookie_ciphertext)
        .bind(input.user_agent.trim())
        .bind(input.validated_at)
        .fetch_one(&mut **tx)
        .await?;
        let account = account_from_row(&row)?;
        append_account_event(&self.db, tx, account.id, account.revision).await?;
        Ok(account)
    }

    pub(crate) async fn clear_credential_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        account_id: Uuid,
        expected_revision: i64,
    ) -> Result<PixivAccountRecord, DbError> {
        let row = sqlx::query(
            r#"
            UPDATE pixiv_account
            SET state = 'unconfigured',
                cookie_key_id = NULL,
                cookie_nonce = NULL,
                cookie_ciphertext = NULL,
                last_validated_at = NULL,
                updated_at = now(),
                revision = revision + 1
            WHERE id = $1
              AND revision = $2
              AND is_current = true
            RETURNING id,
                      pixiv_user_id,
                      display_name,
                      avatar_url,
                      state,
                      cookie_key_id,
                      cookie_nonce,
                      cookie_ciphertext,
                      user_agent,
                      bookmark_writeback_enabled,
                      last_validated_at,
                      revision
            "#,
        )
        .bind(account_id)
        .bind(expected_revision)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(DbError::RevisionConflict)?;
        let account = account_from_row(&row)?;
        append_account_event(&self.db, tx, account.id, account.revision).await?;
        Ok(account)
    }

    pub async fn activate(&self, account_id: Uuid) -> Result<PixivAccountRecord, DbError> {
        let mut tx = self.db.begin().await?;
        let deactivated = sqlx::query(
            r#"
            UPDATE pixiv_account
            SET is_current = false,
                updated_at = now(),
                revision = revision + 1
            WHERE is_current = true
              AND id <> $1
            RETURNING id, revision
            "#,
        )
        .bind(account_id)
        .fetch_all(&mut *tx)
        .await?;
        append_account_row_events(&self.db, &mut tx, deactivated).await?;

        let row = sqlx::query(
            r#"
            UPDATE pixiv_account
            SET is_current = true,
                updated_at = now(),
                revision = revision + 1
            WHERE id = $1
            RETURNING id,
                      pixiv_user_id,
                      display_name,
                      avatar_url,
                      state,
                      cookie_key_id,
                      cookie_nonce,
                      cookie_ciphertext,
                      user_agent,
                      bookmark_writeback_enabled,
                      last_validated_at,
                      revision
            "#,
        )
        .bind(account_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DbError::NotFound)?;
        let account = account_from_row(&row)?;
        append_account_event(&self.db, &mut tx, account.id, account.revision).await?;
        tx.commit().await?;
        Ok(account)
    }

    pub async fn set_state(
        &self,
        account_id: Uuid,
        state: PixivAccountState,
        validated_at: Option<OffsetDateTime>,
    ) -> Result<PixivAccountRecord, DbError> {
        if state == PixivAccountState::Unconfigured {
            return Err(DbError::InvalidValue(
                "unconfigured requires clearing the saved credential".to_owned(),
            ));
        }
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(
            r#"
            UPDATE pixiv_account
            SET state = $2,
                last_validated_at = COALESCE($3, last_validated_at),
                updated_at = now(),
                revision = revision + 1
            WHERE id = $1
            RETURNING id,
                      pixiv_user_id,
                      display_name,
                      avatar_url,
                      state,
                      cookie_key_id,
                      cookie_nonce,
                      cookie_ciphertext,
                      user_agent,
                      bookmark_writeback_enabled,
                      last_validated_at,
                      revision
            "#,
        )
        .bind(account_id)
        .bind(state.as_str())
        .bind(validated_at)
        .fetch_one(&mut *tx)
        .await?;
        let account = account_from_row(&row)?;
        append_account_event(&self.db, &mut tx, account.id, account.revision).await?;
        tx.commit().await?;
        Ok(account)
    }

    pub async fn set_profile(
        &self,
        account_id: Uuid,
        display_name: &str,
        avatar_url: Option<&str>,
    ) -> Result<PixivAccountRecord, DbError> {
        if display_name.trim().is_empty() {
            return Err(DbError::InvalidValue(
                "pixiv display name must not be empty".to_owned(),
            ));
        }
        let avatar_url = normalize_avatar_url(avatar_url)?;
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(
            r#"
            UPDATE pixiv_account
            SET display_name = $2,
                avatar_url = $3,
                updated_at = now(),
                revision = revision + 1
            WHERE id = $1
            RETURNING id,
                      pixiv_user_id,
                      display_name,
                      avatar_url,
                      state,
                      cookie_key_id,
                      cookie_nonce,
                      cookie_ciphertext,
                      user_agent,
                      bookmark_writeback_enabled,
                      last_validated_at,
                      revision
            "#,
        )
        .bind(account_id)
        .bind(display_name.trim())
        .bind(avatar_url)
        .fetch_one(&mut *tx)
        .await?;
        let account = account_from_row(&row)?;
        append_account_event(&self.db, &mut tx, account.id, account.revision).await?;
        tx.commit().await?;
        Ok(account)
    }

    pub async fn set_bookmark_writeback_enabled(
        &self,
        account_id: Uuid,
        enabled: bool,
    ) -> Result<PixivAccountRecord, DbError> {
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(
            r#"
            UPDATE pixiv_account
            SET bookmark_writeback_enabled = $2,
                updated_at = now(),
                revision = revision + 1
            WHERE id = $1
            RETURNING id,
                      pixiv_user_id,
                      display_name,
                      avatar_url,
                      state,
                      cookie_key_id,
                      cookie_nonce,
                      cookie_ciphertext,
                      user_agent,
                      bookmark_writeback_enabled,
                      last_validated_at,
                      revision
            "#,
        )
        .bind(account_id)
        .bind(enabled)
        .fetch_one(&mut *tx)
        .await?;
        let account = account_from_row(&row)?;
        append_account_event(&self.db, &mut tx, account.id, account.revision).await?;
        tx.commit().await?;
        Ok(account)
    }

    pub async fn set_bookmark_writeback_enabled_at_revision(
        &self,
        account_id: Uuid,
        expected_revision: i64,
        enabled: bool,
    ) -> Result<PixivAccountRecord, DbError> {
        let mut tx = self.db.begin().await?;
        let row = sqlx::query(
            r#"
            UPDATE pixiv_account
            SET bookmark_writeback_enabled = $3,
                updated_at = now(),
                revision = revision + 1
            WHERE id = $1
              AND revision = $2
            RETURNING id,
                      pixiv_user_id,
                      display_name,
                      avatar_url,
                      state,
                      cookie_key_id,
                      cookie_nonce,
                      cookie_ciphertext,
                      user_agent,
                      bookmark_writeback_enabled,
                      last_validated_at,
                      revision
            "#,
        )
        .bind(account_id)
        .bind(expected_revision)
        .bind(enabled)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DbError::RevisionConflict)?;
        let account = account_from_row(&row)?;
        append_account_event(&self.db, &mut tx, account.id, account.revision).await?;
        tx.commit().await?;
        Ok(account)
    }

    pub async fn record_bookmark_writeback(
        &self,
        input: RecordBookmarkWriteback,
    ) -> Result<BookmarkWritebackRecord, DbError> {
        let row = sqlx::query(
            r#"
            INSERT INTO bookmark_writeback_command (
                id, pixiv_account_id, operation, target_pixiv_id, status, error_class, result
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id,
                      pixiv_account_id,
                      operation,
                      target_pixiv_id,
                      status,
                      error_class,
                      result,
                      created_at
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(input.account_id)
        .bind(input.operation)
        .bind(input.target_pixiv_id)
        .bind(input.status)
        .bind(input.error_class)
        .bind(Json(input.result))
        .fetch_one(self.db.pool())
        .await?;
        Ok(BookmarkWritebackRecord {
            id: row.get("id"),
            account_id: row.get("pixiv_account_id"),
            operation: row.get("operation"),
            target_pixiv_id: row.get("target_pixiv_id"),
            status: row.get("status"),
            error_class: row.get("error_class"),
            result: row.get::<Json<Value>, _>("result").0,
            created_at: row.get("created_at"),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PixivAccountStatus {
    pub account_id: Option<Uuid>,
    pub state: PixivAccountState,
    pub bookmark_writeback_enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PixivCredentialEnvelope {
    pub key_id: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PixivAccountRecord {
    pub id: Uuid,
    pub pixiv_user_id: i64,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub state: PixivAccountState,
    pub credential: Option<PixivCredentialEnvelope>,
    pub user_agent: String,
    pub bookmark_writeback_enabled: bool,
    pub last_validated_at: Option<OffsetDateTime>,
    pub revision: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavePixivAccount {
    pub pixiv_user_id: i64,
    pub display_name: String,
    pub cookie_key_id: String,
    pub cookie_nonce: Vec<u8>,
    pub cookie_ciphertext: Vec<u8>,
    pub user_agent: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivatePixivAccount {
    pub pixiv_user_id: i64,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub state: PixivAccountState,
    pub cookie_key_id: String,
    pub cookie_nonce: Vec<u8>,
    pub cookie_ciphertext: Vec<u8>,
    pub user_agent: String,
    pub validated_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug)]
pub struct RecordBookmarkWriteback {
    pub account_id: Uuid,
    pub operation: &'static str,
    pub target_pixiv_id: i64,
    pub status: &'static str,
    pub error_class: Option<String>,
    pub result: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BookmarkWritebackRecord {
    pub id: Uuid,
    pub account_id: Uuid,
    pub operation: String,
    pub target_pixiv_id: i64,
    pub status: String,
    pub error_class: Option<String>,
    pub result: Value,
    pub created_at: OffsetDateTime,
}

async fn append_account_row_events(
    db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    rows: Vec<sqlx::postgres::PgRow>,
) -> Result<(), DbError> {
    for row in rows {
        append_account_event(db, tx, row.get("id"), row.get("revision")).await?;
    }
    Ok(())
}

pub(crate) async fn append_account_event(
    db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    account_id: Uuid,
    revision: i64,
) -> Result<(), DbError> {
    EventRepository::new(db.clone())
        .append_in_tx(
            tx,
            EventResource::PixivAccount,
            account_id,
            EventPayload::PixivAccountChanged { revision },
        )
        .await?;
    Ok(())
}

fn account_from_row(row: &sqlx::postgres::PgRow) -> Result<PixivAccountRecord, DbError> {
    let state_value: String = row.get("state");
    let state = PixivAccountState::from_db_value(&state_value).ok_or_else(|| {
        DbError::InvalidValue(format!("unknown pixiv account state {state_value}"))
    })?;
    let cookie_key_id: Option<String> = row.get("cookie_key_id");
    let cookie_nonce: Option<Vec<u8>> = row.get("cookie_nonce");
    let cookie_ciphertext: Option<Vec<u8>> = row.get("cookie_ciphertext");
    let credential = match (cookie_key_id, cookie_nonce, cookie_ciphertext) {
        (Some(key_id), Some(nonce), Some(ciphertext)) => Some(PixivCredentialEnvelope {
            key_id,
            nonce,
            ciphertext,
        }),
        (None, None, None) => None,
        _ => {
            return Err(DbError::InvalidValue(
                "pixiv account credential envelope is incomplete".to_owned(),
            ));
        }
    };
    if (state == PixivAccountState::Unconfigured) != credential.is_none() {
        return Err(DbError::InvalidValue(
            "pixiv account state does not match its credential".to_owned(),
        ));
    }
    Ok(PixivAccountRecord {
        id: row.get("id"),
        pixiv_user_id: row.get("pixiv_user_id"),
        display_name: row.get("display_name"),
        avatar_url: row.get("avatar_url"),
        state,
        credential,
        user_agent: row.get("user_agent"),
        bookmark_writeback_enabled: row.get("bookmark_writeback_enabled"),
        last_validated_at: row.get("last_validated_at"),
        revision: row.get("revision"),
    })
}

fn validate_credential_envelope(
    pixiv_user_id: i64,
    display_name: &str,
    cookie_key_id: &str,
    cookie_nonce: &[u8],
    cookie_ciphertext: &[u8],
    user_agent: &str,
) -> Result<(), DbError> {
    if pixiv_user_id <= 0 {
        return Err(DbError::InvalidValue(
            "pixiv user id must be positive".to_owned(),
        ));
    }
    if display_name.trim().is_empty()
        || cookie_key_id.trim().is_empty()
        || cookie_nonce.len() < 12
        || cookie_ciphertext.is_empty()
        || user_agent.trim().is_empty()
    {
        return Err(DbError::InvalidValue(
            "pixiv account credential envelope is incomplete".to_owned(),
        ));
    }
    Ok(())
}

fn normalize_avatar_url(avatar_url: Option<&str>) -> Result<Option<String>, DbError> {
    let avatar_url = avatar_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if let Some(value) = &avatar_url {
        let parsed = url::Url::parse(value)
            .map_err(|_| DbError::InvalidValue("pixiv avatar URL is invalid".to_owned()))?;
        if parsed.scheme() != "https" || parsed.host_str().is_none() {
            return Err(DbError::InvalidValue(
                "pixiv avatar URL must be an HTTPS URL".to_owned(),
            ));
        }
    }
    Ok(avatar_url)
}
