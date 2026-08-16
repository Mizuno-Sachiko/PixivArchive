pub mod auth;
pub mod csrf;
pub mod origin;

use crate::api::ApiError;
use axum::{
    body::Body,
    http::{Response, StatusCode, header},
    response::IntoResponse,
};

fn cookie_value(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let mut matched = None;
    for cookie in headers.get_all(axum::http::header::COOKIE) {
        let cookie = cookie.to_str().ok()?;
        for part in cookie.split(';') {
            let (key, value) = part.trim().split_once('=')?;
            if key == name {
                if matched.is_some() {
                    return None;
                }
                matched = Some(value.to_owned());
            }
        }
    }
    matched
}

const SESSION_COOKIE: &str = "pa_session";
const CSRF_COOKIE: &str = "pa_csrf";

pub fn session_cookie(token: &str, secure: bool) -> String {
    cookie(SESSION_COOKIE, token, secure, true, false)
}

pub fn csrf_cookie(token: &str, secure: bool) -> String {
    cookie(CSRF_COOKIE, token, secure, false, false)
}

pub fn clear_session_cookie(secure: bool) -> String {
    cookie(SESSION_COOKIE, "", secure, true, true)
}

pub fn clear_csrf_cookie(secure: bool) -> String {
    cookie(CSRF_COOKIE, "", secure, false, true)
}

pub fn request_is_secure(headers: &axum::http::HeaderMap) -> bool {
    [header::ORIGIN, header::REFERER]
        .into_iter()
        .filter_map(|name| headers.get(name))
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| url::Url::parse(value).ok())
        .next()
        .is_some_and(|url| url.scheme() == "https")
}

fn cookie(name: &str, value: &str, secure: bool, http_only: bool, clear: bool) -> String {
    let mut attributes = String::new();
    if secure {
        attributes.push_str("; Secure");
    }
    if http_only {
        attributes.push_str("; HttpOnly");
    }
    attributes.push_str("; SameSite=Strict; Path=/");
    if clear {
        attributes.push_str("; Max-Age=0");
    }
    format!("{name}={value}{attributes}")
}

pub fn no_store(response: &mut Response<Body>) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
}

pub(super) fn response(status: StatusCode) -> Response<Body> {
    match status {
        StatusCode::UNAUTHORIZED => ApiError::authentication_required(),
        StatusCode::FORBIDDEN => ApiError::forbidden("The request is forbidden"),
        StatusCode::SERVICE_UNAVAILABLE => ApiError::service_unavailable(),
        _ => ApiError::new(status, "request_failed", "The request failed"),
    }
    .into_response()
}

pub(super) fn no_store_response(status: StatusCode) -> Response<Body> {
    let mut response = response(status);
    no_store(&mut response);
    response
}

pub(super) fn invalid_session_response(secure: bool) -> Response<Body> {
    let mut response = no_store_response(StatusCode::UNAUTHORIZED);
    response.headers_mut().append(
        header::SET_COOKIE,
        clear_session_cookie(secure).parse().unwrap(),
    );
    response.headers_mut().append(
        header::SET_COOKIE,
        clear_csrf_cookie(secure).parse().unwrap(),
    );
    response
}

pub(super) fn service_unavailable_response() -> Response<Body> {
    no_store_response(StatusCode::SERVICE_UNAVAILABLE)
}

fn is_mutating(method: &axum::http::Method) -> bool {
    matches!(
        *method,
        axum::http::Method::POST
            | axum::http::Method::PUT
            | axum::http::Method::PATCH
            | axum::http::Method::DELETE
    )
}
