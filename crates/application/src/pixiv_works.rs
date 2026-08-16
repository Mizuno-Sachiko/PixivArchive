use pixivarchive_db::{Db, DbError, JobRepository, SavePixivWorkMetadata, WorkRepository};
use pixivarchive_domain::{
    job::{JobErrorClass, JobLease, JobPriority},
    pixiv::{
        PixivAiClassification, PixivImageFormat, PixivWorkDetail, PixivWorkKind, PixivWorkPage,
        PixivWorkPages,
    },
    rule::{
        CandidatePage, EvaluationContext, EvaluationError, PageRuleMetadata, RuleAction,
        RuleCandidate, RuleDefinitionV1, RuleTag,
    },
    work::WorkSourceState,
};
use pixivarchive_pixiv::{
    AdapterResponse, PixivErrorClass, PixivGateway, PixivRequestContext, ResponseProvenance,
};
use serde_json::{Value, json};
use std::sync::Arc;
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct PixivWorkProcessor<G> {
    works: WorkRepository,
    jobs: JobRepository,
    gateway: Arc<G>,
}

impl<G> PixivWorkProcessor<G>
where
    G: PixivGateway + 'static,
{
    pub fn new(db: Db, gateway: Arc<G>) -> Self {
        Self {
            works: WorkRepository::new(db.clone()),
            jobs: JobRepository::new(db),
            gateway,
        }
    }

    pub async fn process(
        &self,
        request: ProcessPixivWork<'_>,
    ) -> Result<ProcessedPixivWork, PixivWorkProcessingError> {
        self.process_with_lease(None, request).await
    }

    pub async fn process_for_job(
        &self,
        lease: JobLease,
        request: ProcessPixivWork<'_>,
    ) -> Result<ProcessedPixivWork, PixivWorkProcessingError> {
        self.process_with_lease(Some(lease), request).await
    }

    async fn process_with_lease(
        &self,
        lease: Option<JobLease>,
        request: ProcessPixivWork<'_>,
    ) -> Result<ProcessedPixivWork, PixivWorkProcessingError> {
        if request.deletion_marker_policy == DeletionMarkerPolicy::Block
            && self
                .works
                .deletion_marker_exists(request.pixiv_work_id)
                .await?
        {
            return Ok(ProcessedPixivWork::BlockedByDeletionMarker);
        }

        let detail_response = fetch_work_detail(
            &self.works,
            self.gateway.as_ref(),
            request.context,
            request.pixiv_work_id,
            lease,
        )
        .await?;
        let detail = &detail_response.value;
        if detail.page_count == 0 {
            return Err(DbError::InvalidValue("Pixiv work has no pages".to_owned()).into());
        }
        let now = OffsetDateTime::now_utc();
        let mut candidate = pending_rule_candidate(detail, &request.discovery, now);
        let mut pages: Option<PixivWorkPages> = None;
        let mut pages_provenance = Vec::new();
        let mut ugoira = None;
        let mut ugoira_provenance = Vec::new();
        let action = if request.forced {
            RuleAction::Download
        } else if let Some(document) = request.rule_document {
            loop {
                match document.evaluate(&EvaluationContext {
                    now,
                    candidate: candidate.clone(),
                }) {
                    Ok(decision) => break decision.action,
                    Err(EvaluationError::PageMetadataRequired { .. }) => {
                        if pages.is_none() {
                            let response = self
                                .gateway
                                .work_pages(request.context, request.pixiv_work_id)
                                .await
                                .map_err(pixiv_error)?;
                            validate_pages(detail, &response.value)?;
                            pages_provenance = response.provenance;
                            pages = Some(response.value);
                        }
                        apply_page_metadata(
                            &mut candidate,
                            pages.as_ref().expect("Pixiv pages were loaded"),
                        );
                    }
                }
            }
        } else {
            RuleAction::Download
        };
        if action == RuleAction::Ignore {
            return Ok(ProcessedPixivWork::Ignored);
        }

        if pages.is_none() {
            let response = self
                .gateway
                .work_pages(request.context, request.pixiv_work_id)
                .await
                .map_err(pixiv_error)?;
            validate_pages(detail, &response.value)?;
            pages_provenance = response.provenance;
            pages = Some(response.value);
        }
        if detail.kind == PixivWorkKind::Ugoira && ugoira.is_none() {
            let response = self
                .gateway
                .ugoira_meta(request.context, request.pixiv_work_id)
                .await
                .map_err(pixiv_error)?;
            ugoira_provenance = response.provenance;
            ugoira = Some(response.value);
        }

        let provenance = json!({
            "detail": provenance_json(detail_response.provenance.clone()),
            "pages": provenance_json(pages_provenance),
            "ugoira": provenance_json(ugoira_provenance),
        });
        let metadata = SavePixivWorkMetadata {
            account_id: Some(request.account_id),
            detail: detail_response.value,
            pages: pages.expect("Pixiv pages were loaded before persistence"),
            ugoira,
            provenance,
        };
        let saved = match (lease, request.deletion_marker_policy) {
            (Some(lease), DeletionMarkerPolicy::Block) => {
                self.works
                    .save_pixiv_metadata_for_job(lease, metadata)
                    .await?
            }
            (None, DeletionMarkerPolicy::Block) => self.works.save_pixiv_metadata(metadata).await?,
            (Some(lease), DeletionMarkerPolicy::RemoveOnSave) => {
                self.works
                    .save_reimported_pixiv_metadata_for_job(lease, metadata)
                    .await?
            }
            (None, DeletionMarkerPolicy::RemoveOnSave) => {
                self.works.save_reimported_pixiv_metadata(metadata).await?
            }
        };
        if action == RuleAction::Download {
            let job_id = match lease {
                Some(lease) => {
                    self.jobs
                        .enqueue_download_if_absent_for_job(
                            lease,
                            request.account_id,
                            saved.id,
                            request.pixiv_work_id,
                            request.download_priority,
                        )
                        .await?
                }
                None => {
                    self.jobs
                        .enqueue_download_if_absent(
                            request.account_id,
                            saved.id,
                            request.pixiv_work_id,
                            request.download_priority,
                        )
                        .await?
                }
            };
            return Ok(ProcessedPixivWork::DownloadQueued {
                work_id: saved.id,
                job_id,
            });
        }
        Ok(ProcessedPixivWork::MetadataSaved { work_id: saved.id })
    }
}

