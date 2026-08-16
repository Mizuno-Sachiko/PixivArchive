mod support;

use pixivarchive_db::{EventRepository, ImportRepository, JobRepository, QueueImportRequest};
use pixivarchive_domain::event::{EventPayload, EventResource};
use pixivarchive_domain::job::{JobKind, JobPriority, JobQuotaSelection, JobState, NewJob};
use pixivarchive_domain::subscription::ImportKind;
use serde_json::json;
use sqlx::Row;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn quota_selection_can_claim_background_maintenance_work_while_higher_priorities_exist() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());

    jobs.enqueue(NewJob::new(
        JobPriority::Immediate,
        JobKind::ImportWork.as_str(),
        json!({ "pixiv_id": 1001 }),
    ))
    .await
    .unwrap();
    let expected_background = jobs
        .enqueue(NewJob::new(
            JobPriority::BackgroundMaintenance,
            JobKind::GenerateDerivative.as_str(),
            json!({ "pixiv_id": 1002 }),
        ))
        .await
        .unwrap();
    jobs.enqueue(NewJob::new(
        JobPriority::ManualImport,
        JobKind::ImportArtist.as_str(),
        json!({ "pixiv_id": 1003 }),
    ))
    .await
    .unwrap();
    jobs.enqueue(NewJob::new(
        JobPriority::ScheduledCollection,
        JobKind::RankingCollection.as_str(),
        json!({ "mode": "daily" }),
    ))
    .await
    .unwrap();

    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::BackgroundMaintenance]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .expect("background maintenance job should be claimable while higher priorities exist");

    assert_eq!(claimed.id, expected_background);
    assert_eq!(claimed.priority, JobPriority::BackgroundMaintenance);
    assert_eq!(claimed.state, JobState::Running);
}

#[tokio::test]
async fn kind_restricted_selection_skips_older_work_from_another_stage() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());

    let discovery = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::ScheduledCollection,
            JobKind::RankingCollection,
            json!({ "mode": "daily" }),
        ))
        .await
        .unwrap();
    let download = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::ScheduledCollection,
            JobKind::DownloadMedia,
            json!({ "work_id": Uuid::now_v7(), "pixiv_work_id": 1002 }),
        ))
        .await
        .unwrap();

    let selection = JobQuotaSelection::with_fallback(JobPriority::ScheduledCollection)
        .restricted_to([JobKind::DownloadMedia]);
    let claimed = jobs
        .claim_next(Uuid::now_v7(), &selection, Duration::minutes(5))
        .await
        .unwrap()
        .expect("the media stage should claim its own queued work");

    assert_eq!(claimed.id, download);
    assert_eq!(jobs.get(discovery).await.unwrap().state, JobState::Queued);
}

#[tokio::test]
async fn completing_a_job_appends_a_replayable_event() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let events = EventRepository::new(db.clone());

    let job_id = jobs
        .enqueue(NewJob::new(
            JobPriority::Immediate,
            JobKind::ImportWork.as_str(),
            json!({ "pixiv_id": 2001 }),
        ))
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::Immediate]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    jobs.complete(claimed.lease(), pixivarchive_db::JobCompletion::TaskOnly)
        .await
        .unwrap();

    let replay = events.list_after(0, 10).await.unwrap();
    assert!(replay.iter().any(|event| {
        event.resource == EventResource::Job
            && event.resource_id == job_id
            && matches!(event.payload, EventPayload::JobCompleted { .. })
    }));
}

#[tokio::test]
async fn concurrent_claimers_do_not_receive_the_same_job() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let job_id = jobs
        .enqueue(NewJob::new(
            JobPriority::Immediate,
            JobKind::ImportWork.as_str(),
            json!({ "pixiv_id": 2101 }),
        ))
        .await
        .unwrap();

    let first = jobs.clone();
    let second = jobs.clone();
    let selection = JobQuotaSelection::new(vec![JobPriority::Immediate]);
    let (a, b) = tokio::join!(
        first.claim_next(Uuid::now_v7(), &selection, Duration::minutes(5)),
        second.claim_next(Uuid::now_v7(), &selection, Duration::minutes(5))
    );

    let claimed: Vec<_> = [a.unwrap(), b.unwrap()].into_iter().flatten().collect();
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, job_id);
}

