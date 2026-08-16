mod support;

use pixivarchive_db::{EventRepository, JobRepository};
use pixivarchive_domain::event::{EventPayload, EventResource};
use pixivarchive_domain::job::{JobKind, JobPriority, NewJob};
use serde_json::json;

#[tokio::test]
async fn rolled_back_state_change_does_not_leave_an_outbox_event() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let events = EventRepository::new(db.clone());
    let job_id = jobs
        .enqueue(NewJob::new(
            JobPriority::Immediate,
            JobKind::ImportWork.as_str(),
            json!({ "pixiv_id": 3001 }),
        ))
        .await
        .unwrap();
    let before_transaction = events
        .list_after(0, 100)
        .await
        .unwrap()
        .into_iter()
        .map(|event| event.id)
        .max()
        .unwrap_or(0);

    let mut tx = db.begin().await.unwrap();
    sqlx::query("UPDATE job SET state = 'cancelled', resource_revision = resource_revision + 1 WHERE id = $1")
        .bind(job_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    events
        .append_in_tx(
            &mut tx,
            EventResource::Job,
            job_id,
            EventPayload::JobCancelled { revision: 2 },
        )
        .await
        .unwrap();
    tx.rollback().await.unwrap();

    let state: String = sqlx::query_scalar("SELECT state FROM job WHERE id = $1")
        .bind(job_id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(state, "queued");

    let replay = events.list_after(before_transaction, 10).await.unwrap();
    assert!(replay.iter().all(|event| {
        event.resource_id != job_id || !matches!(event.payload, EventPayload::JobCancelled { .. })
    }));
}
