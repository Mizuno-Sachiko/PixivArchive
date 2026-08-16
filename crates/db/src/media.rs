use crate::{Db, DbError, EventRepository, JobCompletion, JobRepository};
use pixivarchive_domain::{
    event::{EventPayload, EventResource},
    job::{JobKind, JobLease, JobPriority, NewJob},
    media::{DerivativeFormat, MediaDimensions, MediaFormat, MediaKind},
    pixiv::PixivUgoiraMeta,
};
use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction, types::Json};
use std::path::{Path, PathBuf};
use url::Url;
use uuid::Uuid;

#[derive(Clone)]
pub struct MediaRepository {
    db: Db,
}

impl MediaRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn register_artifact_intent(
        &self,
        lease: JobLease,
        relative_path: &Path,
    ) -> Result<Uuid, DbError> {
        let relative_path = relative_path_string(relative_path)?;
        let mut tx = self.db.begin().await?;
        JobRepository::new(self.db.clone())
            .lock_active_lease_in_tx(&mut tx, lease)
            .await?;
        let id = register_artifact_intent_in_tx(&mut tx, lease.job_id, &relative_path).await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn complete_artifact_job(&self, lease: JobLease) -> Result<(), DbError> {
        JobRepository::new(self.db.clone())
            .complete(lease, JobCompletion::TaskOnly)
            .await
    }

    pub async fn terminal_artifact_intent_ids(&self, limit: u16) -> Result<Vec<Uuid>, DbError> {
        if limit == 0 || limit > 500 {
            return Err(DbError::InvalidValue(
                "artifact cleanup limit must be between 1 and 500".to_owned(),
            ));
        }
        sqlx::query_scalar(
            r#"
            SELECT intent.id
            FROM media_artifact_intent intent
            JOIN job ON job.id = intent.job_id
            WHERE (
                    job.state IN ('completed', 'cancelled')
                    OR (job.state = 'failed' AND job.retryable = false)
                  )
              AND intent.cleanup_after <= now()
              AND (
                    intent.last_cleanup_at IS NULL
                    OR intent.last_cleanup_at <= now() - interval '15 minutes'
                  )
            ORDER BY intent.created_at, intent.id
            LIMIT $1
            "#,
        )
        .bind(i64::from(limit))
        .fetch_all(self.db.pool())
        .await
        .map_err(DbError::from)
    }

    pub async fn lock_terminal_artifact_intent_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        intent_id: Uuid,
    ) -> Result<Option<MediaArtifactIntent>, DbError> {
        let row = sqlx::query(
            r#"
            SELECT intent.id,
                   intent.job_id,
                   intent.relative_path,
                   EXISTS (
                       SELECT 1 FROM media_revision
                       WHERE media_revision.source_path = intent.relative_path
                   ) OR EXISTS (
                       SELECT 1 FROM derivative
                       WHERE derivative.path = intent.relative_path
                   ) AS referenced
            FROM media_artifact_intent intent
            JOIN job ON job.id = intent.job_id
            WHERE intent.id = $1
              AND (
                    job.state IN ('completed', 'cancelled')
                    OR (job.state = 'failed' AND job.retryable = false)
                  )
              AND intent.cleanup_after <= now()
            FOR UPDATE OF intent, job
            "#,
        )
        .bind(intent_id)
        .fetch_optional(&mut **tx)
        .await?;
        Ok(row.map(|row| MediaArtifactIntent {
            id: row.get("id"),
            job_id: row.get("job_id"),
            relative_path: PathBuf::from(row.get::<String, _>("relative_path")),
            referenced: row.get("referenced"),
        }))
    }

    pub async fn delete_artifact_intent_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        intent_id: Uuid,
    ) -> Result<(), DbError> {
        sqlx::query("DELETE FROM media_artifact_intent WHERE id = $1")
            .bind(intent_id)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    pub async fn record_artifact_cleanup_failure_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        intent_id: Uuid,
        error: &str,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            UPDATE media_artifact_intent
            SET cleanup_attempts = cleanup_attempts + 1,
                cleanup_error = $2,
                last_cleanup_at = now()
            WHERE id = $1
            "#,
        )
        .bind(intent_id)
        .bind(error)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn source_file(&self, media_revision_id: Uuid) -> Result<SourceMediaFile, DbError> {
        let row = sqlx::query(
            r#"
            SELECT format, source_path, byte_size
            FROM media_revision
            WHERE id = $1
            "#,
        )
        .bind(media_revision_id)
        .fetch_one(self.db.pool())
        .await?;
        let byte_size = u64::try_from(row.get::<i64, _>("byte_size"))
            .map_err(|_| DbError::InvalidValue("invalid source media byte size".to_owned()))?;
        Ok(SourceMediaFile {
            format: parse_media_format(&row.get::<String, _>("format"))?,
            relative_path: PathBuf::from(row.get::<String, _>("source_path")),
            byte_size,
        })
    }

    pub async fn derivative_file(&self, derivative_id: Uuid) -> Result<SourceMediaFile, DbError> {
        let row = sqlx::query(
            r#"
            SELECT format, path, byte_size
            FROM derivative
            WHERE id = $1
            "#,
        )
        .bind(derivative_id)
        .fetch_one(self.db.pool())
        .await?;
        let byte_size = u64::try_from(row.get::<i64, _>("byte_size"))
            .map_err(|_| DbError::InvalidValue("invalid derivative byte size".to_owned()))?;
        Ok(SourceMediaFile {
            format: parse_media_format(&row.get::<String, _>("format"))?,
            relative_path: PathBuf::from(row.get::<String, _>("path")),
            byte_size,
        })
    }

    pub async fn load_download_plan(
        &self,
        job_id: Uuid,
        work_id: Uuid,
    ) -> Result<MediaDownloadPlan, DbError> {
        let work = sqlx::query(
            r#"
            SELECT job.pixiv_account_id,
                   work.pixiv_work_id,
                   artist.pixiv_artist_id,
                   work_revision.work_kind,
                   work_revision.metadata -> 'ugoira' AS ugoira
            FROM job
            JOIN work ON work.id = $2
            JOIN artist ON artist.id = work.artist_id
            JOIN work_revision ON work_revision.id = work.current_revision_id
            WHERE job.id = $1
            "#,
        )
        .bind(job_id)
        .bind(work_id)
        .fetch_optional(self.db.pool())
        .await?
        .ok_or(DbError::NotFound)?;
        let account_id = work
            .get::<Option<Uuid>, _>("pixiv_account_id")
            .ok_or_else(|| DbError::InvalidValue("media job has no Pixiv account".to_owned()))?;
        let pixiv_work_id = work.get::<i64, _>("pixiv_work_id");
        let pixiv_artist_id = work.get::<i64, _>("pixiv_artist_id");
        let work_kind = work.get::<String, _>("work_kind");
        let ugoira: Option<PixivUgoiraMeta> = work
            .get::<Option<Value>, _>("ugoira")
            .filter(|value| !value.is_null())
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| DbError::InvalidValue(format!("invalid Ugoira metadata: {error}")))?;

        let rows = sqlx::query(
            r#"
            SELECT work_page.id,
                   work_page.page_index,
                   work_page.source_url,
                   current_media.id AS current_media_id,
                   current_media.revision_number,
                   current_media.sha256,
                   current_media.source_path,
                   current_media.metadata
            FROM work_page
            LEFT JOIN media_revision AS current_media
              ON current_media.id = work_page.current_media_revision_id
            WHERE work_page.work_id = $1
              AND work_page.source_state = 'present'
            ORDER BY work_page.page_index
            "#,
        )
        .bind(work_id)
        .fetch_all(self.db.pool())
        .await?;
        if rows.is_empty() {
            return Err(DbError::InvalidValue(
                "media download work has no pages".to_owned(),
            ));
        }

        let mut pages = Vec::with_capacity(rows.len());
        for row in rows {
            let page_index = u32::try_from(row.get::<i32, _>("page_index"))
                .map_err(|_| DbError::InvalidValue("invalid media page index".to_owned()))?;
            let source_url = Url::parse(&row.get::<String, _>("source_url"))
                .map_err(|_| DbError::InvalidValue("invalid media source URL".to_owned()))?;
            let format = media_format_from_url(&source_url)?;
            let current = current_media_from_row(&row)?;
            let revision = current.as_ref().map_or(Ok(1), |current| {
                current
                    .revision_number
                    .checked_add(1)
                    .ok_or_else(|| DbError::InvalidValue("media revision overflow".to_owned()))
            })?;
            pages.push(MediaDownloadPage {
                work_page_id: row.get("id"),
                page_index,
                source_url,
                format,
                revision,
                current,
            });
        }

        let items = if work_kind == "ugoira" {
            let manifest = ugoira.ok_or_else(|| {
                DbError::InvalidValue("Ugoira work has no saved frame manifest".to_owned())
            })?;
            if pages.len() != 1 {
                return Err(DbError::InvalidValue(
                    "Ugoira work must have exactly one logical page".to_owned(),
                ));
            }
            let mut page = pages.remove(0);
            page.source_url = manifest.zip_url.clone();
            page.format = MediaFormat::Zip;
            vec![MediaDownloadItem {
                page,
                media_kind: MediaKind::UgoiraZip,
                ugoira: Some(manifest),
            }]
        } else {
            pages
                .into_iter()
                .map(|page| MediaDownloadItem {
                    page,
                    media_kind: MediaKind::SourceImage,
                    ugoira: None,
                })
                .collect()
        };

        Ok(MediaDownloadPlan {
            account_id,
            work_id,
            pixiv_work_id,
            pixiv_artist_id,
            items,
        })
    }

    pub async fn find_duplicate_source(
        &self,
        byte_size: u64,
        sha256: [u8; 32],
        excluding_media_revision_id: Option<Uuid>,
    ) -> Result<Option<PathBuf>, DbError> {
        let byte_size = i64::try_from(byte_size)
            .map_err(|_| DbError::InvalidValue("media byte size is too large".to_owned()))?;
        let path: Option<String> = sqlx::query_scalar(
            r#"
            SELECT source_path
            FROM media_revision
            WHERE byte_size = $1
              AND sha256 = $2
              AND ($3::uuid IS NULL OR id <> $3)
            ORDER BY created_at, id
            LIMIT 1
            "#,
        )
        .bind(byte_size)
        .bind(sha256.as_slice())
        .bind(excluding_media_revision_id)
        .fetch_optional(self.db.pool())
        .await?;
        Ok(path.map(PathBuf::from))
    }

    pub async fn save_source_revision(
        &self,
        input: SaveSourceMediaRevision,
    ) -> Result<SavedSourceMediaRevision, DbError> {
        validate_source_revision(&input)?;
        let revision_number = i64::try_from(input.revision_number)
            .map_err(|_| DbError::InvalidValue("media revision is too large".to_owned()))?;
        let byte_size = i64::try_from(input.byte_size)
            .map_err(|_| DbError::InvalidValue("media byte size is too large".to_owned()))?;
        let source_path = relative_path_string(&input.relative_path)?;
        let metadata = json!({
            "source_url": input.source_url,
            "download_job_id": input.lease.job_id,
            "dimensions": input.dimensions,
            "ugoira": input.ugoira,
        });
        let mut tx = self.db.begin().await?;
        JobRepository::new(self.db.clone())
            .lock_active_lease_in_tx(&mut tx, input.lease)
            .await?;
        let page = sqlx::query(
            "SELECT work_id, current_media_revision_id FROM work_page WHERE id = $1 FOR UPDATE",
        )
        .bind(input.work_page_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(DbError::NotFound)?;
        if page.get::<Uuid, _>("work_id") != input.work_id {
            return Err(DbError::InvalidValue(
                "media page does not belong to the requested work".to_owned(),
            ));
        }
        if page.get::<Option<Uuid>, _>("current_media_revision_id")
            != input.expected_current_media_revision_id
        {
            return Err(DbError::RevisionConflict);
        }

        let media_revision_id = Uuid::now_v7();
        sqlx::query(
            r#"
            INSERT INTO media_revision (
                id,
                work_page_id,
                revision_number,
                media_kind,
                format,
                source_path,
                byte_size,
                sha256,
                metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(media_revision_id)
        .bind(input.work_page_id)
        .bind(revision_number)
        .bind(media_kind_value(input.media_kind)?)
        .bind(media_format_value(input.format)?)
        .bind(&source_path)
        .bind(byte_size)
        .bind(input.sha256.as_slice())
        .bind(Json(metadata))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r#"
            UPDATE work_page
            SET current_media_revision_id = $2,
                source_path = $3
            WHERE id = $1
            "#,
        )
        .bind(input.work_page_id)
        .bind(media_revision_id)
        .bind(&source_path)
        .execute(&mut *tx)
        .await?;

        JobRepository::new(self.db.clone())
            .enqueue_in_tx(
                &mut tx,
                NewJob::for_kind(
                    input.derivative_priority,
                    JobKind::GenerateDerivative,
                    json!({"media_revision_id": media_revision_id}),
                ),
            )
            .await?;

        let work_revision: Option<i64> = sqlx::query_scalar(
            r#"
            UPDATE work
            SET collection_state = 'collected',
                updated_at = now(),
                resource_revision = resource_revision + 1
            WHERE id = $1
              AND collection_state <> 'trash'
              AND NOT EXISTS (
                  SELECT 1
                  FROM work_page
                  WHERE work_page.work_id = work.id
                    AND work_page.current_media_revision_id IS NULL
              )
            RETURNING resource_revision
            "#,
        )
        .bind(input.work_id)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(revision) = work_revision {
            EventRepository::new(self.db.clone())
                .append_in_tx(
                    &mut tx,
                    EventResource::Work,
                    input.work_id,
                    EventPayload::WorkChanged { revision },
                )
                .await?;
        }
        complete_artifact_intent_in_tx(&mut tx, input.lease.job_id, &source_path).await?;
        if input.complete_job {
            JobRepository::new(self.db.clone())
                .complete_in_tx(&mut tx, input.lease, JobCompletion::TaskOnly)
                .await?;
        }
        tx.commit().await?;

        Ok(SavedSourceMediaRevision {
            id: media_revision_id,
            revision_number: input.revision_number,
            relative_path: input.relative_path,
        })
    }

    pub async fn load_processing_media(
        &self,
        media_revision_id: Uuid,
    ) -> Result<ProcessingMedia, DbError> {
        let row = sqlx::query(
            r#"
            SELECT media_revision.id,
                   media_revision.revision_number,
                   media_revision.media_kind,
                   media_revision.format,
                   media_revision.source_path,
                   media_revision.metadata,
                   work_page.page_index,
                   work.id AS work_id,
                   work.pixiv_work_id,
                   artist.pixiv_artist_id
            FROM media_revision
            JOIN work_page ON work_page.id = media_revision.work_page_id
            JOIN work ON work.id = work_page.work_id
            JOIN artist ON artist.id = work.artist_id
            WHERE media_revision.id = $1
            "#,
        )
        .bind(media_revision_id)
        .fetch_optional(self.db.pool())
        .await?
        .ok_or(DbError::NotFound)?;
        let revision_number = u64::try_from(row.get::<i64, _>("revision_number"))
            .map_err(|_| DbError::InvalidValue("invalid media revision".to_owned()))?;
        let page_index = u32::try_from(row.get::<i32, _>("page_index"))
            .map_err(|_| DbError::InvalidValue("invalid media page index".to_owned()))?;
        let metadata = row.get::<Json<Value>, _>("metadata").0;
        let ugoira = metadata
            .get("ugoira")
            .filter(|value| !value.is_null())
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| DbError::InvalidValue(format!("invalid Ugoira metadata: {error}")))?;
        Ok(ProcessingMedia {
            media_revision_id,
            work_id: row.get("work_id"),
            pixiv_work_id: row.get("pixiv_work_id"),
            pixiv_artist_id: row.get("pixiv_artist_id"),
            page_index,
            revision_number,
            media_kind: parse_media_kind(&row.get::<String, _>("media_kind"))?,
            format: parse_media_format(&row.get::<String, _>("format"))?,
            relative_path: PathBuf::from(row.get::<String, _>("source_path")),
            ugoira,
        })
    }

    pub async fn save_derivative(&self, input: SaveDerivative) -> Result<Uuid, DbError> {
        let width = i32::try_from(input.dimensions.width)
            .map_err(|_| DbError::InvalidValue("derivative width is too large".to_owned()))?;
        let height = i32::try_from(input.dimensions.height)
            .map_err(|_| DbError::InvalidValue("derivative height is too large".to_owned()))?;
        let byte_size = i64::try_from(input.byte_size)
            .map_err(|_| DbError::InvalidValue("derivative byte size is too large".to_owned()))?;
        let path = relative_path_string(&input.relative_path)?;
        let mut tx = self.db.begin().await?;
        JobRepository::new(self.db.clone())
            .lock_active_lease_in_tx(&mut tx, input.lease)
            .await?;
        let previous_path: Option<String> = sqlx::query_scalar(
            r#"
            SELECT path
            FROM derivative
            WHERE media_revision_id = $1
              AND derivative_kind = $2
              AND format = $3
            FOR UPDATE
            "#,
        )
        .bind(input.media_revision_id)
        .bind(input.kind.as_str())
        .bind(input.format.extension())
        .fetch_optional(&mut *tx)
        .await?;
        let id = Uuid::now_v7();
        let saved: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO derivative (
                id,
                media_revision_id,
                derivative_kind,
                format,
                path,
                width,
                height,
                byte_size,
                dominant_color
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (media_revision_id, derivative_kind, format)
            DO UPDATE SET path = excluded.path,
                          width = excluded.width,
                          height = excluded.height,
                          byte_size = excluded.byte_size,
                          dominant_color = excluded.dominant_color,
                          created_at = now()
            RETURNING id
            "#,
        )
        .bind(id)
        .bind(input.media_revision_id)
        .bind(input.kind.as_str())
        .bind(input.format.extension())
        .bind(&path)
        .bind(width)
        .bind(height)
        .bind(byte_size)
        .bind(&input.dominant_color)
        .fetch_one(&mut *tx)
        .await?;
        if let Some(previous_path) = previous_path.filter(|previous| previous != &path) {
            register_artifact_intent_in_tx(&mut tx, input.lease.job_id, &previous_path).await?;
        }
        complete_artifact_intent_in_tx(&mut tx, input.lease.job_id, &path).await?;
        if input.complete_job {
            JobRepository::new(self.db.clone())
                .complete_in_tx(&mut tx, input.lease, JobCompletion::TaskOnly)
                .await?;
        }
        tx.commit().await?;
        Ok(saved)
    }
}

