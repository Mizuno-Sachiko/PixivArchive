mod support;

use pixivarchive_db::{Db, EventRepository};
use pixivarchive_domain::event::{EventPayload, EventResource};
use uuid::Uuid;

#[tokio::test]
async fn empty_event_table_reports_no_replay_boundaries() {
    let _locked = support::LockedDb::new().await;
    let events = EventRepository::new(_locked.db.clone());

    let window = events.replay_window(None, 100).await.unwrap();

    assert!(window.events.is_empty());
    assert_eq!(window.oldest_event_id, None);
    assert_eq!(window.latest_event_id, None);
    assert!(!window.snapshot_refresh);
    assert!(!window.has_more);
}

#[tokio::test]
async fn first_connection_reports_current_boundaries_without_replaying_history() {
    let _locked = support::LockedDb::new().await;
    let events = EventRepository::new(_locked.db.clone());
    let first = append_job_event(
        &_locked.db,
        &events,
        EventPayload::JobQueued { revision: 1 },
    )
    .await;
    let second = append_job_event(
        &_locked.db,
        &events,
        EventPayload::JobCompleted { revision: 2 },
    )
    .await;

    let absent_header = events.replay_window(None, 100).await.unwrap();
    let zero_header = events.replay_window(Some(0), 100).await.unwrap();

    for window in [absent_header, zero_header] {
        assert!(window.events.is_empty());
        assert_eq!(window.oldest_event_id, Some(first));
        assert_eq!(window.latest_event_id, Some(second));
        assert!(!window.snapshot_refresh);
        assert!(!window.has_more);
    }
}

#[tokio::test]
async fn last_event_id_replays_later_events_in_id_order() {
    let _locked = support::LockedDb::new().await;
    let events = EventRepository::new(_locked.db.clone());
    let first = append_job_event(
        &_locked.db,
        &events,
        EventPayload::JobQueued { revision: 1 },
    )
    .await;
    let second = append_job_event(
        &_locked.db,
        &events,
        EventPayload::JobClaimed { revision: 2 },
    )
    .await;
    let third = append_job_event(
        &_locked.db,
        &events,
        EventPayload::JobCompleted { revision: 3 },
    )
    .await;

    let window = events.replay_window(Some(first), 10).await.unwrap();

    let replayed_ids: Vec<_> = window.events.iter().map(|event| event.id).collect();
    assert_eq!(replayed_ids, vec![second, third]);
    assert_eq!(window.oldest_event_id, Some(first));
    assert_eq!(window.latest_event_id, Some(third));
    assert!(!window.snapshot_refresh);
    assert!(!window.has_more);
}

#[tokio::test]
async fn replay_window_reports_more_events_beyond_the_requested_limit() {
    let _locked = support::LockedDb::new().await;
    let events = EventRepository::new(_locked.db.clone());
    let first = append_job_event(
        &_locked.db,
        &events,
        EventPayload::JobQueued { revision: 1 },
    )
    .await;
    let second = append_job_event(
        &_locked.db,
        &events,
        EventPayload::JobClaimed { revision: 2 },
    )
    .await;
    let third = append_job_event(
        &_locked.db,
        &events,
        EventPayload::JobFailed { revision: 3 },
    )
    .await;
    let fourth = append_job_event(
        &_locked.db,
        &events,
        EventPayload::JobCompleted { revision: 4 },
    )
    .await;

    let window = events.replay_window(Some(first), 2).await.unwrap();

    let replayed_ids: Vec<_> = window.events.iter().map(|event| event.id).collect();
    assert_eq!(replayed_ids, vec![second, third]);
    assert_eq!(window.latest_event_id, Some(fourth));
    assert!(window.has_more);
    assert!(!window.snapshot_refresh);
}

#[tokio::test]
async fn retained_history_gap_requests_snapshot_refresh() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let events = EventRepository::new(db.clone());
    let first = append_job_event(&db, &events, EventPayload::JobQueued { revision: 1 }).await;
    let second = append_job_event(&db, &events, EventPayload::JobClaimed { revision: 2 }).await;
    let third = append_job_event(&db, &events, EventPayload::JobCompleted { revision: 3 }).await;
    sqlx::query("DELETE FROM app_event WHERE id IN ($1, $2)")
        .bind(first)
        .bind(second)
        .execute(db.pool())
        .await
        .unwrap();

    let window = events.replay_window(Some(first), 10).await.unwrap();

    assert!(window.snapshot_refresh);
    assert!(window.events.is_empty());
    assert_eq!(window.oldest_event_id, Some(third));
    assert_eq!(window.latest_event_id, Some(third));
    assert!(!window.has_more);
}

