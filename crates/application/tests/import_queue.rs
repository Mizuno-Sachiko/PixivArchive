use pixivarchive_application::{
    imports::{
        ImportQueueService, ImportRequest, ImportService, ImportServiceError, ImportStrategy,
        QueueImportRequest,
    },
    pixiv_accounts::{AccountCookieUpdate, PixivAccountService},
    rules::{PublishRuleVersionRequest, RuleService},
    settings::{QueueSettings, SettingValue, SettingsService},
};
use pixivarchive_db::{DbError, ImportJobCompletion, JobCompletion, JobRepository};
use pixivarchive_domain::settings::SettingGroupKey;
use pixivarchive_domain::{
    job::{JobKind, JobPriority, JobQuotaSelection},
    rule::{RuleAction, RuleDefinitionV1},
    subscription::{ImportKind, ImportRunStatus},
};
use pixivarchive_test_support::{DISCOVERY_LOCK_ID, FakePixivGateway, LockedDb, context};
use sqlx::Row;
use time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn manual_rule_import_stores_the_current_published_rule_snapshot() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway).await;
    let rules = RuleService::new(locked.db.clone());
    let rule = rules
        .create_rule("manual import rule", RuleAction::Ignore)
        .await
        .unwrap();
    let draft = rules.load_draft(rule.id).await.unwrap().unwrap();
    let published = rules
        .publish_version(PublishRuleVersionRequest {
            rule_id: rule.id,
            base_version: None,
            expected_draft_revision: draft.revision,
            created_by: None,
        })
        .await
        .unwrap();

    let queue = ImportQueueService::new(locked.db.clone());
    let queued = queue
        .queue(QueueImportRequest {
            account_id: account.id,
            kind: ImportKind::Work,
            target_pixiv_id: 2_099,
            strategy: ImportStrategy::Rule { rule_id: rule.id },
        })
        .await
        .unwrap();

    let stored: serde_json::Value =
        sqlx::query_scalar("SELECT params -> 'rule_document' FROM import_run WHERE id = $1")
            .bind(queued.run_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(stored, published.definition);

    let default = queue
        .queue(QueueImportRequest {
            account_id: account.id,
            kind: ImportKind::Work,
            target_pixiv_id: 2_100,
            strategy: ImportStrategy::Default,
        })
        .await
        .unwrap();
    let forced = queue
        .queue(QueueImportRequest {
            account_id: account.id,
            kind: ImportKind::Work,
            target_pixiv_id: 2_101,
            strategy: ImportStrategy::Forced,
        })
        .await
        .unwrap();
    let rows = sqlx::query(
        "SELECT id, forced, params -> 'rule_document' AS rule_document \
         FROM import_run WHERE id = ANY($1)",
    )
    .bind(vec![default.run_id, forced.run_id])
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    for row in rows {
        let run_id: Uuid = row.get("id");
        assert_eq!(row.get::<bool, _>("forced"), run_id == forced.run_id);
        assert!(row.get::<serde_json::Value, _>("rule_document").is_null());
    }

    let unavailable = queue
        .queue(QueueImportRequest {
            account_id: account.id,
            kind: ImportKind::Work,
            target_pixiv_id: 2_102,
            strategy: ImportStrategy::Rule {
                rule_id: Uuid::now_v7(),
            },
        })
        .await;
    assert!(matches!(
        unavailable,
        Err(pixivarchive_application::imports::ImportQueueError::RuleUnavailable)
    ));
}

#[tokio::test]
async fn stale_page_cannot_queue_an_import_for_the_previous_pixiv_account() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let accounts = PixivAccountService::new(locked.db.clone(), gateway);
    let account_a = accounts
        .update_cookie(AccountCookieUpdate {
            context: context(),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![0; 12],
            cookie_ciphertext: vec![1],
        })
        .await
        .unwrap();
    accounts
        .update_cookie(AccountCookieUpdate {
            context: pixivarchive_pixiv::PixivRequestContext::new(
                secrecy::SecretString::from("PHPSESSID=20002_account-b"),
                20_002,
                "PixivArchiveTest/1.0",
            ),
            cookie_key_id: "test".to_owned(),
            cookie_nonce: vec![1; 12],
            cookie_ciphertext: vec![2],
        })
        .await
        .unwrap();

    let result = ImportQueueService::new(locked.db.clone())
        .queue(QueueImportRequest {
            account_id: account_a.id,
            kind: ImportKind::Work,
            target_pixiv_id: 2_103,
            strategy: ImportStrategy::Default,
        })
        .await;

    assert!(matches!(
        result,
        Err(
            pixivarchive_application::imports::ImportQueueError::Storage(DbError::RevisionConflict)
        )
    ));
    let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM import_run")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(queued, 0);
}

