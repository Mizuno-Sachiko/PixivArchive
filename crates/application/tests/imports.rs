use pixivarchive_application::{
    imports::{ImportRequest, ImportService},
    pixiv_accounts::{AccountCookieUpdate, PixivAccountService},
};
use pixivarchive_domain::{
    job::JobKind,
    pixiv::{PixivUgoiraFrame, PixivUgoiraMeta, PixivWorkKind},
    rule::{
        Condition, ConditionGroup, ConditionValue, GroupMode, PageQuantifier, RuleAction,
        RuleDefinitionV1, RuleField, RuleOperator,
    },
    subscription::{ImportKind, ImportRunStatus},
};
use pixivarchive_test_support::{
    DISCOVERY_LOCK_ID, FakePixivGateway, LockedDb, context, work_detail, work_page,
};
use sqlx::Row;
use url::Url;
use uuid::Uuid;

#[tokio::test]
async fn work_import_removes_deletion_marker_after_successful_save() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    pixivarchive_db::WorkRepository::new(locked.db.clone())
        .mark_physically_deleted(901, "manual_purge")
        .await
        .unwrap();

    let result = ImportService::new(locked.db.clone(), gateway)
        .import(ImportRequest::work(account.id, context(), 901).forced())
        .await
        .unwrap();

    assert_eq!(result.kind, ImportKind::Work);
    assert_eq!(result.status, ImportRunStatus::DownloadQueued);
    assert!(!work_absent(&locked, 901).await);
    assert!(!deletion_marker_exists(&locked, 901).await);
}

#[tokio::test]
async fn failed_metadata_save_keeps_the_deletion_marker() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    pixivarchive_db::WorkRepository::new(locked.db.clone())
        .mark_physically_deleted(9011, "manual_purge")
        .await
        .unwrap();
    install_work_insert_failure_hook(&locked).await;

    let result = ImportService::new(locked.db.clone(), gateway)
        .import(ImportRequest::work(account.id, context(), 9011).forced())
        .await;

    clear_work_insert_failure_hook(&locked).await;
    assert!(result.is_err());
    assert!(work_absent(&locked, 9011).await);
    assert!(deletion_marker_exists(&locked, 9011).await);
}

#[tokio::test]
async fn rule_based_work_import_can_ignore_without_writing_metadata() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    pixivarchive_db::WorkRepository::new(locked.db.clone())
        .mark_physically_deleted(902, "manual_purge")
        .await
        .unwrap();
    let mut rule_document = RuleDefinitionV1::match_all(
        Uuid::now_v7(),
        "ignore all",
        RuleAction::Download,
        RuleAction::Ignore,
    );
    rule_document.enabled = false;

    let result = ImportService::new(locked.db.clone(), gateway)
        .import(
            ImportRequest::work(account.id, context(), 902)
                .with_rule_document(rule_document.clone()),
        )
        .await
        .unwrap();

    assert_eq!(result.status, ImportRunStatus::Ignored);
    assert!(work_absent(&locked, 902).await);
    assert!(deletion_marker_exists(&locked, 902).await);
    assert_eq!(candidate_count(&locked).await, 0);
    let stored: serde_json::Value =
        sqlx::query_scalar("SELECT params -> 'rule_document' FROM import_run WHERE id = $1")
            .bind(result.id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(RuleDefinitionV1::parse(stored).unwrap(), rule_document);
}

#[tokio::test]
async fn work_import_evaluates_page_metadata_without_downloading_media() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let work_id = 9022;
    let page = work_page(work_id, 0);
    gateway.set_work_pages(pixivarchive_domain::pixiv::PixivWorkPages {
        work_id,
        pages: vec![page],
    });
    let account = account(&locked, gateway.clone()).await;

    let result = ImportService::new(locked.db.clone(), gateway.clone())
        .import(
            ImportRequest::work(account.id, context(), work_id)
                .with_rule_document(page_width_metadata_rule()),
        )
        .await
        .unwrap();

    assert_eq!(result.status, ImportRunStatus::MetadataSaved);
    assert!(gateway.media_requests().is_empty());
    assert_eq!(queued_downloads(&locked).await, 0);
}

