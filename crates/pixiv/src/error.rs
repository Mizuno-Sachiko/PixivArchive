use crate::web::PixivEndpoint;
use reqwest::StatusCode;
use std::{error::Error, fmt};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc2822};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixivErrorClass {
    Network,
    CredentialInvalid,
    AgeRestrictedDisabled,
    RefererForbidden,
    HiddenOrNotFound,
    RateLimited,
    TemporaryPixivError,
    InvalidJsonOrInterstitial,
    CsrfFailed,
    MediaIntegrityFailed,
    ResponseTooLarge,
}

impl PixivErrorClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::CredentialInvalid => "credential_invalid",
            Self::AgeRestrictedDisabled => "age_restricted_disabled",
            Self::RefererForbidden => "referer_forbidden",
            Self::HiddenOrNotFound => "hidden_or_not_found",
            Self::RateLimited => "rate_limited",
            Self::TemporaryPixivError => "temporary_pixiv_error",
            Self::InvalidJsonOrInterstitial => "invalid_json_or_interstitial",
            Self::CsrfFailed => "csrf_failed",
            Self::MediaIntegrityFailed => "media_integrity_failed",
            Self::ResponseTooLarge => "response_too_large",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PixivError {
    class: PixivErrorClass,
    endpoint: Option<PixivEndpoint>,
    http_status: Option<StatusCode>,
    retry_after: Option<Duration>,
    message: Option<String>,
}

impl PixivError {
    pub fn new(class: PixivErrorClass, endpoint: Option<PixivEndpoint>) -> Self {
        Self {
            class,
            endpoint,
            http_status: None,
            retry_after: None,
            message: None,
        }
    }

    pub fn class(&self) -> PixivErrorClass {
        self.class
    }

    pub fn endpoint(&self) -> Option<PixivEndpoint> {
        self.endpoint
    }

    pub fn http_status(&self) -> Option<StatusCode> {
        self.http_status
    }

    pub fn retry_after(&self) -> Option<Duration> {
        self.retry_after
    }

    pub fn media_integrity(message: impl AsRef<str>) -> Self {
        Self::new(PixivErrorClass::MediaIntegrityFailed, None).with_message(message.as_ref())
    }

    pub(crate) fn network(endpoint: PixivEndpoint) -> Self {
        Self::new(PixivErrorClass::Network, Some(endpoint)).with_message("Pixiv request failed")
    }

    pub(crate) fn invalid_json(endpoint: PixivEndpoint) -> Self {
        Self::new(PixivErrorClass::InvalidJsonOrInterstitial, Some(endpoint))
    }

    pub(crate) fn response_too_large(endpoint: PixivEndpoint) -> Self {
        Self::new(PixivErrorClass::ResponseTooLarge, Some(endpoint))
    }

    pub(crate) fn hidden_or_invalid(endpoint: PixivEndpoint, message: impl AsRef<str>) -> Self {
        Self::new(PixivErrorClass::HiddenOrNotFound, Some(endpoint)).with_message(message.as_ref())
    }

    pub(crate) fn credential_invalid(endpoint: PixivEndpoint, message: impl AsRef<str>) -> Self {
        Self::new(PixivErrorClass::CredentialInvalid, Some(endpoint)).with_message(message.as_ref())
    }

    pub(crate) fn with_status(mut self, status: StatusCode) -> Self {
        self.http_status = Some(status);
        self
    }

    pub(crate) fn with_message(mut self, message: &str) -> Self {
        let sanitized = sanitize_message(message);
        if !sanitized.is_empty() {
            self.message = Some(sanitized);
        }
        self
    }

    pub(crate) fn redact_secret(mut self, secret: &str) -> Self {
        if secret.is_empty() {
            return self;
        }
        if let Some(message) = &mut self.message {
            *message = message.replace(secret, "[REDACTED]");
        }
        self
    }
}

impl fmt::Display for PixivError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.class.as_str())?;
        if let Some(endpoint) = self.endpoint {
            write!(formatter, " at {}", endpoint.as_str())?;
        }
        if let Some(status) = self.http_status {
            write!(formatter, " ({})", status.as_u16())?;
        }
        if let Some(message) = &self.message {
            write!(formatter, ": {message}")?;
        }
        Ok(())
    }
}

