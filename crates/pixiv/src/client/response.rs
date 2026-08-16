use super::PixivRequestContext;
use crate::{
    error::{PixivError, PixivErrorClass, classify_http_status},
    web::{PIXIV_REFERER, PixivEndpoint},
};
use futures_util::StreamExt;
use reqwest::{
    Response,
    header::{
        ACCEPT, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HeaderMap, HeaderValue, InvalidHeaderValue,
        REFERER, RETRY_AFTER, USER_AGENT,
    },
};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;

pub(super) fn json_headers(
    context: &PixivRequestContext,
    endpoint: PixivEndpoint,
) -> Result<HeaderMap, PixivError> {
    let mut headers = common_headers(context, endpoint)?;
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(REFERER, HeaderValue::from_static(PIXIV_REFERER));
    Ok(headers)
}

pub(super) fn html_headers(
    context: &PixivRequestContext,
    endpoint: PixivEndpoint,
) -> Result<HeaderMap, PixivError> {
    let mut headers = common_headers(context, endpoint)?;
    headers.insert(ACCEPT, HeaderValue::from_static("text/html"));
    headers.insert(REFERER, HeaderValue::from_static(PIXIV_REFERER));
    Ok(headers)
}

fn common_headers(
    context: &PixivRequestContext,
    endpoint: PixivEndpoint,
) -> Result<HeaderMap, PixivError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        COOKIE,
        context
            .cookie_header_value()
            .map_err(|_| PixivError::new(PixivErrorClass::CredentialInvalid, Some(endpoint)))?,
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(context.user_agent())
            .map_err(|_| PixivError::invalid_json(endpoint))?,
    );
    Ok(headers)
}

pub(super) fn csrf_header_value(
    token: &SecretString,
    endpoint: PixivEndpoint,
) -> Result<HeaderValue, PixivError> {
    sensitive_header_value(token.expose_secret())
        .map_err(|_| PixivError::new(PixivErrorClass::CsrfFailed, Some(endpoint)))
}

fn sensitive_header_value(value: &str) -> Result<HeaderValue, InvalidHeaderValue> {
    let mut header = HeaderValue::from_str(value)?;
    header.set_sensitive(true);
    Ok(header)
}

pub(super) fn redact_write_error(
    context: &PixivRequestContext,
    token: &SecretString,
    error: PixivError,
) -> PixivError {
    context
        .redact_error(error)
        .redact_secret(token.expose_secret())
}

pub(super) async fn read_json_response(
    response: Response,
    endpoint: PixivEndpoint,
    limit: usize,
) -> Result<Value, PixivError> {
    let status = response.status();
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    if !status.is_success() {
        let bytes = read_limited(response, endpoint, limit).await?;
        let message = response_message(&bytes);
        return Err(classify_http_status(
            endpoint,
            status,
            message.as_deref(),
            retry_after.as_deref(),
        ));
    }

    if content_type
        .as_deref()
        .is_some_and(|value| !value.to_ascii_lowercase().starts_with("application/json"))
    {
        return Err(PixivError::invalid_json(endpoint).with_message(&format!(
            "unexpected response content type {}",
            content_type.as_deref().unwrap_or("unknown")
        )));
    }
    let bytes = read_limited(response, endpoint, limit).await?;
    serde_json::from_slice(&bytes).map_err(|error| {
        PixivError::invalid_json(endpoint).with_message(&format!(
            "response JSON could not be decoded at line {} column {}",
            error.line(),
            error.column()
        ))
    })
}

pub(super) async fn read_limited(
    response: Response,
    endpoint: PixivEndpoint,
    limit: usize,
) -> Result<Vec<u8>, PixivError> {
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > limit)
    {
        return Err(PixivError::response_too_large(endpoint));
    }

    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| PixivError::network(endpoint))?;
        let new_length = bytes
            .len()
            .checked_add(chunk.len())
            .ok_or_else(|| PixivError::response_too_large(endpoint))?;
        if new_length > limit {
            return Err(PixivError::response_too_large(endpoint));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn response_message(bytes: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
}

pub(super) fn extract_csrf_token(html: &str) -> Option<SecretString> {
    for (marker, terminator) in [
        ("\"token\":\"", "\""),
        ("token\\\":\\\"", "\\\""),
        ("\"postKey\":\"", "\""),
    ] {
        if let Some(start) = html.find(marker) {
            let value = &html[start + marker.len()..];
            if let Some(end) = value.find(terminator) {
                let token = &value[..end];
                if !token.is_empty()
                    && token.chars().all(|character| {
                        character.is_ascii_alphanumeric() || "_-".contains(character)
                    })
                {
                    return Some(SecretString::from(token));
                }
            }
        }
    }
    None
}

pub(super) fn validate_context(
    context: &PixivRequestContext,
    endpoint: PixivEndpoint,
) -> Result<(), PixivError> {
    require_positive_id(context.user_id(), endpoint)?;
    if context.cookie_is_empty() {
        return Err(PixivError::credential_invalid(endpoint, "Cookie is empty"));
    }
    if context.user_agent().trim().is_empty() {
        return Err(PixivError::invalid_json(endpoint));
    }
    Ok(())
}

pub(super) fn require_positive_id(value: i64, endpoint: PixivEndpoint) -> Result<(), PixivError> {
    if value > 0 {
        Ok(())
    } else {
        Err(PixivError::hidden_or_invalid(
            endpoint,
            "identifier must be positive",
        ))
    }
}

pub(super) fn temporary_cache_error() -> PixivError {
    PixivError::new(
        PixivErrorClass::TemporaryPixivError,
        Some(PixivEndpoint::Csrf),
    )
}
