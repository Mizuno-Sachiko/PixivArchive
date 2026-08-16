use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};
use pixivarchive_application::{
    bookmarks::{BookmarkWritebackRequest, BookmarkWritebackService, BookmarkWritebackStatus},
    gallery::GalleryService,
    imports::{ImportRequest, ImportService},
    pixiv_accounts::{AccountCookieUpdate, PixivAccountService},
    pixiv_works::{
        DeletionMarkerPolicy, PixivWorkProcessor, ProcessPixivWork, ProcessedPixivWork,
        WorkDiscoveryContext,
    },
    rules::{PublishRuleVersionRequest, RuleService, SaveRuleDraftRequest},
    subscriptions::{
        RankingSubscriptionRequest, SubscriptionExecutionService, SubscriptionService,
        SubscriptionUnitRequest,
    },
    trash::TrashService,
};
use pixivarchive_db::{
    JobRepository, MediaRepository, SaveSourceMediaRevision, TrashRepository, WorkRepository,
};
use pixivarchive_domain::{
    job::{ClaimedJob, JobPriority, JobQuotaSelection},
    media::MediaKind,
    pixiv::{
        PixivBookmarkVisibility, PixivBookmarksMode, PixivFollowLatestMode, PixivRankingContent,
        PixivRankingMode, PixivUgoiraFrame, PixivUgoiraMeta, PixivWorkKind, PixivWorkPages,
    },
    rule::{
        Condition, ConditionGroup, ConditionValue, GroupMode, PageQuantifier, RuleAction,
        RuleDefinitionV1, RuleField, RuleOperator,
    },
    subscription::ImportRunStatus,
    work::GallerySearch,
};
use pixivarchive_media::PixivMediaPaths;
use pixivarchive_test_support as support;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use url::Url;
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

use support::{
    FakePixivGateway, LockedDb, configure_bookmarks_subscription, configure_following_subscription,
    context, discovery_work, ranking_entry, work_detail, work_page,
};

const FULL_JOURNEY_LOCK_ID: i64 = 709_020_015;