async fn fetch_work_detail<G>(
    works: &WorkRepository,
    gateway: &G,
    context: &PixivRequestContext,
    pixiv_work_id: i64,
    lease: Option<JobLease>,
) -> Result<AdapterResponse<PixivWorkDetail>, PixivWorkProcessingError>
where
    G: PixivGateway + ?Sized,
{
    match gateway.work_detail(context, pixiv_work_id).await {
        Ok(response) => Ok(response),
        Err(error) => {
            let source_state = match error.class() {
                PixivErrorClass::HiddenOrNotFound => Some(WorkSourceState::Missing),
                PixivErrorClass::AgeRestrictedDisabled => Some(WorkSourceState::Restricted),
                _ => None,
            };
            if let Some(source_state) = source_state {
                match lease {
                    Some(lease) => {
                        works
                            .mark_source_state_for_job(lease, pixiv_work_id, source_state)
                            .await?;
                    }
                    None => {
                        works.mark_source_state(pixiv_work_id, source_state).await?;
                    }
                }
            }
            Err(pixiv_error(error))
        }
    }
}

pub struct ProcessPixivWork<'a> {
    pub context: &'a PixivRequestContext,
    pub account_id: Uuid,
    pub pixiv_work_id: i64,
    pub deletion_marker_policy: DeletionMarkerPolicy,
    pub forced: bool,
    pub rule_document: Option<&'a RuleDefinitionV1>,
    pub discovery: WorkDiscoveryContext,
    pub download_priority: JobPriority,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeletionMarkerPolicy {
    Block,
    RemoveOnSave,
}

