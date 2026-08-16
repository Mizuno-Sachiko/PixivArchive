use pixivarchive_application::{
    gallery::GalleryService,
    settings::{QueueSettings, SettingValue, SettingsService},
    trash::{TrashSelectionCommandError, TrashService},
};
use pixivarchive_db::{Db, DbError, WorkRepository};
use pixivarchive_domain::work::{
    GalleryContextKind, GalleryContextSelectionExpression, GallerySearch,
    GallerySelectionExpression, TrashFilter, TrashSelectionExpression,
};
use pixivarchive_domain::{
    job::{JobKind, JobPriority},
    settings::SettingGroupKey,
};
use pixivarchive_test_support as support;
use std::collections::HashSet;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[tokio::test]
async fn trash_uses_stable_pages_and_complete_filtered_summaries() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let trash = TrashService::new(locked.db.clone());
    let purge_at = OffsetDateTime::now_utc() + Duration::days(30);

    for index in 0..205 {
        let title = if index == 204 {
            "needle-special".to_owned()
        } else {
            format!("paged-work-{index:03}")
        };
        let work = works
            .create_metadata_only(8_000_000 + index, 301, &title)
            .await
            .unwrap();
        works.move_to_trash(work.id, purge_at).await.unwrap();
    }

    let first = trash
        .page(&TrashFilter::default(), None, 200)
        .await
        .unwrap();
    let second = trash
        .page(&TrashFilter::default(), first.next_cursor.as_ref(), 200)
        .await
        .unwrap();
    assert_eq!(first.items.len(), 200);
    assert_eq!(second.items.len(), 5);
    assert!(second.next_cursor.is_none());
    let ids = first
        .items
        .iter()
        .chain(&second.items)
        .map(|item| item.entry.work_id)
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), 205);
    assert_eq!(
        trash
            .summary(&TrashFilter::default())
            .await
            .unwrap()
            .total_count,
        205
    );

    let title_filter = TrashFilter {
        query: Some("needle-special".to_owned()),
        purge_states: Vec::new(),
    };
    let filtered = trash.page(&title_filter, None, 50).await.unwrap();
    assert_eq!(filtered.items.len(), 1);
    assert_eq!(filtered.items[0].title, "needle-special");
    assert_eq!(trash.summary(&title_filter).await.unwrap().total_count, 1);

    sqlx::query("UPDATE trash_entry SET purge_state = 'failed' WHERE work_id = $1")
        .bind(filtered.items[0].entry.work_id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    let failed_filter = TrashFilter {
        query: None,
        purge_states: vec!["failed".to_owned()],
    };
    assert_eq!(
        trash
            .page(&failed_filter, None, 50)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    assert_eq!(trash.summary(&failed_filter).await.unwrap().total_count, 1);

    let actionable_filter = TrashFilter {
        query: None,
        purge_states: vec!["pending".to_owned(), "failed".to_owned()],
    };
    assert_eq!(
        trash.summary(&actionable_filter).await.unwrap().total_count,
        205
    );

    let accepted_count = trash.purge_all().await.unwrap();
    let purge_job_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM job WHERE kind = 'purge_trash'")
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(accepted_count, 205);
    assert_eq!(purge_job_count, 205);
}

#[tokio::test]
async fn trash_selection_rejects_blocked_works_and_invalid_schedules_atomically() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let trash = TrashService::new(locked.db.clone());
    let first = works
        .create_metadata_only(8_001_001, 301, "first batch work")
        .await
        .unwrap();
    let second = works
        .create_metadata_only(8_001_002, 301, "second batch work")
        .await
        .unwrap();
    trash.move_to_trash(first.id, 30).await.unwrap();
    trash.move_to_trash(second.id, 30).await.unwrap();
    let queued = works
        .create_metadata_only(8_001_003, 301, "queued batch work")
        .await
        .unwrap();
    trash.move_to_trash(queued.id, 30).await.unwrap();
    trash.purge(queued.id).await.unwrap();
    let original_schedules = sqlx::query_as::<_, (Uuid, OffsetDateTime)>(
        "SELECT work_id, scheduled_purge_at FROM trash_entry WHERE work_id = ANY($1) ORDER BY work_id",
    )
    .bind([first.id, second.id, queued.id])
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    let expression = TrashSelectionExpression {
        filter: TrashFilter {
            query: Some("batch work".to_owned()),
            purge_states: Vec::new(),
        },
        base_selected: true,
        exception_work_ids: Vec::new(),
    };

    let invalid_schedule = OffsetDateTime::now_utc() - Duration::days(1);
    let reschedule = trash
        .reschedule_selection(&expression, invalid_schedule)
        .await;
    assert!(matches!(
        reschedule,
        Err(TrashSelectionCommandError::Storage(DbError::InvalidValue(
            _
        )))
    ));
    let unchanged_schedules = sqlx::query_as::<_, (Uuid, OffsetDateTime)>(
        "SELECT work_id, scheduled_purge_at FROM trash_entry WHERE work_id = ANY($1) ORDER BY work_id",
    )
    .bind([first.id, second.id, queued.id])
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(unchanged_schedules, original_schedules);

    let valid_schedule = OffsetDateTime::now_utc() + Duration::days(60);
    assert!(matches!(
        trash
            .reschedule_selection(&expression, valid_schedule)
            .await,
        Err(TrashSelectionCommandError::Blocked {
            selected_count: 3,
            blocked_count: 1,
        })
    ));
    assert!(matches!(
        trash.restore_selection(&expression).await,
        Err(TrashSelectionCommandError::Blocked {
            selected_count: 3,
            blocked_count: 1,
        })
    ));
    let actionable_expression = TrashSelectionExpression {
        exception_work_ids: vec![queued.id],
        ..expression
    };
    assert_eq!(
        trash
            .reschedule_selection(&actionable_expression, valid_schedule)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        trash
            .restore_selection(&actionable_expression)
            .await
            .unwrap(),
        2
    );
    let states: Vec<String> = sqlx::query_scalar(
        "SELECT collection_state FROM work WHERE id = ANY($1) ORDER BY pixiv_work_id",
    )
    .bind([first.id, second.id, queued.id])
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(states, vec!["metadata_only", "metadata_only", "trash"]);
}

