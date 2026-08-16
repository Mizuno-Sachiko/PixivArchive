mod support;

use pixivarchive_db::{
    BookmarkRepository, EventRepository, PixivAccountRepository, PixivBookmarkSyncEntry,
    SavePixivAccount, WorkRepository,
};
use pixivarchive_domain::{
    event::{EventPayload, EventResource},
    pixiv::PixivBookmarkVisibility,
};
use time::macros::datetime;
use uuid::Uuid;

#[tokio::test]
async fn bookmark_projection_changes_emit_one_account_scoped_event() {
    let locked = support::LockedDb::new().await;
    let db = locked.db.clone();
    let account_id = account(&locked).await;
    let works = WorkRepository::new(db.clone());
    works
        .create_metadata_only(910_001, 710_001, "first bookmark target")
        .await
        .unwrap();
    works
        .create_metadata_only(910_002, 710_002, "second bookmark target")
        .await
        .unwrap();
    let bookmarks = BookmarkRepository::new(db.clone());

    clear_events(&locked).await;
    bookmarks
        .mark_added(
            account_id,
            910_001,
            Some(810_001),
            PixivBookmarkVisibility::Public,
        )
        .await
        .unwrap();
    assert_bookmark_event(&locked, account_id, Some(1)).await;

    clear_events(&locked).await;
    bookmarks
        .mark_removed_by_work(account_id, 910_001)
        .await
        .unwrap();
    assert_bookmark_event(&locked, account_id, Some(2)).await;

    let entries = [
        PixivBookmarkSyncEntry {
            pixiv_work_id: 910_001,
            visibility: PixivBookmarkVisibility::Public,
        },
        PixivBookmarkSyncEntry {
            pixiv_work_id: 910_002,
            visibility: PixivBookmarkVisibility::Private,
        },
    ];
    clear_events(&locked).await;
    bookmarks
        .reconcile_full(account_id, &entries, datetime!(2026-08-10 12:00 UTC))
        .await
        .unwrap();
    assert_bookmark_event(&locked, account_id, Some(3)).await;

    clear_events(&locked).await;
    bookmarks
        .reconcile_full(account_id, &entries, datetime!(2026-08-10 13:00 UTC))
        .await
        .unwrap();
    assert_bookmark_event(&locked, account_id, None).await;

    clear_events(&locked).await;
    bookmarks
        .reconcile_full(
            account_id,
            &[PixivBookmarkSyncEntry {
                pixiv_work_id: 910_002,
                visibility: PixivBookmarkVisibility::Public,
            }],
            datetime!(2026-08-10 14:00 UTC),
        )
        .await
        .unwrap();
    assert_bookmark_event(&locked, account_id, Some(4)).await;
}

async fn account(locked: &support::LockedDb) -> Uuid {
    PixivAccountRepository::new(locked.db.clone())
        .save_validating(SavePixivAccount {
            pixiv_user_id: 10_001,
            display_name: "Test Artist".to_owned(),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![0; 12],
            cookie_ciphertext: vec![1],
            user_agent: "PixivArchive tests".to_owned(),
        })
        .await
        .unwrap()
        .id
}

async fn clear_events(locked: &support::LockedDb) {
    sqlx::query("DELETE FROM app_event")
        .execute(locked.db.pool())
        .await
        .unwrap();
}

async fn assert_bookmark_event(
    locked: &support::LockedDb,
    account_id: Uuid,
    expected_revision: Option<i64>,
) {
    let actual = EventRepository::new(locked.db.clone())
        .list_after(0, 20)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|event| {
            if event.resource != EventResource::PixivBookmark || event.resource_id != account_id {
                return None;
            }
            match event.payload {
                EventPayload::PixivBookmarkChanged { revision } => Some(revision),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected_revision.into_iter().collect::<Vec<_>>());
}