#[tokio::test]
async fn full_journey_preserves_rules_media_and_deletion_decisions() {
    let locked = LockedDb::new(FULL_JOURNEY_LOCK_ID).await;
    let directory = TestDirectory::new();
    let gateway = FakePixivGateway::new();
    let account = PixivAccountService::new(locked.db.clone(), gateway.clone())
        .update_cookie(AccountCookieUpdate {
            context: context(),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![0; 12],
            cookie_ciphertext: vec![1],
        })
        .await
        .unwrap();

    let rules = RuleService::new(locked.db.clone());
    let rule = rules
        .create_rule("main", RuleAction::MetadataOnly)
        .await
        .unwrap();
    let initial_draft = rules.load_draft(rule.id).await.unwrap().unwrap();
    let draft = rules
        .save_draft(SaveRuleDraftRequest {
            rule_id: rule.id,
            expected_revision: Some(initial_draft.revision),
            base_version: None,
            definition: serde_json::to_value(metadata_document(rule.id)).unwrap(),
        })
        .await
        .unwrap();
    rules
        .publish_version(PublishRuleVersionRequest {
            rule_id: rule.id,
            base_version: None,
            expected_draft_revision: draft.revision,
            created_by: None,
        })
        .await
        .unwrap();

    gateway.set_ranking_items(vec![ranking_entry(41_001, 1)]);
    gateway.set_follow_items(vec![discovery_work(41_002)]);
    gateway.set_bookmark_items(vec![discovery_work(41_003)]);
    let subscriptions = SubscriptionService::new(locked.db.clone());
    let ranking = subscriptions
        .create_ranking(RankingSubscriptionRequest {
            account_id: account.id,
            name: "daily".to_owned(),
            modes: vec![PixivRankingMode::Daily],
            contents: vec![PixivRankingContent::All],
            interval_minutes: 60,
            lookback_pages: 1,
            rule_id: Some(rule.id),
            next_run_at: None,
        })
        .await
        .unwrap();
    let following =
        configure_following_subscription(&locked.db, account.id, PixivFollowLatestMode::All, 60, 1)
            .await;
    let bookmarks =
        configure_bookmarks_subscription(&locked.db, account.id, PixivBookmarksMode::All, 60, 1)
            .await;
    let subscription_executor =
        SubscriptionExecutionService::new(locked.db.clone(), gateway.clone());
    for subscription_id in [ranking.id, following.id, bookmarks.id] {
        let run = subscriptions
            .start_manual_run(subscription_id, false)
            .await
            .unwrap();
        for unit_id in subscription_unit_ids(&locked, run.run_id).await {
            subscription_executor
                .execute_unit(SubscriptionUnitRequest {
                    context: context(),
                    unit_id,
                })
                .await
                .unwrap();
        }
    }

    gateway.set_artist_work_ids(vec![41_004]);
    let imports = ImportService::new(locked.db.clone(), gateway.clone());
    let artist_import = imports
        .import(ImportRequest::artist(account.id, context(), 51_004).forced())
        .await
        .unwrap();
    let work_import = imports
        .import(ImportRequest::work(account.id, context(), 41_005).forced())
        .await
        .unwrap();
    assert_eq!(artist_import.status, ImportRunStatus::MetadataSaved);
    assert_eq!(work_import.status, ImportRunStatus::DownloadQueued);

    let writeback = BookmarkWritebackService::new(locked.db.clone(), gateway.clone());
    let disabled = writeback
        .add(BookmarkWritebackRequest::add(
            account.id,
            context(),
            41_005,
            PixivBookmarkVisibility::Private,
            Vec::new(),
        ))
        .await
        .unwrap();
    assert_eq!(disabled.status, BookmarkWritebackStatus::Disabled);
    PixivAccountService::new(locked.db.clone(), gateway.clone())
        .set_bookmark_writeback_enabled(account.id, true)
        .await
        .unwrap();
    let enabled = writeback
        .add(BookmarkWritebackRequest::add(
            account.id,
            context(),
            41_005,
            PixivBookmarkVisibility::Private,
            vec!["archive".to_owned()],
        ))
        .await
        .unwrap();
    assert_eq!(enabled.status, BookmarkWritebackStatus::Succeeded);

    let static_work_id = 41_006;
    let static_page = work_page(static_work_id, 0);
    gateway.set_work_pages(PixivWorkPages {
        work_id: static_work_id,
        pages: vec![static_page.clone()],
    });
    let static_bytes = png_bytes();
    gateway.set_media(&static_page.original_url, static_bytes.clone(), "image/png");

    let ugoira_work_id = 41_007;
    let mut ugoira_detail = work_detail(ugoira_work_id);
    ugoira_detail.kind = PixivWorkKind::Ugoira;
    gateway.set_work_detail(ugoira_detail);
    gateway.set_work_pages(PixivWorkPages {
        work_id: ugoira_work_id,
        pages: vec![work_page(ugoira_work_id, 0)],
    });
    let ugoira = PixivUgoiraMeta {
        work_id: ugoira_work_id,
        zip_url: Url::parse("https://i.pximg.net/ugoira/41007.zip").unwrap(),
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
    let ugoira_bytes = zip_bytes(&[("000000.jpg", jpeg_bytes()), ("000001.jpg", jpeg_bytes())]);
    gateway.set_media(&ugoira.zip_url, ugoira_bytes.clone(), "application/zip");
    gateway.set_ugoira_meta(ugoira);

    let candidates = PixivWorkProcessor::new(locked.db.clone(), Arc::new(gateway.clone()));
    let static_result = candidates
        .process(ProcessPixivWork {
            context: &context(),
            account_id: account.id,
            pixiv_work_id: static_work_id,
            deletion_marker_policy: DeletionMarkerPolicy::Block,
            forced: false,
            rule_document: Some(&download_document()),
            discovery: WorkDiscoveryContext::default(),
            download_priority: JobPriority::ManualImport,
        })
        .await
        .unwrap();
    let ugoira_result = candidates
        .process(ProcessPixivWork {
            context: &context(),
            account_id: account.id,
            pixiv_work_id: ugoira_work_id,
            deletion_marker_policy: DeletionMarkerPolicy::Block,
            forced: false,
            rule_document: Some(&download_document()),
            discovery: WorkDiscoveryContext::default(),
            download_priority: JobPriority::ManualImport,
        })
        .await
        .unwrap();
    let (static_work, static_job) = match static_result {
        ProcessedPixivWork::DownloadQueued { work_id, job_id } => (work_id, job_id),
        other => panic!("expected queued static download, got {other:?}"),
    };
    let (ugoira_work, ugoira_job) = match ugoira_result {
        ProcessedPixivWork::DownloadQueued { work_id, job_id } => (work_id, job_id),
        other => panic!("expected queued Ugoira download, got {other:?}"),
    };
    let jobs = JobRepository::new(locked.db.clone());
    sqlx::query("UPDATE job SET available_at = now() - interval '1 minute' WHERE id = ANY($1)")
        .bind(vec![static_job, ugoira_job])
        .execute(locked.db.pool())
        .await
        .unwrap();
    let selection = JobQuotaSelection::new(vec![JobPriority::ManualImport]);
    let first_download = jobs
        .claim_next(Uuid::now_v7(), &selection, time::Duration::minutes(5))
        .await
        .unwrap()
        .unwrap();
    let second_download = jobs
        .claim_next(Uuid::now_v7(), &selection, time::Duration::minutes(5))
        .await
        .unwrap()
        .unwrap();
    for claimed in [first_download, second_download] {
        if claimed.id == static_job {
            save_test_media(
                &locked,
                &directory.path,
                static_work,
                claimed,
                &static_bytes,
            )
            .await;
        } else {
            assert_eq!(claimed.id, ugoira_job);
            save_test_media(
                &locked,
                &directory.path,
                ugoira_work,
                claimed,
                &ugoira_bytes,
            )
            .await;
        }
    }
    let derivative_priorities: Vec<String> = sqlx::query_scalar(
        "SELECT priority_class FROM job WHERE kind = 'generate_derivative' ORDER BY created_at",
    )
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(
        derivative_priorities,
        vec!["manual_import", "manual_import"]
    );
    assert_eq!(stored_media_count(&locked, static_work_id).await, 1);
    assert_eq!(stored_media_count(&locked, ugoira_work_id).await, 1);

    let gallery = GalleryService::new(locked.db.clone())
        .search(GallerySearch::default())
        .await
        .unwrap();
    assert!(
        gallery
            .items
            .iter()
            .all(|work| work.pixiv_work_id != 41_001)
    );
    assert!(
        gallery
            .items
            .iter()
            .any(|work| work.pixiv_work_id == static_work_id)
    );
    assert!(
        gallery
            .items
            .iter()
            .any(|work| work.pixiv_work_id == ugoira_work_id)
    );

    let works = WorkRepository::new(locked.db.clone());
    let ugoira_work = works
        .find_by_pixiv_id(ugoira_work_id)
        .await
        .unwrap()
        .unwrap();
    let trash = TrashService::new(locked.db.clone());
    trash.move_to_trash(ugoira_work.id, 30).await.unwrap();
    trash.restore(ugoira_work.id).await.unwrap();

    let static_work = works
        .find_by_pixiv_id(static_work_id)
        .await
        .unwrap()
        .unwrap();
    trash.move_to_trash(static_work.id, 30).await.unwrap();
    let purge_job_id = trash.purge(static_work.id).await.unwrap();
    assert!(purge_job_id != Uuid::nil());
    purge_test_work(&locked, &directory.path, static_work.id).await;

    let restored = imports
        .import(ImportRequest::work(account.id, context(), static_work_id).forced())
        .await
        .unwrap();
    assert_eq!(restored.status, ImportRunStatus::DownloadQueued);
    assert!(!works.deletion_marker_exists(static_work_id).await.unwrap());
}

async fn subscription_unit_ids(locked: &LockedDb, run_id: Uuid) -> Vec<Uuid> {
    sqlx::query_scalar(
        "SELECT id FROM subscription_run_unit WHERE subscription_run_id = $1 ORDER BY source_key",
    )
    .bind(run_id)
    .fetch_all(locked.db.pool())
    .await
    .unwrap()
}

async fn stored_media_count(locked: &LockedDb, pixiv_work_id: i64) -> i64 {
    sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM media_revision
        JOIN work_page ON work_page.id = media_revision.work_page_id
        JOIN work ON work.id = work_page.work_id
        WHERE work.pixiv_work_id = $1
        "#,
    )
    .bind(pixiv_work_id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap()
}

async fn save_test_media(
    locked: &LockedDb,
    media_root: &Path,
    work_id: Uuid,
    claimed: ClaimedJob,
    bytes: &[u8],
) {
    let repository = MediaRepository::new(locked.db.clone());
    let job_id = claimed.id;
    let derivative_priority = claimed.priority;
    let plan = repository
        .load_download_plan(job_id, work_id)
        .await
        .unwrap();
    let item = plan.items.into_iter().next().unwrap();
    let relative_path = match item.media_kind {
        MediaKind::SourceImage => PixivMediaPaths::original_image(
            plan.pixiv_artist_id,
            plan.pixiv_work_id,
            item.page.page_index,
            item.page.revision,
            item.page.format,
        ),
        MediaKind::UgoiraZip => PixivMediaPaths::ugoira_zip(
            plan.pixiv_artist_id,
            plan.pixiv_work_id,
            item.page.revision,
        ),
        MediaKind::Derivative => panic!("download plan contained a derivative"),
    }
    .unwrap();
    let absolute_path = media_root.join(&relative_path);
    fs::create_dir_all(absolute_path.parent().unwrap()).unwrap();
    fs::write(&absolute_path, bytes).unwrap();
    repository
        .register_artifact_intent(claimed.lease(), &relative_path)
        .await
        .unwrap();
    repository
        .save_source_revision(SaveSourceMediaRevision {
            lease: claimed.lease(),
            derivative_priority,
            work_id,
            work_page_id: item.page.work_page_id,
            expected_current_media_revision_id: item.page.current.map(|current| current.id),
            revision_number: item.page.revision,
            media_kind: item.media_kind,
            format: item.page.format,
            source_url: item.page.source_url,
            relative_path,
            byte_size: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
            dimensions: None,
            ugoira: item.ugoira,
            complete_job: false,
        })
        .await
        .unwrap();
}

async fn purge_test_work(locked: &LockedDb, media_root: &Path, work_id: Uuid) {
    let repository = TrashRepository::new(locked.db.clone());
    let plan = repository.load_purge_plan(work_id).await.unwrap();
    repository.begin_purge(work_id).await.unwrap();
    for relative_path in plan.relative_paths {
        let path = media_root.join(relative_path);
        if let Err(error) = fs::remove_file(path) {
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        }
    }
    repository
        .complete_purge(work_id, "manual_purge")
        .await
        .unwrap();
}

fn metadata_document(rule_id: Uuid) -> RuleDefinitionV1 {
    let mut definition = RuleDefinitionV1::match_all(
        rule_id,
        "metadata only",
        RuleAction::Download,
        RuleAction::MetadataOnly,
    );
    definition.enabled = false;
    definition
}

fn download_document() -> RuleDefinitionV1 {
    RuleDefinitionV1 {
        schema_version: 1,
        id: Uuid::now_v7(),
        name: "download wide pages".to_owned(),
        enabled: true,
        group_mode: GroupMode::All,
        groups: vec![ConditionGroup {
            mode: GroupMode::All,
            conditions: vec![Condition {
                field: RuleField::PageWidth,
                operator: RuleOperator::GreaterThan,
                value: Some(ConditionValue::Number(1_000.0)),
                case_sensitive: None,
                tag_scope: None,
                page_quantifier: Some(PageQuantifier::AnyPage),
            }],
        }],
        action: RuleAction::Download,
        default_action: RuleAction::Ignore,
    }
}

fn png_bytes() -> Vec<u8> {
    image_bytes(ImageFormat::Png)
}

fn jpeg_bytes() -> Vec<u8> {
    image_bytes(ImageFormat::Jpeg)
}

fn image_bytes(format: ImageFormat) -> Vec<u8> {
    let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(8, 8, Rgb([20, 80, 200])));
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, format).unwrap();
    output.into_inner()
}

fn zip_bytes(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, bytes) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "pixivarchive-full-journey-{}-{}",
            std::process::id(),
            Uuid::now_v7()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
