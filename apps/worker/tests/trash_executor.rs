use pixivarchive_application::trash::TrashService;
use pixivarchive_db::{Db, JobRepository, WorkRepository};
use pixivarchive_domain::job::{
    ClaimedJob, JobErrorClass, JobKind, JobPriority, JobQuotaSelection, NewJob,
};
use pixivarchive_worker::executors::{ExecutorOutcome, JobExecutor, trash::TrashCleanupExecutor};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};
use time::Duration;
use uuid::Uuid;

mod support;

use support::LockedDb;

#[tokio::test]
async fn purge_removes_every_recorded_file_then_leaves_a_deletion_marker() {
    let locked = LockedDb::new().await;
    let directory = TestDirectory::new();
    let work = WorkRepository::new(locked.db.clone())
        .create_metadata_only(9_301, 401, "purge")
        .await
        .unwrap();
    let source = PathBuf::from("pixiv/401/9301/0/r1/source.png");
    let derivative = PathBuf::from("pixiv/401/9301/0/r1/thumbnail.webp");
    seed_media(&locked.db, work.id, &source, Some(&derivative)).await;
    directory.write(&source, b"source");
    directory.write(&derivative, b"derivative");
    TrashService::new(locked.db.clone())
        .move_to_trash(work.id, 30)
        .await
        .unwrap();

    let outcome = TrashCleanupExecutor::new(locked.db.clone(), directory.path.clone())
        .execute(purge_job(&locked.db, work.id, "manual_purge").await)
        .await;

    assert_eq!(outcome, ExecutorOutcome::Finalized);
    assert!(!directory.path.join(source).exists());
    assert!(!directory.path.join(derivative).exists());
    let work_count: i64 = sqlx::query_scalar("SELECT count(*) FROM work WHERE id = $1")
        .bind(work.id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(work_count, 0);
    let method: String =
        sqlx::query_scalar("SELECT deletion_method FROM deletion_marker WHERE pixiv_work_id = $1")
            .bind(9_301_i64)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(method, "manual_purge");

    let replay = TrashCleanupExecutor::new(locked.db.clone(), directory.path.clone())
        .execute(purge_job(&locked.db, work.id, "manual_purge").await)
        .await;
    assert_eq!(replay, ExecutorOutcome::Finalized);
}

#[tokio::test]
async fn unsafe_media_paths_are_recorded_without_deleting_work_metadata() {
    let locked = LockedDb::new().await;
    let directory = TestDirectory::new();
    let work = WorkRepository::new(locked.db.clone())
        .create_metadata_only(9_302, 402, "unsafe")
        .await
        .unwrap();
    seed_media(&locked.db, work.id, Path::new("../outside.png"), None).await;
    TrashService::new(locked.db.clone())
        .move_to_trash(work.id, 30)
        .await
        .unwrap();

    let outcome = TrashCleanupExecutor::new(locked.db.clone(), directory.path.clone())
        .execute(purge_job(&locked.db, work.id, "retention_expired").await)
        .await;

    assert!(matches!(
        outcome,
        ExecutorOutcome::Failed {
            error_class: JobErrorClass::Permanent,
            ..
        }
    ));
    let row: (String, serde_json::Value) =
        sqlx::query_as("SELECT purge_state, failure_details FROM trash_entry WHERE work_id = $1")
            .bind(work.id)
            .fetch_one(locked.db.pool())
            .await
            .unwrap();
    assert_eq!(row.0, "failed");
    assert_eq!(row.1[0]["relative_path"], "../outside.png");
    let work_count: i64 = sqlx::query_scalar("SELECT count(*) FROM work WHERE id = $1")
        .bind(work.id)
        .fetch_one(locked.db.pool())
        .await
        .unwrap();
    assert_eq!(work_count, 1);
}

async fn seed_media(db: &Db, work_id: Uuid, source: &Path, derivative: Option<&Path>) {
    let page_id = Uuid::now_v7();
    let media_id = Uuid::now_v7();
    sqlx::query(
        r#"
        INSERT INTO work_page (id, work_id, page_index, source_state)
        VALUES ($1, $2, 0, 'present')
        "#,
    )
    .bind(page_id)
    .bind(work_id)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        r#"
        INSERT INTO media_revision (
            id, work_page_id, revision_number, media_kind, format,
            source_path, byte_size, sha256
        )
        VALUES ($1, $2, 1, 'source_image', 'png', $3, 1, $4)
        "#,
    )
    .bind(media_id)
    .bind(page_id)
    .bind(source.to_string_lossy())
    .bind(vec![1_u8; 32])
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE work_page SET current_media_revision_id = $2, source_path = $3 WHERE id = $1",
    )
    .bind(page_id)
    .bind(media_id)
    .bind(source.to_string_lossy())
    .execute(db.pool())
    .await
    .unwrap();
    if let Some(path) = derivative {
        sqlx::query(
            r#"
            INSERT INTO derivative (
                id, media_revision_id, derivative_kind, format,
                path, width, height, byte_size, dominant_color
            )
            VALUES ($1, $2, 'waterfall_thumbnail', 'webp', $3, 1, 1, 1, '#1450c8')
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(media_id)
        .bind(path.to_string_lossy())
        .execute(db.pool())
        .await
        .unwrap();
    }
}

async fn purge_job(db: &Db, work_id: Uuid, deletion_method: &str) -> ClaimedJob {
    let jobs = JobRepository::new(db.clone());
    let job_id = jobs
        .enqueue(NewJob::for_kind(
            JobPriority::BackgroundMaintenance,
            JobKind::PurgeTrash,
            json!({
                "work_id": work_id,
                "deletion_method": deletion_method,
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
    claimed
}

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "pixivarchive-trash-{}-{}",
            std::process::id(),
            Uuid::now_v7()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn write(&self, relative_path: &Path, bytes: &[u8]) {
        let path = self.path.join(relative_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
