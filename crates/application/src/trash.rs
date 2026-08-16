use crate::settings::effective_job_priority_policy;
use pixivarchive_db::{Db, DbError, JobRepository, PixivAccountRepository, WorkRepository};
use pixivarchive_domain::job::{JobKind, JobPriority};
use pixivarchive_domain::subscription::PixivAccountState;
use pixivarchive_domain::work::{
    DuePurge, GalleryContextSelectionExpression, GallerySelectionExpression,
    TrashCollectionSummary, TrashCursor, TrashEntry, TrashFilter, TrashPage,
    TrashSelectionExpression, TrashSelectionMutation, TrashSelectionProjection,
};
use thiserror::Error;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const MIN_RETENTION_DAYS: u16 = 1;
const MAX_RETENTION_DAYS: u16 = 365;
const MAX_DUE_PURGES: u32 = 1_000;

#[derive(Clone)]
pub struct TrashService {
    db: Db,
    works: WorkRepository,
    accounts: PixivAccountRepository,
    jobs: JobRepository,
}

impl TrashService {
    pub fn new(db: Db) -> Self {
        Self {
            db: db.clone(),
            works: WorkRepository::new(db.clone()),
            accounts: PixivAccountRepository::new(db.clone()),
            jobs: JobRepository::new(db),
        }
    }

    pub async fn move_selection_to_trash(
        &self,
        expression: GallerySelectionExpression,
        retention_days: u16,
    ) -> Result<u64, DbError> {
        validate_retention(retention_days)?;
        let scheduled_purge_at =
            OffsetDateTime::now_utc() + Duration::days(i64::from(retention_days));
        let current_account_id = self.accounts.current().await?.and_then(|account| {
            matches!(
                account.state,
                PixivAccountState::Normal | PixivAccountState::Restricted
            )
            .then_some(account.id)
        });
        self.works
            .move_selection_to_trash(&expression, current_account_id, scheduled_purge_at)
            .await
    }

    pub async fn move_context_selection_to_trash(
        &self,
        expression: GalleryContextSelectionExpression,
        retention_days: u16,
    ) -> Result<u64, DbError> {
        validate_retention(retention_days)?;
        let scheduled_purge_at =
            OffsetDateTime::now_utc() + Duration::days(i64::from(retention_days));
        self.works
            .move_context_selection_to_trash(&expression, scheduled_purge_at)
            .await
    }

    pub async fn move_to_trash(
        &self,
        work_id: Uuid,
        retention_days: u16,
    ) -> Result<TrashEntry, DbError> {
        validate_retention(retention_days)?;
        let scheduled_purge_at =
            OffsetDateTime::now_utc() + Duration::days(i64::from(retention_days));
        self.works
            .move_to_trash(work_id, scheduled_purge_at)
            .await?;
        self.works.trash_entry(work_id).await
    }

    pub async fn restore(&self, work_id: Uuid) -> Result<(), DbError> {
        self.works.restore(work_id).await
    }

    pub async fn restore_selection(
        &self,
        expression: &TrashSelectionExpression,
    ) -> Result<u64, TrashSelectionCommandError> {
        require_unblocked(self.works.restore_trash_selection(expression).await?)
    }

    pub async fn reschedule(
        &self,
        work_id: Uuid,
        scheduled_purge_at: OffsetDateTime,
    ) -> Result<(), DbError> {
        self.works
            .reschedule_trash(work_id, scheduled_purge_at)
            .await
    }

    pub async fn reschedule_selection(
        &self,
        expression: &TrashSelectionExpression,
        scheduled_purge_at: OffsetDateTime,
    ) -> Result<u64, TrashSelectionCommandError> {
        require_unblocked(
            self.works
                .reschedule_trash_selection(expression, scheduled_purge_at)
                .await?,
        )
    }

    pub async fn page(
        &self,
        filter: &TrashFilter,
        cursor: Option<&TrashCursor>,
        limit: u16,
    ) -> Result<TrashPage, DbError> {
        self.works.trash_page(filter, cursor, limit).await
    }

    pub async fn summary(&self, filter: &TrashFilter) -> Result<TrashCollectionSummary, DbError> {
        self.works.trash_summary(filter).await
    }

    pub async fn project_selection(
        &self,
        expression: &TrashSelectionExpression,
        visible_work_ids: &[Uuid],
    ) -> Result<TrashSelectionProjection, DbError> {
        self.works
            .project_trash_selection(expression, visible_work_ids)
            .await
    }

    pub async fn purge(&self, work_id: Uuid) -> Result<Uuid, DbError> {
        self.jobs
            .enqueue_trash_purges_if_absent(&[work_id], "manual_purge", JobPriority::Immediate)
            .await?
            .into_iter()
            .next()
            .map(|purge| purge.job_id)
            .ok_or_else(|| DbError::InvalidValue("trash purge batch is empty".to_owned()))
    }

    pub async fn purge_selection(
        &self,
        expression: &TrashSelectionExpression,
    ) -> Result<u64, DbError> {
        self.jobs
            .enqueue_trash_selection_purges_if_absent(
                expression,
                "manual_purge",
                JobPriority::Immediate,
            )
            .await
    }

    pub async fn purge_all(&self) -> Result<u64, DbError> {
        self.jobs
            .enqueue_all_trash_purges_if_absent("manual_purge", JobPriority::Immediate)
            .await
    }

    pub async fn enqueue_due_purges(
        &self,
        now: OffsetDateTime,
        limit: u32,
    ) -> Result<Vec<DuePurge>, DbError> {
        if limit == 0 || limit > MAX_DUE_PURGES {
            return Err(DbError::InvalidValue(format!(
                "due purge limit must be between 1 and {MAX_DUE_PURGES}"
            )));
        }
        let priority = effective_job_priority_policy(&self.db)
            .await?
            .priority_for(JobKind::PurgeTrash);
        self.jobs
            .enqueue_due_trash_purges_if_absent(now, limit, "retention_expired", priority)
            .await
    }
}

fn validate_retention(retention_days: u16) -> Result<(), DbError> {
    if !(MIN_RETENTION_DAYS..=MAX_RETENTION_DAYS).contains(&retention_days) {
        return Err(DbError::InvalidValue(format!(
            "trash retention must be between {MIN_RETENTION_DAYS} and {MAX_RETENTION_DAYS} days"
        )));
    }
    Ok(())
}

fn require_unblocked(mutation: TrashSelectionMutation) -> Result<u64, TrashSelectionCommandError> {
    if mutation.blocked_count > 0 {
        return Err(TrashSelectionCommandError::Blocked {
            selected_count: mutation.selected_count,
            blocked_count: mutation.blocked_count,
        });
    }
    Ok(mutation.affected_count)
}

#[derive(Debug, Error)]
pub enum TrashSelectionCommandError {
    #[error("the trash selection contains works whose purge lifecycle has started")]
    Blocked {
        selected_count: u64,
        blocked_count: u64,
    },
    #[error("trash selection storage failed")]
    Storage(#[from] DbError),
}