#[tokio::test]
async fn import_command_creates_queued_run_and_manual_job_without_pixiv_network_work() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;

    let queued = ImportService::new(locked.db.clone(), gateway.clone())
        .queue(ImportRequest::work(account.id, context(), 2101).forced())
        .await
        .unwrap();

    assert_eq!(queued.account_id, account.id);
    assert_eq!(queued.kind, ImportKind::Work);
    assert_eq!(queued.target_pixiv_id, 2101);
    assert_eq!(queued.strategy, ImportStrategy::Forced);
    assert_eq!(queued.status, ImportRunStatus::Queued);
    assert_eq!(queued.discovered_count, 0);
    assert_eq!(queued.saved_count, 0);
    assert_eq!(queued.error_class, None);
    assert_eq!(queued.error_message, None);
    assert_eq!(queued.finished_at, None);
    let job_id = queued.job_id.expect("queued import must own a job");
    assert_eq!(gateway.work_detail_calls(), 0);
    let row = sqlx::query("SELECT status, job_id FROM import_run WHERE id = $1")
        .bind(queued.run_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("status"), "queued");
    assert_eq!(row.get::<uuid::Uuid, _>("job_id"), job_id);
    assert_eq!(
        job_kind(&locked, job_id).await,
        JobKind::ImportWork.as_str()
    );
}

#[tokio::test]
async fn configured_import_priority_is_used_by_the_root_and_download_child() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    let mut queue = QueueSettings::default();
    queue
        .job_priorities
        .iter_mut()
        .find(|mapping| mapping.job_kind == JobKind::ImportWork)
        .unwrap()
        .priority = JobPriority::Immediate;
    SettingsService::new(locked.db.clone())
        .update(SettingGroupKey::Queue, None, SettingValue::Queue(queue))
        .await
        .unwrap();
    let service = ImportService::new(locked.db.clone(), gateway);

    let queued = service
        .queue(ImportRequest::work(account.id, context(), 2_102).forced())
        .await
        .unwrap();
    let jobs = JobRepository::new(locked.db.clone());
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::Immediate]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(Some(claimed.id), queued.job_id);
    assert_eq!(claimed.priority, JobPriority::Immediate);

    let result = service
        .execute_queued_job_attempt(claimed.lease(), claimed.priority, queued.run_id, context())
        .await
        .unwrap();
    assert_eq!(result.result.status, ImportRunStatus::DownloadQueued);
    let download_priority: String = sqlx::query_scalar(
        "SELECT priority_class FROM job WHERE kind = 'download_media' AND payload ->> 'pixiv_work_id' = '2102'",
    )
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(download_priority, "immediate");
}

#[tokio::test]
async fn cancelling_a_running_import_job_finishes_its_import_run() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    let queued = ImportService::new(locked.db.clone(), gateway)
        .queue(ImportRequest::work(account.id, context(), 2151).forced())
        .await
        .unwrap();
    let jobs = JobRepository::new(locked.db.clone());
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ManualImport]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE import_run SET status = 'running', started_at = now() WHERE id = $1")
        .bind(queued.run_id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    jobs.cancel_requested(claimed.id, claimed.resource_revision)
        .await
        .unwrap();
    let completion = jobs
        .complete(
            claimed.lease(),
            JobCompletion::Import(ImportJobCompletion {
                status: ImportRunStatus::DownloadQueued,
                discovered_count: 1,
                saved_count: 1,
            }),
        )
        .await;
    assert!(matches!(completion, Err(DbError::LeaseConflict)));

    let row = sqlx::query(
        "SELECT status, error_class, error_message, finished_at FROM import_run WHERE id = $1",
    )
    .bind(queued.run_id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("status"), "cancelled");
    assert!(row.get::<Option<String>, _>("error_class").is_none());
    assert!(row.get::<Option<String>, _>("error_message").is_none());
    assert!(
        row.get::<Option<time::OffsetDateTime>, _>("finished_at")
            .is_some()
    );
}

