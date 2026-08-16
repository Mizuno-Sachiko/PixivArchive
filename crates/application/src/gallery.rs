use crate::settings::{SettingsError, SettingsService};
use pixivarchive_db::{Db, DbError, GalleryRepository, PixivAccountRepository};
use pixivarchive_domain::subscription::PixivAccountState;
use pixivarchive_domain::work::{
    GalleryArtistDetail, GalleryContextCursor, GalleryContextPage,
    GalleryContextSelectionExpression, GalleryContextSelectionProjection,
    GalleryOverviewDecoration, GallerySearch, GallerySearchPage, GallerySelectionExpression,
    GallerySelectionProjection, GallerySeriesDetail, GalleryTagDetail, GalleryWorkDetail,
    WorkRevisionSummary,
};
use time::Date;
use uuid::Uuid;

#[derive(Clone)]
pub struct GalleryService {
    repository: GalleryRepository,
    accounts: PixivAccountRepository,
    settings: SettingsService,
}

impl GalleryService {
    pub fn new(db: Db) -> Self {
        let settings = SettingsService::new(db.clone());
        Self::with_settings(db, settings)
    }

    pub fn with_settings(db: Db, settings: SettingsService) -> Self {
        Self {
            repository: GalleryRepository::new(db.clone()),
            accounts: PixivAccountRepository::new(db),
            settings,
        }
    }

    pub async fn search(&self, search: GallerySearch) -> Result<GallerySearchPage, DbError> {
        self.repository
            .search(search, self.current_account_id().await?)
            .await
    }

    pub async fn count(&self, search: &GallerySearch) -> Result<u64, DbError> {
        self.repository
            .count(search, self.current_account_id().await?)
            .await
    }

    pub async fn selection_projection(
        &self,
        expression: &GallerySelectionExpression,
        visible_work_ids: &[Uuid],
    ) -> Result<GallerySelectionProjection, DbError> {
        self.repository
            .selection_projection(
                expression,
                visible_work_ids,
                self.current_account_id().await?,
            )
            .await
    }

    pub async fn context_selection_projection(
        &self,
        expression: &GalleryContextSelectionExpression,
        visible_context_ids: &[Uuid],
    ) -> Result<GalleryContextSelectionProjection, DbError> {
        self.repository
            .context_selection_projection(expression, visible_context_ids)
            .await
    }

    pub async fn overview_decorations(
        &self,
        date: Date,
    ) -> Result<Vec<Option<GalleryOverviewDecoration>>, SettingsError> {
        let allow_nsfw = self.overview_allow_nsfw().await?;
        Ok(self
            .repository
            .overview_decorations(date, allow_nsfw)
            .await?)
    }

    pub async fn shuffle_overview_decorations(
        &self,
        date: Date,
    ) -> Result<Vec<Option<GalleryOverviewDecoration>>, SettingsError> {
        let allow_nsfw = self.overview_allow_nsfw().await?;
        Ok(self
            .repository
            .shuffle_overview_decorations(date, allow_nsfw)
            .await?)
    }

    pub async fn work_detail(&self, work_id: Uuid) -> Result<GalleryWorkDetail, DbError> {
        self.repository
            .work_detail(work_id, self.current_account_id().await?)
            .await
    }

    pub async fn work_id_by_pixiv_id(&self, pixiv_work_id: i64) -> Result<Uuid, DbError> {
        self.repository.work_id_by_pixiv_id(pixiv_work_id).await
    }

    pub async fn artist_detail(
        &self,
        pixiv_artist_id: i64,
    ) -> Result<GalleryArtistDetail, DbError> {
        self.repository.artist_detail(pixiv_artist_id).await
    }

    pub async fn artists(
        &self,
        limit: u16,
        cursor: Option<&GalleryContextCursor>,
        query: Option<&str>,
    ) -> Result<GalleryContextPage<GalleryArtistDetail>, DbError> {
        self.repository.artists(limit, cursor, query).await
    }

    pub async fn tag_detail(&self, tag_name: &str) -> Result<GalleryTagDetail, DbError> {
        self.repository.tag_detail(tag_name).await
    }

    pub async fn tags(
        &self,
        limit: u16,
        cursor: Option<&GalleryContextCursor>,
        query: Option<&str>,
    ) -> Result<GalleryContextPage<GalleryTagDetail>, DbError> {
        self.repository.tags(limit, cursor, query).await
    }

    pub async fn series_detail(
        &self,
        pixiv_series_id: i64,
    ) -> Result<GallerySeriesDetail, DbError> {
        self.repository.series_detail(pixiv_series_id).await
    }

    pub async fn series(
        &self,
        limit: u16,
        cursor: Option<&GalleryContextCursor>,
        query: Option<&str>,
    ) -> Result<GalleryContextPage<GallerySeriesDetail>, DbError> {
        self.repository.series(limit, cursor, query).await
    }

    pub async fn revisions(&self, work_id: Uuid) -> Result<Vec<WorkRevisionSummary>, DbError> {
        self.repository.revisions(work_id).await
    }

    async fn current_account_id(&self) -> Result<Option<Uuid>, DbError> {
        Ok(self.accounts.current().await?.and_then(|account| {
            matches!(
                account.state,
                PixivAccountState::Normal | PixivAccountState::Restricted
            )
            .then_some(account.id)
        }))
    }

    async fn overview_allow_nsfw(&self) -> Result<bool, SettingsError> {
        Ok(self.settings.effective().await?.content.overview_allow_nsfw)
    }
}