#[derive(Clone, Debug)]
pub struct MediaDownloadPlan {
    pub account_id: Uuid,
    pub work_id: Uuid,
    pub pixiv_work_id: i64,
    pub pixiv_artist_id: i64,
    pub items: Vec<MediaDownloadItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceMediaFile {
    pub format: MediaFormat,
    pub relative_path: PathBuf,
    pub byte_size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaArtifactIntent {
    pub id: Uuid,
    pub job_id: Uuid,
    pub relative_path: PathBuf,
    pub referenced: bool,
}

#[derive(Clone, Debug)]
pub struct MediaDownloadItem {
    pub page: MediaDownloadPage,
    pub media_kind: MediaKind,
    pub ugoira: Option<PixivUgoiraMeta>,
}

#[derive(Clone, Debug)]
pub struct MediaDownloadPage {
    pub work_page_id: Uuid,
    pub page_index: u32,
    pub source_url: Url,
    pub format: MediaFormat,
    pub revision: u64,
    pub current: Option<CurrentMediaRevision>,
}

#[derive(Clone, Debug)]
pub struct CurrentMediaRevision {
    pub id: Uuid,
    pub revision_number: u64,
    pub sha256: [u8; 32],
    pub relative_path: PathBuf,
    pub download_job_id: Option<Uuid>,
}

#[derive(Clone, Debug)]
pub struct SaveSourceMediaRevision {
    pub lease: JobLease,
    pub derivative_priority: JobPriority,
    pub work_id: Uuid,
    pub work_page_id: Uuid,
    pub expected_current_media_revision_id: Option<Uuid>,
    pub revision_number: u64,
    pub media_kind: MediaKind,
    pub format: MediaFormat,
    pub source_url: Url,
    pub relative_path: PathBuf,
    pub byte_size: u64,
    pub sha256: [u8; 32],
    pub dimensions: Option<MediaDimensions>,
    pub ugoira: Option<PixivUgoiraMeta>,
    pub complete_job: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedSourceMediaRevision {
    pub id: Uuid,
    pub revision_number: u64,
    pub relative_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ProcessingMedia {
    pub media_revision_id: Uuid,
    pub work_id: Uuid,
    pub pixiv_work_id: i64,
    pub pixiv_artist_id: i64,
    pub page_index: u32,
    pub revision_number: u64,
    pub media_kind: MediaKind,
    pub format: MediaFormat,
    pub relative_path: PathBuf,
    pub ugoira: Option<PixivUgoiraMeta>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DerivativeKind {
    WaterfallThumbnail,
    UgoiraCover,
}

impl DerivativeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WaterfallThumbnail => "waterfall_thumbnail",
            Self::UgoiraCover => "ugoira_cover",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SaveDerivative {
    pub lease: JobLease,
    pub media_revision_id: Uuid,
    pub kind: DerivativeKind,
    pub format: DerivativeFormat,
    pub relative_path: PathBuf,
    pub dimensions: MediaDimensions,
    pub byte_size: u64,
    pub dominant_color: String,
    pub complete_job: bool,
}

async fn register_artifact_intent_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    relative_path: &str,
) -> Result<Uuid, DbError> {
    let id: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO media_artifact_intent AS current (
            id,
            job_id,
            relative_path
        )
        VALUES ($1, $2, $3)
        ON CONFLICT (relative_path)
        DO UPDATE SET job_id = excluded.job_id,
                      cleanup_attempts = 0,
                      cleanup_error = NULL,
                      last_cleanup_at = NULL,
                      cleanup_after = now(),
                      created_at = now()
        WHERE current.job_id = excluded.job_id
           OR EXISTS (
                SELECT 1
                FROM job
                WHERE job.id = current.job_id
                  AND (
                        job.state IN ('completed', 'cancelled')
                        OR (job.state = 'failed' AND job.retryable = false)
                  )
           )
        RETURNING id
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(job_id)
    .bind(relative_path)
    .fetch_optional(&mut **tx)
    .await?;
    id.ok_or(DbError::RevisionConflict)
}

async fn complete_artifact_intent_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    relative_path: &str,
) -> Result<(), DbError> {
    let deleted =
        sqlx::query("DELETE FROM media_artifact_intent WHERE job_id = $1 AND relative_path = $2")
            .bind(job_id)
            .bind(relative_path)
            .execute(&mut **tx)
            .await?;
    if deleted.rows_affected() != 1 {
        return Err(DbError::InvalidValue(
            "media artifact intent is missing".to_owned(),
        ));
    }
    Ok(())
}

fn current_media_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<Option<CurrentMediaRevision>, DbError> {
    let Some(id) = row.get::<Option<Uuid>, _>("current_media_id") else {
        return Ok(None);
    };
    let revision_number = u64::try_from(row.get::<Option<i64>, _>("revision_number").ok_or_else(
        || DbError::InvalidValue("current media revision number is missing".to_owned()),
    )?)
    .map_err(|_| DbError::InvalidValue("invalid current media revision".to_owned()))?;
    let sha256 = row
        .get::<Option<Vec<u8>>, _>("sha256")
        .ok_or_else(|| DbError::InvalidValue("current media SHA-256 is missing".to_owned()))?
        .try_into()
        .map_err(|_| DbError::InvalidValue("invalid current media SHA-256".to_owned()))?;
    let relative_path = PathBuf::from(
        row.get::<Option<String>, _>("source_path")
            .ok_or_else(|| DbError::InvalidValue("current media path is missing".to_owned()))?,
    );
    let metadata = row
        .get::<Option<Json<Value>>, _>("metadata")
        .map(|value| value.0)
        .unwrap_or_else(|| json!({}));
    let download_job_id = metadata
        .get("download_job_id")
        .and_then(Value::as_str)
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| DbError::InvalidValue("invalid media download job ID".to_owned()))?;
    Ok(Some(CurrentMediaRevision {
        id,
        revision_number,
        sha256,
        relative_path,
        download_job_id,
    }))
}

fn media_format_from_url(url: &Url) -> Result<MediaFormat, DbError> {
    let extension = Path::new(url.path())
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| DbError::InvalidValue("media URL has no file extension".to_owned()))?;
    parse_media_format(extension)
}

fn parse_media_format(value: &str) -> Result<MediaFormat, DbError> {
    MediaFormat::from_db_value(value)
        .ok_or_else(|| DbError::InvalidValue(format!("unsupported media format {value}")))
}

fn parse_media_kind(value: &str) -> Result<MediaKind, DbError> {
    match value {
        "source_image" => Ok(MediaKind::SourceImage),
        "ugoira_zip" => Ok(MediaKind::UgoiraZip),
        _ => Err(DbError::InvalidValue(format!(
            "unsupported media kind {value}"
        ))),
    }
}

fn media_format_value(format: MediaFormat) -> Result<&'static str, DbError> {
    match format {
        MediaFormat::Jpeg => Ok("jpg"),
        MediaFormat::Png => Ok("png"),
        MediaFormat::Gif => Ok("gif"),
        MediaFormat::Zip => Ok("zip"),
        MediaFormat::Webp | MediaFormat::Avif => Err(DbError::InvalidValue(
            "derivative format cannot be saved as source media".to_owned(),
        )),
    }
}

