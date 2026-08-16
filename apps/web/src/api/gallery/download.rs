use super::encode_hex;
use crate::api::{ApiError, ApiErrorBody, ApiPath};
use crate::state::AppState;
use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use bytes::Bytes;
use futures_util::stream;
use pixivarchive_application::system::storage_capacity;
use pixivarchive_domain::{media::MediaKind, work::GalleryWorkDetail};
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};
use tokio::{io::AsyncReadExt, sync::OwnedSemaphorePermit};
use uuid::Uuid;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DownloadRepresentation {
    Original,
    Archive,
}

fn download_representation(page_count: usize, media_kind: MediaKind) -> DownloadRepresentation {
    if page_count == 1 && media_kind == MediaKind::SourceImage {
        DownloadRepresentation::Original
    } else {
        DownloadRepresentation::Archive
    }
}

#[utoipa::path(
    get,
    path = "/api/works/{work_id}/download",
    params(("work_id" = Uuid, Path)),
    responses(
        (status = 200, description = "Original media or complete source archive"),
        (status = 206, description = "Requested byte range of a single original file"),
        (status = 429, body = ApiErrorBody),
        (status = 507, body = ApiErrorBody),
        (status = 404, body = ApiErrorBody),
        (status = 409, body = ApiErrorBody),
        (status = 503, body = ApiErrorBody)
    ),
    tag = "Media"
)]
pub(crate) async fn download_work(
    State(state): State<AppState>,
    ApiPath(work_id): ApiPath<Uuid>,
    request: Request,
) -> Result<Response, ApiError> {
    let detail = state.gallery.work_detail(work_id).await?;
    let mut sources = Vec::with_capacity(detail.pages.len());
    for page in &detail.pages {
        let Some(media) = &page.current_media else {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "work_media_incomplete",
                "The work does not have a complete local source archive",
            ));
        };
        sources.push((page.page_index, state.media.source(media.id).await?));
    }

    if let (Some(page), [(page_index, source)]) = (detail.pages.first(), sources.as_slice()) {
        let media = page
            .current_media
            .as_ref()
            .expect("sources follow current media");
        if download_representation(detail.pages.len(), media.media_kind)
            == DownloadRepresentation::Original
        {
            let filename = format!(
                "{}_p{page_index}.{}",
                detail.work.pixiv_work_id,
                source.format.extension()
            );
            let content_disposition =
                HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
                    .map_err(|_| ApiError::service_unavailable())?;
            let content_type = HeaderValue::from_static(source.format.mime_type());
            let mut response = super::media::serve_media(source.clone(), request).await?;
            response
                .headers_mut()
                .insert(header::CONTENT_DISPOSITION, content_disposition);
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, content_type);
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            return Ok(response);
        }
    }

    let export_permit = state
        .work_export_permits
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "work_export_in_progress",
                "Another work archive is being exported",
            )
        })?;

    let required_bytes = sources
        .iter()
        .try_fold(1024_u64 * 1024, |total, (_, source)| {
            total.checked_add(source.byte_size)
        })
        .ok_or_else(ApiError::service_unavailable)?;
    match storage_capacity(state.config.cache_root.path()).await {
        Ok(capacity)
            if capacity.available_bytes <= required_bytes.saturating_add(64 * 1024 * 1024) =>
        {
            return Err(ApiError::new(
                StatusCode::INSUFFICIENT_STORAGE,
                "insufficient_export_storage",
                "There is not enough storage available to export this work",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::Unsupported => {}
        Err(_) => return Err(ApiError::service_unavailable()),
    }

    let export_relative_path = PathBuf::from("exports").join(format!("{}.zip", Uuid::now_v7()));
    let export_path = state
        .config
        .cache_root
        .prepare_file_async(export_relative_path)
        .await
        .map_err(|_| ApiError::service_unavailable())?;
    let pending_archive = PendingArchive::new(export_path, export_permit);
    let pixiv_work_id = detail.work.pixiv_work_id;
    let pending_archive = tokio::task::spawn_blocking(move || {
        build_work_archive(pending_archive.path(), &detail, &sources)?;
        Ok::<_, std::io::Error>(pending_archive)
    })
    .await
    .map_err(|_| ApiError::service_unavailable())?
    .map_err(|_| ApiError::service_unavailable())?;

    let file = tokio::fs::File::open(pending_archive.path())
        .await
        .map_err(|_| ApiError::service_unavailable())?;
    let byte_size = file
        .metadata()
        .await
        .map_err(|_| ApiError::service_unavailable())?
        .len();
    let (path, export_permit) = pending_archive.into_download();
    let body = Body::from_stream(stream::unfold(
        TemporaryDownload {
            file,
            path,
            finished: false,
            _permit: export_permit,
        },
        |mut download| async move {
            if download.finished {
                return None;
            }
            let mut buffer = vec![0; 64 * 1024];
            match download.file.read(&mut buffer).await {
                Ok(0) => None,
                Ok(read) => {
                    buffer.truncate(read);
                    Some((Ok::<Bytes, std::io::Error>(Bytes::from(buffer)), download))
                }
                Err(error) => {
                    download.finished = true;
                    Some((Err(error), download))
                }
            }
        },
    ));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(header::CONTENT_LENGTH, byte_size)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{pixiv_work_id}_all.zip\""),
        )
        .header(header::CACHE_CONTROL, "no-store")
        .body(body)
        .map_err(|_| ApiError::service_unavailable())
}

