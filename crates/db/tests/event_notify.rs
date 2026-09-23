mod support;

use pixivarchive_db::{EventRepository, JobRepository};
use pixivarchive_domain::job::{JobKind, JobPriority, JobQuotaSelection, NewJob};
use serde_json::json;
use sqlx::postgres::PgListener;
use time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn job_state_change_notifies_and_publishes_the_committed_event() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must point at the isolated test database");
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());

    let job_id = jobs
        .enqueue(NewJob::new(
            JobPriority::Immediate,
            JobKind::ImportWork.as_str(),
            json!({ "pixiv_id": 3101 }),
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
    let events = EventRepository::new(db.clone());
    let cursor = events
        .replay_window(None, 100)
        .await
        .unwrap()
        .latest_event_id
        .unwrap_or(0);
    let mut listener = PgListener::connect(&database_url).await.unwrap();
    listener.listen("pixivarchive_events").await.unwrap();
    jobs.complete(claimed.lease(), pixivarchive_db::JobCompletion::TaskOnly)
        .await
        .unwrap();

    tokio::time::timeout(std::time::Duration::from_secs(2), listener.recv())
        .await
        .unwrap()
        .unwrap();
    let published = events.list_after(cursor, 10).await.unwrap();

    assert!(published.iter().any(|event| {
        event.resource_id == job_id
            && matches!(
                event.payload,
                pixivarchive_domain::event::EventPayload::JobCompleted { .. }
            )
    }));
}
