#![allow(dead_code)]

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};
use pixivarchive_db::{Db, SubscriptionRecord, SubscriptionRepository, UpdateSubscription};
use pixivarchive_domain::pixiv::{
    PixivAccountValidation, PixivArtistFollowRequest, PixivArtistFollowState,
    PixivArtistFollowWriteResult, PixivArtistRef, PixivArtistWorkIds, PixivBookmarkAddRequest,
    PixivBookmarkWriteResult, PixivBookmarksCursor, PixivBookmarksMode, PixivBookmarksRequest,
    PixivDimensions, PixivDiscoveryWork, PixivFollowLatestCursor, PixivFollowLatestMode,
    PixivFollowLatestRequest, PixivFollowedArtist, PixivFollowingCursor, PixivFollowingRequest,
    PixivFollowingVisibility, PixivPage, PixivRankingContent, PixivRankingCursor,
    PixivRankingEntry, PixivRankingMode, PixivRankingPage, PixivRankingRequest, PixivUgoiraMeta,
    PixivWorkCounts, PixivWorkDetail, PixivWorkKind, PixivWorkPages,
};
use pixivarchive_pixiv::{
    AdapterResponse, PixivEndpoint, PixivError, PixivErrorClass, PixivGateway, PixivMediaGateway,
    PixivMediaResponse, PixivRequestContext, ResponseProvenance,
};
use secrecy::SecretString;
use serde_json::json;
use sqlx::{Connection, PgConnection};
use std::{
    collections::HashMap,
    io::Cursor,
    sync::{Arc, Mutex},
};
use time::Date;
use tokio::sync::Barrier;
use url::Url;

pub const DISCOVERY_LOCK_ID: i64 = 709020010;
pub const WORKER_LOCK_ID: i64 = 709020043;

pub struct LockedDb {
    pub db: Db,
    _lock: PgConnection,
}

impl LockedDb {
    pub async fn new(lock_id: i64) -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must point at the isolated test database");
        let mut lock = PgConnection::connect(&database_url).await.unwrap();
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(lock_id)
            .execute(&mut lock)
            .await
            .unwrap();

        let db = Db::connect(&database_url).await.unwrap();
        sqlx::migrate!("../../migrations")
            .run(db.pool())
            .await
            .unwrap();
        sqlx::query(
            r#"
            TRUNCATE TABLE
                worker_heartbeat,
                app_event,
                system_setting,
                media_artifact_intent,
                pixiv_bookmark_sync_state,
                pixiv_work_bookmark,
                bookmark_writeback_command,
                import_candidate,
                import_run,
                subscription_cursor,
                ranking_entry,
                subscription_run_unit,
                subscription_run,
                subscription,
                pixiv_following_author_exclusion,
                pixiv_following_author,
                pixiv_account,
                job,
                job_attempt,
                trash_entry,
                deletion_marker,
                work_tag,
                derivative,
                media_revision,
                work_page,
                work,
                work_revision,
                tag,
                series,
                artist,
                rule_draft,
                rule_version,
                download_rule,
                login_rate_limit_reservation,
                login_rate_limit,
                login_attempt,
                admin_session,
                administrator
            RESTART IDENTITY CASCADE
            "#,
        )
        .execute(db.pool())
        .await
        .unwrap();
        Self { db, _lock: lock }
    }
}

pub async fn configure_following_subscription(
    db: &Db,
    account_id: uuid::Uuid,
    mode: PixivFollowLatestMode,
    interval_minutes: i64,
    lookback_pages: i64,
) -> SubscriptionRecord {
    let repository = SubscriptionRepository::new(db.clone());
    let current = repository.following_subscription(account_id).await.unwrap();
    configure_fixed_subscription(
        &repository,
        current,
        interval_minutes,
        lookback_pages,
        json!({ "mode": mode, "source": "following", "language": "zh" }),
    )
    .await
}

pub async fn configure_bookmarks_subscription(
    db: &Db,
    account_id: uuid::Uuid,
    mode: PixivBookmarksMode,
    interval_minutes: i64,
    lookback_pages: i64,
) -> SubscriptionRecord {
    let repository = SubscriptionRepository::new(db.clone());
    let current = repository.bookmarks_subscription(account_id).await.unwrap();
    configure_fixed_subscription(
        &repository,
        current,
        interval_minutes,
        lookback_pages,
        json!({
            "mode": mode,
            "visibility": "all",
            "full_reconcile_hours": 24,
        }),
    )
    .await
}