#[tokio::test]
async fn trash_selection_projects_fixed_filter_and_exception_state() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let trash = TrashService::new(locked.db.clone());
    let first = works
        .create_metadata_only(8_001_004, 301, "first selection range")
        .await
        .unwrap();
    let second = works
        .create_metadata_only(8_001_005, 301, "second selection range")
        .await
        .unwrap();
    let running = works
        .create_metadata_only(8_001_006, 301, "running selection range")
        .await
        .unwrap();
    for work_id in [first.id, second.id, running.id] {
        trash.move_to_trash(work_id, 30).await.unwrap();
    }
    sqlx::query("UPDATE trash_entry SET purge_state = 'running' WHERE work_id = $1")
        .bind(running.id)
        .execute(locked.db.pool())
        .await
        .unwrap();

    let expression = TrashSelectionExpression {
        filter: TrashFilter {
            query: Some("selection range".to_owned()),
            purge_states: Vec::new(),
        },
        base_selected: true,
        exception_work_ids: vec![second.id],
    };
    let selected = trash
        .project_selection(&expression, &[first.id, second.id, running.id])
        .await
        .unwrap();
    assert_eq!(selected.selected_count, 2);
    assert_eq!(selected.blocked_count, 1);
    assert_eq!(
        selected.selected_visible_work_ids,
        vec![first.id, running.id]
    );

    let inverted_expression = TrashSelectionExpression {
        base_selected: false,
        ..expression
    };
    let inverted = trash
        .project_selection(&inverted_expression, &[first.id, second.id, running.id])
        .await
        .unwrap();
    assert_eq!(inverted.selected_count, 1);
    assert_eq!(inverted.blocked_count, 0);
    assert_eq!(inverted.selected_visible_work_ids, vec![second.id]);
}

#[tokio::test]
async fn trash_selection_projects_and_executes_more_than_five_hundred_works() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let trash = TrashService::new(locked.db.clone());
    let mut work_ids = Vec::new();
    for index in 0..501 {
        let work = works
            .create_metadata_only(
                8_010_000 + index,
                301,
                &format!("oversized trash selection {index:03}"),
            )
            .await
            .unwrap();
        trash.move_to_trash(work.id, 30).await.unwrap();
        work_ids.push(work.id);
    }

    let expression = TrashSelectionExpression {
        filter: TrashFilter {
            query: Some("oversized trash selection".to_owned()),
            purge_states: Vec::new(),
        },
        base_selected: true,
        exception_work_ids: Vec::new(),
    };
    let projection = trash
        .project_selection(&expression, &work_ids[..3])
        .await
        .unwrap();
    assert_eq!(projection.selected_count, 501);
    assert_eq!(projection.blocked_count, 0);
    assert_eq!(projection.selected_visible_work_ids.len(), 3);

    let scheduled_purge_at = OffsetDateTime::now_utc() + Duration::days(60);
    assert_eq!(
        trash
            .reschedule_selection(&expression, scheduled_purge_at)
            .await
            .unwrap(),
        501
    );
    assert_eq!(trash.restore_selection(&expression).await.unwrap(), 501);

    for work_id in work_ids {
        trash.move_to_trash(work_id, 30).await.unwrap();
    }
    assert_eq!(trash.purge_selection(&expression).await.unwrap(), 501);
}

