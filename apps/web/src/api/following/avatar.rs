use crate::{
    api::{ApiError, ApiErrorBody, ApiPath},
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Request, State},
    http::{StatusCode, header},
    response::Response,
};
use pixivarchive_application::following::{FollowingAuthorView, FollowingAvatarError};
use pixivarchive_media::{MediaPathError, MediaRoot};
use pixivarchive_pixiv::{PixivErrorClass, is_official_pixiv_asset_url};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::io::AsyncWriteExt;
use tower::ServiceExt;
use tower_http::services::ServeFile;

#[utoipa::path(
    get,
    path = "/api/following/authors/{pixiv_artist_id}/avatar",
    params(("pixiv_artist_id" = i64, Path)),
    responses(
        (status = 200, description = "Cached Pixiv author avatar"),
        (status = 404, body = ApiErrorBody),
        (status = 429, body = ApiErrorBody),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "Following"
)]
pub(crate) async fn author_avatar(
    State(state): State<AppState>,
    ApiPath(pixiv_artist_id): ApiPath<i64>,
    request: Request,
) -> Result<Response, ApiError> {
    let author = state.following.author(pixiv_artist_id).await?;
    let source_url = author
        .avatar_url
        .ok_or_else(|| ApiError::not_found("Author avatar was not found"))?;
    let avatars = Arc::clone(&state.following_avatars);
    let cache_path = resolve_cached_pixiv_avatar(
        &state.config.cache_root,
        &format!("{pixiv_artist_id}-"),
        &source_url,
        move |source| async move { avatars.fetch(source).await.map_err(ApiError::from) },
    )
    .await?;
    serve_cached_avatar(request, cache_path).await
}

pub(crate) async fn resolve_cached_pixiv_avatar<F, Fut>(
    cache_root: &MediaRoot,
    cache_prefix: &str,
    source_url: &str,
    fetch: F,
) -> Result<PathBuf, ApiError>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<pixivarchive_pixiv::PixivMediaResponse, ApiError>>,
{
    let source = url::Url::parse(source_url)
        .map_err(|_| ApiError::not_found("Pixiv avatar was not found"))?;
    if !is_official_pixiv_asset_url(&source) {
        return Err(ApiError::not_found("Pixiv avatar was not found"));
    }
    let relative_path = cached_avatar_relative_path(cache_prefix, &source);
    let cache_path = match cache_root
        .resolve_optional_file_async(relative_path.clone())
        .await
        .map_err(|_| ApiError::service_unavailable())?
    {
        Some(path) => path,
        None => cache_avatar(fetch(source.to_string()).await?, cache_root, &relative_path).await?,
    };
    log_avatar_cleanup(
        remove_stale_avatar_cache(cache_root, cache_prefix, Some(&cache_path)).await,
    );
    Ok(cache_path)
}

pub(crate) async fn serve_cached_avatar(
    request: Request,
    cache_path: PathBuf,
) -> Result<Response, ApiError> {
    let mut response = ServeFile::new(cache_path)
        .oneshot(request)
        .await
        .unwrap_or_else(|error| match error {});
    if response.status() != StatusCode::OK && response.status() != StatusCode::NOT_MODIFIED {
        return Err(ApiError::service_unavailable());
    }
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, max-age=86400"),
    );
    Ok(response.map(Body::new))
}

fn avatar_cache_relative_path(pixiv_artist_id: i64, source: &url::Url) -> PathBuf {
    cached_avatar_relative_path(&format!("{pixiv_artist_id}-"), source)
}

fn cached_avatar_relative_path(cache_prefix: &str, source: &url::Url) -> PathBuf {
    PathBuf::from("avatars").join(format!(
        "{cache_prefix}{}.{}",
        source_identity_digest(source.as_str()),
        source_extension(source)
    ))
}

fn source_identity_digest(source: &str) -> String {
    encode_hex(&Sha256::digest(source.as_bytes()))
}

fn source_extension(source: &url::Url) -> &'static str {
    let Some(extension) = source
        .path_segments()
        .and_then(Iterator::last)
        .and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension))
    else {
        return "jpg";
    };
    match extension.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => "jpg",
        "png" => "png",
        "webp" => "webp",
        "gif" => "gif",
        _ => "jpg",
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

pub(super) async fn remove_author_avatar_cache(
    cache_root: &MediaRoot,
    pixiv_artist_id: i64,
) -> std::io::Result<()> {
    remove_stale_author_avatar_cache(cache_root, pixiv_artist_id, None).await
}

async fn remove_stale_author_avatar_cache(
    cache_root: &MediaRoot,
    pixiv_artist_id: i64,
    keep_path: Option<&Path>,
) -> std::io::Result<()> {
    remove_stale_avatar_cache(cache_root, &format!("{pixiv_artist_id}-"), keep_path).await
}

async fn remove_stale_avatar_cache(
    cache_root: &MediaRoot,
    prefix: &str,
    keep_path: Option<&Path>,
) -> std::io::Result<()> {
    let Some(mut entries) = read_avatar_cache_directory(cache_root).await? else {
        return Ok(());
    };
    let keep_name = keep_path.and_then(Path::file_name);
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        if keep_name == Some(file_name.as_os_str()) {
            continue;
        }
        if file_name.to_string_lossy().starts_with(prefix) {
            tokio::fs::remove_file(entry.path()).await?;
        }
    }
    Ok(())
}

