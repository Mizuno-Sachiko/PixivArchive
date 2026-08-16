use crate::{MediaPathError, MediaRoot};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default)]
pub struct ReflinkCloner;

impl ReflinkCloner {
    pub fn new() -> Self {
        Self
    }

    pub fn clone_identical(
        &self,
        media_root: &MediaRoot,
        source_relative_path: &Path,
        destination_relative_path: &Path,
    ) -> Result<(), ReflinkError> {
        let source = media_root
            .resolve_file(source_relative_path)
            .map_err(ReflinkError::from)?;
        let destination = media_root
            .resolve_file(destination_relative_path)
            .map_err(ReflinkError::from)?;
        self.clone_resolved(&source, &destination)
    }

    fn clone_resolved(&self, source: &Path, destination: &Path) -> Result<(), ReflinkError> {
        let source_metadata = fs::metadata(source).map_err(|_| ReflinkError::Io)?;
        let destination_metadata = fs::metadata(destination).map_err(|_| ReflinkError::Io)?;
        if source_metadata.len() != destination_metadata.len()
            || sha256(source)? != sha256(destination)?
        {
            return Err(ReflinkError::ContentMismatch);
        }
        clone_file(source, destination)
    }
}

fn sha256(path: &Path) -> Result<[u8; 32], ReflinkError> {
    let mut file = fs::File::open(path).map_err(|_| ReflinkError::Io)?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut hasher = Sha256::new();
    loop {
        let read = file.read(&mut buffer).map_err(|_| ReflinkError::Io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

#[cfg(target_os = "linux")]
fn clone_file(source: &Path, destination: &Path) -> Result<(), ReflinkError> {
    use std::os::fd::AsRawFd;

    let source = fs::File::open(source).map_err(|_| ReflinkError::Io)?;
    let destination = fs::OpenOptions::new()
        .write(true)
        .open(destination)
        .map_err(|_| ReflinkError::Io)?;
    // FICLONE receives two valid regular-file descriptors and does not retain them.
    let result = unsafe {
        libc::ioctl(
            destination.as_raw_fd(),
            libc::FICLONE as _,
            source.as_raw_fd(),
        )
    };
    if result == 0 {
        return Ok(());
    }
    match std::io::Error::last_os_error().raw_os_error() {
        Some(libc::EXDEV) => Err(ReflinkError::CrossFilesystem),
        Some(libc::EOPNOTSUPP | libc::ENOTTY | libc::EINVAL) => Err(ReflinkError::Unsupported),
        _ => Err(ReflinkError::Io),
    }
}

#[cfg(not(target_os = "linux"))]
fn clone_file(_source: &Path, _destination: &Path) -> Result<(), ReflinkError> {
    Err(ReflinkError::Unsupported)
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ReflinkError {
    #[error("files must have equal size and SHA-256 before reflinking")]
    ContentMismatch,
    #[error("source and destination are on different filesystems")]
    CrossFilesystem,
    #[error("filesystem does not support FICLONE")]
    Unsupported,
    #[error("reflink path is outside the owned media root")]
    UnsafePath,
    #[error("reflink operation failed")]
    Io,
}

impl From<MediaPathError> for ReflinkError {
    fn from(error: MediaPathError) -> Self {
        match error {
            MediaPathError::InvalidRoot(_)
            | MediaPathError::InvalidRelativePath(_)
            | MediaPathError::UnsafeEntry(_) => Self::UnsafePath,
            MediaPathError::Io { .. } | MediaPathError::Worker(_) => Self::Io,
        }
    }
}