#[tokio::test]
async fn gallery_selection_moves_to_trash_in_one_transaction() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let first = works
        .create_metadata_only(8_001_011, 301, "first gallery work")
        .await
        .unwrap();
    let second = works
        .create_metadata_only(8_001_012, 301, "second gallery work")
        .await
        .unwrap();
    let excluded = works
        .create_metadata_only(8_001_013, 301, "excluded gallery work")
        .await
        .unwrap();
    for work_id in [first.id, second.id, excluded.id] {
        make_gallery_visible(&locked.db, work_id).await;
    }
    let trash = TrashService::new(locked.db.clone());
    let search = GallerySearch {
        restrict_work_ids: vec![first.id, second.id, excluded.id],
        ..GallerySearch::default()
    };

    let expression = GallerySelectionExpression {
        search: search.clone(),
        base_selected: true,
        exception_work_ids: vec![excluded.id],
    };
    let moved = trash
        .move_selection_to_trash(expression.clone(), 30)
        .await
        .unwrap();

    assert_eq!(moved, 2);
    let states = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, collection_state FROM work WHERE id = ANY($1) ORDER BY pixiv_work_id",
    )
    .bind([first.id, second.id, excluded.id])
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(
        states,
        [
            (first.id, "trash".to_owned()),
            (second.id, "trash".to_owned()),
            (excluded.id, "collected".to_owned()),
        ]
    );
    assert_eq!(
        trash.move_selection_to_trash(expression, 30).await.unwrap(),
        0
    );
}

#[tokio::test]
async fn gallery_selection_expression_projects_and_moves_the_fixed_query_with_exceptions() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let first = works
        .create_metadata_only(8_001_031, 301, "expression first")
        .await
        .unwrap();
    let second = works
        .create_metadata_only(8_001_032, 301, "expression second")
        .await
        .unwrap();
    let third = works
        .create_metadata_only(8_001_033, 301, "expression third")
        .await
        .unwrap();
    let fourth = works
        .create_metadata_only(8_001_034, 301, "expression fourth")
        .await
        .unwrap();
    for work_id in [first.id, second.id, third.id, fourth.id] {
        make_gallery_visible(&locked.db, work_id).await;
    }

    let expression = GallerySelectionExpression {
        search: GallerySearch {
            restrict_work_ids: vec![first.id, second.id, third.id, fourth.id],
            ..GallerySearch::default()
        },
        base_selected: true,
        exception_work_ids: vec![first.id, third.id],
    };

    let projection = GalleryService::new(locked.db.clone())
        .selection_projection(&expression, &[first.id, second.id, third.id])
        .await
        .unwrap();
    assert_eq!(projection.selected_count, 2);
    assert_eq!(projection.selected_visible_work_ids, vec![second.id]);

    let moved = TrashService::new(locked.db.clone())
        .move_selection_to_trash(expression, 30)
        .await
        .unwrap();
    assert_eq!(moved, 2);
    let states = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, collection_state FROM work WHERE id = ANY($1) ORDER BY pixiv_work_id",
    )
    .bind([first.id, second.id, third.id, fourth.id])
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(
        states,
        [
            (first.id, "collected".to_owned()),
            (second.id, "trash".to_owned()),
            (third.id, "collected".to_owned()),
            (fourth.id, "trash".to_owned()),
        ]
    );
}

#[tokio::test]
async fn gallery_selection_expression_rolls_back_the_complete_batch() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let first = works
        .create_metadata_only(8_001_041, 301, "expression rollback first")
        .await
        .unwrap();
    let second = works
        .create_metadata_only(8_001_042, 301, "expression rollback second")
        .await
        .unwrap();
    for work_id in [first.id, second.id] {
        make_gallery_visible(&locked.db, work_id).await;
    }
    let expression = GallerySelectionExpression {
        search: GallerySearch {
            restrict_work_ids: vec![first.id, second.id],
            ..GallerySearch::default()
        },
        base_selected: true,
        exception_work_ids: Vec::new(),
    };
    let events_before: i64 = sqlx::query_scalar("SELECT count(*) FROM app_event")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    install_second_trash_entry_failure_hook(&locked.db).await;
    let result = TrashService::new(locked.db.clone())
        .move_selection_to_trash(expression, 30)
        .await;
    clear_second_trash_entry_failure_hook(&locked.db).await;

    assert!(matches!(result, Err(DbError::Constraint(_))));
    let states = sqlx::query_scalar::<_, String>(
        "SELECT collection_state FROM work WHERE id = ANY($1) ORDER BY pixiv_work_id",
    )
    .bind([first.id, second.id])
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(states, ["collected", "collected"]);
    let events_after: i64 = sqlx::query_scalar("SELECT count(*) FROM app_event")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(events_after, events_before);
}