async fn configure_fixed_subscription(
    repository: &SubscriptionRepository,
    current: SubscriptionRecord,
    interval_minutes: i64,
    lookback_pages: i64,
    params: serde_json::Value,
) -> SubscriptionRecord {
    repository
        .update_subscription(UpdateSubscription {
            id: current.id,
            expected_revision: current.revision,
            pixiv_account_id: current.pixiv_account_id,
            rule_id: current.rule_id,
            name: current.name,
            enabled: true,
            interval_minutes,
            lookback_pages,
            params,
            next_run_at: current.next_run_at,
        })
        .await
        .unwrap()
}

#[derive(Clone, Default)]
pub struct FakePixivGateway {
    state: Arc<Mutex<FakePixivState>>,
}

#[derive(Default)]
struct FakePixivState {
    validation_error: Option<PixivError>,
    ranking_error: Option<PixivError>,
    bookmark_error: Option<PixivError>,
    work_detail_error: Option<PixivError>,
    work_detail_pause: Option<WorkDetailPause>,
    add_error: Option<PixivError>,
    delete_error: Option<PixivError>,
    validate_calls: usize,
    ranking_requests: Vec<PixivRankingRequest>,
    bookmark_requests: Vec<PixivBookmarksRequest>,
    follow_requests: Vec<PixivFollowLatestRequest>,
    following_requests: Vec<PixivFollowingRequest>,
    artist_follow_states: HashMap<i64, PixivArtistFollowState>,
    artist_follow_add_requests: Vec<PixivArtistFollowRequest>,
    artist_follow_remove_requests: Vec<i64>,
    add_requests: Vec<PixivBookmarkAddRequest>,
    add_request_user_ids: Vec<i64>,
    delete_requests: Vec<i64>,
    delete_request_user_ids: Vec<i64>,
    ranking_items: Vec<PixivRankingEntry>,
    ranking_pages: Vec<Vec<PixivRankingEntry>>,
    ranking_date: Option<Date>,
    public_bookmark_pages: Vec<Vec<i64>>,
    private_bookmark_pages: Vec<Vec<i64>>,
    follow_items: Vec<PixivDiscoveryWork>,
    follow_pages: Option<Vec<Vec<PixivDiscoveryWork>>>,
    public_following_authors: Vec<PixivFollowedArtist>,
    private_following_authors: Vec<PixivFollowedArtist>,
    artist_work_ids: Vec<i64>,
    work_details: HashMap<i64, PixivWorkDetail>,
    work_pages: HashMap<i64, PixivWorkPages>,
    ugoira_meta: HashMap<i64, PixivUgoiraMeta>,
    work_detail_calls: usize,
    work_pages_calls: usize,
    media: HashMap<String, (Bytes, String)>,
    media_requests: Vec<String>,
}

#[derive(Clone)]
pub struct WorkDetailPause {
    pub entered: Arc<Barrier>,
    pub resume: Arc<Barrier>,
}

impl FakePixivGateway {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fail_validation(&self, class: PixivErrorClass) {
        self.state.lock().unwrap().validation_error =
            Some(PixivError::new(class, Some(PixivEndpoint::ProfileAll)));
    }

    pub fn clear_validation_failure(&self) {
        self.state.lock().unwrap().validation_error = None;
    }

    pub fn fail_ranking(&self, class: PixivErrorClass) {
        self.state.lock().unwrap().ranking_error =
            Some(PixivError::new(class, Some(PixivEndpoint::Ranking)));
    }

    pub fn clear_ranking_failure(&self) {
        self.state.lock().unwrap().ranking_error = None;
    }

    pub fn fail_bookmarks(&self, class: PixivErrorClass) {
        self.state.lock().unwrap().bookmark_error =
            Some(PixivError::new(class, Some(PixivEndpoint::Bookmarks)));
    }

    pub fn fail_work_detail(&self, class: PixivErrorClass) {
        self.state.lock().unwrap().work_detail_error =
            Some(PixivError::new(class, Some(PixivEndpoint::WorkDetail)));
    }

