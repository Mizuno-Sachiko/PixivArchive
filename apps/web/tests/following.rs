mod support;

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{Method, StatusCode},
};
use bytes::Bytes;
use futures_util::stream;
use pixivarchive_application::following::{
    ArtistFollowCommandError, ArtistFollowCommandPort, ArtistFollowStateView, FollowingAuthorView,
    FollowingAvatarError, FollowingAvatarPort, FollowingRefreshError, FollowingRefreshPort,
};
use pixivarchive_db::{
    FollowingAuthorSnapshot, FollowingRepository, SubscriptionRepository, SyncFollowingAuthors,
};
use pixivarchive_domain::pixiv::PixivFollowingVisibility;
use pixivarchive_pixiv::PixivMediaResponse;
use serde_json::json;
use std::sync::{Arc, Mutex};
use support::{TestApp, authenticated_get, login, response_json};
use time::macros::datetime;
use tower::ServiceExt;
use uuid::Uuid;

#[tokio::test]
async fn following_surface_controls_the_fixed_subscription_and_author_selection() {
    let test = TestApp::new(709020025).await;
    let auth = login(&test.app).await;
    let account_id = insert_pixiv_account(&test).await;
    let subscription = SubscriptionRepository::new(test.locked.db.clone())
        .ensure_following_subscription(account_id, datetime!(2026-07-31 12:00 UTC))
        .await
        .unwrap();
    FollowingRepository::new(test.locked.db.clone())
        .sync_authors(SyncFollowingAuthors {
            account_id,
            refreshed_at: datetime!(2026-07-31 12:30 UTC),
            authors: vec![
                FollowingAuthorSnapshot {
                    pixiv_artist_id: 70001,
                    display_name: "Following Artist".to_owned(),
                    avatar_url: Some("https://i.pximg.net/70001.jpg".to_owned()),
                    visibility: PixivFollowingVisibility::Public,
                },
                FollowingAuthorSnapshot {
                    pixiv_artist_id: 70002,
                    display_name: "Second Artist".to_owned(),
                    avatar_url: None,
                    visibility: PixivFollowingVisibility::Private,
                },
            ],
        })
        .await
        .unwrap();

    let response = test
        .app
        .clone()
        .oneshot(authenticated_get("/api/following", &auth))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let state = response_json(response).await;
    assert_eq!(state["subscription"]["id"], subscription.id.to_string());
    assert!(state["last_full_reconciled_at"].is_null());
    assert_eq!(state["authors"][0]["pixiv_artist_id"], 70001);
    assert_eq!(
        state["authors"][0]["avatar_url"],
        "/api/following/authors/70001/avatar"
    );
    assert!(state["authors"][0]["refreshed_at"].is_string());

    let author = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::PUT,
                "/api/following/authors/70001",
                &auth,
                Body::from(
                    json!({
                        "expected_account_id": account_id,
                        "enabled": false,
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(author.status(), StatusCode::OK);
    assert_eq!(response_json(author).await["enabled"], false);

    let authors = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::PUT,
                "/api/following/authors",
                &auth,
                Body::from(
                    json!({
                        "expected_account_id": account_id,
                        "pixiv_artist_ids": [70001, 70002],
                        "enabled": true,
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(authors.status(), StatusCode::OK);
    let authors = response_json(authors).await;
    assert_eq!(authors["authors"].as_array().unwrap().len(), 2);
    assert!(
        authors["authors"]
            .as_array()
            .unwrap()
            .iter()
            .all(|author| { author["enabled"] == true })
    );

    let disabled = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::PUT,
                "/api/following",
                &auth,
                Body::from(
                    json!({
                        "expected_account_id": account_id,
                        "enabled": false,
                        "interval_minutes": 30,
                        "expected_revision": subscription.revision,
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(disabled.status(), StatusCode::OK);
    let disabled = response_json(disabled).await;
    assert_eq!(disabled["enabled"], false);
    assert_eq!(disabled["schedule"]["interval_minutes"], 30);

    let run = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::POST,
                "/api/following/run",
                &auth,
                Body::from(
                    json!({
                        "expected_account_id": account_id,
                        "backfill": true,
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(run.status(), StatusCode::ACCEPTED);
    let run = response_json(run).await;
    assert!(run["job_id"].is_string());
    assert_eq!(run["trigger_kind"], "backfill");
}

#[tokio::test]
async fn generic_subscription_mutations_reject_the_fixed_following_subscription() {
    let test = TestApp::new(709020025).await;
    let auth = login(&test.app).await;
    let account_id = insert_pixiv_account(&test).await;
    let subscription = SubscriptionRepository::new(test.locked.db.clone())
        .ensure_following_subscription(account_id, datetime!(2026-07-31 12:00 UTC))
        .await
        .unwrap();

    let create = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::POST,
                "/api/subscriptions",
                &auth,
                Body::from(
                    json!({
                        "kind": "following",
                        "account_id": account_id,
                        "rule_id": null,
                        "name": "Another following",
                        "interval_minutes": 60,
                        "lookback_pages": 1,
                        "params": {"mode":"all","source":"following"},
                        "next_run_at": null,
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let update = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::PUT,
                &format!("/api/subscriptions/{}", subscription.id),
                &auth,
                Body::from(
                    json!({
                        "expected_revision": subscription.revision,
                        "enabled": false,
                        "account_id": account_id,
                        "rule_id": null,
                        "name": "Changed",
                        "interval_minutes": 120,
                        "lookback_pages": 2,
                        "params": {"mode":"all","source":"following"},
                        "next_run_at": null,
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(update.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let delete = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::DELETE,
            &format!(
                "/api/subscriptions/{}?expected_revision={}",
                subscription.id, subscription.revision
            ),
            &auth,
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn following_refresh_reports_unavailable_without_live_pixiv_commands() {
    let test = TestApp::new(709020025).await;
    let auth = login(&test.app).await;
    let account_id = Uuid::now_v7();

    let response = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            "/api/following/refresh",
            &auth,
            Body::from(json!({ "expected_account_id": account_id }).to_string()),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn artist_follow_api_returns_the_pixiv_verified_state() {
    let commands = FakeArtistFollowCommands::new(true);
    let test = TestApp::new_with_state(709020025, {
        let commands = commands.clone();
        move |state| state.with_artist_follow_commands(Arc::new(commands))
    })
    .await;
    let auth = login(&test.app).await;
    let account_id = Uuid::now_v7();

    let current = test
        .app
        .clone()
        .oneshot(authenticated_get(
            &format!("/api/following/authors/70001/pixiv?expected_account_id={account_id}"),
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(current.status(), StatusCode::OK);
    assert_eq!(response_json(current).await["followed"], true);

    let updated = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::PUT,
                "/api/following/authors/70001/pixiv",
                &auth,
                Body::from(
                    json!({
                        "expected_account_id": account_id,
                        "followed": false,
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(response_json(updated).await["followed"], false);
    assert_eq!(commands.updates(), vec![(account_id, 70001, false)]);
}

#[tokio::test]
async fn avatar_cache_removes_the_previous_source_for_the_same_author() {
    let avatars = FakeFollowingAvatars;
    let test = TestApp::new_with_state(709020025, move |state| {
        state.with_following_avatars(Arc::new(avatars))
    })
    .await;
    let auth = login(&test.app).await;
    let account_id = insert_pixiv_account(&test).await;
    SubscriptionRepository::new(test.locked.db.clone())
        .ensure_following_subscription(account_id, datetime!(2026-08-01 11:00 UTC))
        .await
        .unwrap();
    sync_following_author(
        &test,
        account_id,
        70001,
        Some("https://i.pximg.net/user-profile/img/one.jpg"),
    )
    .await;

    let first = test
        .app
        .clone()
        .oneshot(authenticated_get(
            "/api/following/authors/70001/avatar",
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(avatar_cache_file_count(&test).await, 1);
    assert!(!test.files.media_root.join("avatars").exists());

    sync_following_author(
        &test,
        account_id,
        70001,
        Some("https://i.pximg.net/user-profile/img/two.jpg?ts=2"),
    )
    .await;
    let second = test
        .app
        .clone()
        .oneshot(authenticated_get(
            "/api/following/authors/70001/avatar",
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::OK);

    assert_eq!(avatar_cache_file_count(&test).await, 1);
}

#[tokio::test]
async fn successful_unfollow_removes_the_author_avatar_cache() {
    let avatars = FakeFollowingAvatars;
    let commands = FakeArtistFollowCommands::new(true);
    let test = TestApp::new_with_state(709020025, {
        let commands = commands.clone();
        move |state| {
            state
                .with_following_avatars(Arc::new(avatars))
                .with_artist_follow_commands(Arc::new(commands))
        }
    })
    .await;
    let auth = login(&test.app).await;
    let account_id = insert_pixiv_account(&test).await;
    SubscriptionRepository::new(test.locked.db.clone())
        .ensure_following_subscription(account_id, datetime!(2026-07-31 12:00 UTC))
        .await
        .unwrap();
    sync_following_author(
        &test,
        account_id,
        70001,
        Some("https://i.pximg.net/user-profile/img/artist.jpg"),
    )
    .await;

    let avatar = test
        .app
        .clone()
        .oneshot(authenticated_get(
            "/api/following/authors/70001/avatar",
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(avatar.status(), StatusCode::OK);
    assert_eq!(avatar_cache_file_count(&test).await, 1);

    let updated = test
        .app
        .clone()
        .oneshot(
            test.mutating_request(
                Method::PUT,
                "/api/following/authors/70001/pixiv",
                &auth,
                Body::from(
                    json!({
                        "expected_account_id": account_id,
                        "followed": false,
                    })
                    .to_string(),
                ),
            ),
        )
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);

    assert_eq!(avatar_cache_file_count(&test).await, 0);
}

#[tokio::test]
async fn following_refresh_removes_avatar_cache_for_removed_authors() {
    let avatars = FakeFollowingAvatars;
    let refresh = FakeFollowingRefresh::new(vec![following_author_view(
        70002,
        Some("https://i.pximg.net/user-profile/img/kept.jpg"),
    )]);
    let test = TestApp::new_with_state(709020025, move |state| {
        state
            .with_following_avatars(Arc::new(avatars))
            .with_following_refresh(Arc::new(refresh))
    })
    .await;
    let auth = login(&test.app).await;
    let account_id = insert_pixiv_account(&test).await;
    SubscriptionRepository::new(test.locked.db.clone())
        .ensure_following_subscription(account_id, datetime!(2026-07-31 12:00 UTC))
        .await
        .unwrap();
    sync_following_author(
        &test,
        account_id,
        70001,
        Some("https://i.pximg.net/user-profile/img/removed.jpg"),
    )
    .await;

    let avatar = test
        .app
        .clone()
        .oneshot(authenticated_get(
            "/api/following/authors/70001/avatar",
            &auth,
        ))
        .await
        .unwrap();
    assert_eq!(avatar.status(), StatusCode::OK);
    assert_eq!(avatar_cache_file_count(&test).await, 1);

    FollowingRepository::new(test.locked.db.clone())
        .sync_authors(SyncFollowingAuthors {
            account_id,
            refreshed_at: datetime!(2026-08-01 12:00 UTC),
            authors: vec![FollowingAuthorSnapshot {
                pixiv_artist_id: 70002,
                display_name: "Kept Artist".to_owned(),
                avatar_url: Some("https://i.pximg.net/user-profile/img/kept.jpg".to_owned()),
                visibility: PixivFollowingVisibility::Public,
            }],
        })
        .await
        .unwrap();

    let refreshed = test
        .app
        .clone()
        .oneshot(test.mutating_request(
            Method::POST,
            "/api/following/refresh",
            &auth,
            Body::from(json!({ "expected_account_id": account_id }).to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(refreshed.status(), StatusCode::OK);

    assert_eq!(avatar_cache_file_count(&test).await, 0);
}

#[derive(Clone)]
struct FakeArtistFollowCommands {
    state: Arc<Mutex<FakeArtistFollowState>>,
}

struct FakeArtistFollowState {
    followed: bool,
    updates: Vec<(Uuid, i64, bool)>,
}

impl FakeArtistFollowCommands {
    fn new(followed: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeArtistFollowState {
                followed,
                updates: Vec::new(),
            })),
        }
    }

    fn updates(&self) -> Vec<(Uuid, i64, bool)> {
        self.state.lock().unwrap().updates.clone()
    }
}

#[async_trait]
impl ArtistFollowCommandPort for FakeArtistFollowCommands {
    async fn status(
        &self,
        _expected_account_id: Uuid,
        pixiv_artist_id: i64,
    ) -> Result<ArtistFollowStateView, ArtistFollowCommandError> {
        Ok(ArtistFollowStateView {
            pixiv_artist_id,
            followed: self.state.lock().unwrap().followed,
        })
    }

    async fn set_followed(
        &self,
        expected_account_id: Uuid,
        pixiv_artist_id: i64,
        followed: bool,
    ) -> Result<ArtistFollowStateView, ArtistFollowCommandError> {
        let mut state = self.state.lock().unwrap();
        state.followed = followed;
        state
            .updates
            .push((expected_account_id, pixiv_artist_id, followed));
        Ok(ArtistFollowStateView {
            pixiv_artist_id,
            followed,
        })
    }
}

#[derive(Clone)]
struct FakeFollowingAvatars;

#[async_trait]
impl FollowingAvatarPort for FakeFollowingAvatars {
    async fn fetch(&self, source: String) -> Result<PixivMediaResponse, FollowingAvatarError> {
        let body = Bytes::from(format!("avatar:{source}"));
        Ok(PixivMediaResponse {
            content_length: Some(body.len() as u64),
            content_type: Some("image/jpeg".to_owned()),
            body: Box::pin(stream::once(async move { Ok(body) })),
        })
    }
}

#[derive(Clone)]
struct FakeFollowingRefresh {
    authors: Arc<Vec<FollowingAuthorView>>,
}

impl FakeFollowingRefresh {
    fn new(authors: Vec<FollowingAuthorView>) -> Self {
        Self {
            authors: Arc::new(authors),
        }
    }
}

#[async_trait]
impl FollowingRefreshPort for FakeFollowingRefresh {
    async fn refresh(
        &self,
        _expected_account_id: Uuid,
    ) -> Result<Vec<FollowingAuthorView>, FollowingRefreshError> {
        Ok((*self.authors).clone())
    }
}

async fn insert_pixiv_account(test: &TestApp) -> Uuid {
    let account_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO pixiv_account (
            id, pixiv_user_id, display_name, state, cookie_key_id,
            cookie_nonce, cookie_ciphertext, user_agent, is_current
        )
        VALUES ($1, 90001, 'test', 'normal', 'test', $2, $3, 'test-agent', true)
        "#,
    )
    .bind(account_id)
    .bind(vec![1_u8; 12])
    .bind(vec![2_u8; 32])
    .execute(test.locked.db.pool())
    .await
    .unwrap();
    account_id
}

async fn sync_following_author(
    test: &TestApp,
    account_id: Uuid,
    pixiv_artist_id: i64,
    avatar_url: Option<&str>,
) {
    FollowingRepository::new(test.locked.db.clone())
        .sync_authors(SyncFollowingAuthors {
            account_id,
            refreshed_at: datetime!(2026-07-31 12:30 UTC),
            authors: vec![FollowingAuthorSnapshot {
                pixiv_artist_id,
                display_name: format!("Artist {pixiv_artist_id}"),
                avatar_url: avatar_url.map(str::to_owned),
                visibility: PixivFollowingVisibility::Public,
            }],
        })
        .await
        .unwrap();
}

fn following_author_view(pixiv_artist_id: i64, avatar_url: Option<&str>) -> FollowingAuthorView {
    FollowingAuthorView {
        pixiv_artist_id,
        display_name: format!("Artist {pixiv_artist_id}"),
        avatar_url: avatar_url.map(str::to_owned),
        visibility: PixivFollowingVisibility::Public,
        enabled: true,
        refreshed_at: datetime!(2026-08-01 12:00 UTC),
        last_collected_at: None,
    }
}

async fn avatar_cache_file_count(test: &TestApp) -> usize {
    let directory = test.files.cache_root.join("avatars");
    let Ok(mut entries) = tokio::fs::read_dir(directory).await else {
        return 0;
    };
    let mut count = 0;
    while let Some(entry) = entries.next_entry().await.unwrap() {
        if entry.file_type().await.unwrap().is_file() {
            count += 1;
        }
    }
    count
}
