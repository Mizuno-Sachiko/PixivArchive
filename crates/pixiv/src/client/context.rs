use crate::{
    error::PixivError,
    limit::PixivRequestGate,
    web::{ADAPTER_VERSION, PixivEndpoint},
};
use async_trait::async_trait;
use pixivarchive_domain::pixiv::{
    PixivAccountValidation, PixivArtistFollowRequest, PixivArtistFollowState,
    PixivArtistFollowWriteResult, PixivArtistWorkIds, PixivBookmarkAddRequest,
    PixivBookmarkWriteResult, PixivBookmarksCursor, PixivBookmarksRequest, PixivDiscoveryWork,
    PixivFollowLatestCursor, PixivFollowLatestRequest, PixivFollowedArtist, PixivFollowingCursor,
    PixivFollowingRequest, PixivPage, PixivRankingPage, PixivRankingRequest, PixivUgoiraMeta,
    PixivWorkDetail, PixivWorkPages,
};
use reqwest::header::{HeaderValue, InvalidHeaderValue};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use std::{collections::BTreeSet, fmt, time::Duration as StdDuration};
use url::Url;

use super::media::OFFICIAL_PIXIV_ASSET_HOSTS;

#[derive(Clone, Debug)]
pub struct PixivClientOptions {
    pub web_base_url: Url,
    pub allowed_media_hosts: BTreeSet<String>,
    pub metadata_response_limit: usize,
    pub csrf_response_limit: usize,
    pub request_timeout: StdDuration,
    pub use_system_proxy: bool,
    pub metadata_request_gate: Option<PixivRequestGate>,
    pub media_request_gate: Option<PixivRequestGate>,
}

impl Default for PixivClientOptions {
    fn default() -> Self {
        Self {
            web_base_url: Url::parse("https://www.pixiv.net/")
                .expect("the production Pixiv URL is constant"),
            allowed_media_hosts: OFFICIAL_PIXIV_ASSET_HOSTS
                .into_iter()
                .map(str::to_owned)
                .collect(),
            metadata_response_limit: 8 * 1024 * 1024,
            csrf_response_limit: 2 * 1024 * 1024,
            request_timeout: StdDuration::from_secs(30),
            use_system_proxy: false,
            metadata_request_gate: None,
            media_request_gate: None,
        }
    }
}

#[derive(Clone)]
pub struct PixivRequestContext {
    cookie: SecretString,
    user_id: i64,
    user_agent: String,
}

impl PixivRequestContext {
    pub fn new(cookie: SecretString, user_id: i64, user_agent: impl Into<String>) -> Self {
        Self {
            cookie,
            user_id,
            user_agent: user_agent.into(),
        }
    }

    pub fn user_id(&self) -> i64 {
        self.user_id
    }

    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    pub fn cookie_header_value(&self) -> Result<HeaderValue, InvalidHeaderValue> {
        sensitive_header_value(self.cookie.expose_secret())
    }

    pub(super) fn cookie_is_empty(&self) -> bool {
        self.cookie.expose_secret().trim().is_empty()
    }

    pub(super) fn redact_error(&self, mut error: PixivError) -> PixivError {
        let cookie = self.cookie.expose_secret();
        error = error.redact_secret(cookie);
        for field in cookie.split(';') {
            if let Some((_, value)) = field.split_once('=') {
                error = error.redact_secret(value.trim());
            }
        }
        error
    }
}

impl fmt::Debug for PixivRequestContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PixivRequestContext")
            .field("cookie", &"[REDACTED]")
            .field("user_id", &self.user_id)
            .field("user_agent", &self.user_agent)
            .finish()
    }
}

fn sensitive_header_value(value: &str) -> Result<HeaderValue, InvalidHeaderValue> {
    let mut header = HeaderValue::from_str(value)?;
    header.set_sensitive(true);
    Ok(header)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseProvenance {
    pub adapter_version: &'static str,
    pub endpoint: PixivEndpoint,
    pub raw: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterResponse<T> {
    pub value: T,
    pub provenance: Vec<ResponseProvenance>,
}

impl<T> AdapterResponse<T> {
    pub(super) fn single(value: T, endpoint: PixivEndpoint, raw: Value) -> Self {
        Self {
            value,
            provenance: vec![ResponseProvenance {
                adapter_version: ADAPTER_VERSION,
                endpoint,
                raw,
            }],
        }
    }
}

#[async_trait]
pub trait PixivGateway: Send + Sync {
    async fn validate_account(
        &self,
        context: &PixivRequestContext,
    ) -> Result<AdapterResponse<PixivAccountValidation>, PixivError>;

    async fn ranking_page(
        &self,
        context: &PixivRequestContext,
        request: PixivRankingRequest,
    ) -> Result<AdapterResponse<PixivRankingPage>, PixivError>;

    async fn work_detail(
        &self,
        context: &PixivRequestContext,
        work_id: i64,
    ) -> Result<AdapterResponse<PixivWorkDetail>, PixivError>;

    async fn work_pages(
        &self,
        context: &PixivRequestContext,
        work_id: i64,
    ) -> Result<AdapterResponse<PixivWorkPages>, PixivError>;

    async fn ugoira_meta(
        &self,
        context: &PixivRequestContext,
        work_id: i64,
    ) -> Result<AdapterResponse<PixivUgoiraMeta>, PixivError>;

    async fn follow_latest(
        &self,
        context: &PixivRequestContext,
        request: PixivFollowLatestRequest,
    ) -> Result<AdapterResponse<PixivPage<PixivDiscoveryWork, PixivFollowLatestCursor>>, PixivError>;

    async fn bookmarks(
        &self,
        context: &PixivRequestContext,
        request: PixivBookmarksRequest,
    ) -> Result<AdapterResponse<PixivPage<i64, PixivBookmarksCursor>>, PixivError>;

    async fn following_page(
        &self,
        context: &PixivRequestContext,
        request: PixivFollowingRequest,
    ) -> Result<AdapterResponse<PixivPage<PixivFollowedArtist, PixivFollowingCursor>>, PixivError>;

    async fn artist_work_ids(
        &self,
        context: &PixivRequestContext,
        artist_id: i64,
    ) -> Result<AdapterResponse<PixivArtistWorkIds>, PixivError>;

    async fn artist_follow_state(
        &self,
        context: &PixivRequestContext,
        artist_id: i64,
    ) -> Result<AdapterResponse<PixivArtistFollowState>, PixivError>;

    async fn add_artist_follow(
        &self,
        context: &PixivRequestContext,
        request: PixivArtistFollowRequest,
    ) -> Result<AdapterResponse<PixivArtistFollowWriteResult>, PixivError>;

    async fn remove_artist_follow(
        &self,
        context: &PixivRequestContext,
        artist_id: i64,
    ) -> Result<AdapterResponse<PixivArtistFollowWriteResult>, PixivError>;

    async fn add_bookmark(
        &self,
        context: &PixivRequestContext,
        request: PixivBookmarkAddRequest,
    ) -> Result<AdapterResponse<PixivBookmarkWriteResult>, PixivError>;

    async fn delete_bookmark(
        &self,
        context: &PixivRequestContext,
        bookmark_id: i64,
    ) -> Result<AdapterResponse<PixivBookmarkWriteResult>, PixivError>;
}