    pub fn pause_work_detail(&self) -> WorkDetailPause {
        let pause = WorkDetailPause {
            entered: Arc::new(Barrier::new(2)),
            resume: Arc::new(Barrier::new(2)),
        };
        self.state.lock().unwrap().work_detail_pause = Some(pause.clone());
        pause
    }

    pub fn clear_work_detail_failure(&self) {
        self.state.lock().unwrap().work_detail_error = None;
    }

    pub fn fail_add(&self, class: PixivErrorClass) {
        self.state.lock().unwrap().add_error =
            Some(PixivError::new(class, Some(PixivEndpoint::AddBookmark)));
    }

    pub fn fail_delete(&self, class: PixivErrorClass) {
        self.state.lock().unwrap().delete_error =
            Some(PixivError::new(class, Some(PixivEndpoint::DeleteBookmark)));
    }

    pub fn set_ranking_items(&self, items: Vec<PixivRankingEntry>) {
        self.state.lock().unwrap().ranking_items = items;
    }

    pub fn set_ranking_pages(&self, pages: Vec<Vec<PixivRankingEntry>>) {
        self.state.lock().unwrap().ranking_pages = pages;
    }

    pub fn set_ranking_date(&self, date: Date) {
        self.state.lock().unwrap().ranking_date = Some(date);
    }

    pub fn set_bookmark_items(&self, items: Vec<PixivDiscoveryWork>) {
        self.state.lock().unwrap().public_bookmark_pages =
            vec![items.into_iter().map(|item| item.work_id).collect()];
    }

    pub fn set_public_bookmark_pages(&self, pages: Vec<Vec<i64>>) {
        self.state.lock().unwrap().public_bookmark_pages = pages;
    }

    pub fn set_private_bookmark_pages(&self, pages: Vec<Vec<i64>>) {
        self.state.lock().unwrap().private_bookmark_pages = pages;
    }

    pub fn set_follow_items(&self, items: Vec<PixivDiscoveryWork>) {
        let mut state = self.state.lock().unwrap();
        state.follow_items = items;
        state.follow_pages = None;
    }

    pub fn set_follow_pages(&self, pages: Vec<Vec<PixivDiscoveryWork>>) {
        self.state.lock().unwrap().follow_pages = Some(pages);
    }

    pub fn set_following_authors(
        &self,
        public: Vec<PixivFollowedArtist>,
        private: Vec<PixivFollowedArtist>,
    ) {
        let mut state = self.state.lock().unwrap();
        state.public_following_authors = public;
        state.private_following_authors = private;
    }

    pub fn set_artist_follow_state(&self, state: PixivArtistFollowState) {
        self.state
            .lock()
            .unwrap()
            .artist_follow_states
            .insert(state.artist_id, state);
    }

    pub fn artist_follow_add_requests(&self) -> Vec<PixivArtistFollowRequest> {
        self.state
            .lock()
            .unwrap()
            .artist_follow_add_requests
            .clone()
    }

    pub fn artist_follow_remove_requests(&self) -> Vec<i64> {
        self.state
            .lock()
            .unwrap()
            .artist_follow_remove_requests
            .clone()
    }

    pub fn set_artist_work_ids(&self, ids: Vec<i64>) {
        self.state.lock().unwrap().artist_work_ids = ids;
    }

    pub fn set_work_detail(&self, detail: PixivWorkDetail) {
        self.state
            .lock()
            .unwrap()
            .work_details
            .insert(detail.work_id, detail);
    }

    pub fn set_work_pages(&self, pages: PixivWorkPages) {
        self.state
            .lock()
            .unwrap()
            .work_pages
            .insert(pages.work_id, pages);
    }

    pub fn set_ugoira_meta(&self, meta: PixivUgoiraMeta) {
        self.state
            .lock()
            .unwrap()
            .ugoira_meta
            .insert(meta.work_id, meta);
    }

    pub fn set_media(&self, url: &Url, bytes: Vec<u8>, content_type: &str) {
        self.state.lock().unwrap().media.insert(
            url.as_str().to_owned(),
            (Bytes::from(bytes), content_type.to_owned()),
        );
    }

