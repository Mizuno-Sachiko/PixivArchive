use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};
use pixivarchive_application::{
    jobs::JobService,
    pixiv_accounts::{
        AccountCookieUpdate, PixivAccount, PixivAccountContextError, PixivAccountService,
    },
    system::{MaintenanceOperation, SystemService},
};
use pixivarchive_db::{JobRepository, SavePixivWorkMetadata, WorkRepository};
use pixivarchive_domain::{
    job::{JobKind, JobPriority},
    media::DerivativeFormat,
    pixiv::{PixivUgoiraFrame, PixivUgoiraMeta, PixivWorkKind, PixivWorkPages},
};
use pixivarchive_media::{MediaProbeLimits, MediaStoreConfig, UgoiraLimits};
use pixivarchive_pixiv::{
    PixivError, PixivErrorClass, PixivMediaGateway, PixivMediaResponse, PixivRequestContext,
};
use pixivarchive_test_support::{FakePixivGateway, context, work_detail, work_page};
use pixivarchive_worker::{
    executors::{
        ExecutorRegistry, download::MediaPipelineConfig, subscription::PixivContextProvider,
    },
    runtime::{WorkerRuntime, WorkerRuntimeConfig},
    scheduler::{self, StorageScheduler},
    storage::StorageWriteGuard,
};
use secrecy::SecretString;
use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use time::{Duration, OffsetDateTime};
use url::Url;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

mod support;

use support::LockedDb;

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "pixivarchive-worker-media-{}-{}",
            std::process::id(),
            uuid::Uuid::now_v7()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[derive(Clone)]
struct StaticContextProvider;

#[async_trait]
impl PixivContextProvider for StaticContextProvider {
    async fn context_for_account(
        &self,
        _account_id: uuid::Uuid,
    ) -> Result<PixivRequestContext, PixivAccountContextError> {
        Ok(PixivRequestContext::new(
            SecretString::from("PHPSESSID=worker-media"),
            10_001,
            "PixivArchiveWorkerTest/1.0",
        ))
    }
}

#[derive(Clone, Default)]
struct FakeMediaGateway {
    responses: Arc<Mutex<HashMap<String, (Bytes, String)>>>,
}

impl FakeMediaGateway {
    fn insert(&self, url: &Url, bytes: Vec<u8>, content_type: &str) {
        self.responses.lock().unwrap().insert(
            url.as_str().to_owned(),
            (Bytes::from(bytes), content_type.to_owned()),
        );
    }
}

#[async_trait]
impl PixivMediaGateway for FakeMediaGateway {
    async fn media(
        &self,
        _context: &PixivRequestContext,
        _work_id: i64,
        media_url: Url,
    ) -> Result<PixivMediaResponse, PixivError> {
        let (bytes, content_type) = self
            .responses
            .lock()
            .unwrap()
            .get(media_url.as_str())
            .cloned()
            .ok_or_else(|| PixivError::new(PixivErrorClass::HiddenOrNotFound, None))?;
        Ok(PixivMediaResponse {
            content_length: Some(bytes.len() as u64),
            content_type: Some(content_type),
            body: Box::pin(stream::iter(vec![Ok(bytes)])),
        })
    }
}