impl Error for PixivError {}

pub fn classify_http_status(
    endpoint: PixivEndpoint,
    status: StatusCode,
    message: Option<&str>,
    retry_after: Option<&str>,
) -> PixivError {
    let class = if status == StatusCode::TOO_MANY_REQUESTS {
        PixivErrorClass::RateLimited
    } else if status.is_server_error() {
        PixivErrorClass::TemporaryPixivError
    } else if status.is_redirection() {
        PixivErrorClass::InvalidJsonOrInterstitial
    } else if status == StatusCode::UNAUTHORIZED {
        PixivErrorClass::CredentialInvalid
    } else if status == StatusCode::NOT_FOUND {
        PixivErrorClass::HiddenOrNotFound
    } else if status == StatusCode::FORBIDDEN && endpoint == PixivEndpoint::Media {
        PixivErrorClass::RefererForbidden
    } else if matches!(
        endpoint,
        PixivEndpoint::AddBookmark
            | PixivEndpoint::DeleteBookmark
            | PixivEndpoint::AddArtistFollow
            | PixivEndpoint::RemoveArtistFollow
    ) && matches!(status, StatusCode::BAD_REQUEST | StatusCode::FORBIDDEN)
    {
        PixivErrorClass::CsrfFailed
    } else if matches!(
        endpoint,
        PixivEndpoint::PrivateBookmarks
            | PixivEndpoint::FollowLatest
            | PixivEndpoint::Following
            | PixivEndpoint::MypixivLatest
    ) && matches!(status, StatusCode::BAD_REQUEST | StatusCode::FORBIDDEN)
    {
        PixivErrorClass::CredentialInvalid
    } else if matches!(status, StatusCode::BAD_REQUEST | StatusCode::FORBIDDEN) {
        message
            .and_then(classify_message)
            .unwrap_or(if status == StatusCode::FORBIDDEN {
                PixivErrorClass::HiddenOrNotFound
            } else {
                PixivErrorClass::TemporaryPixivError
            })
    } else {
        PixivErrorClass::TemporaryPixivError
    };

    let mut error = PixivError::new(class, Some(endpoint)).with_status(status);
    if let Some(message) = message {
        error = error.with_message(message);
    }
    if class == PixivErrorClass::RateLimited {
        error.retry_after = retry_after.and_then(parse_retry_after);
    }
    error
}

pub(crate) fn classify_semantic_error(endpoint: PixivEndpoint, message: &str) -> PixivError {
    let class = classify_message(message).unwrap_or(PixivErrorClass::TemporaryPixivError);
    PixivError::new(class, Some(endpoint)).with_message(message)
}

fn classify_message(message: &str) -> Option<PixivErrorClass> {
    let normalized = message.to_ascii_lowercase();
    if normalized.contains("csrf") || normalized.contains("token") {
        Some(PixivErrorClass::CsrfFailed)
    } else if normalized.contains("r-18")
        || normalized.contains("r18")
        || normalized.contains("age restrict")
    {
        Some(PixivErrorClass::AgeRestrictedDisabled)
    } else if normalized.contains("login")
        || normalized.contains("log in")
        || normalized.contains("authentication")
        || normalized.contains("session")
    {
        Some(PixivErrorClass::CredentialInvalid)
    } else if normalized.contains("not found")
        || normalized.contains("deleted")
        || normalized.contains("hidden")
    {
        Some(PixivErrorClass::HiddenOrNotFound)
    } else if normalized.contains("rate limit") {
        Some(PixivErrorClass::RateLimited)
    } else {
        None
    }
}

fn parse_retry_after(value: &str) -> Option<Duration> {
    let value = value.trim();
    if let Some(seconds) = value.parse::<i64>().ok().filter(|seconds| *seconds >= 0) {
        return Some(Duration::seconds(seconds));
    }

    let retry_at = OffsetDateTime::parse(value, &Rfc2822).ok()?;
    Some((retry_at - OffsetDateTime::now_utc()).max(Duration::ZERO))
}

fn sanitize_message(message: &str) -> String {
    message
        .chars()
        .filter(|character| !character.is_control())
        .take(256)
        .collect::<String>()
        .trim()
        .to_owned()
}