    pub fn validate_calls(&self) -> usize {
        self.state.lock().unwrap().validate_calls
    }

    pub fn ranking_requests(&self) -> Vec<PixivRankingRequest> {
        self.state.lock().unwrap().ranking_requests.clone()
    }

    pub fn work_detail_calls(&self) -> usize {
        self.state.lock().unwrap().work_detail_calls
    }

    pub fn work_pages_calls(&self) -> usize {
        self.state.lock().unwrap().work_pages_calls
    }

    pub fn media_requests(&self) -> Vec<String> {
        self.state.lock().unwrap().media_requests.clone()
    }

    pub fn bookmark_requests(&self) -> Vec<PixivBookmarksRequest> {
        self.state.lock().unwrap().bookmark_requests.clone()
    }

    pub fn follow_requests(&self) -> Vec<PixivFollowLatestRequest> {
        self.state.lock().unwrap().follow_requests.clone()
    }

    pub fn following_requests(&self) -> Vec<PixivFollowingRequest> {
        self.state.lock().unwrap().following_requests.clone()
    }

    pub fn add_requests(&self) -> Vec<PixivBookmarkAddRequest> {
        self.state.lock().unwrap().add_requests.clone()
    }

    pub fn add_request_user_ids(&self) -> Vec<i64> {
        self.state.lock().unwrap().add_request_user_ids.clone()
    }

    pub fn delete_requests(&self) -> Vec<i64> {
        self.state.lock().unwrap().delete_requests.clone()
    }

    pub fn delete_request_user_ids(&self) -> Vec<i64> {
        self.state.lock().unwrap().delete_request_user_ids.clone()
    }
}

#[async_trait]
impl PixivGateway for FakePixivGateway {
    async fn validate_account(
        &self,
        context: &PixivRequestContext,
    ) -> Result<AdapterResponse<PixivAccountValidation>, PixivError> {
        let mut state = self.state.lock().unwrap();
        state.validate_calls += 1;
        if let Some(error) = state.validation_error.clone() {
            return Err(error);
        }
        Ok(response(PixivAccountValidation {
            user_id: context.user_id(),
            display_name: "Test Artist".to_owned(),
            avatar_url: None,
            private_bookmarks_verified: true,
        }))
    }

    async fn ranking_page(
        &self,
        _context: &PixivRequestContext,
        request: PixivRankingRequest,
    ) -> Result<AdapterResponse<PixivRankingPage>, PixivError> {
        let mut state = self.state.lock().unwrap();
        state.ranking_requests.push(request);
        if let Some(error) = state.ranking_error.clone() {
            return Err(error);
        }
        let items = if state.ranking_pages.is_empty() {
            state.ranking_items.clone()
        } else {
            state
                .ranking_pages
                .get(request.page.saturating_sub(1) as usize)
                .cloned()
                .unwrap_or_default()
        };
        let date = request.date.or(state.ranking_date);
        Ok(response(PixivRankingPage {
            date,
            items,
            next_cursor: Some(PixivRankingCursor {
                mode: request.mode,
                content: request.content,
                date,
                page: request.page + 1,
            }),
        }))
    }

    async fn work_detail(
        &self,
        _context: &PixivRequestContext,
        work_id: i64,
    ) -> Result<AdapterResponse<PixivWorkDetail>, PixivError> {
        let pause = {
            let mut state = self.state.lock().unwrap();
            state.work_detail_calls += 1;
            state.work_detail_pause.clone()
        };
        if let Some(pause) = pause {
            pause.entered.wait().await;
            pause.resume.wait().await;
        }
        let state = self.state.lock().unwrap();
        if let Some(error) = state.work_detail_error.clone() {
            return Err(error);
        }
        Ok(response(
            state
                .work_details
                .get(&work_id)
                .cloned()
                .unwrap_or_else(|| work_detail(work_id)),
        ))
    }

    async fn work_pages(
        &self,
        _context: &PixivRequestContext,
        work_id: i64,
    ) -> Result<AdapterResponse<PixivWorkPages>, PixivError> {
        let mut state = self.state.lock().unwrap();
        state.work_pages_calls += 1;
        Ok(response(
            state
                .work_pages
                .get(&work_id)
                .cloned()
                .unwrap_or_else(|| PixivWorkPages {
                    work_id,
                    pages: vec![work_page(work_id, 0)],
                }),
        ))
    }