#[tokio::test]
async fn worker_downloads_validates_and_derives_static_pixiv_media() {
    let locked = LockedDb::new().await;
    let directory = TestDirectory::new();
    let account = create_account(&locked).await;
    let pixiv_work_id = 7_101;
    let page = work_page(pixiv_work_id, 0);
    let media_gateway = FakeMediaGateway::default();
    media_gateway.insert(&page.original_url, png_bytes(320, 180), "image/png");
    let work = WorkRepository::new(locked.db.clone())
        .save_pixiv_metadata(SavePixivWorkMetadata {
            account_id: None,
            detail: work_detail(pixiv_work_id),
            pages: PixivWorkPages {
                work_id: pixiv_work_id,
                pages: vec![page],
            },
            ugoira: None,
            provenance: serde_json::json!({"test": true}),
            revision_source: None,
        })
        .await
        .unwrap();
    JobRepository::new(locked.db.clone())
        .enqueue_download_if_absent(
            account.id,
            work.id,
            pixiv_work_id,
            JobPriority::BackgroundMaintenance,
        )
        .await
        .unwrap();

    let mut registry = ExecutorRegistry::new();
    registry.register_pixiv_media(
        locked.db.clone(),
        media_gateway,
        Arc::new(StaticContextProvider),
        media_config(directory.path()),
    );
    let runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        registry,
        test_runtime_config(),
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(
        job_state(&locked, JobKind::DownloadMedia).await,
        "completed"
    );

    let source_path: String = sqlx::query_scalar("SELECT source_path FROM media_revision LIMIT 1")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(source_path, "originals/pixiv/100/7101/7101_p0_r0001.png");
    assert!(directory.path().join(&source_path).is_file());
    assert_eq!(work_state(&locked, work.id).await, "collected");

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(
        job_state(&locked, JobKind::GenerateDerivative).await,
        "completed"
    );

    let (derivative_path, dominant_color): (String, String) =
        sqlx::query_as("SELECT path, dominant_color FROM derivative LIMIT 1")
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(
        derivative_path,
        "derivatives/pixiv/100/7101/7101_p0_r0001_waterfall.webp"
    );
    assert!(directory.path().join(&derivative_path).is_file());
    assert_eq!(dominant_color, "#1450c8");
    let pending_artifacts: i64 = sqlx::query_scalar("SELECT count(*) FROM media_artifact_intent")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(pending_artifacts, 0);

    let maintenance = SystemService::new(
        locked.db.clone(),
        directory.path().to_path_buf(),
        "test",
        None,
    );
    let (left, right) = tokio::join!(
        maintenance.queue_maintenance(MaintenanceOperation::RegenerateDerivatives),
        maintenance.queue_maintenance(MaintenanceOperation::RegenerateDerivatives)
    );
    let regeneration_job_ids = left
        .unwrap()
        .job_ids
        .into_iter()
        .chain(right.unwrap().job_ids)
        .collect::<Vec<_>>();
    assert_eq!(regeneration_job_ids.len(), 1);
    let regeneration_job_id = regeneration_job_ids[0];
    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(
        job_state(&locked, JobKind::GenerateDerivative).await,
        "completed"
    );

    let regenerated_path: String = sqlx::query_scalar("SELECT path FROM derivative LIMIT 1")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_ne!(regenerated_path, derivative_path);
    assert!(regenerated_path.contains(&regeneration_job_id.simple().to_string()));
    assert!(directory.path().join(&derivative_path).is_file());
    assert!(directory.path().join(&regenerated_path).is_file());

    StorageScheduler::new(
        locked.db.clone(),
        StorageWriteGuard::new(directory.path().to_path_buf(), 0),
    )
    .process_due(OffsetDateTime::now_utc())
    .await
    .unwrap();
    assert!(!directory.path().join(derivative_path).exists());
    assert!(directory.path().join(regenerated_path).is_file());
    let pending_artifacts: i64 = sqlx::query_scalar("SELECT count(*) FROM media_artifact_intent")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(pending_artifacts, 0);
}

#[tokio::test]
async fn worker_waits_for_storage_before_requesting_source_media() {
    let locked = LockedDb::new().await;
    let directory = TestDirectory::new();
    let account = create_account(&locked).await;
    let pixiv_work_id = 7_151;
    let page = work_page(pixiv_work_id, 0);
    let work = WorkRepository::new(locked.db.clone())
        .save_pixiv_metadata(SavePixivWorkMetadata {
            account_id: None,
            detail: work_detail(pixiv_work_id),
            pages: PixivWorkPages {
                work_id: pixiv_work_id,
                pages: vec![page],
            },
            ugoira: None,
            provenance: serde_json::json!({"test": true}),
            revision_source: None,
        })
        .await
        .unwrap();
    JobRepository::new(locked.db.clone())
        .enqueue_download_if_absent(
            account.id,
            work.id,
            pixiv_work_id,
            JobPriority::BackgroundMaintenance,
        )
        .await
        .unwrap();

    let mut config = media_config(directory.path());
    config.storage_write_stop_threshold_bytes = u64::MAX;
    let mut registry = ExecutorRegistry::new();
    registry.register_pixiv_media(
        locked.db.clone(),
        FakeMediaGateway::default(),
        Arc::new(StaticContextProvider),
        config,
    );
    let runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        registry,
        test_runtime_config(),
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert_eq!(
        job_state(&locked, JobKind::DownloadMedia).await,
        "waiting_storage"
    );
    let stored_media: i64 = sqlx::query_scalar("SELECT count(*) FROM media_revision")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(stored_media, 0);
}

