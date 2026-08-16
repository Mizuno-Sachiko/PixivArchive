mod support;

use pixivarchive_db::{
    CreateSubscription, DbError, FollowingAuthorSnapshot, FollowingRepository,
    PixivAccountRepository, SavePixivAccount, SubscriptionRepository, SyncFollowingAuthors,
};
use pixivarchive_domain::{pixiv::PixivFollowingVisibility, subscription::SubscriptionKind};
use serde_json::json;
use time::macros::datetime;

#[tokio::test]
async fn sync_replaces_remote_snapshot_and_preserves_existing_exclusions() {
    let locked = support::LockedDb::new().await;
    let account_id = account(&locked).await;
    let repository = FollowingRepository::new(locked.db.clone());
    let first_refresh = datetime!(2026-07-31 12:00 UTC);

    repository
        .sync_authors(SyncFollowingAuthors {
            account_id,
            refreshed_at: first_refresh,
            authors: vec![
                author(101, "Alpha", PixivFollowingVisibility::Public),
                author(102, "Beta", PixivFollowingVisibility::Private),
            ],
        })
        .await
        .unwrap();
    repository
        .set_enabled(account_id, 102, false)
        .await
        .unwrap();

    let second_refresh = datetime!(2026-07-31 13:00 UTC);
    repository
        .sync_authors(SyncFollowingAuthors {
            account_id,
            refreshed_at: second_refresh,
            authors: vec![
                FollowingAuthorSnapshot {
                    pixiv_artist_id: 102,
                    display_name: "Beta Updated".to_owned(),
                    avatar_url: Some("https://i.pximg.net/102-new.jpg".to_owned()),
                    visibility: PixivFollowingVisibility::Public,
                },
                author(103, "Gamma", PixivFollowingVisibility::Private),
            ],
        })
        .await
        .unwrap();

    let authors = repository.list(account_id).await.unwrap();
    assert_eq!(authors.len(), 2);
    assert_eq!(authors[0].pixiv_artist_id, 102);
    assert_eq!(authors[0].display_name, "Beta Updated");
    assert_eq!(authors[0].visibility, PixivFollowingVisibility::Public);
    assert!(!authors[0].enabled);
    assert_eq!(authors[0].refreshed_at, second_refresh);
    assert_eq!(authors[1].pixiv_artist_id, 103);
    assert!(authors[1].enabled);

    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM pixiv_following_author_exclusion WHERE pixiv_account_id = $1 AND pixiv_artist_id = 101)",
    )
    .bind(account_id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap());
}

#[tokio::test]
async fn enabled_ids_and_collection_timestamp_exclude_unchecked_authors() {
    let locked = support::LockedDb::new().await;
    let account_id = account(&locked).await;
    let repository = FollowingRepository::new(locked.db.clone());
    repository
        .sync_authors(SyncFollowingAuthors {
            account_id,
            refreshed_at: datetime!(2026-07-31 12:00 UTC),
            authors: vec![
                author(201, "Enabled", PixivFollowingVisibility::Public),
                author(202, "Disabled", PixivFollowingVisibility::Private),
            ],
        })
        .await
        .unwrap();
    repository
        .set_enabled(account_id, 202, false)
        .await
        .unwrap();

    assert_eq!(
        repository.enabled_artist_ids(account_id).await.unwrap(),
        vec![201]
    );

    let collected_at = datetime!(2026-07-31 14:30 UTC);
    repository
        .mark_enabled_collected(account_id, collected_at)
        .await
        .unwrap();
    let authors = repository.list(account_id).await.unwrap();
    assert_eq!(authors[0].last_collected_at, Some(collected_at));
    assert_eq!(authors[1].last_collected_at, None);

    repository.set_enabled(account_id, 202, true).await.unwrap();
    repository.set_enabled(account_id, 202, true).await.unwrap();
    assert_eq!(
        repository.enabled_artist_ids(account_id).await.unwrap(),
        vec![201, 202]
    );
}

#[tokio::test]
async fn batch_collection_updates_are_atomic_when_any_author_is_unknown() {
    let locked = support::LockedDb::new().await;
    let account_id = account(&locked).await;
    let repository = FollowingRepository::new(locked.db.clone());
    repository
        .sync_authors(SyncFollowingAuthors {
            account_id,
            refreshed_at: datetime!(2026-07-31 12:00 UTC),
            authors: vec![
                author(301, "Alpha", PixivFollowingVisibility::Public),
                author(302, "Beta", PixivFollowingVisibility::Private),
                author(303, "Gamma", PixivFollowingVisibility::Public),
            ],
        })
        .await
        .unwrap();

    let updated = repository
        .set_enabled_many(account_id, &[301, 302], false)
        .await
        .unwrap();
    assert_eq!(updated, 2);
    assert_eq!(
        repository
            .list(account_id)
            .await
            .unwrap()
            .into_iter()
            .map(|author| (author.pixiv_artist_id, author.enabled))
            .collect::<Vec<_>>(),
        vec![(301, false), (302, false), (303, true)]
    );

    let result = repository
        .set_enabled_many(account_id, &[301, 99_999], true)
        .await;
    assert!(matches!(result, Err(DbError::NotFound)));
    assert!(!repository.get(account_id, 301).await.unwrap().enabled);
}

#[tokio::test]
async fn following_subscription_is_unique_per_account() {
    let locked = support::LockedDb::new().await;
    let account_id = account(&locked).await;
    let repository = SubscriptionRepository::new(locked.db.clone());

    repository
        .create_subscription(following_subscription(account_id, "Following"))
        .await
        .unwrap();
    let duplicate = repository
        .create_subscription(following_subscription(account_id, "Duplicate"))
        .await;

    assert!(matches!(duplicate, Err(DbError::Constraint(_))));
}

fn author(
    pixiv_artist_id: i64,
    display_name: &str,
    visibility: PixivFollowingVisibility,
) -> FollowingAuthorSnapshot {
    FollowingAuthorSnapshot {
        pixiv_artist_id,
        display_name: display_name.to_owned(),
        avatar_url: Some(format!("https://i.pximg.net/{pixiv_artist_id}.jpg")),
        visibility,
    }
}

fn following_subscription(account_id: uuid::Uuid, name: &str) -> CreateSubscription {
    CreateSubscription {
        pixiv_account_id: account_id,
        rule_id: None,
        name: name.to_owned(),
        kind: SubscriptionKind::Following,
        interval_minutes: 60,
        lookback_pages: 1,
        params: json!({ "mode": "all", "source": "following", "language": "zh" }),
        next_run_at: None,
    }
}

async fn account(locked: &support::LockedDb) -> uuid::Uuid {
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
