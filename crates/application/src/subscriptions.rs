mod bookmarks;
mod discovery;
mod execution;
mod following;
mod management;
mod ranking;

pub use management::{
    RankingSubscriptionRequest, ScheduledSubscriptionRun, SubscriptionCursorView,
    SubscriptionMutationRequest, SubscriptionRunStartError, SubscriptionRunView,
    SubscriptionService, SubscriptionUpdateRequest, SubscriptionView,
};

pub(crate) const ALLOWED_SYNC_INTERVAL_MINUTES: [i64; 7] = [15, 30, 60, 180, 360, 720, 1_440];

use crate::jobs::{database_error_class, pixiv_error_class};
use crate::{
    following::{FollowingService, FollowingServiceError},
    pixiv_works::{
        DeletionMarkerPolicy, PixivWorkProcessor, ProcessPixivWork, ProcessedPixivWork,
        WorkDiscoveryContext,
    },
};
use pixivarchive_db::{
    BookmarkRepository, Db, DbError, FinishSubscriptionRunUnit, PixivBookmarkSyncEntry,
    SubscriptionRepository,
};
use pixivarchive_domain::{
    job::{JobErrorClass, JobLease, JobPriority},
    pixiv::{
        PixivBookmarksMode, PixivBookmarksRequest, PixivFollowLatestMode, PixivFollowLatestRequest,
        PixivFollowLatestSource, PixivRankingContent, PixivRankingCursor, PixivRankingMode,
        PixivRankingRequest,
    },
    rule::RuleDefinitionV1,
    subscription::{
        SubscriptionKind, SubscriptionRunStatus as DomainRunStatus, SubscriptionSchedule,
    },
};
use pixivarchive_pixiv::{PixivGateway, PixivRequestContext};
use serde_json::{Value, json};
use std::{collections::HashSet, sync::Arc};
use time::{Date, Duration, OffsetDateTime, Time};
use uuid::Uuid;

#[derive(Clone)]
pub struct SubscriptionExecutionService<G> {
    repository: SubscriptionRepository,
    bookmarks: BookmarkRepository,
    following: FollowingService<G>,
    processor: PixivWorkProcessor<G>,
    gateway: Arc<G>,
}

impl<G> SubscriptionExecutionService<G>
where
    G: PixivGateway + 'static,
{
    pub fn new(db: Db, gateway: G) -> Self {
        let gateway = Arc::new(gateway);
        Self {
            repository: SubscriptionRepository::new(db.clone()),
            bookmarks: BookmarkRepository::new(db.clone()),
            following: FollowingService::from_shared(db.clone(), Arc::clone(&gateway)),
            processor: PixivWorkProcessor::new(db, Arc::clone(&gateway)),
            gateway,
        }
    }
}

#[derive(Debug)]
pub struct SubscriptionRunRequest {
    pub context: PixivRequestContext,
    pub subscription_run_id: Uuid,
}

#[derive(Debug)]
pub struct SubscriptionUnitRequest {
    pub context: PixivRequestContext,
    pub unit_id: Uuid,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionExecutionResult {
    pub status: DomainRunStatus,
    pub discovered_count: i32,
    pub ignored_count: i32,
    pub error_class: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubscriptionUnitAttemptResult {
    pub result: SubscriptionExecutionResult,
    pub completion: Option<FinishSubscriptionRunUnit>,
}

#[derive(Clone, Debug)]
struct ExecutedPage {
    discovered_count: i32,
    ignored_count: i32,
    cursor_value: Option<Value>,
}

#[derive(Clone, Copy)]
enum UnitExecutionOwnership {
    Synchronous,
    Job {
        lease: JobLease,
        priority: JobPriority,
    },
}

impl UnitExecutionOwnership {
    fn download_priority(self) -> JobPriority {
        match self {
            Self::Synchronous => JobPriority::ScheduledCollection,
            Self::Job { priority, .. } => priority,
        }
    }
}

fn cursor_page(cursor: Option<&Value>, default_page: u32) -> Result<u32, JobErrorClass> {
    let Some(cursor) = cursor else {
        return Ok(default_page);
    };
    cursor
        .get("page")
        .and_then(Value::as_u64)
        .and_then(|page| u32::try_from(page).ok())
        .ok_or(JobErrorClass::Permanent)
}

fn ranking_cursor_date(cursor: Option<&Value>) -> Option<Date> {
    cursor
        .and_then(|value| value.get("date"))
        .filter(|value| !value.is_null())
        .and_then(|value| serde_json::from_value(value.clone()).ok())
}

fn ranking_max_rank(params: &Value) -> Result<u32, JobErrorClass> {
    let max_rank = params
        .get("max_rank")
        .and_then(Value::as_u64)
        .map(u32::try_from)
        .transpose()
        .map_err(|_| JobErrorClass::Permanent)?
        .unwrap_or(20);
    (1..=5_000)
        .contains(&max_rank)
        .then_some(max_rank)
        .ok_or(JobErrorClass::Permanent)
}

fn ranking_date_time(date: Date) -> OffsetDateTime {
    date.with_time(Time::MIDNIGHT).assume_utc()
}

fn enum_field<T>(params: &Value, key: &str) -> Result<T, JobErrorClass>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(params.get(key).ok_or(JobErrorClass::Permanent)?.clone())
        .map_err(|_| JobErrorClass::Permanent)
}

fn following_error_class(error: FollowingServiceError) -> JobErrorClass {
    match error {
        FollowingServiceError::Storage(error) => database_error_class(&error),
        FollowingServiceError::Pixiv(error) => pixiv_error_class(error.class()),
    }
}

fn subscription_error_message(error_class: JobErrorClass) -> &'static str {
    match error_class {
        JobErrorClass::Network => "Pixiv 网络请求失败",
        JobErrorClass::Server => "Pixiv 服务暂时不可用或响应无法处理",
        JobErrorClass::RateLimit => "Pixiv 请求频率受限",
        JobErrorClass::CredentialInvalid => "Pixiv Cookie 已失效",
        JobErrorClass::Permanent => "来源数据或订阅参数无法处理",
    }
}

fn subscription_schedule(schedule: &Value) -> Result<SubscriptionSchedule, JobErrorClass> {
    SubscriptionSchedule::parse(schedule).map_err(|_| JobErrorClass::Permanent)
}

fn page_size(params: &Value, default: u32) -> u32 {
    params
        .get("page_size")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn unit_rule_document(
    unit: &pixivarchive_db::SubscriptionRunUnitRecord,
) -> Result<Option<RuleDefinitionV1>, JobErrorClass> {
    unit.rule_document
        .clone()
        .map(RuleDefinitionV1::parse)
        .transpose()
        .map_err(|_| JobErrorClass::Permanent)
}

#[cfg(test)]
mod tests {
    use super::pixiv_error_class;
    use pixivarchive_domain::job::JobErrorClass;
    use pixivarchive_pixiv::PixivErrorClass;

    #[test]
    fn invalid_json_or_interstitial_is_retryable_server_failure() {
        assert_eq!(
            pixiv_error_class(PixivErrorClass::InvalidJsonOrInterstitial),
            JobErrorClass::Server
        );
    }

    #[test]
    fn response_larger_than_the_configured_limit_is_permanent() {
        assert_eq!(
            pixiv_error_class(PixivErrorClass::ResponseTooLarge),
            JobErrorClass::Permanent
        );
    }
}