struct PendingArchive {
    path: PathBuf,
    armed: bool,
    permit: Option<OwnedSemaphorePermit>,
}

impl PendingArchive {
    fn new(path: PathBuf, permit: OwnedSemaphorePermit) -> Self {
        Self {
            path,
            armed: true,
            permit: Some(permit),
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn into_download(mut self) -> (PathBuf, OwnedSemaphorePermit) {
        self.armed = false;
        let permit = self
            .permit
            .take()
            .expect("pending archive always owns its export permit");
        (self.path.clone(), permit)
    }
}

impl Drop for PendingArchive {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

struct TemporaryDownload {
    file: tokio::fs::File,
    path: PathBuf,
    finished: bool,
    _permit: OwnedSemaphorePermit,
}

impl Drop for TemporaryDownload {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn build_work_archive(
    archive_path: &Path,
    detail: &GalleryWorkDetail,
    sources: &[(u32, pixivarchive_application::system::MediaSource)],
) -> Result<(), std::io::Error> {
    let file = File::create(archive_path)?;
    let mut archive = ZipWriter::new(file);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let pixiv_work_id = detail.work.pixiv_work_id;
    let mut media_entries = Vec::with_capacity(sources.len());

    for ((page_index, source), page) in sources.iter().zip(&detail.pages) {
        let media = page
            .current_media
            .as_ref()
            .expect("sources follow current media");
        let entry_name = if media.media_kind == MediaKind::UgoiraZip {
            format!("{pixiv_work_id}_ugoira_original.zip")
        } else {
            format!(
                "{pixiv_work_id}_p{page_index}.{}",
                source.format.extension()
            )
        };
        archive.start_file(&entry_name, stored)?;
        let mut source_file = File::open(&source.path)?;
        std::io::copy(&mut source_file, &mut archive)?;
        media_entries.push(serde_json::json!({
            "page_index": page_index,
            "media_revision_id": media.id,
            "media_kind": media.media_kind,
            "format": source.format.extension(),
            "byte_size": source.byte_size,
            "sha256": encode_hex(&media.sha256),
            "archive_entry": entry_name
        }));
    }

    if let Some(ugoira) = &detail.ugoira {
        archive.start_file("ugoira.json", deflated)?;
        archive.write_all(&serde_json::to_vec_pretty(&serde_json::json!({
            "frame_mime_type": ugoira.frame_mime_type,
            "frames": ugoira.frames
        }))?)?;
    }
    archive.start_file("metadata.json", deflated)?;
    archive.write_all(&serde_json::to_vec_pretty(&serde_json::json!({
        "pixiv_work_id": detail.work.pixiv_work_id,
        "title": detail.work.title,
        "pixiv_artist_id": detail.work.pixiv_artist_id,
        "artist_name": detail.work.artist_name,
        "work_kind": detail.work.work_kind,
        "page_count": detail.work.page_count,
        "media": media_entries
    }))?)?;
    archive.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{DownloadRepresentation, PendingArchive, download_representation};
    use pixivarchive_domain::media::MediaKind;
    use std::sync::Arc;
    use tokio::sync::Semaphore;
    use uuid::Uuid;

    #[tokio::test]
    async fn pending_archive_holds_the_export_permit_until_it_is_dropped() {
        let semaphore = Arc::new(Semaphore::new(1));
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let path = std::env::temp_dir().join(format!("pixivarchive-{}.zip", Uuid::now_v7()));
        std::fs::write(&path, b"partial archive").unwrap();
        let pending = PendingArchive::new(path.clone(), permit);

        assert!(semaphore.clone().try_acquire_owned().is_err());
        drop(pending);

        assert!(semaphore.try_acquire_owned().is_ok());
        assert!(!path.exists());
    }

    #[test]
    fn only_a_single_source_image_is_downloaded_without_an_archive() {
        assert_eq!(
            download_representation(1, MediaKind::SourceImage),
            DownloadRepresentation::Original
        );
        assert_eq!(
            download_representation(2, MediaKind::SourceImage),
            DownloadRepresentation::Archive
        );
        assert_eq!(
            download_representation(1, MediaKind::UgoiraZip),
            DownloadRepresentation::Archive
        );
    }
}