fn media_kind_value(kind: MediaKind) -> Result<&'static str, DbError> {
    match kind {
        MediaKind::SourceImage => Ok("source_image"),
        MediaKind::UgoiraZip => Ok("ugoira_zip"),
        MediaKind::Derivative => Err(DbError::InvalidValue(
            "derivative cannot be saved as source media".to_owned(),
        )),
    }
}

fn validate_source_revision(input: &SaveSourceMediaRevision) -> Result<(), DbError> {
    if input.revision_number == 0 || input.byte_size == 0 {
        return Err(DbError::InvalidValue(
            "media revision and byte size must be positive".to_owned(),
        ));
    }
    match (input.media_kind, input.format, input.ugoira.is_some()) {
        (
            MediaKind::SourceImage,
            MediaFormat::Jpeg | MediaFormat::Png | MediaFormat::Gif,
            false,
        )
        | (MediaKind::UgoiraZip, MediaFormat::Zip, true) => Ok(()),
        _ => Err(DbError::InvalidValue(
            "media kind, format, and Ugoira metadata do not agree".to_owned(),
        )),
    }
}

fn relative_path_string(path: &Path) -> Result<String, DbError> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(DbError::InvalidValue(
            "media path must be relative and normalized".to_owned(),
        ));
    }
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| DbError::InvalidValue("media path is not UTF-8".to_owned()))
}