#[tokio::test]
async fn lease_expiry_controls_whether_running_work_can_be_claimed_again() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let job_id = jobs
        .enqueue(NewJob::new(
            JobPriority::Immediate,
            JobKind::ImportWork.as_str(),
            json!({ "pixiv_id": 2201 }),
        ))
        .await
        .unwrap();
    let selection = JobQuotaSelection::new(vec![JobPriority::Immediate]);

    let first_claim = jobs
        .claim_next(Uuid::now_v7(), &selection, Duration::minutes(5))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first_claim.id, job_id);
    assert!(
        jobs.claim_next(Uuid::now_v7(), &selection, Duration::minutes(5))
            .await
            .unwrap()
            .is_none()
    );

    sqlx::query("UPDATE job SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(job_id)
        .execute(db.pool())
        .await
        .unwrap();

    let reclaimed = jobs
        .claim_next(Uuid::now_v7(), &selection, Duration::minutes(5))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reclaimed.id, job_id);

    let running_attempts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job_attempt WHERE job_id = $1 AND state = 'running'",
    )
    .bind(job_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(running_attempts, 1);

    let expired_attempts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job_attempt WHERE job_id = $1 AND state = 'failed' AND error_class = 'lease_expired'",
    )
    .bind(job_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(expired_attempts, 1);
}

#[tokio::test]
async fn fail_records_details_attempt_and_outbox_event() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let events = EventRepository::new(db.clone());
    let job_id = jobs
        .enqueue(NewJob::new(
            JobPriority::Immediate,
            JobKind::ImportWork.as_str(),
            json!({ "pixiv_id": 2301 }),
        ))
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::Immediate]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    jobs.fail(
        claimed.lease(),
        "network",
        true,
        Some(OffsetDateTime::now_utc() + Duration::minutes(1)),
        Some("Pixiv request timed out"),
    )
    .await
    .unwrap();

    let row = sqlx::query!(
        "SELECT state, error_class, retryable, attempts, resource_revision FROM job WHERE id = $1",
        job_id
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(row.state, "failed");
    assert_eq!(row.error_class.as_deref(), Some("network"));
    assert_eq!(row.retryable, Some(true));
    assert_eq!(row.attempts, 1);
    assert!(row.resource_revision > claimed.resource_revision);

    let attempt = sqlx::query!(
        "SELECT state, message FROM job_attempt WHERE job_id = $1 AND attempt_number = 1",
        job_id
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(attempt.state, "failed");
    assert_eq!(attempt.message.as_deref(), Some("Pixiv request timed out"));

    let replay = events.list_after(0, 20).await.unwrap();
    assert!(
        replay
            .iter()
            .any(|event| matches!(event.payload, EventPayload::JobFailed { .. }))
    );
}

#[tokio::test]
async fn retryable_failed_jobs_wait_until_their_retry_time_then_claim_cleanly() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let job_id = jobs
        .enqueue(NewJob::new(
            JobPriority::Immediate,
            JobKind::ImportWork.as_str(),
            json!({ "pixiv_id": 2351 }),
        ))
        .await
        .unwrap();
    let selection = JobQuotaSelection::new(vec![JobPriority::Immediate]);
    let claimed = jobs
        .claim_next(Uuid::now_v7(), &selection, Duration::minutes(5))
        .await
        .unwrap()
        .unwrap();

    jobs.fail(
        claimed.lease(),
        "network",
        true,
        Some(OffsetDateTime::now_utc() + Duration::minutes(10)),
        None,
    )
    .await
    .unwrap();

    assert!(
        jobs.claim_next(Uuid::now_v7(), &selection, Duration::minutes(5))
            .await
            .unwrap()
            .is_none()
    );

    sqlx::query("UPDATE job SET next_retry_at = now() - interval '1 second' WHERE id = $1")
        .bind(job_id)
        .execute(db.pool())
        .await
        .unwrap();

    let retried = jobs
        .claim_next(Uuid::now_v7(), &selection, Duration::minutes(5))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retried.id, job_id);
    assert_eq!(retried.priority, JobPriority::Immediate);
    assert_eq!(retried.state, JobState::Running);

    let job = sqlx::query!(
        r#"
        SELECT state, attempts, error_class, retryable, next_retry_at
        FROM job
        WHERE id = $1
        "#,
        job_id
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(job.state, "running");
    assert_eq!(job.attempts, 2);
    assert!(job.error_class.is_none());
    assert!(job.retryable.is_none());
    assert!(job.next_retry_at.is_none());

    let running_attempts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job_attempt WHERE job_id = $1 AND state = 'running'",
    )
    .bind(job_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(running_attempts, 1);
}