#[tokio::test]
async fn context_selection_moves_artist_and_series_collections_through_one_command() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let artist_first = works
        .create_metadata_only(8_001_051, 351, "artist collection first")
        .await
        .unwrap();
    let artist_second = works
        .create_metadata_only(8_001_052, 351, "artist collection second")
        .await
        .unwrap();
    let series_first = works
        .create_metadata_only(8_001_053, 352, "series collection first")
        .await
        .unwrap();
    let series_second = works
        .create_metadata_only(8_001_054, 353, "series collection second")
        .await
        .unwrap();
    for work_id in [
        artist_first.id,
        artist_second.id,
        series_first.id,
        series_second.id,
    ] {
        make_gallery_visible(&locked.db, work_id).await;
    }
    let series_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO series (id, pixiv_series_id, title) VALUES ($1, 8051, 'selected series')",
    )
    .bind(series_id)
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE work SET series_id = $1 WHERE id = ANY($2)")
        .bind(series_id)
        .bind([series_first.id, series_second.id])
        .execute(locked.db.pool())
        .await
        .unwrap();

    let trash = TrashService::new(locked.db.clone());
    assert_eq!(
        trash
            .move_context_selection_to_trash(
                GalleryContextSelectionExpression {
                    kind: GalleryContextKind::Artist,
                    query: "351".to_owned(),
                    base_selected: true,
                    exception_context_ids: Vec::new(),
                },
                30,
            )
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        trash
            .move_context_selection_to_trash(
                GalleryContextSelectionExpression {
                    kind: GalleryContextKind::Series,
                    query: String::new(),
                    base_selected: false,
                    exception_context_ids: vec![series_id],
                },
                30,
            )
            .await
            .unwrap(),
        2
    );

    let states = sqlx::query_scalar::<_, String>(
        "SELECT collection_state FROM work WHERE id = ANY($1) ORDER BY pixiv_work_id",
    )
    .bind([
        artist_first.id,
        artist_second.id,
        series_first.id,
        series_second.id,
    ])
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(states, ["trash", "trash", "trash", "trash"]);
}

#[tokio::test]
async fn overlapping_context_collections_move_each_work_once() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let shared = works
        .create_metadata_only(8_001_061, 361, "overlapping context")
        .await
        .unwrap();
    let first_only = works
        .create_metadata_only(8_001_062, 362, "first context")
        .await
        .unwrap();
    let second_only = works
        .create_metadata_only(8_001_063, 363, "second context")
        .await
        .unwrap();
    for work_id in [shared.id, first_only.id, second_only.id] {
        make_gallery_visible(&locked.db, work_id).await;
    }
    attach_tag(&locked.db, "context-one", &[shared.id, first_only.id]).await;
    attach_tag(&locked.db, "context-two", &[shared.id, second_only.id]).await;

    let moved = TrashService::new(locked.db.clone())
        .move_context_selection_to_trash(
            GalleryContextSelectionExpression {
                kind: GalleryContextKind::Tag,
                query: "context-".to_owned(),
                base_selected: true,
                exception_context_ids: Vec::new(),
            },
            30,
        )
        .await
        .unwrap();

    assert_eq!(moved, 3);
    let trash_entries: i64 =
        sqlx::query_scalar("SELECT count(*) FROM trash_entry WHERE work_id = ANY($1)")
            .bind([shared.id, first_only.id, second_only.id])
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(trash_entries, 3);
}

#[tokio::test]
async fn context_selection_rolls_back_the_complete_collection() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let first = works
        .create_metadata_only(8_001_071, 371, "context rollback first")
        .await
        .unwrap();
    let second = works
        .create_metadata_only(8_001_072, 371, "context rollback second")
        .await
        .unwrap();
    for work_id in [first.id, second.id] {
        make_gallery_visible(&locked.db, work_id).await;
    }
    let artist_id: Uuid = sqlx::query_scalar("SELECT artist_id FROM work WHERE id = $1")
        .bind(first.id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    let events_before: i64 = sqlx::query_scalar("SELECT count(*) FROM app_event")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    install_second_trash_entry_failure_hook(&locked.db).await;

    let result = TrashService::new(locked.db.clone())
        .move_context_selection_to_trash(
            GalleryContextSelectionExpression {
                kind: GalleryContextKind::Artist,
                query: String::new(),
                base_selected: false,
                exception_context_ids: vec![artist_id],
            },
            30,
        )
        .await;
    clear_second_trash_entry_failure_hook(&locked.db).await;

    assert!(matches!(result, Err(DbError::Constraint(_))));
    let states = sqlx::query_scalar::<_, String>(
        "SELECT collection_state FROM work WHERE id = ANY($1) ORDER BY pixiv_work_id",
    )
    .bind([first.id, second.id])
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(states, ["collected", "collected"]);
    let trash_entries: i64 =
        sqlx::query_scalar("SELECT count(*) FROM trash_entry WHERE work_id = ANY($1)")
            .bind([first.id, second.id])
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(trash_entries, 0);
    let events_after: i64 = sqlx::query_scalar("SELECT count(*) FROM app_event")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(events_after, events_before);
}

