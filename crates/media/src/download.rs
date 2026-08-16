use crate::{MediaPathError, MediaRoot};
use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::{fs::OpenOptions, io::AsyncWriteExt};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct DownloadStager {
    media_root: MediaRoot,
    staging_directory: PathBuf,
    max_bytes: u64,
}

impl DownloadStager {
    pub fn new(
        media_root: impl Into<MediaRoot>,
        staging_directory: impl Into<PathBuf>,
        max_bytes: u64,
    ) -> Self {
        Self {
            media_root: media_root.into(),
            staging_directory: staging_directory.into(),
            max_bytes,
        }
    }

    pub async fn stage<S, E>(
        &self,
        content_length: Option<u64>,
        stream: S,
    ) -> Result<StagedDownload, DownloadError>
    where
        S: Stream<Item = Result<Bytes, E>> + Send,
    {
        if content_length.is_some_and(|length| length > self.max_bytes) {
            return Err(DownloadError::ResponseTooLarge {
                limit: self.max_bytes,
            });
        }
        let media_root = self.media_root.clone();
        let staging_directory = self.staging_directory.clone();
        let staging_directory = media_root
            .prepare_directory_async(staging_directory)
            .await
            .map_err(DownloadError::from)?;
        let mut staged = StagedDownload {
            path: Some(staging_directory.join(format!("{}.part", Uuid::now_v7()))),
            byte_size: 0,
            sha256: [0; 32],
        };
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(staged.path())
            .await
            .map_err(|_| DownloadError::Storage)?;
        let mut stream = Box::pin(stream);
        let mut byte_size = 0_u64;
        let mut hasher = Sha256::new();

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(_) => return Err(DownloadError::Stream),
            };
            byte_size = match byte_size.checked_add(chunk.len() as u64) {
                Some(byte_size) => byte_size,
                None => {
                    return Err(DownloadError::ResponseTooLarge {
                        limit: self.max_bytes,
                    });
                }
            };
            if byte_size > self.max_bytes {
                return Err(DownloadError::ResponseTooLarge {
                    limit: self.max_bytes,
                });
            }
            hasher.update(&chunk);
            if file.write_all(&chunk).await.is_err() {
                return Err(DownloadError::Storage);
            }
        }
        if file.flush().await.is_err() || file.sync_all().await.is_err() {
            return Err(DownloadError::Storage);
        }
        drop(file);

        staged.byte_size = byte_size;
        staged.sha256 = hasher.finalize().into();
        Ok(staged)
    }
}

pub struct StagedDownload {
    path: Option<PathBuf>,
    pub byte_size: u64,
    pub sha256: [u8; 32],
}

impl StagedDownload {
    pub fn path(&self) -> &Path {
        self.path
            .as_deref()
            .expect("staged path is present until promotion")
    }

    pub(crate) fn take_path(&mut self) -> PathBuf {
        self.path
            .take()
            .expect("staged path is present until promotion")
    }
}

impl Drop for StagedDownload {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DownloadError {
    #[error("media response exceeds the configured limit")]
    ResponseTooLarge { limit: u64 },
    #[error("media response stream failed")]
    Stream,
    #[error("staging path is unsafe: {0}")]
    UnsafePath(PathBuf),
    #[error("staging storage failed")]
    Storage,
}

impl From<MediaPathError> for DownloadError {
    fn from(error: MediaPathError) -> Self {
        match error {
            MediaPathError::InvalidRoot(path)
            | MediaPathError::InvalidRelativePath(path)
            | MediaPathError::UnsafeEntry(path) => Self::UnsafePath(path),
            MediaPathError::Io { .. } | MediaPathError::Worker(_) => Self::Storage,
        }
    }
}
