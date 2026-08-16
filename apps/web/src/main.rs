use anyhow::{Context, Result, bail};
use pixivarchive_application::{
    auth::{AuthConfig, AuthService},
    bookmarks::LiveBookmarkCommandPort,
    events::EventStream,
    following::{LiveArtistFollowCommandPort, LiveFollowingAvatarPort, LiveFollowingRefreshPort},
    installation::InstallationData,
    pixiv_accounts::{
        LivePixivAccountCommandPort, PixivAccountAdminError, PixivAccountCommandPort,
        PixivAccountContextFactory, PixivCookieCipher, PixivCookieKeyConfig,
        PixivCookieKeyringConfig,
    },
    rules::LiveRulePreviewPort,
    settings::{DeploymentCapabilities, SettingsService},
    system::SystemCapabilities,
};
use pixivarchive_db::Db;
use pixivarchive_media::MediaRoot;
use pixivarchive_pixiv::{PixivClientOptions, PixivRequestGate, PixivWebClient};
use pixivarchive_web::{
    app,
    cache::prepare as prepare_cache,
    state::{AppState, WebConfig},
};
use std::{net::SocketAddr, path::PathBuf, sync::Arc};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("pixivarchive=info,tower_http=info")),
        )
        .json()
        .init();

    let database_url = required_env("DATABASE_URL")?;
    let static_root = PathBuf::from(required_env("PIXIVARCHIVE_STATIC_ROOT")?);
    let bootstrap_media_root = required_absolute_path_env("PIXIVARCHIVE_MEDIA_ROOT")?;
    let bind = std::env::var("PIXIVARCHIVE_WEB_BIND")
        .unwrap_or_else(|_| "127.0.0.1:7088".to_owned())
        .parse::<SocketAddr>()
        .context("PIXIVARCHIVE_WEB_BIND must be a socket address")?;
    let capabilities = SystemCapabilities {
        webp_derivatives: optional_bool_env("PIXIVARCHIVE_WEBP_AVAILABLE", true)?,
        avif_derivatives: optional_bool_env("PIXIVARCHIVE_AVIF_AVAILABLE", false)?,
        reflink: optional_bool_env("PIXIVARCHIVE_REFLINK_AVAILABLE", false)?,
    };
    let pixiv_user_agent = std::env::var("PIXIVARCHIVE_PIXIV_USER_AGENT")
        .unwrap_or_else(|_| default_pixiv_user_agent());
    let db = Db::connect(&database_url).await?;
    let settings = SettingsService::with_capabilities(
        db.clone(),
        DeploymentCapabilities {
            avif_derivatives: capabilities.avif_derivatives,
        },
    )
    .effective()
    .await?;
    let media_root = MediaRoot::new(settings.storage.active_media_root(bootstrap_media_root));
    verify_readable_media_root(&media_root).await?;
    let installation = InstallationData::new(media_root.path());
    let cache_root = MediaRoot::new(installation.cache_root());
    let pixiv_cookie_key = installation
        .load_pixiv_cookie_key()
        .context("PixivArchive installation data is not prepared")?;
    prepare_cache(&cache_root)
        .await
        .with_context(|| format!("failed to prepare cache at {}", cache_root.path().display()))?;
    let processing = settings.processing.unwrap_or_default();
    let pixiv_options = PixivClientOptions {
        use_system_proxy: optional_bool_env("PIXIVARCHIVE_PIXIV_USE_SYSTEM_PROXY", true)?,
        metadata_request_gate: Some(PixivRequestGate::new(
            usize::from(processing.pixiv_request_concurrency.get()),
            Some((
                u32::from(processing.pixiv_request_rate.requests.get()),
                std::time::Duration::from_secs(u64::from(
                    processing.pixiv_request_rate.per_seconds,
                )),
            )),
        )?),
        ..PixivClientOptions::default()
    };
    let auth = AuthService::new(db.clone(), AuthConfig::new(8)?);
    let events = EventStream::new(db.clone(), database_url);
    let pixiv_gateway = PixivWebClient::new(pixiv_options)?;
    let pixiv_cookie_cipher = PixivCookieCipher::new(PixivCookieKeyringConfig::new(
        PixivCookieKeyConfig::new(pixiv_cookie_key.key_id(), pixiv_cookie_key.key()),
    ))?;
    let pixiv_account_commands = Arc::new(LivePixivAccountCommandPort::new(
        db.clone(),
        pixiv_gateway.clone(),
        pixiv_cookie_cipher.clone(),
        pixiv_user_agent,
    ));
    let startup_account_validation = Arc::clone(&pixiv_account_commands);
    let pixiv_context_factory = PixivAccountContextFactory::new(db.clone(), pixiv_cookie_cipher);
    let following_refresh = Arc::new(LiveFollowingRefreshPort::new(
        db.clone(),
        pixiv_gateway.clone(),
        pixiv_context_factory.clone(),
    ));
    let following_avatars = Arc::new(LiveFollowingAvatarPort::new(
        db.clone(),
        pixiv_gateway.clone(),
        pixiv_context_factory.clone(),
    ));
    let artist_follow_commands = Arc::new(LiveArtistFollowCommandPort::new(
        db.clone(),
        pixiv_gateway.clone(),
        pixiv_context_factory.clone(),
    ));
    let rule_preview = Arc::new(LiveRulePreviewPort::new(
        db.clone(),
        Arc::new(pixiv_gateway.clone()),
        pixiv_context_factory.clone(),
    ));
    let bookmark_commands = Arc::new(LiveBookmarkCommandPort::new(
        db.clone(),
        pixiv_gateway,
        pixiv_context_factory,
    ));
    let state = AppState::new(
        db,
        auth,
        events,
        WebConfig {
            static_root,
            media_root,
            cache_root,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            git_commit: option_env!("PIXIVARCHIVE_GIT_COMMIT").map(str::to_owned),
            capabilities,
        },
    )
    .with_pixiv_account_commands(pixiv_account_commands)
    .with_following_refresh(following_refresh)
    .with_following_avatars(following_avatars)
    .with_artist_follow_commands(artist_follow_commands)
    .with_rule_preview(rule_preview)
    .with_bookmark_commands(bookmark_commands);
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind Web listener at {bind}"))?;
    tracing::info!(%bind, "PixivArchive Web is listening");
    tokio::spawn(async move {
        match startup_account_validation.validate(None).await {
            Ok(account) => tracing::info!(
                account_id = %account.id,
                pixiv_user_id = account.pixiv_user_id,
                state = account.state.as_str(),
                "Pixiv account validation completed"
            ),
            Err(PixivAccountAdminError::NotConfigured) => {}
            Err(error) => {
                tracing::warn!(error = %error, "Pixiv account validation could not start");
            }
        }
    });
    axum::serve(
        listener,
        app(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("Web server stopped unexpectedly")
}

fn required_env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} is required"))
}

