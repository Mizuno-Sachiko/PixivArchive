use crate::state::AppState;
use axum::{Router, middleware, response::Response};
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};

pub fn routes(static_root: PathBuf) -> Router<AppState> {
    let immutable = Router::new()
        .nest_service(
            "/_app/immutable",
            ServeDir::new(static_root.join("_app/immutable")),
        )
        .layer(middleware::map_response(immutable_cache));
    let files = ServeDir::new(&static_root).fallback(ServeFile::new(static_root.join("200.html")));

    Router::new().merge(immutable).fallback_service(files)
}

async fn immutable_cache(mut response: Response) -> Response {
    if response.status().is_success() {
        response.headers_mut().insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }
    response
}
