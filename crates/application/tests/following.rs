use pixivarchive_application::{
    following::{ArtistFollowCommandPort, FollowingService, LiveArtistFollowCommandPort},
    pixiv_accounts::{
        AccountCookieUpdate, LivePixivAccountCommandPort, PixivAccountCommandPort,
        PixivAccountContextFactory, PixivAccountService, PixivCookieCipher, PixivCookieKeyConfig,
        PixivCookieKeyringConfig, UpdatePixivAccountRequest,
    },
};
use pixivarchive_db::{DbError, FollowingAuthorSnapshot, FollowingRepository};
use pixivarchive_domain::pixiv::{
    PixivArtistFollowState, PixivDiscoveryWork, PixivFollowedArtist, PixivFollowingVisibility,
    PixivWorkPages,
};
use pixivarchive_test_support as support;

use support::{FakePixivGateway, LockedDb, context, work_detail, work_page};
use time::OffsetDateTime;

#[tokio::test]
async fn refresh_reads_every_public_and_private_following_page() {
    let locked = LockedDb::new(709020015).await;
    let gateway = FakePixivGateway::new();
    let public = (1..=101).map(followed_artist).collect();
    let private = vec![followed_artist(201)];
    gateway.set_following_authors(public, private);
    let account = account(&locked, gateway.clone()).await;
    let service = FollowingService::new(locked.db.clone(), gateway.clone());

    let authors = service.refresh(account.id, &context()).await.unwrap();

    assert_eq!(authors.len(), 102);
    assert_eq!(authors[0].visibility, PixivFollowingVisibility::Public);
    assert_eq!(authors[100].pixiv_artist_id, 101);
    assert_eq!(authors[101].visibility, PixivFollowingVisibility::Private);
    let requests = gateway.following_requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].offset, 0);
    assert_eq!(requests[1].offset, 100);
    assert_eq!(requests[2].visibility, PixivFollowingVisibility::Private);
}

#[tokio::test]
async fn following_run_skips_unchecked_authors_and_forces_the_rest_to_download() {
    let locked = LockedDb::new(709020015).await;
    let gateway = FakePixivGateway::new();
    gateway.set_following_authors(vec![followed_artist(301), followed_artist(302)], Vec::new());
    gateway.set_follow_items(vec![
        discovery_work_by(3_001, 301),
        discovery_work_by(3_002, 302),
    ]);
    gateway.set_work_detail(work_detail(3_002));
    gateway.set_work_pages(PixivWorkPages {
        work_id: 3_002,
        pages: vec![work_page(3_002, 0)],
    });
    let account = account(&locked, gateway.clone()).await;
    let following = FollowingService::new(locked.db.clone(), gateway.clone());
    following.refresh(account.id, &context()).await.unwrap();
    following.set_enabled(account.id, 301, false).await.unwrap();
    let subscription = following.ensure_subscription(account.id).await.unwrap();
    let run = pixivarchive_application::subscriptions::SubscriptionService::new(locked.db.clone())
        .start_manual_run(subscription.id, false)
        .await
        .unwrap();

    pixivarchive_application::subscriptions::SubscriptionExecutionService::new(
        locked.db.clone(),
        gateway.clone(),
    )
    .execute(
        pixivarchive_application::subscriptions::SubscriptionRunRequest {
            context: context(),
            subscription_run_id: run.run_id,
        },
    )
    .await
    .unwrap();

    assert_eq!(gateway.work_detail_calls(), 1);
    let stored_work_id: i64 = sqlx::query_scalar("SELECT pixiv_work_id FROM work")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(stored_work_id, 3_002);
    let priorities: Vec<String> = sqlx::query_scalar(
        "SELECT priority_class FROM job WHERE kind = 'download_media' ORDER BY created_at",
    )
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(priorities, vec!["scheduled_collection"]);
    let authors = following.list(account.id).await.unwrap();
    assert_eq!(authors[0].last_collected_at, None);
    assert!(authors[1].last_collected_at.is_some());
}