fn required_absolute_path_env(name: &str) -> Result<PathBuf> {
    let value = required_env(name)?;
    if !value.starts_with('/') {
        bail!("{name} must be an absolute path");
    }
    Ok(PathBuf::from(value))
}

async fn verify_readable_media_root(media_root: &MediaRoot) -> Result<()> {
    let root = media_root
        .resolve_directory_async(PathBuf::new())
        .await
        .with_context(|| {
            format!(
                "media root is unavailable at {}",
                media_root.path().display()
            )
        })?;
    let _entries = tokio::fs::read_dir(&root)
        .await
        .with_context(|| format!("media root is not readable at {}", root.display()))?;
    Ok(())
}

fn optional_bool_env(name: &str, default: bool) -> Result<bool> {
    let Ok(value) = std::env::var(name) else {
        return Ok(default);
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => bail!("{name} must be a boolean"),
    }
}

fn default_pixiv_user_agent() -> String {
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::verify_readable_media_root;
    use pixivarchive_media::MediaRoot;
    use uuid::Uuid;

    #[tokio::test]
    async fn web_media_root_requires_a_readable_directory() {
        let root = std::env::temp_dir().join(format!("pixivarchive-web-root-{}", Uuid::now_v7()));
        let file = root.with_extension("file");
        assert!(
            verify_readable_media_root(&MediaRoot::new(&root))
                .await
                .is_err()
        );

        tokio::fs::create_dir_all(&root).await.unwrap();
        assert!(
            verify_readable_media_root(&MediaRoot::new(&root))
                .await
                .is_ok()
        );
        tokio::fs::write(&file, b"not a directory").await.unwrap();
        assert!(
            verify_readable_media_root(&MediaRoot::new(&file))
                .await
                .is_err()
        );

        tokio::fs::remove_file(file).await.unwrap();
        tokio::fs::remove_dir(root).await.unwrap();
    }
}
