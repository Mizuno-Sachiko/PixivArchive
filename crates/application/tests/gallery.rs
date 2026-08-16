use pixivarchive_application::{gallery::GalleryService, pixiv_accounts::PixivAccountAdminService};
use pixivarchive_db::{
    BookmarkRepository, PixivAccountRepository, SavePixivAccount, WorkRepository,
};
use pixivarchive_domain::{pixiv::PixivBookmarkVisibility, subscription::PixivAccountState};
use pixivarchive_test_support as support;

#[tokio::test]
async fn gallery_service_exposes_context_details() {
    let locked = support::LockedDb::new(709020014).await;
    let works = WorkRepository::new(locked.db.clone());
    let work = works
        .create_metadata_only(9_302, 402, "context target")
        .await
        .unwrap();

    let gallery = GalleryService::new(locked.db.clone());
    let detail = gallery.work_detail(work.id).await.unwrap();
    assert_eq!(detail.work.id, work.id);
    let artist = gallery
        .artist_detail(detail.work.pixiv_artist_id)
        .await
        .unwrap();
    assert_eq!(artist.pixiv_artist_id, 402);
    assert_eq!(artist.work_count, 0);
    let revisions = gallery.revisions(work.id).await.unwrap();
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0].title, "context target");
}

#[tokio::test]
async fn gallery_projects_bookmarks_only_for_available_current_accounts() {
    let locked = support::LockedDb::new(709020014).await;
    let work = WorkRepository::new(locked.db.clone())
        .create_metadata_only(9_303, 403, "bookmark projection target")
        .await
        .unwrap();
    let accounts = PixivAccountRepository::new(locked.db.clone());
    let saved = accounts
        .save_validating(SavePixivAccount {
            pixiv_user_id: 10_001,
            display_name: "Test Artist".to_owned(),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![0; 12],
            cookie_ciphertext: vec![1],
            user_agent: "PixivArchiveTest/1.0".to_owned(),
        })
        .await
        .unwrap();
    let normal = accounts
        .set_state(saved.id, PixivAccountState::Normal, None)
        .await
        .unwrap();
    let normal = accounts.activate(normal.id).await.unwrap();
    BookmarkRepository::new(locked.db.clone())
        .mark_added(
            normal.id,
            work.pixiv_id,
            Some(7_001),
            PixivBookmarkVisibility::Public,
        )
        .await
        .unwrap();
    let gallery = GalleryService::new(locked.db.clone());

    let detail = gallery.work_detail(work.id).await.unwrap();
    assert!(detail.work.bookmarked_by_current_account);
    assert_eq!(detail.work.bookmark_id, Some(7_001));

    accounts
        .set_state(normal.id, PixivAccountState::Restricted, None)
        .await
        .unwrap();
    assert!(
        gallery
            .work_detail(work.id)
            .await
            .unwrap()
            .work
            .bookmarked_by_current_account
    );

    accounts
        .set_state(normal.id, PixivAccountState::Validating, None)
        .await
        .unwrap();
    let validating_detail = gallery.work_detail(work.id).await.unwrap();
    assert!(!validating_detail.work.bookmarked_by_current_account);
    assert_eq!(validating_detail.work.bookmark_id, None);

    accounts
        .set_state(normal.id, PixivAccountState::CredentialInvalid, None)
        .await
        .unwrap();
    assert!(
        !gallery
            .work_detail(work.id)
            .await
            .unwrap()
            .work
            .bookmarked_by_current_account
    );

    let normal = accounts
        .set_state(normal.id, PixivAccountState::Normal, None)
        .await
        .unwrap();
    PixivAccountAdminService::new(locked.db.clone())
        .clear_credential(normal.id, normal.revision)
        .await
        .unwrap();
    let cleared_detail = gallery.work_detail(work.id).await.unwrap();
    assert!(!cleared_detail.work.bookmarked_by_current_account);
    assert_eq!(cleared_detail.work.bookmark_id, None);
}
