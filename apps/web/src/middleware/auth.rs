use super::{
    cookie_value, invalid_session_response, request_is_secure, response,
    service_unavailable_response,
};
use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
};
use pixivarchive_application::auth::{AuthError, AuthService};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower::{Layer, Service};

#[derive(Clone)]
pub struct AuthLayer {
    auth: AuthService,
}

impl AuthLayer {
    pub fn new(auth: AuthService) -> Self {
        Self { auth }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthMiddleware {
            inner,
            auth: self.auth.clone(),
        }
    }
}

#[derive(Clone)]
pub struct AuthMiddleware<S> {
    inner: S,
    auth: AuthService,
}

impl<S> Service<Request<Body>> for AuthMiddleware<S>
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

    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        let auth = self.auth.clone();
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let secure = request_is_secure(request.headers());
            let Some(token) = cookie_value(request.headers(), "pa_session") else {
                return Ok(response(StatusCode::UNAUTHORIZED));
            };
            match auth.authenticate(&token).await {
                Ok(context) => {
                    request.extensions_mut().insert(context);
                    inner.call(request).await
                }
                Err(AuthError::InvalidSession) => Ok(invalid_session_response(secure)),
                Err(_) => Ok(service_unavailable_response()),
            }
        })
    }
}