#[tokio::test]
async fn gallery_selection_rolls_back_every_work_when_one_trash_entry_fails() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let first = works
        .create_metadata_only(8_001_021, 301, "first rollback work")
        .await
        .unwrap();
    let second = works
        .create_metadata_only(8_001_022, 301, "second rollback work")
        .await
        .unwrap();
    for work_id in [first.id, second.id] {
        make_gallery_visible(&locked.db, work_id).await;
    }
    let events_before: i64 = sqlx::query_scalar("SELECT count(*) FROM app_event")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    install_second_trash_entry_failure_hook(&locked.db).await;
    let result = TrashService::new(locked.db.clone())
        .move_selection_to_trash(
            GallerySelectionExpression {
                search: GallerySearch {
                    restrict_work_ids: vec![first.id, second.id],
                    ..GallerySearch::default()
                },
                base_selected: true,
                exception_work_ids: Vec::new(),
            },
            30,
        )
        .await;
    clear_second_trash_entry_failure_hook(&locked.db).await;

    assert!(matches!(result, Err(DbError::Constraint(_))));
    let states = sqlx::query_scalar::<_, String>(
        "SELECT collection_state FROM work WHERE id = ANY($1) ORDER BY pixiv_work_id",
    )
    .bind([first.id, second.id])
    .fetch_all(locked.db.pool())
    .await
    .unwrap();
    assert_eq!(states, ["collected", "collected"]);
    let trash_entries: i64 =
        sqlx::query_scalar("SELECT count(*) FROM trash_entry WHERE work_id = ANY($1)")
            .bind([first.id, second.id])
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(trash_entries, 0);
    let events_after: i64 = sqlx::query_scalar("SELECT count(*) FROM app_event")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(events_after, events_before);
}

#[tokio::test]
async fn trash_reclaim_estimate_counts_shared_source_content_once_when_all_references_match() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let first = works
        .create_metadata_only(8_002_001, 301, "shared first")
        .await
        .unwrap();
    let second = works
        .create_metadata_only(8_002_002, 301, "shared second")
        .await
        .unwrap();
    let first_shared = add_source_revision(&locked.db, first.id, 0, 100, [7; 32]).await;
    add_source_revision(&locked.db, first.id, 1, 50, [8; 32]).await;
    add_source_revision(&locked.db, second.id, 0, 100, [7; 32]).await;
    add_derivative(&locked.db, first_shared, 10).await;
    let trash = TrashService::new(locked.db.clone());

    trash.move_to_trash(first.id, 30).await.unwrap();
    let first_summary = trash.summary(&TrashFilter::default()).await.unwrap();
    assert_eq!(first_summary.total_count, 1);
    assert_eq!(first_summary.logical_bytes, 160);
    assert_eq!(first_summary.estimated_reclaimable_bytes, 60);

    trash.move_to_trash(second.id, 30).await.unwrap();
    let all_summary = trash.summary(&TrashFilter::default()).await.unwrap();
    assert_eq!(all_summary.total_count, 2);
    assert_eq!(all_summary.logical_bytes, 260);
    assert_eq!(all_summary.estimated_reclaimable_bytes, 160);

    let filtered_summary = trash
        .summary(&TrashFilter {
            query: Some("shared first".to_owned()),
            purge_states: Vec::new(),
        })
        .await
        .unwrap();
    assert_eq!(filtered_summary.logical_bytes, 160);
    assert_eq!(filtered_summary.estimated_reclaimable_bytes, 60);
}

