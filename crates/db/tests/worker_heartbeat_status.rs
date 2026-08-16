mod support;

use pixivarchive_db::{WorkerHeartbeatRepository, WorkerHeartbeatUpdate};
use support::LockedDb;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn worker_heartbeat_reports_the_latest_process_and_becomes_stale() {
    let locked = LockedDb::new().await;
    let repository = WorkerHeartbeatRepository::new(locked.db.clone());
    let now = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
    let worker_id = Uuid::now_v7();

    repository
        .update(WorkerHeartbeatUpdate {
            worker_id,
            version: "0.1.0".to_owned(),
            git_commit: Some("test-commit".to_owned()),
            started_at: now - Duration::minutes(2),
            seen_at: now,
        })
        .await
        .unwrap();

    let current = repository.current().await.unwrap().unwrap();
    assert_eq!(current.worker_id, worker_id);
    assert_eq!(current.version, "0.1.0");
    assert!(current.is_online(now + Duration::seconds(30), Duration::minutes(1)));
    assert!(!current.is_online(now + Duration::minutes(2), Duration::minutes(1)));
}
