use pixivarchive_application::{
    bookmarks::{
        BookmarkCommandPort, BookmarkCommandRequest, BookmarkWritebackError,
        BookmarkWritebackRequest, BookmarkWritebackService, BookmarkWritebackStatus,
        LiveBookmarkCommandPort,
    },
    imports::{ImportRequest, ImportService},
    pixiv_accounts::{
        AccountCookieUpdate, LivePixivAccountCommandPort, PixivAccountCommandPort,
        PixivAccountContextFactory, PixivAccountService, PixivCookieCipher, PixivCookieKeyConfig,
        PixivCookieKeyringConfig, UpdatePixivAccountRequest,
    },
};
use pixivarchive_db::{BookmarkRepository, DbError, WorkRepository};
use pixivarchive_domain::pixiv::{PixivBookmarkRef, PixivBookmarkVisibility};
use pixivarchive_pixiv::PixivErrorClass;
use pixivarchive_test_support::{
    DISCOVERY_LOCK_ID, FakePixivGateway, LockedDb, context, work_detail,
};

#[tokio::test]
async fn bookmark_writeback_is_disabled_by_default() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;

    let result = BookmarkWritebackService::new(locked.db.clone(), gateway.clone())
        .add(BookmarkWritebackRequest::add(
            account.id,
            context(),
            1201,
            PixivBookmarkVisibility::Private,
            vec!["archive".to_owned()],
        ))
        .await
        .unwrap();

    assert_eq!(result.status, BookmarkWritebackStatus::Disabled);
    assert!(gateway.add_requests().is_empty());
}

#[tokio::test]
async fn add_and_remove_writeback_call_the_gateway_after_enablement() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    PixivAccountService::new(locked.db.clone(), gateway.clone())
        .set_bookmark_writeback_enabled(account.id, true)
        .await
        .unwrap();
    WorkRepository::new(locked.db.clone())
        .create_metadata_only(1202, 77, "bookmark target")
        .await
        .unwrap();
    let service = BookmarkWritebackService::new(locked.db.clone(), gateway.clone());

    let added = service
        .add(BookmarkWritebackRequest::add(
            account.id,
            context(),
            1202,
            PixivBookmarkVisibility::Public,
            vec!["ok".to_owned()],
        ))
        .await
        .unwrap();
    let removed = service
        .remove(BookmarkWritebackRequest::remove(
            account.id,
            context(),
            1202,
        ))
        .await
        .unwrap();

    assert_eq!(added.status, BookmarkWritebackStatus::Succeeded);
    assert_eq!(removed.status, BookmarkWritebackStatus::Succeeded);
    assert_eq!(gateway.add_requests().len(), 1);
    assert_eq!(gateway.delete_requests(), vec![9001]);
    assert_eq!(command_count(&locked).await, 2);
}