pub(super) async fn remove_unreferenced_avatar_cache(
    cache_root: &MediaRoot,
    authors: &[FollowingAuthorView],
) -> std::io::Result<()> {
    let active = active_avatar_cache_names(cache_root, authors);
    let Some(mut entries) = read_avatar_cache_directory(cache_root).await? else {
        return Ok(());
    };
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        if avatar_cache_author_id(&file_name).is_some() && !active.contains(&file_name) {
            tokio::fs::remove_file(entry.path()).await?;
        }
    }
    Ok(())
}

async fn read_avatar_cache_directory(
    cache_root: &MediaRoot,
) -> std::io::Result<Option<tokio::fs::ReadDir>> {
    let directory = match cache_root.resolve_directory_async("avatars").await {
        Ok(directory) => directory,
        Err(MediaPathError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(error) => return Err(std::io::Error::other(error)),
    };
    match tokio::fs::read_dir(directory).await {
        Ok(entries) => Ok(Some(entries)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn active_avatar_cache_names(
    _cache_root: &MediaRoot,
    authors: &[FollowingAuthorView],
) -> HashSet<OsString> {
    authors
        .iter()
        .filter_map(|author| {
            let source = author.avatar_url.as_ref()?;
            let source = url::Url::parse(source).ok()?;
            if !is_official_pixiv_asset_url(&source) {
                return None;
            }
            avatar_cache_relative_path(author.pixiv_artist_id, &source)
                .file_name()
                .map(OsStr::to_owned)
        })
        .collect()
}

fn avatar_cache_author_id(file_name: &OsStr) -> Option<i64> {
    let file_name = file_name.to_str()?;
    let (artist_id, _) = file_name.split_once('-')?;
    artist_id.parse().ok().filter(|value| *value > 0)
}

pub(super) fn log_avatar_cleanup(result: std::io::Result<()>) {
    if let Err(error) = result {
        tracing::warn!(%error, "failed to clean Pixiv avatar cache");
    }
}

async fn cache_avatar(
    mut response: pixivarchive_pixiv::PixivMediaResponse,
    cache_root: &MediaRoot,
    relative_path: &Path,
) -> Result<PathBuf, ApiError> {
    const MAX_AVATAR_BYTES: u64 = 8 * 1024 * 1024;
    if !response
        .content_type
        .as_deref()
        .is_some_and(|value| value.starts_with("image/"))
    {
        return Err(ApiError::service_unavailable());
    }
    if response
        .content_length
        .is_some_and(|size| size > MAX_AVATAR_BYTES)
    {
        return Err(ApiError::service_unavailable());
    }
    let cache_path = cache_root
        .prepare_file_async(relative_path.to_path_buf())
        .await
        .map_err(|_| ApiError::service_unavailable())?;
    let directory = cache_path
        .parent()
        .ok_or_else(ApiError::service_unavailable)?;
    let temporary = directory.join(format!(".{}.tmp", uuid::Uuid::now_v7()));
    let mut output = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .await
        .map_err(|_| ApiError::service_unavailable())?;
    let written = async {
        use futures_util::StreamExt as _;

        let mut written = 0_u64;
        while let Some(chunk) = response.body.next().await {
            let chunk = chunk.map_err(|_| ApiError::service_unavailable())?;
            written = written
                .checked_add(chunk.len() as u64)
                .ok_or_else(ApiError::service_unavailable)?;
            if written > MAX_AVATAR_BYTES {
                return Err(ApiError::service_unavailable());
            }
            output
                .write_all(&chunk)
                .await
                .map_err(|_| ApiError::service_unavailable())?;
        }
        output
            .flush()
            .await
            .map_err(|_| ApiError::service_unavailable())?;
        if written == 0 {
            return Err(ApiError::service_unavailable());
        }
        Ok(())
    }
    .await;
    if let Err(error) = written {
        drop(output);
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error);
    }
    drop(output);
    if let Err(error) = tokio::fs::rename(&temporary, cache_path).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        if cache_root
            .resolve_optional_file_async(relative_path.to_path_buf())
            .await
            .map_err(|_| ApiError::service_unavailable())?
            .is_none()
        {
            tracing::warn!(%error, "failed to publish cached Pixiv avatar");
            return Err(ApiError::service_unavailable());
        }
    }
    cache_root
        .resolve_file_async(relative_path.to_path_buf())
        .await
        .map_err(|_| ApiError::service_unavailable())
}

impl From<FollowingAvatarError> for ApiError {
    fn from(error: FollowingAvatarError) -> Self {
        match error {
            FollowingAvatarError::NotConfigured => {
                Self::not_found("Pixiv account is not configured")
            }
            FollowingAvatarError::Pixiv(error) if error.class() == PixivErrorClass::RateLimited => {
                Self::new(
                    StatusCode::TOO_MANY_REQUESTS,
                    "rate_limited",
                    "Pixiv request rate is limited",
                )
            }
            FollowingAvatarError::Unavailable
            | FollowingAvatarError::Storage(_)
            | FollowingAvatarError::Context(_)
            | FollowingAvatarError::Pixiv(_) => Self::service_unavailable(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::remove_author_avatar_cache;
    use pixivarchive_media::MediaRoot;

    #[tokio::test]
    async fn avatar_cleanup_reports_an_invalid_cache_directory() {
        let cache_root = std::env::temp_dir().join(format!(
            "pixivarchive-avatar-cleanup-{}",
            uuid::Uuid::now_v7()
        ));
        let cache_root_boundary = MediaRoot::new(&cache_root);
        let cache_directory = cache_root_boundary.path().join("avatars");
        tokio::fs::create_dir_all(cache_directory.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&cache_directory, b"not a directory")
            .await
            .unwrap();

        let result = remove_author_avatar_cache(&cache_root_boundary, 70001).await;

        assert!(result.is_err());
        tokio::fs::remove_dir_all(cache_root).await.unwrap();
    }
}
