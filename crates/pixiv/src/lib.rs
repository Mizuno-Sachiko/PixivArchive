pub mod client;
mod dto;
pub mod error;
mod limit;
mod mapper;
#[cfg(test)]
mod mapper_tests;
mod web;

pub use client::{
    AdapterResponse, OFFICIAL_PIXIV_ASSET_HOSTS, PixivAssetGateway, PixivClientOptions,
    PixivGateway, PixivMediaGateway, PixivMediaResponse, PixivMediaStream, PixivRequestContext,
    PixivWebClient, ResponseProvenance, is_official_pixiv_asset_url,
};
pub use error::{PixivError, PixivErrorClass};
pub use limit::{PixivRequestGate, PixivRequestGateError, PixivRequestPermit};
pub use web::{ADAPTER_VERSION, PixivEndpoint};

pub const CRATE_NAME: &str = "pixivarchive-pixiv";
