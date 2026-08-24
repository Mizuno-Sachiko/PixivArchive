use pixivarchive_application::pixiv_accounts::{AccountCookieUpdate, PixivAccountService};
use pixivarchive_application::pixiv_works::{
    DeletionMarkerPolicy, PixivWorkProcessor, ProcessPixivWork, ProcessedPixivWork,
    WorkDiscoveryContext,
};
use pixivarchive_domain::{
    job::JobPriority,
    pixiv::PixivWorkPages,
    rule::{
        Condition, ConditionGroup, ConditionValue, GroupMode, PageQuantifier, RuleAction,
        RuleDefinitionV1, RuleField, RuleOperator,
    },
};
use pixivarchive_test_support as support;
use std::sync::Arc;
use uuid::Uuid;

use support::{FakePixivGateway, LockedDb, context, work_detail, work_page};

#[tokio::test]
async fn automatic_collection_keeps_deletion_marker_and_skips_pixiv_requests() {
    let locked = LockedDb::new(709020012).await;
    let gateway = FakePixivGateway::new();
    let work_id = 9_100;
    pixivarchive_db::WorkRepository::new(locked.db.clone())
        .mark_physically_deleted(work_id, "manual_purge")
        .await
        .unwrap();
    let service = PixivWorkProcessor::new(locked.db.clone(), Arc::new(gateway.clone()));

    let result = service
        .process(ProcessPixivWork {
            context: &context(),
            account_id: Uuid::now_v7(),
            pixiv_work_id: work_id,
            deletion_marker_policy: DeletionMarkerPolicy::Block,
            forced: false,
            rule_document: None,
            discovery: WorkDiscoveryContext::default(),
            revision_source: None,
            download_priority: JobPriority::ScheduledCollection,
        })
        .await
        .unwrap();

    assert_eq!(result, ProcessedPixivWork::BlockedByDeletionMarker);
    assert_eq!(gateway.work_detail_calls(), 0);
    assert!(
        pixivarchive_db::WorkRepository::new(locked.db.clone())
            .deletion_marker_exists(work_id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn metadata_rule_can_ignore_without_fetching_pages_or_writing_a_work() {
    let locked = LockedDb::new(709020012).await;
    let gateway = FakePixivGateway::new();
    let work_id = 9_101;
    let mut detail = work_detail(work_id);
    detail.title = "blocked title".to_owned();
    gateway.set_work_detail(detail);
    let service = PixivWorkProcessor::new(locked.db.clone(), Arc::new(gateway.clone()));

    let result = service
        .process(ProcessPixivWork {
            context: &context(),
            account_id: Uuid::now_v7(),
            pixiv_work_id: work_id,
            deletion_marker_policy: DeletionMarkerPolicy::Block,
            forced: false,
            rule_document: Some(&metadata_first_document()),
            discovery: WorkDiscoveryContext::default(),
            revision_source: None,
            download_priority: JobPriority::ScheduledCollection,
        })
        .await
        .unwrap();

    assert_eq!(result, ProcessedPixivWork::Ignored);
    assert_eq!(gateway.work_detail_calls(), 1);
    assert_eq!(gateway.work_pages_calls(), 0);
    assert!(gateway.media_requests().is_empty());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM work")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn missing_rule_document_defaults_to_downloading_the_work() {
    let locked = LockedDb::new(709020012).await;
    let gateway = FakePixivGateway::new();
    let work_id = 9_104;
    gateway.set_work_detail(work_detail(work_id));
    gateway.set_work_pages(PixivWorkPages {
        work_id,
        pages: vec![work_page(work_id, 0)],
    });
    let account = PixivAccountService::new(locked.db.clone(), gateway.clone())
        .update_cookie(AccountCookieUpdate {
            context: context(),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![0; 12],
            cookie_ciphertext: vec![1],
        })
        .await
        .unwrap();
    let service = PixivWorkProcessor::new(locked.db.clone(), Arc::new(gateway));

    let result = service
        .process(ProcessPixivWork {
            context: &context(),
            account_id: account.id,
            pixiv_work_id: work_id,
            deletion_marker_policy: DeletionMarkerPolicy::Block,
            forced: false,
            rule_document: None,
            discovery: WorkDiscoveryContext::default(),
            revision_source: None,
            download_priority: JobPriority::ScheduledCollection,
        })
        .await
        .unwrap();

    assert!(matches!(result, ProcessedPixivWork::DownloadQueued { .. }));
}

#[tokio::test]
async fn forced_candidate_downloads_even_when_rule_would_ignore() {
    let locked = LockedDb::new(709020012).await;
    let gateway = FakePixivGateway::new();
    let work_id = 9_105;
    let mut detail = work_detail(work_id);
    detail.title = "blocked title".to_owned();
    gateway.set_work_detail(detail);
    gateway.set_work_pages(PixivWorkPages {
        work_id,
        pages: vec![work_page(work_id, 0)],
    });
    let account = PixivAccountService::new(locked.db.clone(), gateway.clone())
        .update_cookie(AccountCookieUpdate {
            context: context(),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![0; 12],
            cookie_ciphertext: vec![1],
        })
        .await
        .unwrap();
    let service = PixivWorkProcessor::new(locked.db.clone(), Arc::new(gateway));

    let result = service
        .process(ProcessPixivWork {
            context: &context(),
            account_id: account.id,
            pixiv_work_id: work_id,
            deletion_marker_policy: DeletionMarkerPolicy::Block,
            forced: true,
            rule_document: Some(&metadata_first_document()),
            discovery: WorkDiscoveryContext::default(),
            revision_source: None,
            download_priority: JobPriority::ManualImport,
        })
        .await
        .unwrap();

    assert!(matches!(result, ProcessedPixivWork::DownloadQueued { .. }));
}

#[tokio::test]
async fn page_quantifiers_fetch_page_metadata_without_downloading_media() {
    let locked = LockedDb::new(709020012).await;
    let gateway = FakePixivGateway::new();
    let work_id = 9_102;
    let mut detail = work_detail(work_id);
    detail.page_count = 2;
    gateway.set_work_detail(detail);
    let pages = PixivWorkPages {
        work_id,
        pages: vec![work_page(work_id, 0), work_page(work_id, 1)],
    };
    gateway.set_work_pages(pages);
    let service = PixivWorkProcessor::new(locked.db.clone(), Arc::new(gateway.clone()));

    let result = service
        .process(ProcessPixivWork {
            context: &context(),
            account_id: Uuid::now_v7(),
            pixiv_work_id: work_id,
            deletion_marker_policy: DeletionMarkerPolicy::Block,
            forced: false,
            rule_document: Some(&page_width_document()),
            discovery: WorkDiscoveryContext::default(),
            revision_source: None,
            download_priority: JobPriority::ScheduledCollection,
        })
        .await
        .unwrap();

    assert!(matches!(result, ProcessedPixivWork::MetadataSaved { .. }));
    assert_eq!(gateway.work_pages_calls(), 1);
    assert!(gateway.media_requests().is_empty());
    let media_count: i64 = sqlx::query_scalar("SELECT count(*) FROM media_revision")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(media_count, 0);
}

#[tokio::test]
async fn download_action_enqueues_media_without_downloading_it() {
    let locked = LockedDb::new(709020012).await;
    let gateway = FakePixivGateway::new();
    let work_id = 9_103;
    let detail = work_detail(work_id);
    gateway.set_work_detail(detail);
    gateway.set_work_pages(PixivWorkPages {
        work_id,
        pages: vec![work_page(work_id, 0)],
    });
    let account = PixivAccountService::new(locked.db.clone(), gateway.clone())
        .update_cookie(AccountCookieUpdate {
            context: context(),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![0; 12],
            cookie_ciphertext: vec![1],
        })
        .await
        .unwrap();
    let service = PixivWorkProcessor::new(locked.db.clone(), Arc::new(gateway.clone()));
    let document = RuleDefinitionV1 {
        schema_version: 1,
        id: Uuid::now_v7(),
        name: "download validated page".to_owned(),
        enabled: true,
        group_mode: GroupMode::All,
        groups: vec![ConditionGroup {
            mode: GroupMode::All,
            conditions: vec![page_width_condition()],
        }],
        action: RuleAction::Download,
        default_action: RuleAction::Ignore,
    };

    let result = service
        .process(ProcessPixivWork {
            context: &context(),
            account_id: account.id,
            pixiv_work_id: work_id,
            deletion_marker_policy: DeletionMarkerPolicy::Block,
            forced: false,
            rule_document: Some(&document),
            discovery: WorkDiscoveryContext::default(),
            revision_source: None,
            download_priority: JobPriority::ScheduledCollection,
        })
        .await
        .unwrap();
    assert!(matches!(result, ProcessedPixivWork::DownloadQueued { .. }));
    let media_count: i64 = sqlx::query_scalar("SELECT count(*) FROM media_revision")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(media_count, 0);
    assert!(gateway.media_requests().is_empty());
}

fn metadata_first_document() -> RuleDefinitionV1 {
    RuleDefinitionV1 {
        schema_version: 1,
        id: Uuid::now_v7(),
        name: "ignore title".to_owned(),
        enabled: true,
        group_mode: GroupMode::All,
        groups: vec![ConditionGroup {
            mode: GroupMode::All,
            conditions: vec![Condition {
                field: RuleField::Title,
                operator: RuleOperator::Contains,
                value: Some(ConditionValue::Text("blocked".to_owned())),
                case_sensitive: Some(false),
                tag_scope: None,
                page_quantifier: None,
            }],
        }],
        action: RuleAction::Ignore,
        default_action: RuleAction::MetadataOnly,
    }
}

fn page_width_document() -> RuleDefinitionV1 {
    RuleDefinitionV1 {
        schema_version: 1,
        id: Uuid::now_v7(),
        name: "first matching page".to_owned(),
        enabled: true,
        group_mode: GroupMode::All,
        groups: vec![ConditionGroup {
            mode: GroupMode::All,
            conditions: vec![page_width_condition()],
        }],
        action: RuleAction::MetadataOnly,
        default_action: RuleAction::Ignore,
    }
}

fn page_width_condition() -> Condition {
    Condition {
        field: RuleField::PageWidth,
        operator: RuleOperator::GreaterThan,
        value: Some(ConditionValue::Number(1_000.0)),
        case_sensitive: None,
        tag_scope: None,
        page_quantifier: Some(PageQuantifier::AnyPage),
    }
}
