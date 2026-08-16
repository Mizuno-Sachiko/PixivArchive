use super::AuthError;
use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::SaltString,
};
use rand::{TryRngCore, rngs::OsRng};
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct Verification {
    pub matched: bool,
    pub replacement_phc: Option<String>,
}

pub async fn hash(password: String, permits: Arc<Semaphore>) -> Result<String, AuthError> {
    run_argon2(permits, move || {
        let mut salt = [0u8; 16];
        OsRng.try_fill_bytes(&mut salt).map_err(AuthError::rng)?;
        let salt = SaltString::encode_b64(&salt).map_err(AuthError::internal)?;
        argon2()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(AuthError::internal)
    })
    .await
}

pub async fn verify(
    phc: String,
    password: String,
    permits: Arc<Semaphore>,
) -> Result<Verification, AuthError> {
    run_argon2(permits, move || {
        let parsed = PasswordHash::new(&phc).map_err(|_| AuthError::InvalidCredentials)?;
        if parsed.algorithm.as_str() != "argon2id" || parsed.version != Some(Version::V0x13.into())
        {
            return Err(AuthError::InvalidCredentials);
        }
        let matched = argon2()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok();
        let replacement_phc = if matched && needs_rehash(&parsed) {
            Some(hash_sync(&password)?)
        } else {
            None
        };
        Ok(Verification {
            matched,
            replacement_phc,
        })
    })
    .await
}

fn hash_sync(password: &str) -> Result<String, AuthError> {
    let mut salt = [0u8; 16];
    OsRng.try_fill_bytes(&mut salt).map_err(AuthError::rng)?;
    let salt = SaltString::encode_b64(&salt).map_err(AuthError::internal)?;
    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(AuthError::internal)
}

fn needs_rehash(parsed: &PasswordHash<'_>) -> bool {
    let Some(params) = Params::try_from(parsed).ok() else {
        return true;
    };
    params != *argon2().params()
}

async fn run_argon2<T: Send + 'static>(
    permits: Arc<Semaphore>,
    work: impl FnOnce() -> Result<T, AuthError> + Send + 'static,
) -> Result<T, AuthError> {
    let permit = permits
        .try_acquire_owned()
        .map_err(|_| AuthError::RateLimited {
            retry_after_seconds: 1,
        })?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
    .map_err(AuthError::internal)?
}

fn argon2() -> Argon2<'static> {
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(19_456, 2, 1, Some(32)).unwrap(),
    )
}