#[tokio::test]
async fn trash_snapshots_retention_restores_and_reschedules_without_moving_files() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let first = works
        .create_metadata_only(9_201, 301, "first")
        .await
        .unwrap();
    let second = works
        .create_metadata_only(9_202, 301, "second")
        .await
        .unwrap();
    let trash = TrashService::new(locked.db.clone());

    let before = OffsetDateTime::now_utc();
    let first_entry = trash.move_to_trash(first.id, 30).await.unwrap();
    let second_entry = trash.move_to_trash(second.id, 60).await.unwrap();
    assert!(first_entry.scheduled_purge_at >= before + Duration::days(30));
    assert!(first_entry.scheduled_purge_at < before + Duration::days(31));
    assert!(second_entry.scheduled_purge_at >= before + Duration::days(60));

    let rescheduled = before + Duration::days(7);
    let rescheduled = rescheduled
        .replace_nanosecond((rescheduled.nanosecond() / 1_000) * 1_000)
        .unwrap();
    trash.reschedule(first.id, rescheduled).await.unwrap();
    let saved_schedule: OffsetDateTime =
        sqlx::query_scalar("SELECT scheduled_purge_at FROM trash_entry WHERE work_id = $1")
            .bind(first.id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(saved_schedule, rescheduled);

    trash.restore(first.id).await.unwrap();
    let state: String = sqlx::query_scalar("SELECT collection_state FROM work WHERE id = $1")
        .bind(first.id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(state, "metadata_only");
}

#[tokio::test]
async fn manual_purges_are_immediate_while_due_purges_use_the_configured_priority() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let manual = works
        .create_metadata_only(9_211, 311, "manual")
        .await
        .unwrap();
    let expired = works
        .create_metadata_only(9_212, 311, "expired")
        .await
        .unwrap();
    let trash = TrashService::new(locked.db.clone());
    let mut queue = QueueSettings::default();
    queue
        .job_priorities
        .iter_mut()
        .find(|mapping| mapping.job_kind == JobKind::PurgeTrash)
        .unwrap()
        .priority = JobPriority::ScheduledCollection;
    SettingsService::new(locked.db.clone())
        .update(SettingGroupKey::Queue, None, SettingValue::Queue(queue))
        .await
        .unwrap();
    trash.move_to_trash(manual.id, 30).await.unwrap();
    trash.move_to_trash(expired.id, 30).await.unwrap();
    trash
        .reschedule(expired.id, OffsetDateTime::now_utc())
        .await
        .unwrap();

    let manual_job = trash.purge(manual.id).await.unwrap();
    assert_eq!(manual_job, trash.purge(manual.id).await.unwrap());
    sqlx::query(
        r#"
        UPDATE job
        SET state = 'running',
            lease_owner = $2,
            lease_expires_at = now() + interval '1 minute'
        WHERE id = $1
        "#,
    )
    .bind(manual_job)
    .bind(Uuid::now_v7())
    .execute(locked.db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE trash_entry SET purge_state = 'running' WHERE work_id = $1")
        .bind(manual.id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(manual_job, trash.purge(manual.id).await.unwrap());
    let due = trash
        .enqueue_due_purges(OffsetDateTime::now_utc(), 50)
        .await
        .unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].work_id, expired.id);

    let rows: Vec<(String, String, serde_json::Value)> =
        sqlx::query_as("SELECT kind, priority_class, payload FROM job ORDER BY created_at")
            .fetch_all(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|(kind, _, _)| kind == JobKind::PurgeTrash.as_str())
    );
    assert!(rows.iter().any(|(_, priority, payload)| {
        priority == JobPriority::Immediate.as_str()
            && payload["work_id"] == manual.id.to_string()
            && payload["deletion_method"] == "manual_purge"
    }));
    assert!(rows.iter().any(|(_, priority, payload)| {
        priority == JobPriority::ScheduledCollection.as_str()
            && payload["work_id"] == expired.id.to_string()
            && payload["deletion_method"] == "retention_expired"
    }));
}

#[tokio::test]
async fn manual_batch_and_all_purges_promote_queued_due_jobs_to_immediate() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let selected = works
        .create_metadata_only(9_213, 311, "selected manual purge")
        .await
        .unwrap();
    let remaining = works
        .create_metadata_only(9_214, 311, "remaining manual purge")
        .await
        .unwrap();
    let trash = TrashService::new(locked.db.clone());
    let mut queue = QueueSettings::default();
    queue
        .job_priorities
        .iter_mut()
        .find(|mapping| mapping.job_kind == JobKind::PurgeTrash)
        .unwrap()
        .priority = JobPriority::ScheduledCollection;
    SettingsService::new(locked.db.clone())
        .update(SettingGroupKey::Queue, None, SettingValue::Queue(queue))
        .await
        .unwrap();

    trash.move_to_trash(selected.id, 30).await.unwrap();
    trash.move_to_trash(remaining.id, 30).await.unwrap();
    let due_at = OffsetDateTime::now_utc();
    trash.reschedule(selected.id, due_at).await.unwrap();
    trash.reschedule(remaining.id, due_at).await.unwrap();
    let due = trash.enqueue_due_purges(due_at, 50).await.unwrap();
    assert_eq!(due.len(), 2);

    let selected_expression = TrashSelectionExpression {
        filter: TrashFilter::default(),
        base_selected: false,
        exception_work_ids: vec![selected.id],
    };
    assert_eq!(
        trash.purge_selection(&selected_expression).await.unwrap(),
        1
    );
    let selected_job: Uuid = sqlx::query_scalar(
        "SELECT id FROM job WHERE kind = 'purge_trash' AND (payload ->> 'work_id')::uuid = $1",
    )
    .bind(selected.id)
    .fetch_one(locked.db.pool())
    .await
    .unwrap();
    assert!(due.iter().any(|purge| purge.job_id == selected_job));
    assert_eq!(trash.purge_all().await.unwrap(), 2);

    let priorities: Vec<String> =
        sqlx::query_scalar("SELECT priority_class FROM job WHERE kind = 'purge_trash' ORDER BY id")
            .fetch_all(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(priorities, vec!["immediate", "immediate"]);
}

