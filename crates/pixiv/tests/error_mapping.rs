use pixivarchive_pixiv::{
    PixivEndpoint, PixivRequestContext,
    error::{PixivError, PixivErrorClass, classify_http_status},
};
use reqwest::StatusCode;
use secrecy::SecretString;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc2822};

#[test]
fn request_context_and_cookie_header_redact_the_cookie() {
    let context = PixivRequestContext::new(
        SecretString::from("PHPSESSID=cookie-canary"),
        10001,
        "Mozilla/5.0 fixture",
    );

    let debug = format!("{context:?}");
    assert!(!debug.contains("cookie-canary"));
    assert!(debug.contains("[REDACTED]"));

    let header = context.cookie_header_value().unwrap();
    assert!(header.is_sensitive());
    assert!(!format!("{header:?}").contains("cookie-canary"));
}

#[test]
fn stable_http_statuses_map_to_typed_error_classes() {
    assert_eq!(
        classify_http_status(
            PixivEndpoint::ProfileAll,
            StatusCode::UNAUTHORIZED,
            None,
            None,
        )
        .class(),
        PixivErrorClass::CredentialInvalid
    );
    assert_eq!(
        classify_http_status(
            PixivEndpoint::Ranking,
            StatusCode::FORBIDDEN,
            Some("R-18 viewing is disabled"),
            None,
        )
        .class(),
        PixivErrorClass::AgeRestrictedDisabled
    );
    assert_eq!(
        classify_http_status(PixivEndpoint::Media, StatusCode::FORBIDDEN, None, None,).class(),
        PixivErrorClass::RefererForbidden
    );
    assert_eq!(
        classify_http_status(PixivEndpoint::WorkDetail, StatusCode::NOT_FOUND, None, None,).class(),
        PixivErrorClass::HiddenOrNotFound
    );
    assert_eq!(
        classify_http_status(
            PixivEndpoint::AddBookmark,
            StatusCode::BAD_REQUEST,
            Some("Invalid CSRF token"),
            None,
        )
        .class(),
        PixivErrorClass::CsrfFailed
    );
    assert_eq!(
        classify_http_status(
            PixivEndpoint::WorkDetail,
            StatusCode::BAD_GATEWAY,
            None,
            None,
        )
        .class(),
        PixivErrorClass::TemporaryPixivError
    );
}

#[test]
fn stable_statuses_take_precedence_over_unstable_response_messages() {
    assert_eq!(
        classify_http_status(
            PixivEndpoint::Ranking,
            StatusCode::TOO_MANY_REQUESTS,
            Some("login required"),
            Some("42"),
        )
        .class(),
        PixivErrorClass::RateLimited
    );
    assert_eq!(
        classify_http_status(
            PixivEndpoint::WorkDetail,
            StatusCode::BAD_GATEWAY,
            Some("csrf token"),
            None,
        )
        .class(),
        PixivErrorClass::TemporaryPixivError
    );
    assert_eq!(
        classify_http_status(
            PixivEndpoint::WorkDetail,
            StatusCode::FOUND,
            Some("login required"),
            None,
        )
        .class(),
        PixivErrorClass::InvalidJsonOrInterstitial
    );
}

#[test]
fn rate_limit_keeps_retry_after_seconds() {
    let error = classify_http_status(
        PixivEndpoint::Ranking,
        StatusCode::TOO_MANY_REQUESTS,
        None,
        Some("42"),
    );

    assert_eq!(error.class(), PixivErrorClass::RateLimited);
    assert_eq!(error.retry_after(), Some(Duration::seconds(42)));
}

#[test]
fn rate_limit_accepts_retry_after_http_date() {
    let retry_at = OffsetDateTime::now_utc() + Duration::seconds(120);
    let retry_after = retry_at.format(&Rfc2822).unwrap();
    let error = classify_http_status(
        PixivEndpoint::Ranking,
        StatusCode::TOO_MANY_REQUESTS,
        None,
        Some(&retry_after),
    );

    let delay = error.retry_after().unwrap();
    assert!(delay >= Duration::seconds(118));
    assert!(delay <= Duration::seconds(120));
}

#[test]
fn media_integrity_class_is_available_to_the_media_pipeline() {
    let error = PixivError::media_integrity("ugoira manifest mismatch");

    assert_eq!(error.class(), PixivErrorClass::MediaIntegrityFailed);
    assert!(error.to_string().contains("media_integrity_failed"));
}