#[tokio::test]
async fn synchronous_import_records_a_terminal_failure() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.fail_work_detail(pixivarchive_pixiv::PixivErrorClass::Network);
    let account = account(&locked, gateway.clone()).await;
    pixivarchive_db::WorkRepository::new(locked.db.clone())
        .mark_physically_deleted(9021, "manual_purge")
        .await
        .unwrap();

    ImportService::new(locked.db.clone(), gateway)
        .import(ImportRequest::work(account.id, context(), 9021).forced())
        .await
        .unwrap_err();

    let row = sqlx::query(
        "SELECT status, error_class, finished_at FROM import_run ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("status"), "failed");
    assert_eq!(
        row.get::<Option<String>, _>("error_class").as_deref(),
        Some("network")
    );
    assert!(
        row.get::<Option<time::OffsetDateTime>, _>("finished_at")
            .is_some()
    );
    assert!(deletion_marker_exists(&locked, 9021).await);
}

#[tokio::test]
async fn forced_work_import_queues_download_without_applying_rules() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let mut detail = work_detail(903);
    detail.page_count = 3;
    gateway.set_work_detail(detail);
    gateway.set_work_pages(pixivarchive_domain::pixiv::PixivWorkPages {
        work_id: 903,
        pages: vec![work_page(903, 0), work_page(903, 1), work_page(903, 2)],
    });
    let account = account(&locked, gateway.clone()).await;
    let service = ImportService::new(locked.db.clone(), gateway);

    let first = service
        .import(
            ImportRequest::work(account.id, context(), 903)
                .forced()
                .with_rule_document(disabled_default_rule(RuleAction::Ignore)),
        )
        .await
        .unwrap();
    let second = service
        .import(
            ImportRequest::work(account.id, context(), 903)
                .forced()
                .with_rule_document(disabled_default_rule(RuleAction::MetadataOnly)),
        )
        .await
        .unwrap();

    assert_eq!(first.status, ImportRunStatus::DownloadQueued);
    assert_eq!(second.status, ImportRunStatus::DownloadQueued);
    assert_eq!(queued_downloads(&locked).await, 1);
    assert_eq!(page_urls(&locked, 903).await.len(), 3);
}

#[tokio::test]
async fn work_import_saves_detail_tags_pages_counts_and_provenance() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let mut detail = work_detail(904);
    detail.title = "detailed title".to_owned();
    detail.page_count = 2;
    detail.counts.bookmarks = 12_345;
    detail.tags.push(pixivarchive_domain::pixiv::PixivTag {
        name: "blue".to_owned(),
        translated_name: Some("蓝色".to_owned()),
    });
    gateway.set_work_detail(detail);
    gateway.set_work_pages(pixivarchive_domain::pixiv::PixivWorkPages {
        work_id: 904,
        pages: vec![work_page(904, 0), work_page(904, 1)],
    });
    let account = account(&locked, gateway.clone()).await;

    let result = ImportService::new(locked.db.clone(), gateway)
        .import(ImportRequest::work(account.id, context(), 904).forced())
        .await
        .unwrap();

    assert_eq!(result.status, ImportRunStatus::DownloadQueued);
    let row = work_snapshot(&locked, 904).await;
    assert_eq!(row.title, "detailed title");
    assert_eq!(row.bookmarks, Some(12_345));
    assert_eq!(row.page_count, 2);
    assert_eq!(tag_names(&locked, 904).await, vec!["blue", "original"]);
    assert_eq!(page_urls(&locked, 904).await.len(), 2);
    assert_eq!(
        row.metadata["provenance"]["detail"][0]["adapter_version"],
        "test"
    );
    let revision_source_count: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM work_revision_source source
        JOIN work_revision revision ON revision.id = source.work_revision_id
        JOIN work ON work.id = revision.work_id
        WHERE work.pixiv_work_id = $1
        "#,
    )
    .bind(904_i64)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(revision_source_count, 0);
}