#[tokio::test]
async fn due_purge_reuse_does_not_lower_an_immediate_manual_job() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let work = works
        .create_metadata_only(9_215, 311, "manual purge remains immediate")
        .await
        .unwrap();
    let trash = TrashService::new(locked.db.clone());
    let mut queue = QueueSettings::default();
    queue
        .job_priorities
        .iter_mut()
        .find(|mapping| mapping.job_kind == JobKind::PurgeTrash)
        .unwrap()
        .priority = JobPriority::ScheduledCollection;
    SettingsService::new(locked.db.clone())
        .update(SettingGroupKey::Queue, None, SettingValue::Queue(queue))
        .await
        .unwrap();

    trash.move_to_trash(work.id, 30).await.unwrap();
    let due_at = OffsetDateTime::now_utc();
    trash.reschedule(work.id, due_at).await.unwrap();
    let manual_job_id = trash.purge(work.id).await.unwrap();
    let due = trash.enqueue_due_purges(due_at, 50).await.unwrap();

    assert_eq!(due.len(), 1);
    assert_eq!(due[0].job_id, manual_job_id);
    let priority: String = sqlx::query_scalar("SELECT priority_class FROM job WHERE id = $1")
        .bind(manual_job_id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(priority, JobPriority::Immediate.as_str());
}

#[tokio::test]
async fn trash_purge_batch_rolls_back_when_any_job_cannot_be_enqueued() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let first = works
        .create_metadata_only(9_221, 321, "atomic purge first")
        .await
        .unwrap();
    let second = works
        .create_metadata_only(9_222, 321, "atomic purge second")
        .await
        .unwrap();
    let trash = TrashService::new(locked.db.clone());
    trash.move_to_trash(first.id, 30).await.unwrap();
    trash.move_to_trash(second.id, 30).await.unwrap();
    install_second_purge_job_failure_hook(&locked.db).await;

    let result = trash
        .purge_selection(&TrashSelectionExpression {
            filter: TrashFilter::default(),
            base_selected: true,
            exception_work_ids: Vec::new(),
        })
        .await;
    let job_count: i64 = sqlx::query_scalar("SELECT count(*) FROM job WHERE kind = 'purge_trash'")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    clear_second_purge_job_failure_hook(&locked.db).await;

    assert!(matches!(result, Err(DbError::Constraint(_))));
    assert_eq!(job_count, 0);
}

#[tokio::test]
async fn trash_purge_selection_accepts_empty_and_skips_non_actionable_rows() {
    let locked = support::LockedDb::new(709020013).await;
    let works = WorkRepository::new(locked.db.clone());
    let work = works
        .create_metadata_only(9_231, 331, "purge validation")
        .await
        .unwrap();
    let trash = TrashService::new(locked.db.clone());
    trash.move_to_trash(work.id, 30).await.unwrap();

    let empty = TrashSelectionExpression {
        filter: TrashFilter::default(),
        base_selected: false,
        exception_work_ids: Vec::new(),
    };
    assert_eq!(trash.purge_selection(&empty).await.unwrap(), 0);

    sqlx::query("UPDATE trash_entry SET purge_state = 'running' WHERE work_id = $1")
        .bind(work.id)
        .execute(locked.db.pool())
        .await
        .unwrap();
    let all = TrashSelectionExpression {
        filter: TrashFilter::default(),
        base_selected: true,
        exception_work_ids: Vec::new(),
    };
    assert_eq!(trash.purge_selection(&all).await.unwrap(), 0);
    let job_count: i64 = sqlx::query_scalar("SELECT count(*) FROM job WHERE kind = 'purge_trash'")
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(job_count, 0);
}

async fn clear_second_purge_job_failure_hook(db: &Db) {
    sqlx::query("DROP TRIGGER IF EXISTS fail_second_purge_job ON job")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION IF EXISTS fail_second_purge_job()")
        .execute(db.pool())
        .await
        .unwrap();
}

