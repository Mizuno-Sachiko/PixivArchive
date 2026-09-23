use pixivarchive_db::{DbError, ProcessingMedia};
use pixivarchive_domain::{
    job::JobErrorClass,
    media::{MediaFormat, MediaKind},
};
use pixivarchive_media::{
    DerivativeError, MediaPathError, MediaRoot, UgoiraError, UgoiraLimits, UgoiraManifestValidator,
};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(super) async fn prepare_media_source(
    media_root: &MediaRoot,
    media: &ProcessingMedia,
    limits: UgoiraLimits,
) -> Result<PreparedMediaSource, PrepareSourceError> {
    match media.media_kind {
        MediaKind::SourceImage => {
            let source = media_root
                .resolve_file_async(media.relative_path.clone())
                .await
                .map_err(PrepareSourceError::MediaPath)?;
            return Ok(PreparedMediaSource::Original { path: source });
        }
        MediaKind::UgoiraZip => {}
        MediaKind::Derivative => return Err(PrepareSourceError::UnsupportedMediaKind),
    }

    let manifest = media
        .ugoira
        .clone()
        .ok_or(PrepareSourceError::MissingUgoiraMetadata)?;
    let extension =
        frame_extension(&manifest.frame_mime_type).ok_or(PrepareSourceError::UnsupportedFrame)?;
    let destination_relative =
        PathBuf::from("staging").join(format!("ugoira-cover-{}.{}", Uuid::now_v7(), extension));
    let destination = media_root
        .prepare_file_async(destination_relative.clone())
        .await
        .map_err(PrepareSourceError::MediaPath)?;
    let media_root = media_root.clone();
    let source_relative = media.relative_path.clone();
    let temporary = create_temporary_source(destination, move |_destination| {
        UgoiraManifestValidator::new(limits)
            .extract_first_frame(
                &media_root,
                &source_relative,
                &manifest,
                &destination_relative,
            )
            .map(|_| ())
    })
    .await?;
    Ok(PreparedMediaSource::Temporary { source: temporary })
}

async fn create_temporary_source<F>(
    path: PathBuf,
    operation: F,
) -> Result<TemporarySource, PrepareSourceError>
where
    F: FnOnce(&Path) -> Result<(), UgoiraError> + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        operation(&path)?;
        // The extractor removes partial output on failure. Arm ownership only after
        // success so a create-new collision cannot remove a pre-existing file.
        Ok(TemporarySource { path })
    })
    .await
    .map_err(|_| PrepareSourceError::Worker)?
    .map_err(PrepareSourceError::Ugoira)
}

pub(super) enum PreparedMediaSource {
    Original { path: PathBuf },
    Temporary { source: TemporarySource },
}

impl PreparedMediaSource {
    pub(super) fn path(&self) -> &Path {
        match self {
            Self::Original { path } => path,
            Self::Temporary { source } => &source.path,
        }
    }
}

pub(super) struct TemporarySource {
    path: PathBuf,
}

impl Drop for TemporarySource {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(error = %error, "temporary media source could not be removed");
        }
    }
}

pub(super) enum PrepareSourceError {
    MissingUgoiraMetadata,
    UnsupportedMediaKind,
    UnsupportedFrame,
    Worker,
    MediaPath(MediaPathError),
    Ugoira(UgoiraError),
}

pub(super) struct MediaProcessingFailure {
    error_class: JobErrorClass,
    message: String,
}

impl MediaProcessingFailure {
    pub(super) fn permanent(message: impl Into<String>) -> Self {
        Self {
            error_class: JobErrorClass::Permanent,
            message: message.into(),
        }
    }

    pub(super) fn server(message: impl Into<String>) -> Self {
        Self {
            error_class: JobErrorClass::Server,
            message: message.into(),
        }
    }

    pub(super) fn error_class(&self) -> JobErrorClass {
        self.error_class
    }

    pub(super) fn message(&self) -> &str {
        &self.message
    }

    pub(super) fn database(error: DbError) -> Self {
        let message = format!("媒体数据库操作失败：{error}");
        match error {
            DbError::Connection(_)
            | DbError::Query(_)
            | DbError::LeaseConflict
            | DbError::RevisionConflict => Self::server(message),
            _ => Self::permanent(message),
        }
    }

    pub(super) fn prepare_source(error: PrepareSourceError) -> Self {
        match error {
            PrepareSourceError::Worker
            | PrepareSourceError::MediaPath(MediaPathError::Io { .. })
            | PrepareSourceError::MediaPath(MediaPathError::Worker(_))
            | PrepareSourceError::Ugoira(UgoiraError::Archive) => {
                Self::server("媒体源文件暂时无法读取")
            }
            PrepareSourceError::MissingUgoiraMetadata => Self::permanent("动图缺少帧元数据"),
            PrepareSourceError::UnsupportedMediaKind => Self::permanent("媒体类型不支持派生处理"),
            PrepareSourceError::UnsupportedFrame => Self::permanent("动图帧格式不支持派生处理"),
            PrepareSourceError::MediaPath(_) => Self::permanent("媒体路径无效"),
            PrepareSourceError::Ugoira(_) => Self::permanent("动图源文件无法解析"),
        }
    }

    pub(super) fn derivative(error: DerivativeError) -> Self {
        let message = format!("派生图生成失败：{error}");
        match error {
            DerivativeError::Process | DerivativeError::Storage => Self::server(message),
            _ => Self::permanent(message),
        }
    }
}

fn frame_extension(content_type: &str) -> Option<&'static str> {
    match content_type {
        "image/jpeg" => Some(MediaFormat::Jpeg.extension()),
        "image/png" => Some(MediaFormat::Png.extension()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{PrepareSourceError, create_temporary_source};
    use pixivarchive_media::ugoira::UgoiraError;
    use std::{
        fs,
        sync::mpsc,
        time::{Duration, Instant},
    };
    use tokio::sync::oneshot;
    use uuid::Uuid;

    #[tokio::test]
    async fn cancelled_temporary_source_task_removes_late_file() {
        let path =
            std::env::temp_dir().join(format!("pixivarchive-ugoira-cover-{}.jpg", Uuid::now_v7()));
        let task_path = path.clone();
        let (started_tx, started_rx) = oneshot::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let task = tokio::spawn(async move {
            create_temporary_source(task_path, move |destination| {
                fs::write(destination, b"frame").unwrap();
                let _ = started_tx.send(());
                release_rx.recv().unwrap();
                Ok(())
            })
            .await
        });

        started_rx.await.unwrap();
        task.abort();
        release_tx.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while path.exists() && Instant::now() < deadline {
            tokio::task::yield_now().await;
        }

        assert!(!path.exists());
    }

    #[tokio::test]
    async fn failed_temporary_source_preserves_existing_file() {
        let path = std::env::temp_dir().join(format!(
            "pixivarchive-existing-cover-{}.jpg",
            Uuid::now_v7()
        ));
        fs::write(&path, b"existing").unwrap();

        let result = create_temporary_source(path.clone(), |_| Err(UgoiraError::Archive)).await;

        assert!(matches!(
            result,
            Err(PrepareSourceError::Ugoira(UgoiraError::Archive))
        ));
        assert_eq!(fs::read(&path).unwrap(), b"existing");
        fs::remove_file(path).unwrap();
    }
}