#[tokio::test]
async fn count_updates_do_not_create_revisions_but_content_updates_do() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let detail = work_detail(9041);
    gateway.set_work_detail(detail.clone());
    let account = account(&locked, gateway.clone()).await;
    let service = ImportService::new(locked.db.clone(), gateway.clone());

    service
        .import(ImportRequest::work(account.id, context(), 9041).forced())
        .await
        .unwrap();
    assert_eq!(work_revision_count(&locked, 9041).await, 1);

    let mut counts_changed = detail.clone();
    counts_changed.counts.bookmarks += 1;
    gateway.set_work_detail(counts_changed);
    service
        .import(ImportRequest::work(account.id, context(), 9041).forced())
        .await
        .unwrap();
    assert_eq!(work_revision_count(&locked, 9041).await, 1);
    assert_eq!(work_snapshot(&locked, 9041).await.bookmarks, Some(101));

    let mut content_changed = detail;
    content_changed.title = "revised title".to_owned();
    gateway.set_work_detail(content_changed);
    service
        .import(ImportRequest::work(account.id, context(), 9041).forced())
        .await
        .unwrap();
    assert_eq!(work_revision_count(&locked, 9041).await, 2);
    assert_eq!(work_snapshot(&locked, 9041).await.title, "revised title");
}

#[tokio::test]
async fn unavailable_pixiv_work_updates_source_state_without_removing_the_archive() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    let service = ImportService::new(locked.db.clone(), gateway.clone());

    service
        .import(ImportRequest::work(account.id, context(), 9043).forced())
        .await
        .unwrap();

    gateway.fail_work_detail(pixivarchive_pixiv::PixivErrorClass::HiddenOrNotFound);
    service
        .import(ImportRequest::work(account.id, context(), 9043).forced())
        .await
        .unwrap_err();
    assert_eq!(source_state(&locked, 9043).await, "missing");
    assert!(!work_absent(&locked, 9043).await);

    gateway.fail_work_detail(pixivarchive_pixiv::PixivErrorClass::AgeRestrictedDisabled);
    service
        .import(ImportRequest::work(account.id, context(), 9043).forced())
        .await
        .unwrap_err();
    assert_eq!(source_state(&locked, 9043).await, "restricted");
    assert!(!work_absent(&locked, 9043).await);
}

#[tokio::test]
async fn source_deleted_pages_keep_their_media_history() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let mut detail = work_detail(9042);
    detail.page_count = 3;
    gateway.set_work_detail(detail.clone());
    gateway.set_work_pages(pixivarchive_domain::pixiv::PixivWorkPages {
        work_id: 9042,
        pages: vec![work_page(9042, 0), work_page(9042, 1), work_page(9042, 2)],
    });
    let account = account(&locked, gateway.clone()).await;
    let service = ImportService::new(locked.db.clone(), gateway.clone());

    service
        .import(ImportRequest::work(account.id, context(), 9042).forced())
        .await
        .unwrap();
    assert_eq!(page_urls(&locked, 9042).await.len(), 3);
    let removed_page_id: Uuid = sqlx::query_scalar(
        r#"
        SELECT work_page.id
        FROM work
        JOIN work_page ON work_page.work_id = work.id
        WHERE work.pixiv_work_id = 9042
          AND work_page.page_index = 2
        "#,
    )
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    let media_revision_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO media_revision (
            id, work_page_id, revision_number, media_kind, format,
            source_path, byte_size, sha256
        )
        VALUES ($1, $2, 1, 'source_image', 'png', 'history/source.png', 1, $3)
        "#,
    )
    .bind(media_revision_id)
    .bind(removed_page_id)
    .bind(vec![9_u8; 32])
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE work_page SET current_media_revision_id = $2 WHERE id = $1")
        .bind(removed_page_id)
        .bind(media_revision_id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    gateway.set_work_pages(pixivarchive_domain::pixiv::PixivWorkPages {
        work_id: 9042,
        pages: vec![work_page(9042, 0), work_page(9042, 1)],
    });
    detail.page_count = 2;
    gateway.set_work_detail(detail);
    service
        .import(ImportRequest::work(account.id, context(), 9042).forced())
        .await
        .unwrap();

    assert_eq!(page_urls(&locked, 9042).await.len(), 2);
    assert_eq!(
        page_states(&locked, 9042).await,
        vec![
            (0, "present".to_owned()),
            (1, "present".to_owned()),
            (2, "deleted".to_owned()),
        ]
    );
    let retained_media: i64 =
        sqlx::query_scalar("SELECT count(*) FROM media_revision WHERE id = $1")
            .bind(media_revision_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(retained_media, 1);
}

#[tokio::test]
async fn ugoira_import_fetches_and_saves_animation_manifest() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let mut detail = work_detail(905);
    detail.kind = PixivWorkKind::Ugoira;
    gateway.set_work_detail(detail);
    gateway.set_ugoira_meta(PixivUgoiraMeta {
        work_id: 905,
        zip_url: Url::parse("https://i.pximg.net/img-zip-ugoira/905_ugoira1920x1080.zip").unwrap(),
        frame_mime_type: "image/jpeg".to_owned(),
        frames: vec![PixivUgoiraFrame {
            file: "000000.jpg".to_owned(),
            delay_ms: 80,
        }],
    });
    let account = account(&locked, gateway.clone()).await;

    ImportService::new(locked.db.clone(), gateway)
        .import(ImportRequest::work(account.id, context(), 905).forced())
        .await
        .unwrap();

    let row = work_snapshot(&locked, 905).await;
    assert_eq!(row.metadata["ugoira"]["frames"][0]["delay_ms"], 80);
    assert_eq!(
        row.metadata["ugoira"]["zip_url"],
        "https://i.pximg.net/img-zip-ugoira/905_ugoira1920x1080.zip"
    );
}