#[tokio::test]
async fn successful_import_completion_updates_the_job_and_run_atomically() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    let queued = ImportService::new(locked.db.clone(), gateway)
        .queue(ImportRequest::work(account.id, context(), 2161).forced())
        .await
        .unwrap();
    let jobs = JobRepository::new(locked.db.clone());
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ManualImport]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    sqlx::query("UPDATE import_run SET status = 'running', started_at = now() WHERE id = $1")
        .bind(queued.run_id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    jobs.complete(
        claimed.lease(),
        JobCompletion::Import(ImportJobCompletion {
            status: ImportRunStatus::DownloadQueued,
            discovered_count: 1,
            saved_count: 1,
        }),
    )
    .await
    .unwrap();

    let state = sqlx::query(
        "SELECT j.state AS job_state, r.status AS run_status, r.discovered_count, r.saved_count FROM job j JOIN import_run r ON r.job_id = j.id WHERE j.id = $1",
    )
    .bind(claimed.id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(state.get::<String, _>("job_state"), "completed");
    assert_eq!(state.get::<String, _>("run_status"), "download_queued");
    assert_eq!(state.get::<i32, _>("discovered_count"), 1);
    assert_eq!(state.get::<i32, _>("saved_count"), 1);
    assert!(matches!(
        jobs.cancel_requested(claimed.id, claimed.resource_revision)
            .await,
        Err(DbError::RevisionConflict)
    ));
}

#[tokio::test]
async fn permanent_job_failure_finishes_its_import_run_atomically() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    let queued = ImportService::new(locked.db.clone(), gateway)
        .queue(ImportRequest::work(account.id, context(), 2171).forced())
        .await
        .unwrap();
    let jobs = JobRepository::new(locked.db.clone());
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ManualImport]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    jobs.fail(
        claimed.lease(),
        "permanent",
        false,
        None,
        Some("source data cannot be processed"),
    )
    .await
    .unwrap();

    let row = sqlx::query(
        "SELECT status, error_class, error_message, finished_at FROM import_run WHERE id = $1",
    )
    .bind(queued.run_id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("status"), "failed");
    assert_eq!(row.get::<String, _>("error_class"), "permanent");
    assert_eq!(
        row.get::<String, _>("error_message"),
        "source data cannot be processed"
    );
    assert!(
        row.get::<Option<time::OffsetDateTime>, _>("finished_at")
            .is_some()
    );
}

#[tokio::test]
async fn stale_import_worker_attempt_failure_requires_current_lease() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.fail_work_detail(pixivarchive_pixiv::PixivErrorClass::TemporaryPixivError);
    let pause = gateway.pause_work_detail();
    let account = account(&locked, gateway.clone()).await;
    let queued = ImportService::new(locked.db.clone(), gateway.clone())
        .queue(ImportRequest::work(account.id, context(), 2181).forced())
        .await
        .unwrap();
    let jobs = JobRepository::new(locked.db.clone());
    let first_claim = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ManualImport]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let first_lease = first_claim.lease();
    let first_job_id = first_claim.id;
    let first_revision = first_claim.resource_revision;
    let service = ImportService::new(locked.db.clone(), gateway);
    let attempt = tokio::spawn(async move {
        service
            .execute_queued_job_attempt(
                first_lease,
                JobPriority::ManualImport,
                queued.run_id,
                context(),
            )
            .await
    });
    pause.entered.wait().await;
    sqlx::query("UPDATE job SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(first_job_id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    let second_claim = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ManualImport]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second_claim.id, first_job_id);
    assert_ne!(second_claim.resource_revision, first_revision);
    pause.resume.wait().await;
    let attempt = attempt.await.unwrap().unwrap_err();
    assert!(matches!(
        attempt,
        ImportServiceError::Storage(DbError::LeaseConflict)
    ));

    let row = sqlx::query(
        "SELECT status, error_class, error_message, finished_at FROM import_run WHERE id = $1",
    )
    .bind(queued.run_id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>("status"), "running");
    assert!(row.get::<Option<String>, _>("error_class").is_none());
    assert!(row.get::<Option<String>, _>("error_message").is_none());
    assert!(
        row.get::<Option<time::OffsetDateTime>, _>("finished_at")
            .is_none()
    );
}

