use anyhow::{Context, Result, bail};
use pixivarchive_application::{
    installation::InstallationData,
    jobs::{JobService, QueueQuotaWeights},
    pixiv_accounts::{
        PixivAccountContextFactory, PixivCookieCipher, PixivCookieKeyConfig,
        PixivCookieKeyringConfig,
    },
    settings::{DeploymentCapabilities, SettingsService},
};
use pixivarchive_db::{Db, WorkerHeartbeatRepository, WorkerHeartbeatUpdate};
use pixivarchive_media::{MediaProbeLimits, MediaRoot, MediaStoreConfig, UgoiraLimits};
use pixivarchive_pixiv::{PixivClientOptions, PixivRequestGate, PixivWebClient};
use pixivarchive_worker::{
    executors::{ExecutionGate, ExecutorRegistry, download::MediaPipelineConfig},
    runtime::{
        MEDIA_DOWNLOAD_JOB_KINDS, MEDIA_PROCESSING_JOB_KINDS, PIXIV_COLLECTION_JOB_KINDS,
        WorkerRuntime, WorkerRuntimeConfig,
    },
    scheduler::{StorageScheduler, SubscriptionScheduler, TrashScheduler},
    storage::StorageWriteGuard,
};
use std::{path::PathBuf, sync::Arc};
use time::OffsetDateTime;
use tokio::io::AsyncWriteExt;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("pixivarchive=info")),
        )
        .json()
        .init();

    let database_url = required_env("DATABASE_URL")?;
    let bootstrap_media_root = required_absolute_path_env("PIXIVARCHIVE_MEDIA_ROOT")?;

    let avif_available = optional_bool_env("PIXIVARCHIVE_AVIF_AVAILABLE", false)?;
    let db = Db::connect(&database_url).await?;
    let settings = SettingsService::with_capabilities(
        db.clone(),
        DeploymentCapabilities {
            avif_derivatives: avif_available,
        },
    )
    .effective()
    .await?;
    let media_root = MediaRoot::new(settings.storage.active_media_root(bootstrap_media_root));
    prepare_writable_media_root(&media_root).await?;
    let pixiv_cookie_key = InstallationData::new(media_root.path())
        .load_pixiv_cookie_key()
        .context("PixivArchive installation data is not prepared")?;
    let context_provider = Arc::new(PixivAccountContextFactory::new(
        db.clone(),
        PixivCookieCipher::new(PixivCookieKeyringConfig::new(PixivCookieKeyConfig::new(
            pixiv_cookie_key.key_id(),
            pixiv_cookie_key.key(),
        )))?,
    ));
    let storage_write_guard = StorageWriteGuard::new(
        media_root.path().to_path_buf(),
        settings.storage.media_write_stop_threshold_bytes,
    );
    let media_config = media_pipeline_config(media_root, &settings, avif_available)?;
    let processing = settings.processing.clone().unwrap_or_default();
    let pixiv_gate = PixivRequestGate::new(
        usize::from(processing.pixiv_request_concurrency.get()),
        Some((
            u32::from(processing.pixiv_request_rate.requests.get()),
            std::time::Duration::from_secs(u64::from(processing.pixiv_request_rate.per_seconds)),
        )),
    )?;
    let download_gate = PixivRequestGate::new(
        usize::from(processing.media_download_concurrency.get()),
        Some((
            u32::from(processing.media_download_rate.requests.get()),
            std::time::Duration::from_secs(u64::from(processing.media_download_rate.per_seconds)),
        )),
    )?;
    let pixiv_options = PixivClientOptions {
        use_system_proxy: optional_bool_env("PIXIVARCHIVE_PIXIV_USE_SYSTEM_PROXY", true)?,
        metadata_request_gate: Some(pixiv_gate),
        media_request_gate: Some(download_gate),
        ..PixivClientOptions::default()
    };
    let pixiv = PixivWebClient::new(pixiv_options)?;
    let cpu_gate = ExecutionGate::new(usize::from(processing.media_cpu_concurrency.get()), None)?;
    let mut registry = ExecutorRegistry::new();
    registry.register_pixiv_discovery(db.clone(), pixiv.clone(), context_provider.clone());
    registry.register_pixiv_media_with_cpu_gate(
        db.clone(),
        pixiv,
        context_provider,
        media_config,
        cpu_gate,
    );

    let jobs = JobService::from_effective_settings(db.clone(), &settings)
        .map_err(|_| anyhow::anyhow!("retry settings are invalid"))?;
    let quota_weights = QueueQuotaWeights::from(&settings.queue.quota_weights);
    let pixiv_request_concurrency = usize::from(processing.pixiv_request_concurrency.get());
    let media_download_concurrency = usize::from(processing.media_download_concurrency.get());
    let media_processing_concurrency = usize::from(processing.media_cpu_concurrency.get());
    let discovery_runtime = WorkerRuntime::new(
        jobs.clone(),
        registry.clone(),
        WorkerRuntimeConfig {
            max_concurrency: pixiv_request_concurrency,
            ..WorkerRuntimeConfig::default()
        },
    )
    .with_job_kinds(PIXIV_COLLECTION_JOB_KINDS.iter().copied())
    .with_quota_weights(quota_weights)
    .with_subscription_scheduler(SubscriptionScheduler::new(db.clone()));
    let download_runtime = WorkerRuntime::new(
        jobs.clone(),
        registry.clone(),
        WorkerRuntimeConfig {
            max_concurrency: media_download_concurrency,
            ..WorkerRuntimeConfig::default()
        },
    )
    .with_job_kinds(MEDIA_DOWNLOAD_JOB_KINDS.iter().copied())
    .with_quota_weights(quota_weights)
    .with_storage_scheduler(StorageScheduler::new(db.clone(), storage_write_guard));
    let processing_runtime = WorkerRuntime::new(
        jobs,
        registry,
        WorkerRuntimeConfig {
            max_concurrency: media_processing_concurrency,
            ..WorkerRuntimeConfig::default()
        },
    )
    .with_job_kinds(MEDIA_PROCESSING_JOB_KINDS.iter().copied())
    .with_quota_weights(quota_weights)
    .with_trash_scheduler(TrashScheduler::new(db.clone()));

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let worker_id = uuid::Uuid::now_v7();
    let started_at = OffsetDateTime::now_utc();
    let heartbeat = WorkerHeartbeatRepository::new(db);
    heartbeat
        .update(worker_heartbeat(worker_id, started_at))
        .await
        .context("failed to write the initial worker heartbeat")?;
    tokio::spawn(heartbeat_loop(
        heartbeat,
        worker_id,
        started_at,
        shutdown_rx.clone(),
    ));
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });
    tracing::info!(
        pixiv_request_concurrency,
        media_download_concurrency,
        media_processing_concurrency,
        "PixivArchive Worker started"
    );
    tokio::try_join!(
        discovery_runtime.run_until_shutdown(shutdown_rx.clone()),
        download_runtime.run_until_shutdown(shutdown_rx.clone()),
        processing_runtime.run_until_shutdown(shutdown_rx),
    )?;
    Ok(())
}

