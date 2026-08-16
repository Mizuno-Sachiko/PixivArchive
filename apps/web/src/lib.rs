pub mod api;
pub mod cache;
pub mod middleware;
pub mod openapi;
pub mod state;
pub mod static_files;

use axum::{Router, extract::ConnectInfo};
use state::AppState;
use std::net::SocketAddr;

pub fn app(state: AppState) -> Router {
    let static_root = state.config.static_root.clone();
    Router::new()
        .nest("/api", api::routes(&state))
        .merge(api::system::health_routes())
        .merge(static_files::routes(static_root))
        .with_state(state)
}

pub fn source_bucket(connect_info: &ConnectInfo<SocketAddr>) -> String {
    connect_info.0.ip().to_string()
}