    async fn ugoira_meta(
        &self,
        _context: &PixivRequestContext,
        work_id: i64,
    ) -> Result<AdapterResponse<PixivUgoiraMeta>, PixivError> {
        let state = self.state.lock().unwrap();
        state
            .ugoira_meta
            .get(&work_id)
            .cloned()
            .map(response)
            .ok_or_else(|| {
                PixivError::new(
                    PixivErrorClass::HiddenOrNotFound,
                    Some(PixivEndpoint::UgoiraMeta),
                )
            })
    }

    async fn follow_latest(
        &self,
        _context: &PixivRequestContext,
        request: PixivFollowLatestRequest,
    ) -> Result<
        AdapterResponse<
            pixivarchive_domain::pixiv::PixivPage<PixivDiscoveryWork, PixivFollowLatestCursor>,
        >,
        PixivError,
    > {
        let mut state = self.state.lock().unwrap();
        state.follow_requests.push(request.clone());
        if let Some(pages) = &state.follow_pages {
            let index = request.page.saturating_sub(1) as usize;
            let items = pages.get(index).cloned().unwrap_or_default();
            let next_cursor = (!items.is_empty()).then_some(PixivFollowLatestCursor {
                source: request.source,
                mode: request.mode,
                tag: request.tag,
                language: request.language,
                page: request.page + 1,
            });
            return Ok(response(pixivarchive_domain::pixiv::PixivPage {
                items,
                next_cursor,
            }));
        }
        Ok(response(pixivarchive_domain::pixiv::PixivPage {
            items: state.follow_items.clone(),
            next_cursor: Some(PixivFollowLatestCursor {
                source: request.source,
                mode: request.mode,
                tag: request.tag,
                language: request.language,
                page: request.page + 1,
            }),
        }))
    }

    async fn following_page(
        &self,
        _context: &PixivRequestContext,
        request: PixivFollowingRequest,
    ) -> Result<AdapterResponse<PixivPage<PixivFollowedArtist, PixivFollowingCursor>>, PixivError>
    {
        let mut state = self.state.lock().unwrap();
        state.following_requests.push(request.clone());
        let authors = match request.visibility {
            PixivFollowingVisibility::Public => state.public_following_authors.clone(),
            PixivFollowingVisibility::Private => state.private_following_authors.clone(),
        };
        let start = request.offset as usize;
        let end = start
            .saturating_add(request.limit as usize)
            .min(authors.len());
        let items = authors.get(start..end).unwrap_or_default().to_vec();
        let next_cursor = (end < authors.len()).then_some(PixivFollowingCursor {
            user_id: request.user_id,
            visibility: request.visibility,
            offset: end as u32,
            limit: request.limit,
            language: request.language,
        });
        Ok(response(PixivPage { items, next_cursor }))
    }

    async fn bookmarks(
        &self,
        _context: &PixivRequestContext,
        request: PixivBookmarksRequest,
    ) -> Result<AdapterResponse<PixivPage<i64, PixivBookmarksCursor>>, PixivError> {
        let mut state = self.state.lock().unwrap();
        state.bookmark_requests.push(request.clone());
        if let Some(error) = state.bookmark_error.clone() {
            return Err(error);
        }
        let pages = match request.visibility {
            pixivarchive_domain::pixiv::PixivBookmarkVisibility::Public => {
                &state.public_bookmark_pages
            }
            pixivarchive_domain::pixiv::PixivBookmarkVisibility::Private => {
                &state.private_bookmark_pages
            }
        };
        let mut consumed = 0_u32;
        for (index, items) in pages.iter().enumerate() {
            let next_offset = consumed.saturating_add(items.len() as u32);
            if request.offset == consumed {
                let next_cursor = (index + 1 < pages.len()).then(|| PixivBookmarksCursor {
                    user_id: request.user_id,
                    visibility: request.visibility,
                    mode: request.mode,
                    tag: request.tag.clone(),
                    offset: next_offset,
                });
                return Ok(response(pixivarchive_domain::pixiv::PixivPage {
                    items: items.clone(),
                    next_cursor,
                }));
            }
            consumed = next_offset;
        }
        Ok(response(pixivarchive_domain::pixiv::PixivPage {
            items: Vec::new(),
            next_cursor: None,
        }))
    }

