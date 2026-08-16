use crate::{
    download::{DownloadError, DownloadStager, StagedDownload},
    probe::{ExpectedMedia, MediaProbe, MediaProbeLimits, MediaProbeResult, ProbeError},
    root::{MediaPathError, MediaRoot},
    ugoira::{UgoiraArchiveValidator, UgoiraError},
};
use bytes::Bytes;
use futures_util::Stream;
use pixivarchive_domain::pixiv::PixivUgoiraMeta;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
use thiserror::Error;
use tokio::{
    fs,
    io::{AsyncReadExt, BufReader},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaStoreConfig {
    pub max_download_bytes: u64,
    pub probe_limits: MediaProbeLimits,
}

#[derive(Clone, Debug)]
pub struct MediaStore {
    root: MediaRoot,
    stager: DownloadStager,
    probe: MediaProbe,
}

impl MediaStore {
    pub fn new(root: impl Into<MediaRoot>, config: MediaStoreConfig) -> Self {
        let root = root.into();
        Self {
            stager: DownloadStager::new(root.clone(), "staging", config.max_download_bytes),
            probe: MediaProbe::new(config.probe_limits),
            root,
        }
    }

    pub async fn ingest<S, E>(
        &self,
        request: IngestRequest,
        stream: S,
    ) -> Result<StoredMedia, StorageError>
    where
        S: Stream<Item = Result<Bytes, E>> + Send,
    {
        validate_relative_path(&request.relative_path, &request.expected)?;
        let (staged, probe) = self.stage_and_probe(&request, stream).await?;
        self.promote(request.relative_path, staged, probe).await
    }

    pub async fn ingest_ugoira<S, E>(
        &self,
        request: IngestRequest,
        stream: S,
        manifest: &PixivUgoiraMeta,
    ) -> Result<StoredMedia, StorageError>
    where
        S: Stream<Item = Result<Bytes, E>> + Send,
    {
        validate_relative_path(&request.relative_path, &request.expected)?;
        let (staged, probe) = self.stage_and_probe(&request, stream).await?;
        let path = staged.path().to_path_buf();
        let manifest = manifest.clone();
        tokio::task::spawn_blocking(move || UgoiraArchiveValidator.validate(&path, &manifest))
            .await
            .map_err(|_| StorageError::Storage)?
            .map_err(StorageError::Ugoira)?;
        self.promote(request.relative_path, staged, probe).await
    }

    pub async fn promote_staged(
        &self,
        request: IngestRequest,
        staged: StagedDownload,
        probe: MediaProbeResult,
    ) -> Result<StoredMedia, StorageError> {
        validate_relative_path(&request.relative_path, &request.expected)?;
        if probe.format != request.expected.format || probe.byte_size != staged.byte_size {
            return Err(StorageError::Storage);
        }
        self.promote(request.relative_path, staged, probe).await
    }

    async fn stage_and_probe<S, E>(
        &self,
        request: &IngestRequest,
        stream: S,
    ) -> Result<(StagedDownload, MediaProbeResult), StorageError>
    where
        S: Stream<Item = Result<Bytes, E>> + Send,
    {
        let staged = self
            .stager
            .stage(request.content_length, stream)
            .await
            .map_err(StorageError::from)?;
        let probe = self.probe;
        let path = staged.path().to_path_buf();
        let expected = request.expected.clone();
        let probe = tokio::task::spawn_blocking(move || probe.probe(&path, &expected))
            .await
            .map_err(|_| StorageError::Storage)?
            .map_err(StorageError::Probe)?;
        Ok((staged, probe))
    }

    async fn promote(
        &self,
        relative_path: PathBuf,
        mut staged: StagedDownload,
        probe: MediaProbeResult,
    ) -> Result<StoredMedia, StorageError> {
        let destination = self
            .root
            .prepare_file_async(relative_path.clone())
            .await
            .map_err(|error| storage_path_error(error, &relative_path))?;

        if let Some(matches) =
            existing_destination_matches(&destination, &staged, &relative_path).await?
        {
            if matches {
                return Ok(stored_media(destination, relative_path, &staged, probe));
            }
            return Err(StorageError::DestinationConflict);
        }

        match fs::hard_link(staged.path(), &destination).await {
            Ok(()) => {
                fs::remove_file(staged.path())
                    .await
                    .map_err(|_| StorageError::Storage)?;
                let _ = staged.take_path();
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if existing_destination_matches(&destination, &staged, &relative_path)
                    .await?
                    .is_some_and(|matches| matches)
                {
                    return Ok(stored_media(destination, relative_path, &staged, probe));
                }
                return Err(StorageError::DestinationConflict);
            }
            Err(_) => return Err(StorageError::Storage),
        }

        Ok(stored_media(destination, relative_path, &staged, probe))
    }
}

async fn existing_destination_matches(
    destination: &Path,
    staged: &StagedDownload,
    relative_path: &Path,
) -> Result<Option<bool>, StorageError> {
    match fs::symlink_metadata(destination).await {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StorageError::UnsafeDestination {
            path: relative_path.to_path_buf(),
        }),
        Ok(metadata) if metadata.is_file() => existing_matches(destination, staged).await.map(Some),
        Ok(_) => Ok(Some(false)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(StorageError::Storage),
    }
}

#[derive(Clone, Debug)]
pub struct IngestRequest {
    pub relative_path: PathBuf,
    pub expected: ExpectedMedia,
    pub content_length: Option<u64>,
}

impl IngestRequest {
    pub fn new(relative_path: impl Into<PathBuf>, expected: ExpectedMedia) -> Self {
        Self {
            relative_path: relative_path.into(),
            expected,
            content_length: None,
        }
    }

    pub fn with_content_length(mut self, content_length: u64) -> Self {
        self.content_length = Some(content_length);
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoredMedia {
    pub absolute_path: PathBuf,
    pub relative_path: PathBuf,
    pub byte_size: u64,
    pub sha256: [u8; 32],
    pub probe: MediaProbeResult,
}

async fn existing_matches(
    destination: &Path,
    staged: &StagedDownload,
) -> Result<bool, StorageError> {
    let existing_size = fs::metadata(destination)
        .await
        .map_err(|_| StorageError::Storage)?
        .len();
    Ok(existing_size == staged.byte_size && sha256_file(destination).await? == staged.sha256)
}

fn stored_media(
    absolute_path: PathBuf,
    relative_path: PathBuf,
    staged: &StagedDownload,
    probe: MediaProbeResult,
) -> StoredMedia {
    StoredMedia {
        absolute_path,
        relative_path,
        byte_size: staged.byte_size,
        sha256: staged.sha256,
        probe,
    }
}

fn validate_relative_path(path: &Path, expected: &ExpectedMedia) -> Result<(), StorageError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(StorageError::InvalidRelativePath);
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or(StorageError::InvalidRelativePath)?;
    if !expected.format.accepts_extension(extension) {
        return Err(StorageError::InvalidRelativePath);
    }
    Ok(())
}

async fn sha256_file(path: &Path) -> Result<[u8; 32], StorageError> {
    let file = fs::File::open(path)
        .await
        .map_err(|_| StorageError::Storage)?;
    let mut reader = BufReader::new(file);
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut hasher = Sha256::new();
    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|_| StorageError::Storage)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum StorageError {
    #[error("media path must be a safe relative path with the expected extension")]
    InvalidRelativePath,
    #[error("media response exceeds the configured limit")]
    ResponseTooLarge { limit: u64 },
    #[error("media response stream failed")]
    Stream,
    #[error("media validation failed")]
    Probe(ProbeError),
    #[error("Ugoira validation failed")]
    Ugoira(UgoiraError),
    #[error("destination already contains different media")]
    DestinationConflict,
    #[error("media destination contains a symbolic link or leaves the media root: {path}")]
    UnsafeDestination { path: PathBuf },
    #[error("media storage failed")]
    Storage,
}

impl From<DownloadError> for StorageError {
    fn from(error: DownloadError) -> Self {
        match error {
            DownloadError::ResponseTooLarge { limit } => Self::ResponseTooLarge { limit },
            DownloadError::Stream => Self::Stream,
            DownloadError::UnsafePath(path) => Self::UnsafeDestination { path },
            DownloadError::Storage => Self::Storage,
        }
    }
}

fn storage_path_error(error: MediaPathError, relative_path: &Path) -> StorageError {
    match error {
        MediaPathError::InvalidRelativePath(_) => StorageError::InvalidRelativePath,
        MediaPathError::InvalidRoot(_) | MediaPathError::UnsafeEntry(_) => {
            StorageError::UnsafeDestination {
                path: relative_path.to_path_buf(),
            }
        }
        MediaPathError::Io { .. } | MediaPathError::Worker(_) => StorageError::Storage,
    }
}