#[tokio::test]
async fn worker_waits_for_storage_before_generating_a_derivative() {
    let locked = LockedDb::new().await;
    let directory = TestDirectory::new();
    let account = create_account(&locked).await;
    let pixiv_work_id = 7_152;
    let page = work_page(pixiv_work_id, 0);
    let media_gateway = FakeMediaGateway::default();
    media_gateway.insert(&page.original_url, png_bytes(320, 180), "image/png");
    let work = WorkRepository::new(locked.db.clone())
        .save_pixiv_metadata(SavePixivWorkMetadata {
            account_id: None,
            detail: work_detail(pixiv_work_id),
            pages: PixivWorkPages {
                work_id: pixiv_work_id,
                pages: vec![page],
            },
            ugoira: None,
            provenance: serde_json::json!({"test": true}),
            revision_source: None,
        })
        .await
        .unwrap();
    JobRepository::new(locked.db.clone())
        .enqueue_download_if_absent(
            account.id,
            work.id,
            pixiv_work_id,
            JobPriority::BackgroundMaintenance,
        )
        .await
        .unwrap();

    let mut download_registry = ExecutorRegistry::new();
    download_registry.register_pixiv_media(
        locked.db.clone(),
        media_gateway.clone(),
        Arc::new(StaticContextProvider),
        media_config(directory.path()),
    );
    let download_runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        download_registry,
        test_runtime_config(),
    );
    let mut rotation = scheduler::default_rotation();
    assert!(download_runtime.process_once(&mut rotation).await.unwrap());

    let mut blocked_config = media_config(directory.path());
    blocked_config.storage_write_stop_threshold_bytes = u64::MAX;
    let mut blocked_registry = ExecutorRegistry::new();
    blocked_registry.register_pixiv_media(
        locked.db.clone(),
        media_gateway,
        Arc::new(StaticContextProvider),
        blocked_config,
    );
    let blocked_runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        blocked_registry,
        test_runtime_config(),
    );
    assert!(blocked_runtime.process_once(&mut rotation).await.unwrap());

    assert_eq!(
        job_state(&locked, JobKind::GenerateDerivative).await,
        "waiting_storage"
    );
    let derivatives: i64 = sqlx::query_scalar("SELECT count(*) FROM derivative")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(derivatives, 0);
}

#[tokio::test]
async fn worker_preserves_ugoira_zip_and_uses_its_first_frame_for_processing() {
    let locked = LockedDb::new().await;
    let directory = TestDirectory::new();
    let account = create_account(&locked).await;
    let pixiv_work_id = 7_201;
    let mut detail = work_detail(pixiv_work_id);
    detail.kind = PixivWorkKind::Ugoira;
    let page = work_page(pixiv_work_id, 0);
    let manifest = PixivUgoiraMeta {
        work_id: pixiv_work_id,
        zip_url: Url::parse("https://i.pximg.net/ugoira/7201.zip").unwrap(),
        frame_mime_type: "image/jpeg".to_owned(),
        frames: vec![
            PixivUgoiraFrame {
                file: "000000.jpg".to_owned(),
                delay_ms: 80,
            },
            PixivUgoiraFrame {
                file: "000001.jpg".to_owned(),
                delay_ms: 120,
            },
        ],
    };
    let media_gateway = FakeMediaGateway::default();
    media_gateway.insert(
        &manifest.zip_url,
        zip_bytes(&[
            ("000000.jpg", image_bytes(ImageFormat::Jpeg, 160, 90)),
            ("000001.jpg", image_bytes(ImageFormat::Jpeg, 160, 90)),
        ]),
        "application/zip",
    );
    let work = WorkRepository::new(locked.db.clone())
        .save_pixiv_metadata(SavePixivWorkMetadata {
            account_id: None,
            detail,
            pages: PixivWorkPages {
                work_id: pixiv_work_id,
                pages: vec![page],
            },
            ugoira: Some(manifest),
            provenance: serde_json::json!({"test": true}),
            revision_source: None,
        })
        .await
        .unwrap();
    JobRepository::new(locked.db.clone())
        .enqueue_download_if_absent(
            account.id,
            work.id,
            pixiv_work_id,
            JobPriority::BackgroundMaintenance,
        )
        .await
        .unwrap();

    let mut registry = ExecutorRegistry::new();
    registry.register_pixiv_media(
        locked.db.clone(),
        media_gateway,
        Arc::new(StaticContextProvider),
        media_config(directory.path()),
    );
    let runtime = WorkerRuntime::new(
        JobService::new(locked.db.clone()),
        registry,
        test_runtime_config(),
    );
    let mut rotation = scheduler::default_rotation();

    assert!(runtime.process_once(&mut rotation).await.unwrap());
    assert!(runtime.process_once(&mut rotation).await.unwrap());

    let source_path: String = sqlx::query_scalar("SELECT source_path FROM media_revision LIMIT 1")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(
        source_path,
        "originals/pixiv/100/7201/7201_ugoira_r0001.zip"
    );
    assert!(directory.path().join(source_path).is_file());
    let derivative_kind: String =
        sqlx::query_scalar("SELECT derivative_kind FROM derivative LIMIT 1")
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(derivative_kind, "ugoira_cover");
    let dominant_color: String =
        sqlx::query_scalar("SELECT dominant_color FROM derivative LIMIT 1")
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert!(dominant_color.starts_with('#'));
    assert!(directory_is_empty(&directory.path().join("staging")));
}