#[tokio::test]
async fn failing_jobs_requires_retry_fields_to_match_terminal_state() {
    let _locked = support::LockedDb::new().await;
    let jobs = JobRepository::new(_locked.db.clone());
    let selection = JobQuotaSelection::new(vec![JobPriority::Immediate]);

    let retry_job = jobs
        .enqueue(NewJob::new(
            JobPriority::Immediate,
            JobKind::ImportWork.as_str(),
            json!({ "pixiv_id": 2352 }),
        ))
        .await
        .unwrap();
    let retry_claim = jobs
        .claim_next(Uuid::now_v7(), &selection, Duration::minutes(5))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retry_claim.id, retry_job);
    let retry_without_time = jobs
        .fail(retry_claim.lease(), "network", true, None, None)
        .await;
    assert!(retry_without_time.is_err());

    let terminal_job = jobs
        .enqueue(NewJob::new(
            JobPriority::Immediate,
            JobKind::ImportWork.as_str(),
            json!({ "pixiv_id": 2353 }),
        ))
        .await
        .unwrap();
    let terminal_claim = jobs
        .claim_next(Uuid::now_v7(), &selection, Duration::minutes(5))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(terminal_claim.id, terminal_job);
    let terminal_with_time = jobs
        .fail(
            terminal_claim.lease(),
            JobKind::ImportArtist.as_str(),
            false,
            Some(OffsetDateTime::now_utc() + Duration::minutes(1)),
            None,
        )
        .await;
    assert!(terminal_with_time.is_err());
}

#[tokio::test]
async fn retrying_a_failed_import_job_resets_its_run_in_the_same_transaction() {
    let locked = support::LockedDb::new().await;
    let db = locked.db.clone();
    let account_id = insert_pixiv_account(&db, 30_001).await;
    let queued = ImportRepository::new(db.clone())
        .queue(
            QueueImportRequest {
                account_id,
                kind: ImportKind::Work,
                target_pixiv_id: 3_001,
                forced: false,
                rule_document: None,
            },
            JobPriority::ManualImport,
        )
        .await
        .unwrap();
    let jobs = JobRepository::new(db.clone());
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ManualImport]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(Some(claimed.id), queued.job_id);
    sqlx::query(
        r#"
        UPDATE import_run
        SET status = 'running',
            discovered_count = 4,
            saved_count = 2,
            started_at = now()
        WHERE id = $1
        "#,
    )
    .bind(queued.id)
    .execute(db.pool())
    .await
    .unwrap();
    jobs.fail(
        claimed.lease(),
        "permanent",
        false,
        None,
        Some("invalid response"),
    )
    .await
    .unwrap();

    let failed = jobs.get(claimed.id).await.unwrap();
    jobs.retry_requested(failed.id, failed.resource_revision)
        .await
        .unwrap();

    let state = sqlx::query(
        r#"
        SELECT j.state AS job_state,
               r.status AS run_status,
               r.discovered_count,
               r.saved_count,
               r.error_class,
               r.error_message,
               r.started_at,
               r.finished_at
        FROM job j
        JOIN import_run r ON r.job_id = j.id
        WHERE j.id = $1
        "#,
    )
    .bind(claimed.id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(state.get::<String, _>("job_state"), "queued");
    assert_eq!(state.get::<String, _>("run_status"), "queued");
    assert_eq!(state.get::<i32, _>("discovered_count"), 0);
    assert_eq!(state.get::<i32, _>("saved_count"), 0);
    assert!(state.get::<Option<String>, _>("error_class").is_none());
    assert!(state.get::<Option<String>, _>("error_message").is_none());
    assert!(
        state
            .get::<Option<OffsetDateTime>, _>("started_at")
            .is_none()
    );
    assert!(
        state
            .get::<Option<OffsetDateTime>, _>("finished_at")
            .is_none()
    );
}