#[tokio::test]
async fn stale_import_worker_cannot_persist_a_successful_pixiv_response() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let pause = gateway.pause_work_detail();
    let account = account(&locked, gateway.clone()).await;
    let queued = ImportService::new(locked.db.clone(), gateway.clone())
        .queue(ImportRequest::work(account.id, context(), 2191).forced())
        .await
        .unwrap();
    let jobs = JobRepository::new(locked.db.clone());
    let first_claim = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ManualImport]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let first_lease = first_claim.lease();
    let service = ImportService::new(locked.db.clone(), gateway);
    let attempt = tokio::spawn(async move {
        service
            .execute_queued_job_attempt(
                first_lease,
                JobPriority::ManualImport,
                queued.run_id,
                context(),
            )
            .await
    });
    pause.entered.wait().await;
    sqlx::query("UPDATE job SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(first_claim.id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    let reclaimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ManualImport]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reclaimed.id, first_claim.id);
    pause.resume.wait().await;

    let error = attempt.await.unwrap().unwrap_err();
    assert!(
        matches!(error, ImportServiceError::Storage(DbError::LeaseConflict)),
        "unexpected stale import error: {error:?}"
    );
    let work_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM work WHERE pixiv_work_id = 2191")
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    let candidate_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM import_candidate WHERE import_run_id = $1")
            .bind(queued.run_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    let download_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job WHERE kind = 'download_media' AND payload ->> 'pixiv_work_id' = '2191'",
    )
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(work_count, 0);
    assert_eq!(candidate_count, 0);
    assert_eq!(download_count, 0);
}

#[tokio::test]
async fn import_worker_execution_marks_terminal_failure_instead_of_leaving_running() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    gateway.fail_work_detail(pixivarchive_pixiv::PixivErrorClass::HiddenOrNotFound);
    let account = account(&locked, gateway.clone()).await;
    let queued = ImportService::new(locked.db.clone(), gateway.clone())
        .queue(ImportRequest::work(account.id, context(), 2201).forced())
        .await
        .unwrap();

    let result = ImportService::new(locked.db.clone(), gateway)
        .execute_queued(queued.run_id, context())
        .await
        .unwrap();

    assert_eq!(result.status, ImportRunStatus::Failed);
    let row = sqlx::query("SELECT status, error_class, finished_at FROM import_run WHERE id = $1")
        .bind(queued.run_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("status"), "failed");
    assert_eq!(
        row.get::<Option<String>, _>("error_class").as_deref(),
        Some("permanent")
    );
    assert!(
        row.get::<Option<time::OffsetDateTime>, _>("finished_at")
            .is_some()
    );
}

#[tokio::test]
async fn queued_import_executes_the_rule_document_stored_with_the_command() {
    let locked = LockedDb::new(DISCOVERY_LOCK_ID).await;
    let gateway = FakePixivGateway::new();
    let account = account(&locked, gateway.clone()).await;
    let rule_document = RuleDefinitionV1::match_all(
        uuid::Uuid::now_v7(),
        "download all",
        RuleAction::Download,
        RuleAction::Download,
    );
    let service = ImportService::new(locked.db.clone(), gateway);
    let queued = service
        .queue(
            ImportRequest::work(account.id, context(), 2301)
                .with_rule_document(rule_document.clone()),
        )
        .await
        .unwrap();

    let stored: serde_json::Value =
        sqlx::query_scalar("SELECT params -> 'rule_document' FROM import_run WHERE id = $1")
            .bind(queued.run_id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(RuleDefinitionV1::parse(stored).unwrap(), rule_document);

    let result = service
        .execute_queued(queued.run_id, context())
        .await
        .unwrap();
    assert_eq!(result.status, ImportRunStatus::DownloadQueued);
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

async fn job_kind(locked: &LockedDb, job_id: uuid::Uuid) -> String {
    sqlx::query_scalar("SELECT kind FROM job WHERE id = $1")
        .bind(job_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap()
}