#[derive(Clone, Debug, Default)]
pub struct WorkDiscoveryContext {
    pub ranking_rank: Option<u32>,
    pub ranking_date: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProcessedPixivWork {
    BlockedByDeletionMarker,
    Ignored,
    MetadataSaved { work_id: Uuid },
    DownloadQueued { work_id: Uuid, job_id: Uuid },
}

#[derive(Debug, Error)]
pub enum PixivWorkProcessingError {
    #[error("Pixiv work storage failed")]
    Storage(#[from] DbError),
    #[error("Pixiv work request failed")]
    Pixiv(JobErrorClass),
    #[error("Pixiv work rule evaluation failed")]
    Rule(#[from] EvaluationError),
}

impl PixivWorkProcessingError {
    pub fn error_class(&self) -> JobErrorClass {
        match self {
            Self::Pixiv(error_class) => *error_class,
            Self::Storage(error) => crate::jobs::database_error_class(error),
            Self::Rule(_) => JobErrorClass::Permanent,
        }
    }

    pub fn is_permanent_pixiv(&self) -> bool {
        matches!(self, Self::Pixiv(JobErrorClass::Permanent))
    }
}

fn pixiv_error(error: pixivarchive_pixiv::PixivError) -> PixivWorkProcessingError {
    tracing::warn!(error = %error, "Pixiv work request failed");
    PixivWorkProcessingError::Pixiv(crate::jobs::pixiv_error_class(error.class()))
}

fn provenance_json(provenance: Vec<ResponseProvenance>) -> Vec<Value> {
    provenance
        .into_iter()
        .map(|entry| {
            json!({
                "adapter_version": entry.adapter_version,
                "endpoint": entry.endpoint.as_str(),
                "raw": entry.raw,
            })
        })
        .collect()
}

pub(crate) fn rule_preview_candidate(
    detail: &PixivWorkDetail,
    pages: &PixivWorkPages,
    now: OffsetDateTime,
) -> Result<RuleCandidate, PixivWorkProcessingError> {
    validate_pages(detail, pages)?;
    let discovery = WorkDiscoveryContext::default();
    let mut candidate = pending_rule_candidate(detail, &discovery, now);
    apply_page_metadata(&mut candidate, pages);
    Ok(candidate)
}

fn pending_rule_candidate(
    detail: &PixivWorkDetail,
    discovery: &WorkDiscoveryContext,
    now: OffsetDateTime,
) -> RuleCandidate {
    rule_candidate_with_pages(
        detail,
        detail.page_count,
        (0..detail.page_count)
            .map(|_| CandidatePage { metadata: None })
            .collect(),
        discovery,
        now,
    )
}

fn rule_candidate_with_pages(
    detail: &PixivWorkDetail,
    page_count: u32,
    pages: Vec<CandidatePage>,
    discovery: &WorkDiscoveryContext,
    now: OffsetDateTime,
) -> RuleCandidate {
    let bookmark_rate = (detail.counts.views > 0)
        .then_some(detail.counts.bookmarks as f64 / detail.counts.views as f64);
    let bookmarks_per_day = detail.published_at.and_then(|published_at| {
        let age_seconds = (now - published_at).whole_seconds();
        (age_seconds > 0)
            .then_some(detail.counts.bookmarks as f64 / (age_seconds as f64 / 86_400.0))
    });
    RuleCandidate {
        pixiv_work_id: detail.work_id,
        content_type: detail.kind.as_str().to_owned(),
        title: Some(detail.title.clone()),
        description: Some(detail.description.clone()),
        artist_id: Some(detail.artist.pixiv_id),
        artist_name: Some(detail.artist.name.clone()),
        published_at: detail.published_at,
        updated_at: detail.updated_at,
        tags: detail
            .tags
            .iter()
            .map(|tag| RuleTag {
                original: tag.name.clone(),
                translation: tag.translated_name.clone(),
            })
            .collect(),
        page_count,
        age_rating: Some(detail.age_rating.as_str().to_owned()),
        ai_generated: match detail.ai_classification {
            PixivAiClassification::Unknown => None,
            PixivAiClassification::NotAiGenerated => Some(false),
            PixivAiClassification::AiGenerated => Some(true),
        },
        original_work: Some(detail.is_original),
        bookmarked_by_current_account: detail.bookmarked_by_current_account,
        bookmark_count: Some(detail.counts.bookmarks),
        view_count: Some(detail.counts.views),
        like_count: Some(detail.counts.likes),
        comment_count: Some(detail.counts.comments),
        bookmark_rate,
        bookmarks_per_day,
        ranking_rank: discovery.ranking_rank,
        ranking_date: discovery.ranking_date,
        series_id: detail.series.as_ref().map(|series| series.pixiv_id),
        series_title: detail.series.as_ref().map(|series| series.title.clone()),
        series_order: detail.series.as_ref().and_then(|series| series.order),
        pages,
    }
}

fn validate_pages(
    detail: &PixivWorkDetail,
    pages: &PixivWorkPages,
) -> Result<(), PixivWorkProcessingError> {
    if pages.work_id != detail.work_id
        || pages.pages.len() != usize::try_from(detail.page_count).unwrap_or(usize::MAX)
        || pages
            .pages
            .iter()
            .enumerate()
            .any(|(index, page)| page.page_index != index as u32)
    {
        return Err(DbError::InvalidValue(
            "Pixiv work page metadata does not match the work detail".to_owned(),
        )
        .into());
    }
    Ok(())
}

fn apply_page_metadata(candidate: &mut RuleCandidate, pages: &PixivWorkPages) {
    candidate.page_count = u32::try_from(pages.pages.len()).unwrap_or(u32::MAX);
    candidate.pages = pages
        .pages
        .iter()
        .map(|page| CandidatePage {
            metadata: Some(page_rule_metadata(page)),
        })
        .collect();
}

fn page_rule_metadata(page: &PixivWorkPage) -> PageRuleMetadata {
    let extension = page
        .format_hint
        .map(image_extension)
        .or_else(|| {
            page.original_url
                .path_segments()
                .and_then(Iterator::last)
                .and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension))
        })
        .map(normalize_image_extension);
    let width = page.dimensions.width;
    let height = page.dimensions.height;
    PageRuleMetadata {
        original_extension: extension,
        width: Some(width),
        height: Some(height),
        aspect_ratio: Some(width as f64 / height as f64),
        orientation: Some(
            match width.cmp(&height) {
                std::cmp::Ordering::Greater => "landscape",
                std::cmp::Ordering::Less => "portrait",
                std::cmp::Ordering::Equal => "square",
            }
            .to_owned(),
        ),
    }
}

fn image_extension(format: PixivImageFormat) -> &'static str {
    match format {
        PixivImageFormat::Jpeg => "jpg",
        PixivImageFormat::Png => "png",
        PixivImageFormat::Gif => "gif",
    }
}

fn normalize_image_extension(extension: &str) -> String {
    match extension.to_ascii_lowercase().as_str() {
        "jpeg" => "jpg".to_owned(),
        extension => extension.to_owned(),
    }
}

#[cfg(test)]
mod error_classification_tests {
    use super::PixivWorkProcessingError;
    use pixivarchive_db::DbError;
    use pixivarchive_domain::job::JobErrorClass;

    #[test]
    fn temporary_storage_failures_remain_retryable() {
        assert_eq!(
            PixivWorkProcessingError::Storage(DbError::RevisionConflict).error_class(),
            JobErrorClass::Server
        );
    }

    #[test]
    fn invalid_work_data_remains_permanent() {
        assert_eq!(
            PixivWorkProcessingError::Storage(DbError::InvalidValue("bad work".to_owned()))
                .error_class(),
            JobErrorClass::Permanent
        );
    }
}