fn worker_heartbeat(worker_id: uuid::Uuid, started_at: OffsetDateTime) -> WorkerHeartbeatUpdate {
    WorkerHeartbeatUpdate {
        worker_id,
        version: env!("CARGO_PKG_VERSION").to_owned(),
        git_commit: option_env!("PIXIVARCHIVE_GIT_COMMIT").map(str::to_owned),
        started_at,
        seen_at: OffsetDateTime::now_utc(),
    }
}

async fn heartbeat_loop(
    repository: WorkerHeartbeatRepository,
    worker_id: uuid::Uuid,
    started_at: OffsetDateTime,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await;
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if let Err(error) = repository.update(worker_heartbeat(worker_id, started_at)).await {
                    tracing::error!(error = %error, "failed to update worker heartbeat");
                }
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        }
    }
}

fn media_pipeline_config(
    media_root: MediaRoot,
    settings: &pixivarchive_application::settings::EffectiveSettings,
    avif_available: bool,
) -> Result<MediaPipelineConfig> {
    let ugoira = settings.ugoira.clone().unwrap_or_default();
    // Browser playback limits must not prevent the original Pixiv archive from being preserved.
    let max_download_bytes = 8 * 1024 * 1024 * 1024;
    Ok(MediaPipelineConfig {
        media_root,
        store: MediaStoreConfig {
            max_download_bytes,
            probe_limits: MediaProbeLimits {
                max_bytes: max_download_bytes,
                ..MediaProbeLimits::default()
            },
        },
        ugoira: UgoiraLimits {
            max_zip_bytes: ugoira.max_zip_bytes,
            max_frames: ugoira.max_frames.get() as usize,
            max_entry_bytes: ugoira.max_zip_bytes,
            max_total_expanded_bytes: ugoira.max_zip_bytes.saturating_mul(40),
            max_pixels_per_frame: ugoira.max_pixels_per_frame,
        },
        derivative_program: PathBuf::from(
            std::env::var("PIXIVARCHIVE_VIPSTHUMBNAIL")
                .unwrap_or_else(|_| "vipsthumbnail".to_owned()),
        ),
        derivative_format: settings.derivative.format,
        derivative_max_width: settings.derivative.max_width,
        derivative_quality: match settings.derivative.format {
            pixivarchive_domain::media::DerivativeFormat::Webp => settings.derivative.webp_quality,
            pixivarchive_domain::media::DerivativeFormat::Avif => settings.derivative.avif_quality,
        },
        avif_available,
        storage_write_stop_threshold_bytes: settings.storage.media_write_stop_threshold_bytes,
    })
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

async fn prepare_writable_media_root(media_root: &MediaRoot) -> Result<()> {
    let staging = media_root
        .prepare_directory_async("staging")
        .await
        .with_context(|| {
            format!(
                "failed to prepare media staging at {}",
                media_root.path().join("staging").display()
            )
        })?;
    let probe = staging.join(format!(".write-check-{}", uuid::Uuid::now_v7()));
    let write_result = async {
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&probe)
            .await
            .with_context(|| format!("media staging is not writable at {}", staging.display()))?;
        file.write_all(b"pixivarchive")
            .await
            .with_context(|| format!("media staging is not writable at {}", staging.display()))?;
        file.flush()
            .await
            .with_context(|| format!("media staging is not writable at {}", staging.display()))?;
        Ok::<(), anyhow::Error>(())
    }
    .await;
    if let Err(error) = write_result {
        let _ = tokio::fs::remove_file(&probe).await;
        return Err(error);
    }
    tokio::fs::remove_file(&probe)
        .await
        .with_context(|| format!("media staging cannot remove files at {}", staging.display()))?;
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

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("SIGTERM handler must be available");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    use super::prepare_writable_media_root;
    use pixivarchive_media::MediaRoot;
    use uuid::Uuid;

    #[tokio::test]
    async fn worker_media_root_prepares_writable_staging() {
        let root =
            std::env::temp_dir().join(format!("pixivarchive-worker-root-{}", Uuid::now_v7()));

        prepare_writable_media_root(&MediaRoot::new(&root))
            .await
            .unwrap();
        assert!(root.join("staging").is_dir());
        assert_eq!(std::fs::read_dir(root.join("staging")).unwrap().count(), 0);

        tokio::fs::remove_dir(root.join("staging")).await.unwrap();
        tokio::fs::remove_dir(root).await.unwrap();
    }

    #[cfg(any(unix, windows))]
    #[tokio::test]
    async fn worker_media_root_rejects_a_symbolic_link_without_writing_outside() {
        #[cfg(unix)]
        use std::os::unix::fs::symlink;
        #[cfg(windows)]
        use std::os::windows::fs::symlink_dir as symlink;

        let media_root =
            std::env::temp_dir().join(format!("pixivarchive-worker-root-link-{}", Uuid::now_v7()));
        let outside = std::env::temp_dir().join(format!(
            "pixivarchive-worker-root-outside-{}",
            Uuid::now_v7()
        ));
        tokio::fs::create_dir_all(&outside).await.unwrap();
        symlink(&outside, &media_root).unwrap();

        let result = prepare_writable_media_root(&MediaRoot::new(&media_root)).await;
        let wrote_outside = outside.join("staging").exists();

        #[cfg(windows)]
        tokio::fs::remove_dir(&media_root).await.unwrap();
        #[cfg(unix)]
        tokio::fs::remove_file(&media_root).await.unwrap();
        tokio::fs::remove_dir_all(&outside).await.unwrap();

        assert!(result.is_err());
        assert!(!wrote_outside);
    }
}