#[tokio::test]
async fn artist_follow_commands_verify_pixiv_and_update_the_subscription_author() {
    let locked = LockedDb::new(709020015).await;
    let gateway = FakePixivGateway::new();
    gateway.set_artist_follow_state(PixivArtistFollowState {
        artist_id: 70001,
        name: "Artist Alpha".to_owned(),
        profile_image_url: Some("https://i.pximg.net/70001.jpg".to_owned()),
        followed: false,
    });
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
    let commands = LiveArtistFollowCommandPort::new(
        locked.db.clone(),
        gateway.clone(),
        PixivAccountContextFactory::new(locked.db.clone(), cipher),
    );

    assert!(!commands.status(account.id, 70001).await.unwrap().followed);
    assert!(
        commands
            .set_followed(account.id, 70001, true)
            .await
            .unwrap()
            .followed
    );
    assert_eq!(
        gateway.artist_follow_add_requests()[0].visibility,
        PixivFollowingVisibility::Public
    );
    let author = FollowingRepository::new(locked.db.clone())
        .get(account.id, 70001)
        .await
        .unwrap();
    assert_eq!(author.display_name, "Artist Alpha");
    assert!(author.enabled);

    assert!(
        !commands
            .set_followed(account.id, 70001, false)
            .await
            .unwrap()
            .followed
    );
    assert_eq!(gateway.artist_follow_remove_requests(), vec![70001]);
    assert!(matches!(
        FollowingRepository::new(locked.db.clone())
            .get(account.id, 70001)
            .await,
        Err(DbError::NotFound)
    ));
}

#[tokio::test]
async fn artist_follow_command_restores_local_author_when_pixiv_already_follows() {
    let locked = LockedDb::new(709020015).await;
    let gateway = FakePixivGateway::new();
    gateway.set_artist_follow_state(PixivArtistFollowState {
        artist_id: 70002,
        name: "Artist Beta".to_owned(),
        profile_image_url: Some("https://i.pximg.net/70002.jpg".to_owned()),
        followed: true,
    });
    let (account, commands) = command_account(&locked, gateway.clone()).await;

    assert!(
        commands
            .set_followed(account.id, 70002, true)
            .await
            .unwrap()
            .followed
    );

    assert!(gateway.artist_follow_add_requests().is_empty());
    let author = FollowingRepository::new(locked.db.clone())
        .get(account.id, 70002)
        .await
        .unwrap();
    assert_eq!(author.display_name, "Artist Beta");
    assert!(author.enabled);
}

#[tokio::test]
async fn artist_follow_command_removes_local_author_when_pixiv_already_unfollows() {
    let locked = LockedDb::new(709020015).await;
    let gateway = FakePixivGateway::new();
    gateway.set_artist_follow_state(PixivArtistFollowState {
        artist_id: 70003,
        name: "Artist Gamma".to_owned(),
        profile_image_url: Some("https://i.pximg.net/70003.jpg".to_owned()),
        followed: false,
    });
    let (account, commands) = command_account(&locked, gateway.clone()).await;
    FollowingRepository::new(locked.db.clone())
        .upsert_author(
            account.id,
            OffsetDateTime::now_utc(),
            FollowingAuthorSnapshot {
                pixiv_artist_id: 70003,
                display_name: "Stale Artist Gamma".to_owned(),
                avatar_url: None,
                visibility: PixivFollowingVisibility::Public,
            },
        )
        .await
        .unwrap();

    assert!(
        !commands
            .set_followed(account.id, 70003, false)
            .await
            .unwrap()
            .followed
    );

    assert!(gateway.artist_follow_remove_requests().is_empty());
    assert!(matches!(
        FollowingRepository::new(locked.db.clone())
            .get(account.id, 70003)
            .await,
        Err(DbError::NotFound)
    ));
}

fn followed_artist(pixiv_id: i64) -> PixivFollowedArtist {
    PixivFollowedArtist {
        pixiv_id,
        name: format!("Artist {pixiv_id}"),
        profile_image_url: Some(format!("https://i.pximg.net/{pixiv_id}.jpg")),
    }
}

fn discovery_work_by(work_id: i64, artist_id: i64) -> PixivDiscoveryWork {
    let mut work = support::discovery_work(work_id);
    work.artist.pixiv_id = artist_id;
    work
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

async fn command_account(
    locked: &LockedDb,
    gateway: FakePixivGateway,
) -> (
    pixivarchive_application::pixiv_accounts::PixivAccount,
    LiveArtistFollowCommandPort<FakePixivGateway>,
) {
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
    let commands = LiveArtistFollowCommandPort::new(
        locked.db.clone(),
        gateway,
        PixivAccountContextFactory::new(locked.db.clone(), cipher),
    );
    (account, commands)
}
