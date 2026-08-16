use crate::jobs::JobService;
use async_trait::async_trait;
use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};
use pixivarchive_db::{
    ActivatePixivAccount, Db, DbError, PixivAccountRecord, PixivAccountRepository,
    PixivAccountStatus, SubscriptionRepository,
};
use pixivarchive_domain::{
    pixiv::{PixivRankingContent, PixivRankingMode, PixivRankingRequest},
    subscription::PixivAccountState,
};
use pixivarchive_pixiv::{PixivEndpoint, PixivErrorClass, PixivGateway, PixivRequestContext};
use rand::{TryRngCore, rngs::OsRng};
use secrecy::{ExposeSecret, SecretString};
use std::{collections::HashSet, sync::Arc};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PixivAccount {
    pub id: Uuid,
    pub pixiv_user_id: i64,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub state: PixivAccountState,
    pub bookmark_writeback_enabled: bool,
    pub last_validated_at: Option<OffsetDateTime>,
    pub revision: i64,
}

impl From<PixivAccountRecord> for PixivAccount {
    fn from(record: PixivAccountRecord) -> Self {
        Self {
            id: record.id,
            pixiv_user_id: record.pixiv_user_id,
            display_name: record.display_name,
            avatar_url: record.avatar_url,
            state: record.state,
            bookmark_writeback_enabled: record.bookmark_writeback_enabled,
            last_validated_at: record.last_validated_at,
            revision: record.revision,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PixivAccountSummary {
    pub account_id: Option<Uuid>,
    pub state: PixivAccountState,
    pub bookmark_writeback_enabled: bool,
}

impl From<PixivAccountStatus> for PixivAccountSummary {
    fn from(status: PixivAccountStatus) -> Self {
        Self {
            account_id: status.account_id,
            state: status.state,
            bookmark_writeback_enabled: status.bookmark_writeback_enabled,
        }
    }
}

#[derive(Clone)]
pub struct PixivAccountService<G> {
    db: Db,
    accounts: PixivAccountRepository,
    subscriptions: SubscriptionRepository,
    jobs: JobService,
    gateway: Arc<G>,
}

impl<G> PixivAccountService<G>
where
    G: PixivGateway + 'static,
{
    pub fn new(db: Db, gateway: G) -> Self {
        Self {
            db: db.clone(),
            accounts: PixivAccountRepository::new(db.clone()),
            subscriptions: SubscriptionRepository::new(db.clone()),
            jobs: JobService::new(db),
            gateway: Arc::new(gateway),
        }
    }

    pub async fn status(&self) -> Result<PixivAccountSummary, DbError> {
        Ok(self.accounts.status().await?.into())
    }

    pub async fn current(&self) -> Result<Option<PixivAccount>, DbError> {
        Ok(self.current_record().await?.map(PixivAccount::from))
    }

    async fn current_record(&self) -> Result<Option<PixivAccountRecord>, DbError> {
        self.accounts.current().await
    }

    pub async fn update_cookie(
        &self,
        update: AccountCookieUpdate,
    ) -> Result<PixivAccount, AccountServiceError> {
        Ok(self.update_cookie_record(update).await?.into())
    }

    async fn update_cookie_record(
        &self,
        update: AccountCookieUpdate,
    ) -> Result<PixivAccountRecord, AccountServiceError> {
        let existing = self
            .accounts
            .find_by_pixiv_user_id(update.context.user_id())
            .await?;
        let validation = self
            .gateway
            .validate_account(&update.context)
            .await
            .map_err(|error| {
                tracing::warn!(
                    pixiv_user_id = update.context.user_id(),
                    endpoint = ?error.endpoint(),
                    error_class = ?error.class(),
                    error = %error,
                    "Pixiv candidate credential validation failed"
                );
                AccountServiceError::Validation {
                    class: error.class(),
                    endpoint: error.endpoint(),
                }
            })?;
        if validation.value.user_id != update.context.user_id() {
            return Err(AccountServiceError::Validation {
                class: PixivErrorClass::CredentialInvalid,
                endpoint: Some(PixivEndpoint::Profile),
            });
        }

        let state = match existing.as_ref() {
            Some(account) => self.validated_state(account, &update.context).await?,
            None => PixivAccountState::Normal,
        };
        let recover_waiting_jobs = existing
            .as_ref()
            .is_some_and(|account| account.state != PixivAccountState::Normal);
        self.persist_validated_account(
            ActivatePixivAccount {
                pixiv_user_id: update.context.user_id(),
                display_name: validation.value.display_name,
                avatar_url: validation.value.avatar_url,
                state,
                cookie_key_id: update.cookie_key_id,
                cookie_nonce: update.cookie_nonce,
                cookie_ciphertext: update.cookie_ciphertext,
                user_agent: update.context.user_agent().to_owned(),
                validated_at: (state == PixivAccountState::Normal)
                    .then_some(OffsetDateTime::now_utc()),
            },
            recover_waiting_jobs,
        )
        .await
    }

    pub async fn validate_saved(
        &self,
        context: &PixivRequestContext,
    ) -> Result<PixivAccount, AccountServiceError> {
        Ok(self.validate_saved_record(context).await?.into())
    }

    async fn validate_saved_record(
        &self,
        context: &PixivRequestContext,
    ) -> Result<PixivAccountRecord, AccountServiceError> {
        let account = self.accounts.current().await?.ok_or(DbError::NotFound)?;
        if account.pixiv_user_id != context.user_id() {
            return Err(DbError::InvalidValue(
                "Pixiv account identity does not match the saved credential".to_owned(),
            )
            .into());
        }
        let account = if account.state == PixivAccountState::Normal {
            account
        } else {
            self.accounts
                .set_state(account.id, PixivAccountState::Validating, None)
                .await?
        };
        self.validate_account(account, context).await
    }

    async fn validate_account(
        &self,
        account: PixivAccountRecord,
        context: &PixivRequestContext,
    ) -> Result<PixivAccountRecord, AccountServiceError> {
        let validation = match self.gateway.validate_account(context).await {
            Ok(validation) => validation,
            Err(error) => {
                tracing::warn!(
                    account_id = %account.id,
                    pixiv_user_id = account.pixiv_user_id,
                    endpoint = ?error.endpoint(),
                    error_class = ?error.class(),
                    error = %error,
                    "Pixiv account validation failed"
                );
                if error.class() == PixivErrorClass::CredentialInvalid {
                    self.jobs.block_account(account.id).await?;
                    return Ok(self.accounts.get(account.id).await?);
                }
                let state = match error.class() {
                    PixivErrorClass::AgeRestrictedDisabled => PixivAccountState::Restricted,
                    _ => PixivAccountState::Validating,
                };
                return Ok(self.accounts.set_state(account.id, state, None).await?);
            }
        };
        if validation.value.user_id != account.pixiv_user_id {
            self.jobs.block_account(account.id).await?;
            return Ok(self.accounts.get(account.id).await?);
        }

        let state = match self.validated_state(&account, context).await {
            Ok(state) => state,
            Err(AccountServiceError::Validation {
                class: PixivErrorClass::CredentialInvalid,
                ..
            }) => {
                self.jobs.block_account(account.id).await?;
                return Ok(self.accounts.get(account.id).await?);
            }
            Err(error) => return Err(error),
        };

        let recover_waiting_jobs = account.state != PixivAccountState::Normal;
        let credential = account.credential.ok_or_else(|| {
            DbError::InvalidValue("configured Pixiv account is missing its credential".to_owned())
        })?;
        self.persist_validated_account(
            ActivatePixivAccount {
                pixiv_user_id: account.pixiv_user_id,
                display_name: validation.value.display_name,
                avatar_url: validation.value.avatar_url,
                state,
                cookie_key_id: credential.key_id,
                cookie_nonce: credential.nonce,
                cookie_ciphertext: credential.ciphertext,
                user_agent: account.user_agent,
                validated_at: (state == PixivAccountState::Normal)
                    .then_some(OffsetDateTime::now_utc()),
            },
            recover_waiting_jobs,
        )
        .await
    }

    async fn validated_state(
        &self,
        account: &PixivAccountRecord,
        context: &PixivRequestContext,
    ) -> Result<PixivAccountState, AccountServiceError> {
        if !self
            .subscriptions
            .has_enabled_r18_subscription(account.id)
            .await?
        {
            return Ok(PixivAccountState::Normal);
        }

        let result = self
            .gateway
            .ranking_page(
                context,
                PixivRankingRequest {
                    mode: PixivRankingMode::R18,
                    content: PixivRankingContent::All,
                    date: None,
                    page: 1,
                },
            )
            .await;
        let Err(error) = result else {
            return Ok(PixivAccountState::Normal);
        };
        tracing::warn!(
            account_id = %account.id,
            pixiv_user_id = account.pixiv_user_id,
            endpoint = ?error.endpoint(),
            error_class = ?error.class(),
            error = %error,
            "Pixiv age-restricted ranking probe failed"
        );
        match error.class() {
            PixivErrorClass::CredentialInvalid => Err(AccountServiceError::Validation {
                class: error.class(),
                endpoint: error.endpoint(),
            }),
            PixivErrorClass::AgeRestrictedDisabled => Ok(PixivAccountState::Restricted),
            _ => Ok(PixivAccountState::Validating),
        }
    }

    async fn persist_validated_account(
        &self,
        account: ActivatePixivAccount,
        recover_waiting_jobs: bool,
    ) -> Result<PixivAccountRecord, AccountServiceError> {
        let priorities = if account.state == PixivAccountState::Normal && recover_waiting_jobs {
            Some(crate::settings::effective_job_priority_policy(&self.db).await?)
        } else {
            None
        };
        Ok(self
            .jobs
            .activate_validated_account(account, priorities.as_ref())
            .await?)
    }

    pub async fn set_bookmark_writeback_enabled(
        &self,
        account_id: Uuid,
        enabled: bool,
    ) -> Result<PixivAccount, DbError> {
        Ok(self
            .accounts
            .set_bookmark_writeback_enabled(account_id, enabled)
            .await?
            .into())
    }
}

#[derive(Debug)]
pub struct AccountCookieUpdate {
    pub context: PixivRequestContext,
    pub cookie_key_id: String,
    pub cookie_nonce: Vec<u8>,
    pub cookie_ciphertext: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum AccountServiceError {
    #[error("pixiv account storage failed")]
    Storage(#[from] DbError),
    #[error("Pixiv candidate validation failed: {class:?}")]
    Validation {
        class: PixivErrorClass,
        endpoint: Option<PixivEndpoint>,
    },
}

#[derive(Clone)]
pub struct PixivAccountAdminService {
    repository: PixivAccountRepository,
    jobs: JobService,
}

impl PixivAccountAdminService {
    pub fn new(db: Db) -> Self {
        Self {
            repository: PixivAccountRepository::new(db.clone()),
            jobs: JobService::new(db),
        }
    }

    pub async fn current(&self) -> Result<Option<PixivAccount>, DbError> {
        Ok(self.repository.current().await?.map(PixivAccount::from))
    }

    pub async fn get(&self, account_id: Uuid) -> Result<PixivAccount, DbError> {
        self.repository
            .get(account_id)
            .await
            .map(PixivAccount::from)
    }

    pub async fn set_bookmark_writeback(
        &self,
        expected_account_id: Uuid,
        expected_revision: i64,
        enabled: bool,
    ) -> Result<PixivAccount, DbError> {
        let account = self.repository.require_current(expected_account_id).await?;
        if account.state.blocks_subscription_runs() {
            return Err(DbError::RevisionConflict);
        }
        Ok(self
            .repository
            .set_bookmark_writeback_enabled_at_revision(account.id, expected_revision, enabled)
            .await?
            .into())
    }

    pub async fn clear_credential(
        &self,
        expected_account_id: Uuid,
        expected_revision: i64,
    ) -> Result<PixivAccount, DbError> {
        self.repository.require_current(expected_account_id).await?;
        Ok(self
            .jobs
            .clear_account_credential(expected_account_id, expected_revision)
            .await?
            .into())
    }
}

#[derive(Debug)]
pub struct UpdatePixivAccountRequest {
    pub cookie: String,
}

#[async_trait]
pub trait PixivAccountCommandPort: Send + Sync {
    async fn update(
        &self,
        request: UpdatePixivAccountRequest,
    ) -> Result<PixivAccount, PixivAccountAdminError>;

    async fn validate(
        &self,
        expected_account_id: Option<Uuid>,
    ) -> Result<PixivAccount, PixivAccountAdminError>;
}

#[derive(Clone, Default)]
pub struct DisabledPixivAccountCommandPort;

#[async_trait]
impl PixivAccountCommandPort for DisabledPixivAccountCommandPort {
    async fn update(
        &self,
        _request: UpdatePixivAccountRequest,
    ) -> Result<PixivAccount, PixivAccountAdminError> {
        Err(PixivAccountAdminError::Unavailable)
    }

    async fn validate(
        &self,
        _expected_account_id: Option<Uuid>,
    ) -> Result<PixivAccount, PixivAccountAdminError> {
        Err(PixivAccountAdminError::Unavailable)
    }
}

#[derive(Clone)]
pub struct LivePixivAccountCommandPort<G> {
    accounts: PixivAccountService<G>,
    cipher: PixivCookieCipher,
    user_agent: String,
}

impl<G> LivePixivAccountCommandPort<G>
where
    G: PixivGateway + 'static,
{
    pub fn new(
        db: Db,
        gateway: G,
        cipher: PixivCookieCipher,
        user_agent: impl Into<String>,
    ) -> Self {
        Self {
            accounts: PixivAccountService::new(db, gateway),
            cipher,
            user_agent: user_agent.into(),
        }
    }
}

#[async_trait]
impl<G> PixivAccountCommandPort for LivePixivAccountCommandPort<G>
where
    G: PixivGateway + 'static,
{
    async fn update(
        &self,
        request: UpdatePixivAccountRequest,
    ) -> Result<PixivAccount, PixivAccountAdminError> {
        if self.user_agent.trim().is_empty() {
            return Err(PixivAccountAdminError::InvalidInput);
        }

        let (pixiv_user_id, cookie) = normalize_cookie(&request.cookie)?;
        let encrypted = self.cipher.encrypt(pixiv_user_id, &cookie)?;
        let context =
            PixivRequestContext::new(cookie, pixiv_user_id, self.user_agent.trim().to_owned());
        self.accounts
            .update_cookie(AccountCookieUpdate {
                context,
                cookie_key_id: encrypted.key_id,
                cookie_nonce: encrypted.nonce.to_vec(),
                cookie_ciphertext: encrypted.ciphertext,
            })
            .await
            .map_err(PixivAccountAdminError::from)
    }

    async fn validate(
        &self,
        expected_account_id: Option<Uuid>,
    ) -> Result<PixivAccount, PixivAccountAdminError> {
        let account = self
            .accounts
            .current_record()
            .await?
            .ok_or(PixivAccountAdminError::NotConfigured)?;
        if expected_account_id.is_some_and(|expected| expected != account.id) {
            return Err(DbError::RevisionConflict.into());
        }
        if account.state == PixivAccountState::Unconfigured {
            return Err(PixivAccountAdminError::NotConfigured);
        }
        let context = PixivRequestContext::new(
            self.cipher.decrypt(&account)?,
            account.pixiv_user_id,
            account.user_agent.clone(),
        );
        self.accounts
            .validate_saved(&context)
            .await
            .map_err(PixivAccountAdminError::from)
    }
}

#[derive(Clone)]
pub struct PixivCookieKeyConfig {
    key_id: String,
    key: [u8; 32],
}

impl PixivCookieKeyConfig {
    pub fn new(key_id: impl Into<String>, key: [u8; 32]) -> Self {
        Self {
            key_id: key_id.into(),
            key,
        }
    }
}

#[derive(Clone)]
pub struct PixivCookieKeyringConfig {
    primary: PixivCookieKeyConfig,
    previous: Vec<PixivCookieKeyConfig>,
}

impl PixivCookieKeyringConfig {
    pub fn new(primary: PixivCookieKeyConfig) -> Self {
        Self {
            primary,
            previous: Vec::new(),
        }
    }

    pub fn with_previous(mut self, previous: Vec<PixivCookieKeyConfig>) -> Self {
        self.previous = previous;
        self
    }
}

#[derive(Clone)]
pub struct PixivCookieCipher {
    primary: PixivCookieKeyConfig,
    keys: Vec<PixivCookieKeyConfig>,
}

impl PixivCookieCipher {
    pub fn new(config: PixivCookieKeyringConfig) -> Result<Self, PixivCookieCipherError> {
        let mut ids = HashSet::new();
        for key in std::iter::once(&config.primary).chain(config.previous.iter()) {
            if key.key_id.trim().is_empty() || !ids.insert(key.key_id.clone()) {
                return Err(PixivCookieCipherError::InvalidKeyring);
            }
        }
        let mut keys = vec![config.primary.clone()];
        keys.extend(config.previous);
        Ok(Self {
            primary: config.primary,
            keys,
        })
    }

    pub fn encrypt(
        &self,
        pixiv_user_id: i64,
        cookie: &SecretString,
    ) -> Result<EncryptedPixivCookie, PixivCookieCipherError> {
        if pixiv_user_id <= 0 || cookie.expose_secret().is_empty() {
            return Err(PixivCookieCipherError::InvalidCredential);
        }
        let mut nonce = [0_u8; 24];
        OsRng
            .try_fill_bytes(&mut nonce)
            .map_err(|_| PixivCookieCipherError::Random)?;
        let cipher = XChaCha20Poly1305::new(&self.primary.key.into());
        let nonce_ref =
            XNonce::try_from(&nonce[..]).map_err(|_| PixivCookieCipherError::Encryption)?;
        let ciphertext = cipher
            .encrypt(
                &nonce_ref,
                Payload {
                    msg: cookie.expose_secret().as_bytes(),
                    aad: &cookie_aad(pixiv_user_id),
                },
            )
            .map_err(|_| PixivCookieCipherError::Encryption)?;
        Ok(EncryptedPixivCookie {
            key_id: self.primary.key_id.clone(),
            nonce,
            ciphertext,
        })
    }

    pub fn decrypt(
        &self,
        record: &PixivAccountRecord,
    ) -> Result<SecretString, PixivCookieCipherError> {
        let credential = record
            .credential
            .as_ref()
            .ok_or(PixivCookieCipherError::InvalidCredential)?;
        if credential.nonce.len() != 24 {
            return Err(PixivCookieCipherError::InvalidCredential);
        }
        let key = self
            .keys
            .iter()
            .find(|key| key.key_id == credential.key_id)
            .ok_or(PixivCookieCipherError::InvalidCredential)?;
        let cipher = XChaCha20Poly1305::new(&key.key.into());
        let nonce_ref = XNonce::try_from(credential.nonce.as_slice())
            .map_err(|_| PixivCookieCipherError::InvalidCredential)?;
        let plaintext = cipher
            .decrypt(
                &nonce_ref,
                Payload {
                    msg: &credential.ciphertext,
                    aad: &cookie_aad(record.pixiv_user_id),
                },
            )
            .map_err(|_| PixivCookieCipherError::InvalidCredential)?;
        let cookie =
            String::from_utf8(plaintext).map_err(|_| PixivCookieCipherError::InvalidCredential)?;
        Ok(SecretString::from(cookie))
    }
}

#[derive(Clone)]
pub struct PixivAccountContextFactory {
    accounts: PixivAccountRepository,
    cipher: PixivCookieCipher,
}

impl PixivAccountContextFactory {
    pub fn new(db: Db, cipher: PixivCookieCipher) -> Self {
        Self {
            accounts: PixivAccountRepository::new(db),
            cipher,
        }
    }

    pub async fn load(
        &self,
        account_id: Uuid,
    ) -> Result<PixivRequestContext, PixivAccountContextError> {
        let account = self.accounts.get(account_id).await?;
        self.context_for_record(&account)
    }

    pub(crate) fn context_for_record(
        &self,
        record: &PixivAccountRecord,
    ) -> Result<PixivRequestContext, PixivAccountContextError> {
        if record.state.blocks_subscription_runs() {
            return Err(PixivAccountContextError::AccountUnavailable {
                state: record.state,
            });
        }
        Ok(PixivRequestContext::new(
            self.cipher.decrypt(record)?,
            record.pixiv_user_id,
            record.user_agent.clone(),
        ))
    }
}

#[derive(Debug, Error)]
pub enum PixivAccountContextError {
    #[error("Pixiv account state {state:?} does not allow requests")]
    AccountUnavailable { state: PixivAccountState },
    #[error("Pixiv account storage failed")]
    Storage(#[from] DbError),
    #[error("Pixiv Cookie protection failed")]
    Cipher(#[from] PixivCookieCipherError),
}

pub struct EncryptedPixivCookie {
    pub key_id: String,
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PixivCookieCipherError {
    #[error("Pixiv Cookie keyring is invalid")]
    InvalidKeyring,
    #[error("Pixiv Cookie credential is invalid")]
    InvalidCredential,
    #[error("Pixiv Cookie encryption failed")]
    Encryption,
    #[error("secure random generation failed")]
    Random,
}

#[derive(Debug, Error)]
pub enum PixivAccountAdminError {
    #[error("Pixiv account input is invalid")]
    InvalidInput,
    #[error("Pixiv account is not configured")]
    NotConfigured,
    #[error("Pixiv account commands are unavailable")]
    Unavailable,
    #[error("Pixiv account validation failed: {class:?}")]
    Validation {
        class: PixivErrorClass,
        endpoint: Option<PixivEndpoint>,
    },
    #[error("Pixiv account storage failed")]
    Storage(#[from] DbError),
    #[error("Pixiv Cookie protection failed")]
    Cipher(#[from] PixivCookieCipherError),
}

impl From<AccountServiceError> for PixivAccountAdminError {
    fn from(error: AccountServiceError) -> Self {
        match error {
            AccountServiceError::Storage(error) => Self::Storage(error),
            AccountServiceError::Validation { class, endpoint } => {
                Self::Validation { class, endpoint }
            }
        }
    }
}

fn cookie_aad(pixiv_user_id: i64) -> Vec<u8> {
    let mut aad = b"PixivArchive:pixiv-cookie:v1:".to_vec();
    aad.extend_from_slice(&pixiv_user_id.to_be_bytes());
    aad
}

fn normalize_cookie(input: &str) -> Result<(i64, SecretString), PixivAccountAdminError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(PixivAccountAdminError::InvalidInput);
    }
    let value = if input.contains('=') {
        input
            .split(';')
            .filter_map(|part| part.trim().split_once('='))
            .find_map(|(name, value)| (name.trim() == "PHPSESSID").then(|| value.trim()))
            .ok_or(PixivAccountAdminError::InvalidInput)?
    } else {
        input
    };
    let (user_id, session) = value
        .split_once('_')
        .ok_or(PixivAccountAdminError::InvalidInput)?;
    if user_id.is_empty()
        || session.is_empty()
        || !user_id.bytes().all(|byte| byte.is_ascii_digit())
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b';')
    {
        return Err(PixivAccountAdminError::InvalidInput);
    }
    let user_id = user_id
        .parse::<i64>()
        .ok()
        .filter(|user_id| *user_id > 0)
        .ok_or(PixivAccountAdminError::InvalidInput)?;
    Ok((user_id, SecretString::from(format!("PHPSESSID={value}"))))
}

#[cfg(test)]
mod tests {
    use super::{
        PixivCookieCipher, PixivCookieCipherError, PixivCookieKeyConfig, PixivCookieKeyringConfig,
        normalize_cookie,
    };
    use pixivarchive_db::{PixivAccountRecord, PixivCredentialEnvelope};
    use pixivarchive_domain::subscription::PixivAccountState;
    use secrecy::{ExposeSecret, SecretString};
    use uuid::Uuid;

    #[test]
    fn pixiv_cookie_cipher_binds_ciphertext_to_the_account() {
        let cipher = PixivCookieCipher::new(PixivCookieKeyringConfig::new(
            PixivCookieKeyConfig::new("primary", [7; 32]),
        ))
        .unwrap();
        let encrypted = cipher
            .encrypt(10_001, &SecretString::from("PHPSESSID=secret"))
            .unwrap();
        let mut record = PixivAccountRecord {
            id: Uuid::now_v7(),
            pixiv_user_id: 10_001,
            display_name: "test".to_owned(),
            avatar_url: None,
            state: PixivAccountState::Normal,
            credential: Some(PixivCredentialEnvelope {
                key_id: encrypted.key_id,
                nonce: encrypted.nonce.to_vec(),
                ciphertext: encrypted.ciphertext,
            }),
            user_agent: "test-agent".to_owned(),
            bookmark_writeback_enabled: false,
            last_validated_at: None,
            revision: 1,
        };

        assert_eq!(
            cipher.decrypt(&record).unwrap().expose_secret(),
            "PHPSESSID=secret"
        );
        record.pixiv_user_id += 1;
        assert_eq!(
            cipher.decrypt(&record).unwrap_err(),
            PixivCookieCipherError::InvalidCredential
        );
    }

    #[test]
    fn pixiv_cookie_accepts_a_value_or_cookie_header_and_extracts_the_user_id() {
        for input in [
            "10001_session-value",
            "PHPSESSID=10001_session-value",
            "other=ignored; PHPSESSID=10001_session-value; locale=zh",
        ] {
            let (user_id, cookie) = normalize_cookie(input).unwrap();
            assert_eq!(user_id, 10_001);
            assert_eq!(cookie.expose_secret(), "PHPSESSID=10001_session-value");
        }
        for input in ["", "PHPSESSID=", "PHPSESSID=no-id", "PHPSESSID=0_value"] {
            assert!(normalize_cookie(input).is_err());
        }
    }
}
