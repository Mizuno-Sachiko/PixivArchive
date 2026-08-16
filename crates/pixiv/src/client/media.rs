use super::response::{require_positive_id, validate_context};
use super::{PixivRequestContext, PixivWebClient};
use crate::{
    error::{PixivError, PixivErrorClass, classify_http_status},
    limit::PixivRequestPermit,
    web::{PIXIV_REFERER, PixivEndpoint, artwork_referer},
};
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderValue, REFERER, RETRY_AFTER, USER_AGENT};
use std::{fmt, net::IpAddr, pin::Pin};
use url::Url;

pub type PixivMediaStream = Pin<Box<dyn Stream<Item = Result<Bytes, PixivError>> + Send + 'static>>;

pub const OFFICIAL_PIXIV_ASSET_HOSTS: [&str; 2] = ["i.pximg.net", "s.pximg.net"];

pub fn is_official_pixiv_asset_url(url: &Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url
            .host_str()
            .is_some_and(|host| OFFICIAL_PIXIV_ASSET_HOSTS.contains(&host))
}

pub struct PixivMediaResponse {
    pub content_length: Option<u64>,
    pub content_type: Option<String>,
    pub body: PixivMediaStream,
}

impl fmt::Debug for PixivMediaResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PixivMediaResponse")
            .field("content_length", &self.content_length)
            .field("content_type", &self.content_type)
            .finish_non_exhaustive()
    }
}

#[async_trait]
pub trait PixivMediaGateway: Send + Sync {
    async fn media(
        &self,
        context: &PixivRequestContext,
        work_id: i64,
        media_url: Url,
    ) -> Result<PixivMediaResponse, PixivError>;
}

#[async_trait]
pub trait PixivAssetGateway: Send + Sync {
    async fn asset(
        &self,
        context: &PixivRequestContext,
        asset_url: String,
    ) -> Result<PixivMediaResponse, PixivError>;
}

impl PixivWebClient {
    async fn media_request_permit(&self) -> Option<PixivRequestPermit> {
        match &self.options.media_request_gate {
            Some(gate) => Some(gate.enter().await),
            None => None,
        }
    }

    async fn request_media(
        &self,
        context: &PixivRequestContext,
        work_id: i64,
        media_url: Url,
    ) -> Result<PixivMediaResponse, PixivError> {
        validate_context(context, PixivEndpoint::Media)?;
        require_positive_id(work_id, PixivEndpoint::Media)?;
        let referer = HeaderValue::from_str(artwork_referer(work_id)?.as_str())
            .map_err(|_| PixivError::invalid_json(PixivEndpoint::Media))?;
        self.request_asset_with_referer(context, media_url, referer)
            .await
    }

    async fn request_asset(
        &self,
        context: &PixivRequestContext,
        asset_url: Url,
    ) -> Result<PixivMediaResponse, PixivError> {
        validate_context(context, PixivEndpoint::Media)?;
        self.request_asset_with_referer(context, asset_url, HeaderValue::from_static(PIXIV_REFERER))
            .await
    }

    async fn request_asset_with_referer(
        &self,
        context: &PixivRequestContext,
        asset_url: Url,
        referer: HeaderValue,
    ) -> Result<PixivMediaResponse, PixivError> {
        let host = asset_url.host_str().ok_or_else(|| {
            PixivError::new(
                PixivErrorClass::RefererForbidden,
                Some(PixivEndpoint::Media),
            )
        })?;
        if !media_scheme_allowed(&asset_url, host)
            || asset_url.username() != ""
            || asset_url.password().is_some()
            || !self.options.allowed_media_hosts.contains(host)
        {
            return Err(PixivError::new(
                PixivErrorClass::RefererForbidden,
                Some(PixivEndpoint::Media),
            ));
        }

        let user_agent = HeaderValue::from_str(context.user_agent())
            .map_err(|_| PixivError::invalid_json(PixivEndpoint::Media))?;
        let request_permit = self.media_request_permit().await;
        let response = self
            .http
            .get(asset_url)
            .header(USER_AGENT, user_agent)
            .header(ACCEPT, HeaderValue::from_static("*/*"))
            .header(REFERER, referer)
            .send()
            .await
            .map_err(|_| PixivError::network(PixivEndpoint::Media))?;
        if response.status().is_success() {
            let content_length = response.content_length();
            let content_type = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let response_stream = Box::pin(response.bytes_stream());
            let body = futures_util::stream::unfold(
                (response_stream, request_permit),
                |(mut stream, permit)| async move {
                    stream.as_mut().next().await.map(|chunk| {
                        (
                            chunk.map_err(|_| PixivError::network(PixivEndpoint::Media)),
                            (stream, permit),
                        )
                    })
                },
            );
            return Ok(PixivMediaResponse {
                content_length,
                content_type,
                body: Box::pin(body),
            });
        }
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok());
        Err(classify_http_status(
            PixivEndpoint::Media,
            response.status(),
            None,
            retry_after,
        ))
    }
}

#[async_trait]
impl PixivMediaGateway for PixivWebClient {
    async fn media(
        &self,
        context: &PixivRequestContext,
        work_id: i64,
        media_url: Url,
    ) -> Result<PixivMediaResponse, PixivError> {
        self.request_media(context, work_id, media_url).await
    }
}

#[async_trait]
impl PixivAssetGateway for PixivWebClient {
    async fn asset(
        &self,
        context: &PixivRequestContext,
        asset_url: String,
    ) -> Result<PixivMediaResponse, PixivError> {
        let asset_url = Url::parse(&asset_url).map_err(|_| {
            PixivError::new(
                PixivErrorClass::RefererForbidden,
                Some(PixivEndpoint::Media),
            )
        })?;
        self.request_asset(context, asset_url).await
    }
}

fn media_scheme_allowed(url: &Url, host: &str) -> bool {
    url.scheme() == "https"
        || (url.scheme() == "http"
            && (host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<IpAddr>()
                    .is_ok_and(|address| address.is_loopback())))
}