#[tokio::test]
async fn artist_import_fetches_id_list_and_normalizes_each_work_through_work_import() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.set_artist_work_ids(vec![1001, 1002, 1001]);
    let account = account(&locked, gateway.clone()).await;
    let works = pixivarchive_db::WorkRepository::new(locked.db.clone());
    works
        .mark_physically_deleted(1001, "manual_purge")
        .await
        .unwrap();
    works
        .mark_physically_deleted(1002, "retention_expired")
        .await
        .unwrap();

    let result = ImportService::new(locked.db.clone(), gateway)
        .import(ImportRequest::artist(account.id, context(), 500).forced())
        .await
        .unwrap();

    assert_eq!(result.kind, ImportKind::Artist);
    assert_eq!(result.status, ImportRunStatus::MetadataSaved);
    assert_eq!(work_count(&locked).await, 2);
    assert!(!deletion_marker_exists(&locked, 1001).await);
    assert!(!deletion_marker_exists(&locked, 1002).await);
}

async fn account(
    locked: &LockedDb,
    gateway: FakePixivGateway,
) -> pixivarchive_application::pixiv_accounts::PixivAccount {
    PixivAccountService::new(locked.db.clone(), gateway)
        .update_cookie(AccountCookieUpdate {
            context: context(),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![0; 12],
            cookie_ciphertext: vec![1],
        })
        .await
        .unwrap()
}

async fn work_absent(locked: &LockedDb, pixiv_work_id: i64) -> bool {
    pixivarchive_db::WorkRepository::new(locked.db.clone())
        .find_by_pixiv_id(pixiv_work_id)
        .await
        .unwrap()
        .is_none()
}

async fn deletion_marker_exists(locked: &LockedDb, pixiv_work_id: i64) -> bool {
    pixivarchive_db::WorkRepository::new(locked.db.clone())
        .deletion_marker_exists(pixiv_work_id)
        .await
        .unwrap()
}

async fn install_work_insert_failure_hook(locked: &LockedDb) {
    clear_work_insert_failure_hook(locked).await;
    sqlx::query(
        r#"
        CREATE FUNCTION fail_reimported_work_insert() RETURNS trigger
        LANGUAGE plpgsql AS $$
        BEGIN
            RAISE EXCEPTION 'forced reimport failure';
        END;
        $$
        "#,
    )
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        CREATE TRIGGER fail_reimported_work_insert
        BEFORE INSERT ON work
        FOR EACH ROW
        EXECUTE FUNCTION fail_reimported_work_insert()
        "#,
    )
    .execute(locked.db.pool())
    .await
    .unwrap();
}

