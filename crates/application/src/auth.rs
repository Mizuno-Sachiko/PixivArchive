mod error;
mod password;
mod rate_limit;
mod session;

use crate::settings::SettingsService;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
pub use error::AuthError;
use pixivarchive_db::{
    AuthRepository, Db, DbError,
    auth::{IssueSession, LoginAttempt, RateLimitKind, RateLimitLease, UpdatePassword},
};
use pixivarchive_domain::auth::SessionContext;
use rand::{TryRngCore, rngs::OsRng};
use std::{
    fmt,
    sync::{Arc, Mutex},
};
use time::{Duration, OffsetDateTime};
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AuthService {
    repository: AuthRepository,
    settings: SettingsService,
    clock: Arc<dyn Clock>,
    argon2: Arc<Semaphore>,
}

impl AuthService {
    pub fn new(db: Db, config: AuthConfig) -> Self {
        Self::with_clock(db, config, SystemClock)
    }

    pub fn with_clock(db: Db, config: AuthConfig, clock: impl Clock + 'static) -> Self {
        Self {
            repository: AuthRepository::new(db.clone()),
            settings: SettingsService::new(db),
            argon2: Arc::new(Semaphore::new(config.argon2_permits)),
            clock: Arc::new(clock),
        }
    }

    pub async fn synchronize_password(
        &self,
        password: impl Into<String>,
    ) -> Result<PasswordSync, AuthError> {
        let password = password.into();
        let Some(administrator) = self
            .repository
            .optional_administrator()
            .await
            .map_err(AuthError::internal)?
        else {
            let phc = password::hash(password, self.argon2.clone()).await?;
            self.repository
                .create_administrator("admin", &phc, self.now())
                .await
                .map_err(AuthError::internal)?;
            return Ok(PasswordSync::Created);
        };
        let matches_environment_password = match password::verify(
            administrator.password_phc.clone(),
            password.clone(),
            self.argon2.clone(),
        )
        .await
        {
            Ok(verification) => verification.matched,
            Err(AuthError::InvalidCredentials) => false,
            Err(error) => return Err(error),
        };
        if matches_environment_password {
            return Ok(PasswordSync::Unchanged);
        }
        let phc = password::hash(password, self.argon2.clone()).await?;
        self.repository
            .update_password_and_revoke_sessions(UpdatePassword {
                administrator_snapshot: &administrator,
                new_phc: &phc,
                increment_version: true,
                now: self.now(),
            })
            .await
            .map_err(map_credential_state_error)?;
        Ok(PasswordSync::Updated)
    }

    pub async fn login(&self, request: LoginRequest) -> Result<IssuedSession, AuthError> {
        let now = self.now();
        let security = self
            .settings
            .effective()
            .await
            .map_err(AuthError::internal)?
            .security;
        let administrator = self
            .repository
            .administrator()
            .await
            .map_err(AuthError::internal)?;
        let session_token = session::new_token()?;
        let csrf_token = session::new_token()?;
        let session_digest = session::digest(&session_token);
        let csrf_digest = session::digest(&csrf_token);
        let reservations = rate_limit::login_reservations(&request.source_bucket, &security);
        let lease = match self.repository.reserve_rate_limit(&reservations, now).await {
            Ok(lease) => lease,
            Err(DbError::RateLimited {
                retry_after_seconds,
            }) => {
                self.repository
                    .record_login_attempt(
                        login_attempt(None, &request, false, Some("rate_limited")),
                        now,
                    )
                    .await
                    .map_err(AuthError::internal)?;
                return Err(AuthError::RateLimited {
                    retry_after_seconds,
                });
            }
            Err(error) => return Err(AuthError::internal(error)),
        };

        let password_verification = match password::verify(
            administrator.password_phc.clone(),
            request.password.clone(),
            self.argon2.clone(),
        )
        .await
        {
            Ok(verification) => verification,
            Err(error) => {
                self.repository
                    .release_rate_limit(lease)
                    .await
                    .map_err(AuthError::internal)?;
                return Err(error);
            }
        };
        if !password_verification.matched {
            self.record_login_failure(
                lease,
                &rate_limit::password_failure_kinds(),
                login_attempt(
                    Some(administrator.id),
                    &request,
                    false,
                    Some("invalid_password"),
                ),
                now,
            )
            .await?;
            return Err(AuthError::InvalidCredentials);
        }

        let context = match self
            .repository
            .finalize_successful_login(
                IssueSession {
                    administrator_snapshot: &administrator,
                    token_digest: &session_digest,
                    csrf_digest: &csrf_digest,
                    now,
                    idle_timeout: security.session_idle_timeout(),
                    absolute_timeout: security.session_absolute_timeout(),
                },
                password_verification.replacement_phc.as_deref(),
                lease.clone(),
                login_attempt(Some(administrator.id), &request, true, None),
            )
            .await
        {
            Ok(context) => context,
            Err(error) => {
                self.repository
                    .release_rate_limit(lease)
                    .await
                    .map_err(AuthError::internal)?;
                return Err(map_credential_state_error(error));
            }
        };
        Ok(IssuedSession {
            context,
            session_token,
            csrf_token,
        })
    }

