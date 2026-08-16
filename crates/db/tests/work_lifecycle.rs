mod support;

use pixivarchive_db::{
    DbError, EventRepository, JobRepository, TrashPurgeFailure, TrashRepository, WorkRepository,
};
use pixivarchive_domain::event::{EventPayload, EventResource};
use pixivarchive_domain::job::{CollectionState, JobKind, JobPriority, JobQuotaSelection, NewJob};
use pixivarchive_domain::work::TrashActionBlockReason;
use serde_json::json;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn work_can_move_through_metadata_trash_restore_and_deletion_marker() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let works = WorkRepository::new(db.clone());

    let work = works
        .create_metadata_only(900_001, 700_001, "first saved title")
        .await
        .unwrap();
    assert_eq!(work.collection_state, CollectionState::MetadataOnly);
    assert_eq!(work.id.get_version_num(), 7);

    works
        .move_to_trash(work.id, OffsetDateTime::now_utc() + Duration::days(30))
        .await
        .unwrap();
    let trashed = works.find_by_pixiv_id(900_001).await.unwrap().unwrap();
    assert_eq!(trashed.collection_state, CollectionState::Trash);

    works.restore(work.id).await.unwrap();
    let restored = works.find_by_pixiv_id(900_001).await.unwrap().unwrap();
    assert_eq!(restored.collection_state, CollectionState::MetadataOnly);

    works
        .mark_physically_deleted(900_001, "manual_purge")
        .await
        .unwrap();
    assert!(works.find_by_pixiv_id(900_001).await.unwrap().is_none());
    assert!(works.deletion_marker_exists(900_001).await.unwrap());

    let replay = EventRepository::new(db.clone())
        .list_after(0, 20)
        .await
        .unwrap();
    assert!(replay.iter().any(|event| {
        event.resource == EventResource::Work
            && event.resource_id == work.id
            && matches!(event.payload, EventPayload::WorkDeleted { .. })
    }));
    assert!(replay.iter().any(|event| {
        event.resource == EventResource::DeletionMarker
            && matches!(
                event.payload,
                EventPayload::DeletionMarkerCreated {
                    pixiv_work_id: 900_001,
                    ..
                }
            )
    }));
}

#[tokio::test]
async fn deletion_markers_block_automatic_metadata_creation_and_emit_events() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let works = WorkRepository::new(db.clone());

    works
        .mark_physically_deleted(900_011, "manual_purge")
        .await
        .unwrap();
    assert!(works.deletion_marker_exists(900_011).await.unwrap());

    let blocked = works
        .create_metadata_only(900_011, 700_011, "blocked")
        .await;
    assert!(blocked.is_err());

    let replay = EventRepository::new(db.clone())
        .list_after(0, 20)
        .await
        .unwrap();
    assert!(replay.iter().any(|event| {
        event.resource == EventResource::DeletionMarker
            && matches!(
                event.payload,
                EventPayload::DeletionMarkerCreated {
                    pixiv_work_id: 900_011,
                    ..
                }
            )
    }));
}

#[tokio::test]
async fn started_trash_purge_permanently_blocks_restore_and_rescheduling() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let works = WorkRepository::new(db.clone());
    let trash = TrashRepository::new(db.clone());
    let work = works
        .create_metadata_only(900_021, 700_021, "purging")
        .await
        .unwrap();
    let purge_at = OffsetDateTime::now_utc() + Duration::days(30);
    works.move_to_trash(work.id, purge_at).await.unwrap();
    trash.begin_purge(work.id).await.unwrap();

    assert!(matches!(
        works.restore(work.id).await,
        Err(DbError::RevisionConflict)
    ));
    assert!(matches!(
        works
            .reschedule_trash(work.id, purge_at + Duration::days(1))
            .await,
        Err(DbError::RevisionConflict)
    ));
    trash.begin_purge(work.id).await.unwrap();
    let attempts: i32 =
        sqlx::query_scalar("SELECT purge_attempts FROM trash_entry WHERE work_id = $1")
            .bind(work.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(attempts, 1);

    trash
        .record_failure(
            work.id,
            &[TrashPurgeFailure {
                relative_path: "missing.png".into(),
                error: "test failure".to_owned(),
            }],
        )
        .await
        .unwrap();
    assert!(matches!(
        works.restore(work.id).await,
        Err(DbError::RevisionConflict)
    ));
    assert!(matches!(
        works
            .reschedule_trash(work.id, purge_at + Duration::days(1))
            .await,
        Err(DbError::RevisionConflict)
    ));
    let entry = works.trash_entry(work.id).await.unwrap();
    assert_eq!(
        entry.capabilities.blocked_reason,
        Some(TrashActionBlockReason::PurgeStarted)
    );
}

