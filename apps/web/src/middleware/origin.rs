use super::{is_mutating, response};
use axum::{
    body::Body,
    http::{Request, Response, StatusCode, header},
};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower::{Layer, Service};
use url::Url;

#[derive(Clone, Copy, Default)]
pub struct OriginLayer;

impl OriginLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for OriginLayer {
    type Service = OriginService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        OriginService { inner }
    }
}

#[derive(Clone)]
pub struct OriginService<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for OriginService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        Box::pin(async move {
            if is_mutating(request.method()) && !origin_matches(&request) {
                return Ok(response(StatusCode::FORBIDDEN));
            }
            inner.call(request).await
        })
    }
}

fn origin_matches(request: &Request<Body>) -> bool {
    let Some(authority) = request_authority(request) else {
        return false;
    };
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let Ok(origin) = origin.to_str() else {
            return false;
        };
        if origin == "null" {
            return false;
        }
        return Url::parse(origin).is_ok_and(|origin| {
            is_origin_header(&origin) && matches_authority(&origin, authority)
        });
    }
    let Some(referer) = request.headers().get(header::REFERER) else {
        return false;
    };
    let Ok(referer) = referer.to_str() else {
        return false;
    };
    Url::parse(referer).is_ok_and(|referer| matches_authority(&referer, authority))
}

fn request_authority(request: &Request<Body>) -> Option<&str> {
    request
        .headers()
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .or_else(|| {
            request
                .uri()
                .authority()
                .map(|authority| authority.as_str())
        })
}

fn matches_authority(actual: &Url, authority: &str) -> bool {
    if !matches!(actual.scheme(), "http" | "https") {
        return false;
    }
    let Ok(expected) = Url::parse(&format!("{}://{authority}", actual.scheme())) else {
        return false;
    };
    actual.host_str() == expected.host_str()
        && actual.port_or_known_default() == expected.port_or_known_default()
        && actual.username().is_empty()
        && actual.password().is_none()
        && expected.username().is_empty()
        && expected.password().is_none()
}

fn is_origin_header(actual: &Url) -> bool {
    actual.path() == "/" && actual.query().is_none() && actual.fragment().is_none()
}
