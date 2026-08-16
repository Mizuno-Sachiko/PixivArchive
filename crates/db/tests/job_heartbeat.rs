mod support;

use pixivarchive_db::{DbError, JobRepository};
use pixivarchive_domain::job::{JobKind, JobPriority, JobQuotaSelection, NewJob};
use serde_json::json;
use sqlx::Row;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn heartbeat_extends_running_lease_and_keeps_attempts_unchanged() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let owner = Uuid::now_v7();
    let claimed = enqueue_and_claim(&jobs, owner).await;
    let previous_lease: OffsetDateTime =
        sqlx::query_scalar("SELECT lease_expires_at FROM job WHERE id = $1")
            .bind(claimed.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    let lower_bound: OffsetDateTime = sqlx::query_scalar("SELECT now()")
        .fetch_one(db.pool())
        .await
        .unwrap();

    let heartbeat = jobs
        .heartbeat(
            claimed.id,
            claimed.resource_revision,
            owner,
            Duration::minutes(30),
        )
        .await
        .unwrap();

    assert_eq!(heartbeat.resource_revision, claimed.resource_revision);
    assert!(heartbeat.lease_expires_at >= lower_bound + Duration::minutes(30));
    assert!(heartbeat.lease_expires_at > previous_lease);

    let job =
        sqlx::query("SELECT attempts, resource_revision, lease_expires_at FROM job WHERE id = $1")
            .bind(claimed.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(job.get::<i32, _>("attempts"), 1);
    assert_eq!(
        job.get::<i64, _>("resource_revision"),
        claimed.resource_revision
    );
    assert_eq!(
        job.get::<Option<OffsetDateTime>, _>("lease_expires_at"),
        Some(heartbeat.lease_expires_at)
    );

    let running_attempts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM job_attempt WHERE job_id = $1 AND state = 'running'",
    )
    .bind(claimed.id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(running_attempts, 1);

    let event_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app_event WHERE resource_id = $1")
            .bind(claimed.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(event_count, 2);
}

#[tokio::test]
async fn heartbeat_rejects_stale_owner() {
    let _locked = support::LockedDb::new().await;
    let jobs = JobRepository::new(_locked.db.clone());
    let owner = Uuid::now_v7();
    let claimed = enqueue_and_claim(&jobs, owner).await;

    let error = jobs
        .heartbeat(
            claimed.id,
            claimed.resource_revision,
            Uuid::now_v7(),
            Duration::minutes(30),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, DbError::LeaseConflict));
}

#[tokio::test]
async fn heartbeat_rejects_stale_revision() {
    let _locked = support::LockedDb::new().await;
    let jobs = JobRepository::new(_locked.db.clone());
    let owner = Uuid::now_v7();
    let claimed = enqueue_and_claim(&jobs, owner).await;

    let error = jobs
        .heartbeat(
            claimed.id,
            claimed.resource_revision - 1,
            owner,
            Duration::minutes(30),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, DbError::RevisionConflict));
}

#[tokio::test]
async fn heartbeat_rejects_old_worker_after_expired_lease_is_reclaimed() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let first_owner = Uuid::now_v7();
    let claimed = enqueue_and_claim(&jobs, first_owner).await;

    sqlx::query("UPDATE job SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(claimed.id)
        .execute(db.pool())
        .await
        .unwrap();

    let second_owner = Uuid::now_v7();
    let reclaimed = jobs
        .claim_next(
            second_owner,
            &JobQuotaSelection::new(vec![JobPriority::Immediate]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();

    let error = jobs
        .heartbeat(
            claimed.id,
            claimed.resource_revision,
            first_owner,
            Duration::minutes(30),
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        DbError::RevisionConflict | DbError::LeaseConflict
    ));

    let row = sqlx::query("SELECT resource_revision, lease_owner FROM job WHERE id = $1")
        .bind(claimed.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(
        row.get::<i64, _>("resource_revision"),
        reclaimed.resource_revision
    );
    assert_eq!(
        row.get::<Option<Uuid>, _>("lease_owner"),
        Some(second_owner)
    );
}

#[tokio::test]
async fn heartbeat_rejects_expired_lease_before_another_worker_reclaims_it() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let owner = Uuid::now_v7();
    let claimed = enqueue_and_claim(&jobs, owner).await;

    sqlx::query("UPDATE job SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(claimed.id)
        .execute(db.pool())
        .await
        .unwrap();

    let error = jobs
        .heartbeat(
            claimed.id,
            claimed.resource_revision,
            owner,
            Duration::minutes(30),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, DbError::LeaseConflict));
}

#[tokio::test]
async fn heartbeat_uses_fixed_window_instead_of_accumulating_each_beat() {
    let _locked = support::LockedDb::new().await;
    let jobs = JobRepository::new(_locked.db.clone());
    let owner = Uuid::now_v7();
    let claimed = enqueue_and_claim(&jobs, owner).await;

    let first = jobs
        .heartbeat(
            claimed.id,
            claimed.resource_revision,
            owner,
            Duration::minutes(30),
        )
        .await
        .unwrap();
    let second = jobs
        .heartbeat(
            claimed.id,
            claimed.resource_revision,
            owner,
            Duration::minutes(30),
        )
        .await
        .unwrap();

    assert!(second.lease_expires_at >= first.lease_expires_at);
    assert!(second.lease_expires_at < first.lease_expires_at + Duration::seconds(10));
}

#[tokio::test]
async fn heartbeat_rejects_terminal_jobs() {
    let _locked = support::LockedDb::new().await;
    let jobs = JobRepository::new(_locked.db.clone());
    let owner = Uuid::now_v7();
    let claimed = enqueue_and_claim(&jobs, owner).await;
    jobs.complete(claimed.lease(), pixivarchive_db::JobCompletion::TaskOnly)
        .await
        .unwrap();

    let error = jobs
        .heartbeat(
            claimed.id,
            claimed.resource_revision + 1,
            owner,
            Duration::minutes(30),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, DbError::RevisionConflict));
}

