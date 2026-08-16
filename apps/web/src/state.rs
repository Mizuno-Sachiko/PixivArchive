use pixivarchive_application::{
    auth::AuthService,
    bookmarks::{BookmarkCommandPort, DisabledBookmarkCommandPort},
    events::EventStream,
    favorites::FavoritesAdminService,
    following::{
        ArtistFollowCommandPort, DisabledArtistFollowCommandPort, DisabledFollowingAvatarPort,
        DisabledFollowingRefreshPort, FollowingAdminService, FollowingAvatarPort,
        FollowingRefreshPort,
    },
    gallery::GalleryService,
    imports::ImportQueueService,
    jobs::JobService,
    pixiv_accounts::{
        DisabledPixivAccountCommandPort, PixivAccountAdminService, PixivAccountCommandPort,
    },
    rules::{DisabledRulePreviewPort, RulePreviewPort, RuleService},
    settings::{DeploymentCapabilities, SettingsService},
    subscriptions::SubscriptionService,
    system::{MediaSourceService, SystemCapabilities, SystemService},
    trash::TrashService,
};
use pixivarchive_db::Db;
use pixivarchive_media::MediaRoot;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::Semaphore;

#[derive(Clone, Debug)]
pub struct WebConfig {
    pub static_root: PathBuf,
    pub media_root: MediaRoot,
    pub cache_root: MediaRoot,
    pub version: String,
    pub git_commit: Option<String>,
    pub capabilities: SystemCapabilities,
}

#[derive(Clone)]
pub struct AppState {
    pub(crate) auth: AuthService,
    pub(crate) rules: RuleService,
    pub(crate) rule_preview: Arc<dyn RulePreviewPort>,
    pub(crate) subscriptions: SubscriptionService,
    pub(crate) imports: ImportQueueService,
    pub(crate) following: FollowingAdminService,
    pub(crate) artist_follow_commands: Arc<dyn ArtistFollowCommandPort>,
    pub(crate) favorites: FavoritesAdminService,
    pub(crate) following_refresh: Arc<dyn FollowingRefreshPort>,
    pub(crate) following_avatars: Arc<dyn FollowingAvatarPort>,
    pub(crate) pixiv_accounts: PixivAccountAdminService,
    pub(crate) pixiv_account_commands: Arc<dyn PixivAccountCommandPort>,
    pub(crate) bookmarks: Arc<dyn BookmarkCommandPort>,
    pub(crate) jobs: JobService,
    pub(crate) gallery: GalleryService,
    pub(crate) trash: TrashService,
    pub(crate) settings: SettingsService,
    pub(crate) system: SystemService,
    pub(crate) media: MediaSourceService,
    pub(crate) events: EventStream,
    pub(crate) config: WebConfig,
    pub(crate) work_export_permits: Arc<Semaphore>,
}

impl AppState {
    pub fn new(db: Db, auth: AuthService, events: EventStream, config: WebConfig) -> Self {
        let settings = SettingsService::with_capabilities(
            db.clone(),
            DeploymentCapabilities {
                avif_derivatives: config.capabilities.avif_derivatives,
            },
        );
        Self {
            auth,
            rules: RuleService::new(db.clone()),
            rule_preview: Arc::new(DisabledRulePreviewPort),
            subscriptions: SubscriptionService::new(db.clone()),
            imports: ImportQueueService::new(db.clone()),
            following: FollowingAdminService::new(db.clone()),
            artist_follow_commands: Arc::new(DisabledArtistFollowCommandPort),
            favorites: FavoritesAdminService::new(db.clone()),
            following_refresh: Arc::new(DisabledFollowingRefreshPort),
            following_avatars: Arc::new(DisabledFollowingAvatarPort),
            pixiv_accounts: PixivAccountAdminService::new(db.clone()),
            pixiv_account_commands: Arc::new(DisabledPixivAccountCommandPort),
            bookmarks: Arc::new(DisabledBookmarkCommandPort),
            jobs: JobService::new(db.clone()),
            gallery: GalleryService::with_settings(db.clone(), settings.clone()),
            trash: TrashService::new(db.clone()),
            settings,
            system: SystemService::new(
                db.clone(),
                config.media_root.path().to_path_buf(),
                config.version.clone(),
                config.git_commit.clone(),
            )
            .with_capabilities(config.capabilities),
            media: MediaSourceService::new(db, config.media_root.clone()),
            events,
            config,
            work_export_permits: Arc::new(Semaphore::new(1)),
        }
    }

    pub fn with_bookmark_commands(mut self, bookmarks: Arc<dyn BookmarkCommandPort>) -> Self {
        self.bookmarks = bookmarks;
        self
    }

    pub fn with_pixiv_account_commands(
        mut self,
        commands: Arc<dyn PixivAccountCommandPort>,
    ) -> Self {
        self.pixiv_account_commands = commands;
        self
    }

    pub fn with_following_refresh(mut self, refresh: Arc<dyn FollowingRefreshPort>) -> Self {
        self.following_refresh = refresh;
        self
    }

    pub fn with_artist_follow_commands(
        mut self,
        commands: Arc<dyn ArtistFollowCommandPort>,
    ) -> Self {
        self.artist_follow_commands = commands;
        self
    }

    pub fn with_following_avatars(mut self, avatars: Arc<dyn FollowingAvatarPort>) -> Self {
        self.following_avatars = avatars;
        self
    }

    pub fn with_rule_preview(mut self, preview: Arc<dyn RulePreviewPort>) -> Self {
        self.rule_preview = preview;
        self
    }
}
