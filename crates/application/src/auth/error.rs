use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("authentication is temporarily unavailable")]
    RateLimited { retry_after_seconds: u64 },
    #[error("invalid session")]
    InvalidSession,
    #[error("forbidden")]
    Forbidden,
    #[error("authentication service failed")]
    Internal,
}

impl AuthError {
    pub fn is_invalid_credentials(&self) -> bool {
        matches!(self, Self::InvalidCredentials)
    }

    pub fn is_rate_limited(&self) -> bool {
        matches!(self, Self::RateLimited { .. })
    }

    pub(crate) fn internal<T>(_error: T) -> Self {
        Self::Internal
    }

    pub(crate) fn rng<T>(_error: T) -> Self {
        Self::Internal
    }
}