#[tokio::test]
async fn queued_trash_purge_blocks_actions_until_the_job_is_cancelled() {
    let locked = support::LockedDb::new().await;
    let db = locked.db.clone();
    let works = WorkRepository::new(db.clone());
    let jobs = JobRepository::new(db.clone());
    let work = works
        .create_metadata_only(900_024, 700_024, "queued purge")
        .await
        .unwrap();
    let purge_at = OffsetDateTime::now_utc() + Duration::days(30);
    works.move_to_trash(work.id, purge_at).await.unwrap();
    let purge = jobs
        .enqueue_trash_purges_if_absent(&[work.id], "manual_purge", JobPriority::Immediate)
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    let queued_entry = works.trash_entry(work.id).await.unwrap();
    assert_eq!(
        queued_entry.capabilities.blocked_reason,
        Some(TrashActionBlockReason::PurgeQueued)
    );
    assert!(matches!(
        works.restore(work.id).await,
        Err(DbError::RevisionConflict)
    ));
    assert!(matches!(
        works
            .reschedule_trash(work.id, purge_at + Duration::days(1))
            .await,
        Err(DbError::RevisionConflict)
    ));

    let queued_job = jobs.get(purge.job_id).await.unwrap();
    jobs.cancel_requested(queued_job.id, queued_job.resource_revision)
        .await
        .unwrap();
    let cancelled_entry = works.trash_entry(work.id).await.unwrap();
    assert!(cancelled_entry.capabilities.can_restore);
    assert!(cancelled_entry.capabilities.can_reschedule);
    works.restore(work.id).await.unwrap();
}

#[tokio::test]
async fn trash_purge_completion_requires_an_active_claim() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let works = WorkRepository::new(db.clone());
    let trash = TrashRepository::new(db);
    let work = works
        .create_metadata_only(900_022, 700_022, "purging")
        .await
        .unwrap();
    works
        .move_to_trash(work.id, OffsetDateTime::now_utc() + Duration::days(30))
        .await
        .unwrap();

    assert!(matches!(
        trash.complete_purge(work.id, "manual_purge").await,
        Err(DbError::RevisionConflict)
    ));
    trash.begin_purge(work.id).await.unwrap();
    trash.complete_purge(work.id, "manual_purge").await.unwrap();
    assert!(works.deletion_marker_exists(900_022).await.unwrap());
}

#[tokio::test]
async fn started_purge_cannot_be_cancelled_and_finishes_with_its_job() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let works = WorkRepository::new(db.clone());
    let trash = TrashRepository::new(db.clone());
    let jobs = JobRepository::new(db.clone());
    let work = works
        .create_metadata_only(900_023, 700_023, "purging atomically")
        .await
        .unwrap();
    works
        .move_to_trash(work.id, OffsetDateTime::now_utc() + Duration::days(30))
        .await
        .unwrap();
    let job_id = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::BackgroundMaintenance,
            JobKind::PurgeTrash,
            json!({
                "work_id": work.id,
                "deletion_method": "manual_purge",
            }),
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
        .unwrap();
    assert_eq!(claimed.id, job_id);

    trash
        .begin_purge_job(claimed.lease(), work.id)
        .await
        .unwrap();
    assert!(matches!(
        jobs.cancel_requested(claimed.id, claimed.resource_revision)
            .await,
        Err(DbError::RevisionConflict)
    ));
    trash
        .record_failure(
            work.id,
            &[TrashPurgeFailure {
                relative_path: "missing.png".into(),
                error: "test failure".to_owned(),
            }],
        )
        .await
        .unwrap();
    assert!(matches!(
        jobs.cancel_requested(claimed.id, claimed.resource_revision)
            .await,
        Err(DbError::RevisionConflict)
    ));
    trash
        .begin_purge_job(claimed.lease(), work.id)
        .await
        .unwrap();
    trash
        .complete_purge_job(claimed.lease(), work.id, "manual_purge")
        .await
        .unwrap();

    assert_eq!(jobs.get(job_id).await.unwrap().state.as_str(), "completed");
    assert!(works.deletion_marker_exists(900_023).await.unwrap());
}

#[tokio::test]
async fn stale_trash_purge_lease_cannot_record_failure() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let works = WorkRepository::new(db.clone());
    let trash = TrashRepository::new(db.clone());
    let jobs = JobRepository::new(db.clone());
    let work = works
        .create_metadata_only(900_024, 700_024, "stale purger")
        .await
        .unwrap();
    works
        .move_to_trash(work.id, OffsetDateTime::now_utc() + Duration::days(30))
        .await
        .unwrap();
    let job_id = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::BackgroundMaintenance,
            JobKind::PurgeTrash,
            json!({
                "work_id": work.id,
                "deletion_method": "manual_purge",
            }),
        ))
        .await
        .unwrap();
    let first = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::BackgroundMaintenance]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.id, job_id);

    trash.begin_purge_job(first.lease(), work.id).await.unwrap();
    sqlx::query("UPDATE job SET lease_expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(job_id)
        .execute(db.pool())
        .await
        .unwrap();
    let second = jobs
        .claim_next(
            Uuid::now_v7(),
            &JobQuotaSelection::new(vec![JobPriority::BackgroundMaintenance]),
            Duration::minutes(5),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second.id, job_id);

    let failures = [TrashPurgeFailure {
        relative_path: "missing.png".into(),
        error: "test failure".to_owned(),
    }];
    assert!(matches!(
        trash
            .record_failure_job(first.lease(), work.id, &failures)
            .await,
        Err(DbError::LeaseConflict)
    ));
    let purge_state: String =
        sqlx::query_scalar("SELECT purge_state FROM trash_entry WHERE work_id = $1")
            .bind(work.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(purge_state, "running");

    trash
        .record_failure_job(second.lease(), work.id, &failures)
        .await
        .unwrap();
    let purge_state: String =
        sqlx::query_scalar("SELECT purge_state FROM trash_entry WHERE work_id = $1")
            .bind(work.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(purge_state, "failed");
}