async fn clear_work_insert_failure_hook(locked: &LockedDb) {
    sqlx::query("DROP TRIGGER IF EXISTS fail_reimported_work_insert ON work")
        .execute(locked.db.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION IF EXISTS fail_reimported_work_insert()")
        .execute(locked.db.pool())
        .await
        .unwrap();
}

async fn work_count(locked: &LockedDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM work")
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn queued_downloads(locked: &LockedDb) -> i64 {
    sqlx::query("SELECT kind FROM job WHERE kind = $1")
        .bind(JobKind::DownloadMedia.as_str())
        .fetch_all(locked.db.pool())
        .await
        .unwrap()
        .len() as i64
}

async fn source_state(locked: &LockedDb, pixiv_work_id: i64) -> String {
    sqlx::query_scalar("SELECT source_state FROM work WHERE pixiv_work_id = $1")
        .bind(pixiv_work_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn candidate_count(locked: &LockedDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM import_candidate")
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

struct WorkSnapshot {
    title: String,
    bookmarks: Option<i64>,
    page_count: i32,
    metadata: serde_json::Value,
}

async fn work_snapshot(locked: &LockedDb, pixiv_work_id: i64) -> WorkSnapshot {
    let row = sqlx::query(
        r#"
        SELECT wr.title, w.bookmark_count, wr.page_count, wr.metadata
        FROM work w
        JOIN work_revision wr ON wr.id = w.current_revision_id
        WHERE w.pixiv_work_id = $1
        "#,
    )
    .bind(pixiv_work_id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    WorkSnapshot {
        title: row.get("title"),
        bookmarks: row.get("bookmark_count"),
        page_count: row.get("page_count"),
        metadata: row.get("metadata"),
    }
}

async fn tag_names(locked: &LockedDb, pixiv_work_id: i64) -> Vec<String> {
    sqlx::query_scalar(
        r#"
        SELECT tag.raw_name
        FROM work
        JOIN work_tag ON work_tag.work_id = work.id
        JOIN tag ON tag.id = work_tag.tag_id
        WHERE work.pixiv_work_id = $1
        ORDER BY tag.raw_name
        "#,
    )
    .bind(pixiv_work_id)
    .fetch_all(locked.db.pool())
    .await
    .unwrap()
}

async fn page_urls(locked: &LockedDb, pixiv_work_id: i64) -> Vec<String> {
    sqlx::query_scalar(
        r#"
        SELECT work_page.source_url
        FROM work
        JOIN work_page ON work_page.work_id = work.id
        WHERE work.pixiv_work_id = $1
          AND work_page.source_state = 'present'
        ORDER BY work_page.page_index
        "#,
    )
    .bind(pixiv_work_id)
    .fetch_all(locked.db.pool())
    .await
    .unwrap()
}

async fn page_states(locked: &LockedDb, pixiv_work_id: i64) -> Vec<(i32, String)> {
    sqlx::query_as(
        r#"
        SELECT work_page.page_index, work_page.source_state
        FROM work
        JOIN work_page ON work_page.work_id = work.id
        WHERE work.pixiv_work_id = $1
        ORDER BY work_page.page_index
        "#,
    )
    .bind(pixiv_work_id)
    .fetch_all(locked.db.pool())
    .await
    .unwrap()
}

async fn work_revision_count(locked: &LockedDb, pixiv_work_id: i64) -> i64 {
    sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM work_revision
        JOIN work ON work.id = work_revision.work_id
        WHERE work.pixiv_work_id = $1
        "#,
    )
    .bind(pixiv_work_id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap()
}

fn disabled_default_rule(action: RuleAction) -> RuleDefinitionV1 {
    let mut definition = RuleDefinitionV1::match_all(
        Uuid::now_v7(),
        "default action",
        RuleAction::Download,
        action,
    );
    definition.enabled = false;
    definition
}

fn page_width_metadata_rule() -> RuleDefinitionV1 {
    RuleDefinitionV1 {
        schema_version: 1,
        id: Uuid::now_v7(),
        name: "page width".to_owned(),
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
        action: RuleAction::MetadataOnly,
        default_action: RuleAction::Ignore,
    }
}