    pub async fn authenticate(&self, session_token: &str) -> Result<SessionContext, AuthError> {
        let security = self
            .settings
            .effective()
            .await
            .map_err(AuthError::internal)?
            .security;
        self.repository
            .authenticate_session(
                &session::digest(session_token),
                self.now(),
                security.session_idle_timeout(),
                security.last_activity_persist_interval(),
            )
            .await
            .map_err(map_session_error)
    }

    pub async fn verify_csrf(
        &self,
        context: &SessionContext,
        cookie_token: &str,
        header_token: &str,
    ) -> Result<(), AuthError> {
        if !constant_time_eq::constant_time_eq(cookie_token.as_bytes(), header_token.as_bytes()) {
            return Err(AuthError::Forbidden);
        }
        let valid = self
            .repository
            .verify_csrf_digest(context.session_id, &session::digest(header_token))
            .await
            .map_err(AuthError::internal)?;
        if valid {
            Ok(())
        } else {
            Err(AuthError::Forbidden)
        }
    }

    pub async fn logout(&self, context: &SessionContext) -> Result<(), AuthError> {
        self.repository
            .revoke_session(context.session_id, self.now())
            .await
            .map_err(AuthError::internal)
    }

    fn now(&self) -> OffsetDateTime {
        self.clock.now()
    }

    async fn record_login_failure(
        &self,
        lease: RateLimitLease,
        failed_kinds: &[RateLimitKind],
        attempt: LoginAttempt<'_>,
        now: OffsetDateTime,
    ) -> Result<(), AuthError> {
        let release_lease = lease.clone();
        match self
            .repository
            .record_rate_limit_failure(lease, failed_kinds, attempt, now)
            .await
        {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = self.repository.release_rate_limit(release_lease).await;
                Err(AuthError::internal(error))
            }
        }
    }
}

#[derive(Clone)]
pub struct AuthConfig {
    argon2_permits: usize,
}

impl AuthConfig {
    pub fn new(argon2_permits: usize) -> Result<Self, AuthError> {
        if argon2_permits == 0 {
            return Err(AuthError::Internal);
        }
        Ok(Self { argon2_permits })
    }

    pub fn new_for_tests() -> Result<Self, AuthError> {
        Self::new(8)
    }
}

pub struct LoginRequest {
    password: String,
    source_bucket: String,
}

impl LoginRequest {
    pub fn new(password: impl Into<String>, source_bucket: impl Into<String>) -> Self {
        Self {
            password: password.into(),
            source_bucket: source_bucket.into(),
        }
    }
}

pub struct IssuedSession {
    context: SessionContext,
    session_token: String,
    csrf_token: String,
}

impl IssuedSession {
    pub fn context(&self) -> &SessionContext {
        &self.context
    }

    pub fn session_token(&self) -> &str {
        &self.session_token
    }

    pub fn csrf_token(&self) -> &str {
        &self.csrf_token
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PasswordSync {
    Created,
    Unchanged,
    Updated,
}

pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

#[derive(Clone)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

#[derive(Clone)]
pub struct StaticClock {
    inner: Arc<Mutex<OffsetDateTime>>,
}

impl StaticClock {
    pub fn new(now: OffsetDateTime) -> Self {
        Self {
            inner: Arc::new(Mutex::new(now)),
        }
    }

    pub fn advance(&self, duration: Duration) {
        let mut guard = self.inner.lock().unwrap();
        *guard += duration;
    }
}

impl Clock for StaticClock {
    fn now(&self) -> OffsetDateTime {
        *self.inner.lock().unwrap()
    }
}

fn random_bytes<const N: usize>() -> Result<[u8; N], AuthError> {
    let mut bytes = [0_u8; N];
    OsRng.try_fill_bytes(&mut bytes).map_err(AuthError::rng)?;
    Ok(bytes)
}

fn token_from_bytes(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn login_attempt<'a>(
    administrator_id: Option<uuid::Uuid>,
    request: &'a LoginRequest,
    succeeded: bool,
    failure_reason: Option<&'a str>,
) -> LoginAttempt<'a> {
    LoginAttempt {
        administrator_id,
        account_bucket: "admin",
        entry_bucket: &request.source_bucket,
        source_bucket: &request.source_bucket,
        succeeded,
        failure_reason,
    }
}

fn map_session_error(error: DbError) -> AuthError {
    match error {
        DbError::NotFound => AuthError::InvalidSession,
        error => AuthError::internal(error),
    }
}

fn map_credential_state_error(error: DbError) -> AuthError {
    match error {
        DbError::NotFound | DbError::RevisionConflict => AuthError::InvalidCredentials,
        error => AuthError::internal(error),
    }
}

impl fmt::Debug for AuthService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthService")
    }
}