#[tokio::test]
async fn cursor_immediately_before_oldest_event_replays_from_oldest() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let events = EventRepository::new(db.clone());
    let first = append_job_event(&db, &events, EventPayload::JobQueued { revision: 1 }).await;
    let second = append_job_event(&db, &events, EventPayload::JobClaimed { revision: 2 }).await;
    let third = append_job_event(&db, &events, EventPayload::JobCompleted { revision: 3 }).await;
    sqlx::query("DELETE FROM app_event WHERE id = $1")
        .bind(first)
        .execute(db.pool())
        .await
        .unwrap();

    let window = events.replay_window(Some(second - 1), 10).await.unwrap();

    let replayed_ids: Vec<_> = window.events.iter().map(|event| event.id).collect();
    assert_eq!(replayed_ids, vec![second, third]);
    assert_eq!(window.oldest_event_id, Some(second));
    assert_eq!(window.latest_event_id, Some(third));
    assert!(!window.snapshot_refresh);
    assert!(!window.has_more);
}

#[tokio::test]
async fn future_cursor_requests_snapshot_refresh() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let events = EventRepository::new(db.clone());
    let first = append_job_event(&db, &events, EventPayload::JobQueued { revision: 1 }).await;

    let window = events.replay_window(Some(first + 10), 10).await.unwrap();

    assert!(window.events.is_empty());
    assert_eq!(window.oldest_event_id, Some(first));
    assert_eq!(window.latest_event_id, Some(first));
    assert!(window.snapshot_refresh);
    assert!(!window.has_more);
}

#[tokio::test]
async fn replay_window_caps_single_batch_at_one_thousand_persisted_rows() {
    let _locked = support::LockedDb::new().await;
    let db = _locked.db.clone();
    let events = EventRepository::new(db.clone());
    let cursor = append_job_event(&db, &events, EventPayload::JobQueued { revision: 1 }).await;
    let inserted = insert_job_events_without_notify(&db, 1_001).await;

    let window = events.replay_window(Some(cursor), 5_000).await.unwrap();

    let replayed_ids: Vec<_> = window.events.iter().map(|event| event.id).collect();
    assert_eq!(replayed_ids, inserted[..1_000]);
    assert_eq!(window.oldest_event_id, Some(cursor));
    assert_eq!(window.latest_event_id, inserted.last().copied());
    assert!(window.has_more);
    assert!(!window.snapshot_refresh);
}

#[tokio::test]
async fn replay_window_rejects_non_positive_limits() {
    let _locked = support::LockedDb::new().await;
    let events = EventRepository::new(_locked.db.clone());

    let error = events.replay_window(Some(1), 0).await.unwrap_err();

    assert!(error.to_string().contains("replay limit must be positive"));
}

async fn append_job_event(db: &Db, events: &EventRepository, payload: EventPayload) -> i64 {
    let mut tx = db.begin().await.unwrap();
    let event = events
        .append_in_tx(&mut tx, EventResource::Job, Uuid::now_v7(), payload)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    event.id
}

async fn insert_job_events_without_notify(db: &Db, count: i64) -> Vec<i64> {
    let before: i64 = sqlx::query_scalar("SELECT coalesce(max(id), 0) FROM app_event")
        .fetch_one(db.pool())
        .await
        .unwrap();
    sqlx::query(
        r#"
        INSERT INTO app_event (resource, resource_id, payload)
        SELECT 'job', gen_random_uuid(), jsonb_build_object('type', 'job_claimed', 'revision', n)
        FROM generate_series(1, $1) AS n
        "#,
    )
    .bind(count)
    .execute(db.pool())
    .await
    .unwrap();

    sqlx::query_scalar("SELECT id FROM app_event WHERE id > $1 ORDER BY id")
        .bind(before)
        .fetch_all(db.pool())
        .await
        .unwrap()
}