#[tokio::test]
async fn remove_resolves_missing_local_bookmark_id_from_pixiv() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    PixivAccountService::new(locked.db.clone(), gateway.clone())
        .set_bookmark_writeback_enabled(account.id, true)
        .await
        .unwrap();
    WorkRepository::new(locked.db.clone())
        .create_metadata_only(1205, 77, "bookmark target")
        .await
        .unwrap();
    BookmarkRepository::new(locked.db.clone())
        .mark_added(account.id, 1205, None, PixivBookmarkVisibility::Public)
        .await
        .unwrap();
    let mut detail = work_detail(1205);
    detail.bookmarked_by_current_account = Some(true);
    detail.bookmark = Some(PixivBookmarkRef {
        bookmark_id: 9002,
        visibility: PixivBookmarkVisibility::Public,
    });
    gateway.set_work_detail(detail);

    let removed = BookmarkWritebackService::new(locked.db.clone(), gateway.clone())
        .remove(BookmarkWritebackRequest::remove(
            account.id,
            context(),
            1205,
        ))
        .await
        .unwrap();

    assert_eq!(removed.status, BookmarkWritebackStatus::Succeeded);
    assert_eq!(gateway.delete_requests(), vec![9002]);
    assert_eq!(
        BookmarkRepository::new(locked.db.clone())
            .active_bookmark_id(account.id, 1205)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn remove_reconciles_local_state_when_pixiv_bookmark_is_already_absent() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    PixivAccountService::new(locked.db.clone(), gateway.clone())
        .set_bookmark_writeback_enabled(account.id, true)
        .await
        .unwrap();
    WorkRepository::new(locked.db.clone())
        .create_metadata_only(1206, 77, "bookmark target")
        .await
        .unwrap();
    BookmarkRepository::new(locked.db.clone())
        .mark_added(
            account.id,
            1206,
            Some(9003),
            PixivBookmarkVisibility::Public,
        )
        .await
        .unwrap();
    gateway.fail_delete(PixivErrorClass::HiddenOrNotFound);

    let removed = BookmarkWritebackService::new(locked.db.clone(), gateway.clone())
        .remove(BookmarkWritebackRequest::remove(
            account.id,
            context(),
            1206,
        ))
        .await
        .unwrap();

    assert_eq!(removed.status, BookmarkWritebackStatus::Succeeded);
    assert_eq!(gateway.delete_requests(), vec![9003]);
    assert_eq!(
        BookmarkRepository::new(locked.db.clone())
            .active_bookmark_id(account.id, 1206)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn live_command_port_decrypts_the_saved_cookie_before_writeback() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let cipher = PixivCookieCipher::new(PixivCookieKeyringConfig::new(PixivCookieKeyConfig::new(
        "test", [9; 32],
    )))
    .unwrap();
    let account = LivePixivAccountCommandPort::new(
        locked.db.clone(),
        gateway.clone(),
        cipher.clone(),
        "PixivArchiveTest/1.0",
    )
    .update(UpdatePixivAccountRequest {
        cookie: "10001_session-value".to_owned(),
    })
    .await
    .unwrap();
    PixivAccountService::new(locked.db.clone(), gateway.clone())
        .set_bookmark_writeback_enabled(account.id, true)
        .await
        .unwrap();
    let commands = LiveBookmarkCommandPort::new(
        locked.db.clone(),
        gateway.clone(),
        PixivAccountContextFactory::new(locked.db.clone(), cipher),
    );

    let result = commands
        .add(BookmarkCommandRequest {
            account_id: account.id,
            target_pixiv_id: 1204,
            visibility: PixivBookmarkVisibility::Private,
            tags: Vec::new(),
        })
        .await
        .unwrap();

    assert_eq!(result.status, BookmarkWritebackStatus::Succeeded);
    assert_eq!(gateway.add_requests().len(), 1);
}

#[tokio::test]
async fn stale_page_cannot_write_back_to_the_previous_pixiv_account() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let cipher = PixivCookieCipher::new(PixivCookieKeyringConfig::new(PixivCookieKeyConfig::new(
        "test", [9; 32],
    )))
    .unwrap();
    let accounts = LivePixivAccountCommandPort::new(
        locked.db.clone(),
        gateway.clone(),
        cipher.clone(),
        "PixivArchiveTest/1.0",
    );
    let account_a = accounts
        .update(UpdatePixivAccountRequest {
            cookie: "10001_account-a".to_owned(),
        })
        .await
        .unwrap();
    PixivAccountService::new(locked.db.clone(), gateway.clone())
        .set_bookmark_writeback_enabled(account_a.id, true)
        .await
        .unwrap();
    let account_b = accounts
        .update(UpdatePixivAccountRequest {
            cookie: "20002_account-b".to_owned(),
        })
        .await
        .unwrap();

    let result = LiveBookmarkCommandPort::new(
        locked.db.clone(),
        gateway.clone(),
        PixivAccountContextFactory::new(locked.db.clone(), cipher),
    )
    .add(BookmarkCommandRequest {
        account_id: account_a.id,
        target_pixiv_id: 1207,
        visibility: PixivBookmarkVisibility::Private,
        tags: Vec::new(),
    })
    .await;

    assert_ne!(account_a.id, account_b.id);
    assert!(matches!(
        result,
        Err(BookmarkWritebackError::Storage(DbError::RevisionConflict))
    ));
    assert!(gateway.add_request_user_ids().is_empty());
}

#[tokio::test]
async fn writeback_service_rejects_a_context_from_another_pixiv_account() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    PixivAccountService::new(locked.db.clone(), gateway.clone())
        .set_bookmark_writeback_enabled(account.id, true)
        .await
        .unwrap();

    let result = BookmarkWritebackService::new(locked.db.clone(), gateway.clone())
        .add(BookmarkWritebackRequest::add(
            account.id,
            pixivarchive_pixiv::PixivRequestContext::new(
                secrecy::SecretString::from("PHPSESSID=20002_account-b"),
                20_002,
                "PixivArchiveTest/1.0",
            ),
            1208,
            PixivBookmarkVisibility::Private,
            Vec::new(),
        ))
        .await;

    assert!(result.is_err());
    assert!(gateway.add_requests().is_empty());
}

#[tokio::test]
async fn writeback_failure_is_recorded_without_failing_archival_work() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    PixivAccountService::new(locked.db.clone(), gateway.clone())
        .set_bookmark_writeback_enabled(account.id, true)
        .await
        .unwrap();
    gateway.fail_add(PixivErrorClass::CsrfFailed);

    let writeback = BookmarkWritebackService::new(locked.db.clone(), gateway.clone())
        .add(BookmarkWritebackRequest::add(
            account.id,
            context(),
            1203,
            PixivBookmarkVisibility::Private,
            Vec::new(),
        ))
        .await
        .unwrap();
    let archived = ImportService::new(locked.db.clone(), gateway)
        .import(ImportRequest::work(account.id, context(), 1203).forced())
        .await
        .unwrap();

    assert_eq!(writeback.status, BookmarkWritebackStatus::Failed);
    assert_eq!(
        archived.status,
        pixivarchive_domain::subscription::ImportRunStatus::DownloadQueued
    );
    assert_eq!(failed_command_count(&locked).await, 1);
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

async fn command_count(locked: &LockedDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM bookmark_writeback_command")
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}

async fn failed_command_count(locked: &LockedDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM bookmark_writeback_command WHERE status = 'failed'")
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}