    async fn artist_work_ids(
        &self,
        _context: &PixivRequestContext,
        artist_id: i64,
    ) -> Result<AdapterResponse<PixivArtistWorkIds>, PixivError> {
        Ok(response(PixivArtistWorkIds {
            artist_id,
            work_ids: self.state.lock().unwrap().artist_work_ids.clone(),
        }))
    }

    async fn artist_follow_state(
        &self,
        _context: &PixivRequestContext,
        artist_id: i64,
    ) -> Result<AdapterResponse<PixivArtistFollowState>, PixivError> {
        Ok(response(
            self.state
                .lock()
                .unwrap()
                .artist_follow_states
                .get(&artist_id)
                .cloned()
                .unwrap_or(PixivArtistFollowState {
                    artist_id,
                    name: format!("Artist {artist_id}"),
                    profile_image_url: None,
                    followed: false,
                }),
        ))
    }

    async fn add_artist_follow(
        &self,
        _context: &PixivRequestContext,
        request: PixivArtistFollowRequest,
    ) -> Result<AdapterResponse<PixivArtistFollowWriteResult>, PixivError> {
        let mut state = self.state.lock().unwrap();
        state.artist_follow_add_requests.push(request.clone());
        state
            .artist_follow_states
            .entry(request.artist_id)
            .and_modify(|artist| artist.followed = true)
            .or_insert_with(|| PixivArtistFollowState {
                artist_id: request.artist_id,
                name: format!("Artist {}", request.artist_id),
                profile_image_url: None,
                followed: true,
            });
        Ok(response(PixivArtistFollowWriteResult {
            artist_id: request.artist_id,
        }))
    }

    async fn remove_artist_follow(
        &self,
        _context: &PixivRequestContext,
        artist_id: i64,
    ) -> Result<AdapterResponse<PixivArtistFollowWriteResult>, PixivError> {
        let mut state = self.state.lock().unwrap();
        state.artist_follow_remove_requests.push(artist_id);
        if let Some(artist) = state.artist_follow_states.get_mut(&artist_id) {
            artist.followed = false;
        }
        Ok(response(PixivArtistFollowWriteResult { artist_id }))
    }

    async fn add_bookmark(
        &self,
        context: &PixivRequestContext,
        request: PixivBookmarkAddRequest,
    ) -> Result<AdapterResponse<PixivBookmarkWriteResult>, PixivError> {
        let mut state = self.state.lock().unwrap();
        state.add_request_user_ids.push(context.user_id());
        state.add_requests.push(request);
        if let Some(error) = state.add_error.clone() {
            return Err(error);
        }
        Ok(response(PixivBookmarkWriteResult {
            bookmark_id: Some(9001),
        }))
    }

    async fn delete_bookmark(
        &self,
        context: &PixivRequestContext,
        bookmark_id: i64,
    ) -> Result<AdapterResponse<PixivBookmarkWriteResult>, PixivError> {
        let mut state = self.state.lock().unwrap();
        state.delete_request_user_ids.push(context.user_id());
        state.delete_requests.push(bookmark_id);
        if let Some(error) = state.delete_error.clone() {
            return Err(error);
        }
        Ok(response(PixivBookmarkWriteResult { bookmark_id: None }))
    }
}

#[async_trait]
impl PixivMediaGateway for FakePixivGateway {
    async fn media(
        &self,
        _context: &PixivRequestContext,
        _work_id: i64,
        media_url: Url,
    ) -> Result<PixivMediaResponse, PixivError> {
        let mut state = self.state.lock().unwrap();
        state.media_requests.push(media_url.as_str().to_owned());
        let (bytes, content_type) =
            state
                .media
                .get(media_url.as_str())
                .cloned()
                .ok_or_else(|| {
                    PixivError::new(
                        PixivErrorClass::HiddenOrNotFound,
                        Some(PixivEndpoint::Media),
                    )
                })?;
        Ok(PixivMediaResponse {
            content_length: Some(bytes.len() as u64),
            content_type: Some(content_type),
            body: Box::pin(stream::iter(vec![Ok(bytes)])),
        })
    }
}