async fn create_account(locked: &LockedDb) -> PixivAccount {
    PixivAccountService::new(locked.db.clone(), FakePixivGateway::new())
        .update_cookie(AccountCookieUpdate {
            context: context(),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![0; 12],
            cookie_ciphertext: vec![1],
        })
        .await
        .unwrap()
}

fn media_config(root: &Path) -> MediaPipelineConfig {
    MediaPipelineConfig {
        media_root: root.into(),
        store: MediaStoreConfig {
            max_download_bytes: 4 * 1024 * 1024,
            probe_limits: MediaProbeLimits {
                max_bytes: 4 * 1024 * 1024,
                max_width: 4_096,
                max_height: 4_096,
                max_pixels: 16_000_000,
            },
        },
        ugoira: UgoiraLimits {
            max_zip_bytes: 4 * 1024 * 1024,
            max_frames: 100,
            max_entry_bytes: 2 * 1024 * 1024,
            max_total_expanded_bytes: 16 * 1024 * 1024,
            max_pixels_per_frame: 16_000_000,
        },
        derivative_program: PathBuf::from("vipsthumbnail"),
        derivative_format: DerivativeFormat::Webp,
        derivative_max_width: 120,
        derivative_quality: 82,
        avif_available: false,
        storage_write_stop_threshold_bytes: 0,
    }
}

fn test_runtime_config() -> WorkerRuntimeConfig {
    WorkerRuntimeConfig {
        max_concurrency: 1,
        lease_duration: Duration::seconds(5),
        heartbeat_interval: std::time::Duration::from_millis(100),
        poll_interval: std::time::Duration::from_millis(20),
        shutdown_grace: std::time::Duration::from_millis(100),
    }
}

fn png_bytes(width: u32, height: u32) -> Vec<u8> {
    image_bytes(ImageFormat::Png, width, height)
}

fn image_bytes(format: ImageFormat, width: u32, height: u32) -> Vec<u8> {
    let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(width, height, Rgb([20, 80, 200])));
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, format).unwrap();
    output.into_inner()
}

fn zip_bytes(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, bytes) in entries {
        writer.start_file(*name, options).unwrap();
        std::io::Write::write_all(&mut writer, bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn directory_is_empty(path: &Path) -> bool {
    !path.exists() || fs::read_dir(path).unwrap().next().is_none()
}

async fn job_state(locked: &LockedDb, kind: JobKind) -> String {
    sqlx::query_scalar("SELECT state FROM job WHERE kind = $1 ORDER BY created_at DESC LIMIT 1")
        .bind(kind.as_str())
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn work_state(locked: &LockedDb, work_id: uuid::Uuid) -> String {
    sqlx::query_scalar("SELECT collection_state FROM work WHERE id = $1")
        .bind(work_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}