#[tokio::test]
async fn terminal_transition_rejects_an_expired_lease() {
    let _locked = support::LockedDb::new().await;
    let jobs = JobRepository::new(_locked.db.clone());
    let claimed = enqueue_and_claim(&jobs, Uuid::now_v7()).await;
    sqlx::query("UPDATE job SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(claimed.id)
        .execute(_locked.db.pool())
        .await
        .unwrap();

    let error = jobs
        .complete(claimed.lease(), pixivarchive_db::JobCompletion::TaskOnly)
        .await
        .unwrap_err();

    assert!(matches!(error, DbError::LeaseConflict));
    let state: String = sqlx::query_scalar("SELECT state FROM job WHERE id = $1")
        .bind(claimed.id)
        .fetch_one(_locked.db.pool())
        .await
        .unwrap();
    assert_eq!(state, "running");
}

#[tokio::test]
async fn claim_next_rejects_non_positive_lease_duration() {
    let _locked = support::LockedDb::new().await;
    let jobs = JobRepository::new(_locked.db.clone());
    let selection = JobQuotaSelection::new(vec![JobPriority::Immediate]);

    for lease_duration in [Duration::ZERO, Duration::seconds(-1)] {
        let error = jobs
            .claim_next(Uuid::now_v7(), &selection, lease_duration)
            .await
            .unwrap_err();
        assert!(matches!(error, DbError::InvalidValue(_)));
    }
}

#[tokio::test]
async fn claim_next_returns_the_database_lease_expiry() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let owner = Uuid::now_v7();

    let claimed = enqueue_and_claim(&jobs, owner).await;

    let stored_lease: Option<OffsetDateTime> =
        sqlx::query_scalar("SELECT lease_expires_at FROM job WHERE id = $1")
            .bind(claimed.id)
            .fetch_one(db.pool())
            .await
            .unwrap();

    assert_eq!(stored_lease, Some(claimed.lease_expires_at));
}

#[tokio::test]
async fn heartbeat_rejects_non_positive_extension() {
    let _locked = support::LockedDb::new().await;
    let jobs = JobRepository::new(_locked.db.clone());
    let owner = Uuid::now_v7();
    let claimed = enqueue_and_claim(&jobs, owner).await;

    for extend_by in [Duration::ZERO, Duration::seconds(-1)] {
        let error = jobs
            .heartbeat(claimed.id, claimed.resource_revision, owner, extend_by)
            .await
            .unwrap_err();
        assert!(matches!(error, DbError::InvalidValue(_)));
    }
}

async fn enqueue_and_claim(
    jobs: &JobRepository,
    owner: Uuid,
) -> pixivarchive_domain::job::ClaimedJob {
    jobs.enqueue(NewJob::new(
        JobPriority::Immediate,
        JobKind::ImportWork.as_str(),
        json!({ "pixiv_id": 2501 }),
    ))
    .await
    .unwrap();
    jobs.claim_next(
        owner,
        &JobQuotaSelection::new(vec![JobPriority::Immediate]),
        Duration::minutes(5),
    )
    .await
    .unwrap()
    .unwrap()
}
