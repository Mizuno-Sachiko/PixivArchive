use crate::error::{PixivError, PixivErrorClass};
use reqwest::{
    Client,
    header::{ACCEPT_ENCODING, HeaderMap, HeaderValue},
    redirect::Policy,
};
use secrecy::SecretString;
use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex},
};

mod context;
mod media;
mod metadata;
mod response;

pub use context::{
    AdapterResponse, PixivClientOptions, PixivGateway, PixivRequestContext, ResponseProvenance,
};
pub use media::{
    OFFICIAL_PIXIV_ASSET_HOSTS, PixivAssetGateway, PixivMediaGateway, PixivMediaResponse,
    PixivMediaStream, is_official_pixiv_asset_url,
};

#[derive(Clone)]
pub struct PixivWebClient {
    pub(super) http: Client,
    pub(super) options: PixivClientOptions,
    pub(super) csrf_tokens: Arc<Mutex<HashMap<i64, Arc<SecretString>>>>,
}

impl fmt::Debug for PixivWebClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PixivWebClient")
            .field("web_base_url", &self.options.web_base_url)
            .field("allowed_media_hosts", &self.options.allowed_media_hosts)
            .field(
                "metadata_response_limit",
                &self.options.metadata_response_limit,
            )
            .field("csrf_response_limit", &self.options.csrf_response_limit)
            .finish_non_exhaustive()
    }
}

impl PixivWebClient {
    pub fn new(options: PixivClientOptions) -> Result<Self, PixivError> {
        if options.metadata_response_limit == 0 || options.csrf_response_limit == 0 {
            return Err(PixivError::new(PixivErrorClass::ResponseTooLarge, None));
        }
        let mut default_headers = HeaderMap::new();
        default_headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
        let mut builder = Client::builder()
            .redirect(Policy::none())
            .timeout(options.request_timeout)
            .default_headers(default_headers)
            .no_gzip()
            .no_brotli()
            .no_zstd()
            .no_deflate();
        if !options.use_system_proxy {
            builder = builder.no_proxy();
        }
        let http = builder
            .build()
            .map_err(|_| PixivError::new(PixivErrorClass::Network, None))?;
        Ok(Self {
            http,
            options,
            csrf_tokens: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}
