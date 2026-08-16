use super::{
    model::{
        ImportAttemptResult, ImportRequest, ImportResult, ImportRun, ImportRunSummary,
        ImportServiceError, pixiv_error_class,
    },
    queue::ImportQueueService,
};
use crate::pixiv_works::{
    DeletionMarkerPolicy, PixivWorkProcessor, ProcessPixivWork, ProcessedPixivWork,
    WorkDiscoveryContext,
};
use pixivarchive_db::{
    CreateImportRun, Db, DbError, ImportRepository, ImportRunRecord as DbImportRunRecord,
};
use pixivarchive_domain::{
    job::{JobLease, JobPriority},
    subscription::{ImportKind, ImportRunStatus},
};
use pixivarchive_pixiv::{PixivGateway, PixivRequestContext};
use std::{collections::HashSet, sync::Arc};
use uuid::Uuid;

#[derive(Clone)]
pub struct ImportService<G> {
    queue: ImportQueueService,
    runs: ImportRepository,
    gateway: Arc<G>,
    processor: PixivWorkProcessor<G>,
}

impl<G> ImportService<G>
where
    G: PixivGateway + 'static,
{
    pub fn new(db: Db, gateway: G) -> Self {
        let gateway = Arc::new(gateway);
        Self {
            queue: ImportQueueService::new(db.clone()),
            runs: ImportRepository::new(db.clone()),
            processor: PixivWorkProcessor::new(db, Arc::clone(&gateway)),
            gateway,
        }
    }

    pub async fn import(&self, request: ImportRequest) -> Result<ImportResult, ImportServiceError> {
        let run_id = self
            .runs
            .create(CreateImportRun {
                account_id: request.account_id,
                kind: request.kind,
                target_pixiv_id: request.target_pixiv_id,
                forced: request.forced,
                rule_document: request.rule_document.clone(),
                status: ImportRunStatus::Running,
            })
            .await?;
        let result = match request.kind {
            ImportKind::Work => {
                self.import_work(
                    run_id,
                    &request,
                    request.target_pixiv_id,
                    None,
                    JobPriority::ManualImport,
                )
                .await
            }
            ImportKind::Artist => {
                self.import_artist(run_id, &request, None, JobPriority::ManualImport)
                    .await
            }
        };
        match result {
            Ok(result) => {
                self.runs
                    .finish(
                        run_id,
                        result.status,
                        result.discovered_count,
                        result.saved_count,
                        None,
                    )
                    .await?;
                Ok(result)
            }
            Err(error) => {
                self.runs
                    .finish(
                        run_id,
                        ImportRunStatus::Failed,
                        0,
                        0,
                        Some(error.error_class().as_str()),
                    )
                    .await?;
                Err(error)
            }
        }
    }

    pub async fn queue(
        &self,
        request: ImportRequest,
    ) -> Result<ImportRunSummary, ImportServiceError> {
        Ok(self
            .queue
            .queue_resolved(
                request.account_id,
                request.kind,
                request.target_pixiv_id,
                request.forced,
                request.rule_document,
            )
            .await?)
    }

    pub async fn execute_queued(
        &self,
        run_id: Uuid,
        context: PixivRequestContext,
    ) -> Result<ImportResult, ImportServiceError> {
        let attempt = self.execute_queued_attempt(run_id, context).await?;
        if let Some(error_class) = attempt.error_class {
            self.runs
                .finalize_failure(run_id, error_class.as_str())
                .await?;
        } else {
            self.runs
                .finish(
                    run_id,
                    attempt.result.status,
                    attempt.result.discovered_count,
                    attempt.result.saved_count,
                    None,
                )
                .await?;
        }
        Ok(attempt.result)
    }

    pub async fn execute_queued_attempt(
        &self,
        run_id: Uuid,
        context: PixivRequestContext,
    ) -> Result<ImportAttemptResult, ImportServiceError> {
        let run = self.runs.load(run_id).await?;
        self.runs.mark_running(run_id).await?;
        let kind = run.kind;
        let request = Self::request_from_run(run, context);
        let result = self
            .execute_run(run_id, &request, None, JobPriority::ManualImport)
            .await;
        match result {
            Ok(result) => Ok(ImportAttemptResult {
                result,
                error_class: None,
            }),
            Err(error) => {
                let error_class = error.error_class();
                self.runs
                    .record_attempt_failure(run_id, error_class.as_str())
                    .await?;
                Ok(Self::failed_attempt(run_id, kind, error_class))
            }
        }
    }

    pub async fn execute_queued_job_attempt(
        &self,
        lease: JobLease,
        priority: JobPriority,
        run_id: Uuid,
        context: PixivRequestContext,
    ) -> Result<ImportAttemptResult, ImportServiceError> {
        let run = self.runs.mark_running_for_job(lease, run_id).await?;
        let kind = run.kind;
        let request = Self::request_from_run(run, context);
        let result = self
            .execute_run(run_id, &request, Some(lease), priority)
            .await;
        match result {
            Ok(result) => Ok(ImportAttemptResult {
                result,
                error_class: None,
            }),
            Err(error) => {
                let error_class = error.error_class();
                self.runs
                    .record_job_attempt_failure(lease, run_id, error_class.as_str())
                    .await?;
                Ok(Self::failed_attempt(run_id, kind, error_class))
            }
        }
    }

    pub async fn finalize_queued_failure(
        &self,
        run_id: Uuid,
        error_class: &str,
    ) -> Result<(), DbError> {
        self.runs.finalize_failure(run_id, error_class).await
    }

    pub async fn load_run_by_job(&self, job_id: Uuid) -> Result<ImportRun, DbError> {
        self.runs.load_by_job(job_id).await.map(ImportRun::from)
    }

    async fn execute_run(
        &self,
        run_id: Uuid,
        request: &ImportRequest,
        lease: Option<JobLease>,
        priority: JobPriority,
    ) -> Result<ImportResult, ImportServiceError> {
        match request.kind {
            ImportKind::Work => {
                self.import_work(run_id, request, request.target_pixiv_id, lease, priority)
                    .await
            }
            ImportKind::Artist => self.import_artist(run_id, request, lease, priority).await,
        }
    }

    fn request_from_run(run: DbImportRunRecord, context: PixivRequestContext) -> ImportRequest {
        ImportRequest {
            account_id: run.account_id,
            context,
            kind: run.kind,
            target_pixiv_id: run.target_pixiv_id,
            forced: run.forced,
            rule_document: run.rule_document,
        }
    }

    fn failed_attempt(
        run_id: Uuid,
        kind: ImportKind,
        error_class: pixivarchive_domain::job::JobErrorClass,
    ) -> ImportAttemptResult {
        ImportAttemptResult {
            result: ImportResult {
                id: run_id,
                kind,
                status: ImportRunStatus::Failed,
                discovered_count: 0,
                saved_count: 0,
            },
            error_class: Some(error_class),
        }
    }

    async fn import_artist(
        &self,
        run_id: Uuid,
        request: &ImportRequest,
        lease: Option<JobLease>,
        priority: JobPriority,
    ) -> Result<ImportResult, ImportServiceError> {
        let ids = self
            .gateway
            .artist_work_ids(&request.context, request.target_pixiv_id)
            .await
            .map_err(|error| ImportServiceError::Pixiv(pixiv_error_class(error.class())))?
            .value
            .work_ids;
        let mut seen = HashSet::new();
        let mut saved = 0;
        for work_id in ids {
            if seen.insert(work_id)
                && matches!(
                    self.import_work(run_id, request, work_id, lease, priority)
                        .await?
                        .status,
                    ImportRunStatus::MetadataSaved | ImportRunStatus::DownloadQueued
                )
            {
                saved += 1;
            }
        }
        Ok(ImportResult {
            id: run_id,
            kind: ImportKind::Artist,
            status: if saved == 0 {
                ImportRunStatus::Ignored
            } else {
                ImportRunStatus::MetadataSaved
            },
            discovered_count: i32::try_from(seen.len()).unwrap_or(i32::MAX),
            saved_count: saved,
        })
    }

    async fn import_work(
        &self,
        run_id: Uuid,
        request: &ImportRequest,
        work_id: i64,
        lease: Option<JobLease>,
        priority: JobPriority,
    ) -> Result<ImportResult, ImportServiceError> {
        let process_request = ProcessPixivWork {
            context: &request.context,
            account_id: request.account_id,
            pixiv_work_id: work_id,
            deletion_marker_policy: DeletionMarkerPolicy::RemoveOnSave,
            forced: request.forced,
            rule_document: request.rule_document.as_ref(),
            discovery: WorkDiscoveryContext::default(),
            download_priority: priority,
        };
        let processed = match lease {
            Some(lease) => {
                self.processor
                    .process_for_job(lease, process_request)
                    .await?
            }
            None => self.processor.process(process_request).await?,
        };
        match processed {
            ProcessedPixivWork::BlockedByDeletionMarker => {
                self.record_candidate(lease, run_id, work_id, "blocked")
                    .await?;
                Ok(ImportResult {
                    id: run_id,
                    kind: ImportKind::Work,
                    status: ImportRunStatus::BlockedByDeletionMarker,
                    discovered_count: 1,
                    saved_count: 0,
                })
            }
            ProcessedPixivWork::Ignored => Ok(ImportResult {
                id: run_id,
                kind: ImportKind::Work,
                status: ImportRunStatus::Ignored,
                discovered_count: 1,
                saved_count: 0,
            }),
            ProcessedPixivWork::MetadataSaved { .. } => {
                self.record_candidate(lease, run_id, work_id, "metadata_only")
                    .await?;
                Ok(ImportResult {
                    id: run_id,
                    kind: ImportKind::Work,
                    status: ImportRunStatus::MetadataSaved,
                    discovered_count: 1,
                    saved_count: 1,
                })
            }
            ProcessedPixivWork::DownloadQueued { .. } => {
                self.record_candidate(lease, run_id, work_id, "download")
                    .await?;
                Ok(ImportResult {
                    id: run_id,
                    kind: ImportKind::Work,
                    status: ImportRunStatus::DownloadQueued,
                    discovered_count: 1,
                    saved_count: 1,
                })
            }
        }
    }

    async fn record_candidate(
        &self,
        lease: Option<JobLease>,
        run_id: Uuid,
        work_id: i64,
        action: &str,
    ) -> Result<(), DbError> {
        match lease {
            Some(lease) => {
                self.runs
                    .record_candidate_for_job(lease, run_id, work_id, action)
                    .await
            }
            None => self.runs.record_candidate(run_id, work_id, action).await,
        }
    }
}