#[tokio::test]
async fn retrying_an_automatically_requeued_import_run_clears_attempt_details() {
    let locked = support::LockedDb::new().await;
    let db = locked.db.clone();
    let account_id = insert_pixiv_account(&db, 30_002).await;
    let queued = ImportRepository::new(db.clone())
        .queue(
            QueueImportRequest {
                account_id,
                kind: ImportKind::Work,
                target_pixiv_id: 3_002,
                forced: false,
                rule_document: None,
            },
            JobPriority::ManualImport,
        )
        .await
        .unwrap();
    let jobs = JobRepository::new(db.clone());
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ManualImport]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(Some(claimed.id), queued.job_id);
    sqlx::query(
        r#"
        UPDATE import_run
        SET status = 'queued',
            discovered_count = 3,
            saved_count = 1,
            error_class = 'network',
            error_message = 'timed out',
            started_at = now()
        WHERE id = $1
        "#,
    )
    .bind(queued.id)
    .execute(db.pool())
    .await
    .unwrap();
    jobs.fail(
        claimed.lease(),
        "network",
        true,
        Some(OffsetDateTime::now_utc() + Duration::minutes(10)),
        Some("timed out"),
    )
    .await
    .unwrap();

    let failed = jobs.get(claimed.id).await.unwrap();
    jobs.retry_requested(failed.id, failed.resource_revision)
        .await
        .unwrap();

    let state = sqlx::query(
        r#"
        SELECT status,
               discovered_count,
               saved_count,
               error_class,
               error_message,
               started_at,
               finished_at
        FROM import_run
        WHERE id = $1
        "#,
    )
    .bind(queued.id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(state.get::<String, _>("status"), "queued");
    assert_eq!(state.get::<i32, _>("discovered_count"), 0);
    assert_eq!(state.get::<i32, _>("saved_count"), 0);
    assert!(state.get::<Option<String>, _>("error_class").is_none());
    assert!(state.get::<Option<String>, _>("error_message").is_none());
    assert!(
        state
            .get::<Option<OffsetDateTime>, _>("started_at")
            .is_none()
    );
    assert!(
        state
            .get::<Option<OffsetDateTime>, _>("finished_at")
            .is_none()
    );
}

#[tokio::test]
async fn retrying_an_import_with_a_successful_run_rolls_back_the_job_update() {
    let locked = support::LockedDb::new().await;
    let db = locked.db.clone();
    let account_id = insert_pixiv_account(&db, 30_003).await;
    let queued = ImportRepository::new(db.clone())
        .queue(
            QueueImportRequest {
                account_id,
                kind: ImportKind::Work,
                target_pixiv_id: 3_003,
                forced: false,
                rule_document: None,
            },
            JobPriority::ManualImport,
        )
        .await
        .unwrap();
    let jobs = JobRepository::new(db.clone());
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ManualImport]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(Some(claimed.id), queued.job_id);
    sqlx::query("UPDATE import_run SET status = 'download_queued' WHERE id = $1")
        .bind(queued.id)
        .execute(db.pool())
        .await
        .unwrap();
    jobs.fail(
        claimed.lease(),
        "network",
        true,
        Some(OffsetDateTime::now_utc() + Duration::minutes(10)),
        None,
    )
    .await
    .unwrap();
    let failed = jobs.get(claimed.id).await.unwrap();

    let result = jobs
        .retry_requested(failed.id, failed.resource_revision)
        .await;
    assert!(matches!(
        result,
        Err(pixivarchive_db::DbError::RevisionConflict)
    ));
    let unchanged = jobs.get(claimed.id).await.unwrap();
    assert_eq!(unchanged.state, JobState::Failed);
    assert_eq!(unchanged.resource_revision, failed.resource_revision);
}