pub fn context() -> PixivRequestContext {
    PixivRequestContext::new(
        SecretString::from("PHPSESSID=test"),
        10001,
        "PixivArchiveTest/1.0",
    )
}

pub fn discovery_work(work_id: i64) -> PixivDiscoveryWork {
    PixivDiscoveryWork {
        work_id,
        title: format!("work {work_id}"),
        kind: PixivWorkKind::Illustration,
        age_rating: pixivarchive_domain::pixiv::PixivAgeRating::AllAge,
        ai_classification: pixivarchive_domain::pixiv::PixivAiClassification::NotAiGenerated,
        is_original: true,
        artist: artist(100),
        tags: Vec::new(),
        page_count: 1,
        dimensions: None,
        view_count: Some(1000),
        bookmarked_by_current_account: Some(false),
        bookmark: None,
    }
}

pub fn ranking_entry(work_id: i64, rank: u32) -> PixivRankingEntry {
    PixivRankingEntry {
        work: discovery_work(work_id),
        rank,
        previous_rank: None,
    }
}

pub fn work_detail(work_id: i64) -> PixivWorkDetail {
    PixivWorkDetail {
        work_id,
        title: format!("work {work_id}"),
        description: format!("description {work_id}"),
        kind: PixivWorkKind::Illustration,
        age_rating: pixivarchive_domain::pixiv::PixivAgeRating::AllAge,
        ai_classification: pixivarchive_domain::pixiv::PixivAiClassification::NotAiGenerated,
        is_original: true,
        artist: artist(100),
        published_at: None,
        updated_at: None,
        tags: vec![pixivarchive_domain::pixiv::PixivTag {
            name: "original".to_owned(),
            translated_name: Some("translated".to_owned()),
        }],
        page_count: 1,
        dimensions: PixivDimensions {
            width: 1000,
            height: 1200,
        },
        counts: PixivWorkCounts {
            bookmarks: 100,
            likes: 10,
            comments: 1,
            views: 1000,
        },
        bookmarked_by_current_account: Some(false),
        bookmark: None,
        series: None,
    }
}

pub fn work_page(work_id: i64, page_index: u32) -> pixivarchive_domain::pixiv::PixivWorkPage {
    pixivarchive_domain::pixiv::PixivWorkPage {
        page_index,
        original_url: Url::parse(&format!(
            "https://i.pximg.net/img-original/img/2026/07/30/{work_id}_p{page_index}.png"
        ))
        .unwrap(),
        dimensions: PixivDimensions {
            width: 1200 + page_index,
            height: 1600 + page_index,
        },
        format_hint: Some(pixivarchive_domain::pixiv::PixivImageFormat::Png),
    }
}

pub fn png_bytes() -> Vec<u8> {
    let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(4, 4, Rgb([20, 80, 200])));
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, ImageFormat::Png).unwrap();
    output.into_inner()
}

pub fn all_ranking_modes() -> Vec<PixivRankingMode> {
    vec![
        PixivRankingMode::Daily,
        PixivRankingMode::Weekly,
        PixivRankingMode::Monthly,
        PixivRankingMode::Rookie,
        PixivRankingMode::Original,
        PixivRankingMode::AiGenerated,
        PixivRankingMode::R18,
        PixivRankingMode::R18g,
        PixivRankingMode::Male,
        PixivRankingMode::Female,
    ]
}

pub fn all_ranking_contents() -> Vec<PixivRankingContent> {
    vec![
        PixivRankingContent::All,
        PixivRankingContent::Illustration,
        PixivRankingContent::Manga,
        PixivRankingContent::Ugoira,
    ]
}

fn artist(pixiv_id: i64) -> PixivArtistRef {
    PixivArtistRef {
        pixiv_id,
        name: format!("artist {pixiv_id}"),
        account_name: None,
    }
}

fn response<T>(value: T) -> AdapterResponse<T> {
    AdapterResponse {
        value,
        provenance: vec![ResponseProvenance {
            adapter_version: "test",
            endpoint: PixivEndpoint::ProfileAll,
            raw: json!({}),
        }],
    }
}
