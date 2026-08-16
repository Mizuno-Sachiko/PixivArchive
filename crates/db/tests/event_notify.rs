mod support;

use pixivarchive_db::JobRepository;
use pixivarchive_domain::job::{JobKind, JobPriority, JobQuotaSelection, NewJob};
use serde_json::json;
use sqlx::postgres::PgListener;
use time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn job_state_change_notifies_the_inserted_event_id() {
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must point at the isolated test database");
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let jobs = JobRepository::new(db.clone());
    let mut listener = PgListener::connect(&database_url).await.unwrap();
    listener.listen("pixivarchive_events").await.unwrap();

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
    let before_complete: i64 =
        sqlx::query_scalar::<_, Option<i64>>("SELECT max(id) FROM app_event")
            .fetch_one(db.pool())
            .await
            .unwrap()
            .unwrap_or(0);
    jobs.complete(claimed.lease(), pixivarchive_db::JobCompletion::TaskOnly)
        .await
        .unwrap();

    let stored_id: i64 = sqlx::query_scalar::<_, Option<i64>>(
        "SELECT max(id) FROM app_event WHERE resource = 'job' AND resource_id = $1",
    )
    .bind(job_id)
    .fetch_one(db.pool())
    .await
    .unwrap()
    .unwrap();
    assert!(stored_id > before_complete);

    let mut notified_id = 0;
    for _ in 0..4 {
        let notification = tokio::time::timeout(std::time::Duration::from_secs(2), listener.recv())
            .await
            .unwrap()
            .unwrap();
        let payload_id: i64 = notification.payload().parse().unwrap();
        if payload_id > before_complete {
            notified_id = payload_id;
            break;
        }
    }

    assert_eq!(notified_id, stored_id);
}
