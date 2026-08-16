use super::{
    cookie_value, invalid_session_response, is_mutating, request_is_secure, response,
    service_unavailable_response,
};
use axum::{
    body::Body,
    http::{Request, Response, StatusCode},
};
use pixivarchive_application::auth::{AuthError, AuthService};
use pixivarchive_domain::auth::SessionContext;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower::{Layer, Service};

#[derive(Clone)]
pub struct CsrfLayer {
    auth: AuthService,
}

impl CsrfLayer {
    pub fn new(auth: AuthService) -> Self {
        Self { auth }
    }
}

impl<S> Layer<S> for CsrfLayer {
    type Service = CsrfMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CsrfMiddleware {
            inner,
            auth: self.auth.clone(),
        }
    }
}

#[derive(Clone)]
pub struct CsrfMiddleware<S> {
    inner: S,
    auth: AuthService,
}

impl<S> Service<Request<Body>> for CsrfMiddleware<S>
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
            if !is_mutating(request.method()) {
                return inner.call(request).await;
            }
            let secure = request_is_secure(request.headers());
            let context =
                if let Some(context) = request.extensions().get::<SessionContext>().cloned() {
                    context
                } else {
                    let Some(token) = cookie_value(request.headers(), "pa_session") else {
                        return Ok(invalid_session_response(secure));
                    };
                    let context = match auth.authenticate(&token).await {
                        Ok(context) => context,
                        Err(AuthError::InvalidSession) => {
                            return Ok(invalid_session_response(secure));
                        }
                        Err(_) => return Ok(service_unavailable_response()),
                    };
                    request.extensions_mut().insert(context.clone());
                    context
                };
            let Some(cookie_token) = cookie_value(request.headers(), "pa_csrf") else {
                return Ok(response(StatusCode::FORBIDDEN));
            };
            let Some(header_token) = request
                .headers()
                .get("X-CSRF-Token")
                .and_then(|value| value.to_str().ok())
            else {
                return Ok(response(StatusCode::FORBIDDEN));
            };
            match auth
                .verify_csrf(&context, &cookie_token, header_token)
                .await
            {
                Ok(()) => inner.call(request).await,
                Err(AuthError::Forbidden) => Ok(response(StatusCode::FORBIDDEN)),
                Err(_) => Ok(service_unavailable_response()),
            }
        })
    }
}