async fn clear_second_trash_entry_failure_hook(db: &Db) {
    sqlx::query("DROP TRIGGER IF EXISTS fail_second_trash_entry ON trash_entry")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DROP FUNCTION IF EXISTS fail_second_trash_entry()")
        .execute(db.pool())
        .await
        .unwrap();
}

async fn install_second_trash_entry_failure_hook(db: &Db) {
    clear_second_trash_entry_failure_hook(db).await;
    sqlx::query(
        r#"
        CREATE FUNCTION fail_second_trash_entry()
        RETURNS trigger
        LANGUAGE plpgsql
        AS $$
        BEGIN
            IF EXISTS (SELECT 1 FROM trash_entry) THEN
                RAISE EXCEPTION 'blocked by gallery trash test' USING ERRCODE = '23514';
            END IF;
            RETURN NEW;
        END;
        $$;
        "#,
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        CREATE TRIGGER fail_second_trash_entry
            BEFORE INSERT ON trash_entry
            FOR EACH ROW
            EXECUTE FUNCTION fail_second_trash_entry()
        "#,
    )
    .execute(db.pool())
    .await
    .unwrap();
}

async fn install_second_purge_job_failure_hook(db: &Db) {
    clear_second_purge_job_failure_hook(db).await;
    sqlx::query(
        r#"
        CREATE FUNCTION fail_second_purge_job()
        RETURNS trigger
        LANGUAGE plpgsql
        AS $$
        BEGIN
            IF NEW.kind = 'purge_trash'
               AND EXISTS (SELECT 1 FROM job WHERE kind = 'purge_trash') THEN
                RAISE EXCEPTION 'blocked by trash batch test' USING ERRCODE = '23514';
            END IF;
            RETURN NEW;
        END;
        $$;
        "#,
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        CREATE TRIGGER fail_second_purge_job
            BEFORE INSERT ON job
            FOR EACH ROW
            EXECUTE FUNCTION fail_second_purge_job()
        "#,
    )
    .execute(db.pool())
    .await
    .unwrap();
}

async fn add_source_revision(
    db: &Db,
    work_id: Uuid,
    page_index: i32,
    byte_size: i64,
    sha256: [u8; 32],
) -> Uuid {
    let page_id = Uuid::now_v7();
    let media_revision_id = Uuid::now_v7();
    sqlx::query("INSERT INTO work_page (id, work_id, page_index) VALUES ($1, $2, $3)")
        .bind(page_id)
        .bind(work_id)
        .bind(page_index)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        r#"
        INSERT INTO media_revision (
            id, work_page_id, revision_number, media_kind, format,
            source_path, byte_size, sha256
        )
        VALUES ($1, $2, 1, 'source_image', 'jpg', $3, $4, $5)
        "#,
    )
    .bind(media_revision_id)
    .bind(page_id)
    .bind(format!("test/{media_revision_id}.jpg"))
    .bind(byte_size)
    .bind(sha256.as_slice())
    .execute(db.pool())
    .await
    .unwrap();
    media_revision_id
}

async fn make_gallery_visible(db: &Db, work_id: Uuid) {
    let media_revision_id = add_source_revision(db, work_id, 0, 1, [9; 32]).await;
    sqlx::query(
        r#"
        UPDATE work_page
        SET current_media_revision_id = $2
        WHERE work_id = $1
          AND page_index = 0
        "#,
    )
    .bind(work_id)
    .bind(media_revision_id)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("UPDATE work SET collection_state = 'collected' WHERE id = $1")
        .bind(work_id)
        .execute(db.pool())
        .await
        .unwrap();
}

async fn attach_tag(db: &Db, name: &str, work_ids: &[Uuid]) -> Uuid {
    let tag_id = Uuid::now_v7();
    sqlx::query("INSERT INTO tag (id, raw_name) VALUES ($1, $2)")
        .bind(tag_id)
        .bind(name)
        .execute(db.pool())
        .await
        .unwrap();
    for work_id in work_ids {
        sqlx::query("INSERT INTO work_tag (work_id, tag_id) VALUES ($1, $2)")
            .bind(*work_id)
            .bind(tag_id)
            .execute(db.pool())
            .await
            .unwrap();
    }
    tag_id
}

async fn add_derivative(db: &Db, media_revision_id: Uuid, byte_size: i64) {
    sqlx::query(
        r#"
        INSERT INTO derivative (
            id, media_revision_id, derivative_kind, format, path,
            width, height, byte_size, dominant_color
        )
        VALUES ($1, $2, 'waterfall_thumbnail', 'webp', $3, 1, 1, $4, '#000000')
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(media_revision_id)
    .bind(format!("test/{media_revision_id}.webp"))
    .bind(byte_size)
    .execute(db.pool())
    .await
    .unwrap();
}