#[tokio::test]
async fn cancel_checks_revision_records_attempt_and_outbox_event() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let events = EventRepository::new(db.clone());
    let job_id = jobs
        .enqueue(NewJob::new(
            JobPriority::Immediate,
            JobKind::ImportWork.as_str(),
            json!({ "pixiv_id": 2401 }),
        ))
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::Immediate]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    let conflict = jobs
        .cancel_requested(job_id, claimed.resource_revision - 1)
        .await;
    assert!(conflict.is_err());

    jobs.cancel_requested(job_id, claimed.resource_revision)
        .await
        .unwrap();
    let state: String = sqlx::query_scalar("SELECT state FROM job WHERE id = $1")
        .bind(job_id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(state, "cancelled");

    let attempt_state: String = sqlx::query_scalar(
        "SELECT state FROM job_attempt WHERE job_id = $1 AND attempt_number = 1",
    )
    .bind(job_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(attempt_state, "cancelled");

    let replay = events.list_after(0, 20).await.unwrap();
    assert!(
        replay
            .iter()
            .any(|event| matches!(event.payload, EventPayload::JobCancelled { .. }))
    );
}

#[tokio::test]
async fn storage_write_pause_only_holds_jobs_that_create_media_files() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let download = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::ManualImport,
            JobKind::DownloadMedia,
            json!({ "work_id": Uuid::now_v7(), "pixiv_work_id": 2501 }),
        ))
        .await
        .unwrap();
    let derivative = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::BackgroundMaintenance,
            JobKind::GenerateDerivative,
            json!({ "media_revision_id": Uuid::now_v7() }),
        ))
        .await
        .unwrap();
    let metadata = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::ManualImport,
            JobKind::ImportWork,
            json!({ "pixiv_work_id": 2502 }),
        ))
        .await
        .unwrap();

    assert_eq!(jobs.set_storage_write_allowed(false).await.unwrap(), 2);
    assert_eq!(
        jobs.get(download).await.unwrap().state,
        JobState::WaitingStorage
    );
    assert_eq!(
        jobs.get(derivative).await.unwrap().state,
        JobState::WaitingStorage
    );
    assert_eq!(jobs.get(metadata).await.unwrap().state, JobState::Queued);

    assert_eq!(jobs.set_storage_write_allowed(true).await.unwrap(), 2);
    assert_eq!(jobs.get(download).await.unwrap().state, JobState::Queued);
    assert_eq!(jobs.get(derivative).await.unwrap().state, JobState::Queued);
}

#[tokio::test]
async fn storage_pause_preserves_retryable_media_failures() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db);
    let job_id = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::ManualImport,
            JobKind::DownloadMedia,
            json!({ "work_id": Uuid::now_v7(), "pixiv_work_id": 2551 }),
        ))
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ManualImport]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    let retry_at = OffsetDateTime::now_utc().replace_nanosecond(0).unwrap() + Duration::minutes(20);
    jobs.fail(
        claimed.lease(),
        "network",
        true,
        Some(retry_at),
        Some("temporary"),
    )
    .await
    .unwrap();

    assert_eq!(jobs.set_storage_write_allowed(false).await.unwrap(), 0);
    let failed = jobs.get(job_id).await.unwrap();
    assert_eq!(failed.state, JobState::Failed);
    assert_eq!(failed.error_class.as_deref(), Some("network"));
    assert_eq!(failed.retryable, Some(true));
    assert_eq!(failed.next_retry_at, Some(retry_at));
}

#[tokio::test]
async fn running_media_job_enters_storage_wait_without_consuming_a_retry() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let events = EventRepository::new(db.clone());
    let job_id = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::ManualImport,
            JobKind::DownloadMedia,
            json!({ "work_id": Uuid::now_v7(), "pixiv_work_id": 2601 }),
        ))
        .await
        .unwrap();
    let claimed = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::ManualImport]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    jobs.wait_for_storage(claimed.lease()).await.unwrap();

    let waiting = jobs.get(job_id).await.unwrap();
    assert_eq!(waiting.state, JobState::WaitingStorage);
    assert_eq!(waiting.attempts, 1);
    assert!(waiting.error_class.is_none());
    assert!(waiting.retryable.is_none());
    assert!(waiting.next_retry_at.is_none());
    let attempt = jobs.list_attempts(job_id).await.unwrap();
    assert_eq!(attempt[0].state, "waiting_storage");
    assert!(attempt[0].error_class.is_none());

    let replay = events.list_after(0, 20).await.unwrap();
    assert!(
        replay
            .iter()
            .any(|event| { matches!(event.payload, EventPayload::JobWaitingStorage { .. }) })
    );
}

async fn insert_pixiv_account(db: &pixivarchive_db::Db, pixiv_user_id: i64) -> Uuid {
    let account_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO pixiv_account (
            id,
            pixiv_user_id,
            display_name,
            state,
            cookie_key_id,
            cookie_nonce,
            cookie_ciphertext
        )
        VALUES (
            $1,
            $2,
            'import owner',
            'normal',
            'test',
            decode('000000000000000000000000', 'hex'),
            decode('01', 'hex')
        )
        "#,
    )
    .bind(account_id)
    .bind(pixiv_user_id)
    .execute(db.pool())
    .await
    .unwrap();
    account_id
}
