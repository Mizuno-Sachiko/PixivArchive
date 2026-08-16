use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaRoot {
    path: PathBuf,
}

impl MediaRoot {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn prepare(&self) -> Result<PathBuf, MediaPathError> {
        validate_absolute_root(&self.path)?;
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) => validate_directory_entry(&self.path, metadata)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir_all(&self.path).map_err(|source| MediaPathError::Io {
                    path: self.path.clone(),
                    source,
                })?;
                let metadata =
                    fs::symlink_metadata(&self.path).map_err(|source| MediaPathError::Io {
                        path: self.path.clone(),
                        source,
                    })?;
                validate_directory_entry(&self.path, metadata)?;
            }
            Err(source) => {
                return Err(MediaPathError::Io {
                    path: self.path.clone(),
                    source,
                });
            }
        }
        canonicalize(&self.path)
    }

    pub fn prepare_directory(
        &self,
        relative_path: impl AsRef<Path>,
    ) -> Result<PathBuf, MediaPathError> {
        let relative_path = relative_path.as_ref();
        validate_relative_path(relative_path, true)?;
        let root = self.prepare()?;
        walk_directory(&root, relative_path, true)
    }

    pub async fn prepare_directory_async(
        &self,
        relative_path: impl Into<PathBuf>,
    ) -> Result<PathBuf, MediaPathError> {
        let root = self.clone();
        let worker_path = self.path.clone();
        let relative_path = relative_path.into();
        tokio::task::spawn_blocking(move || root.prepare_directory(relative_path))
            .await
            .map_err(|_| MediaPathError::Worker(worker_path))?
    }

    pub fn resolve_directory(
        &self,
        relative_path: impl AsRef<Path>,
    ) -> Result<PathBuf, MediaPathError> {
        let relative_path = relative_path.as_ref();
        validate_relative_path(relative_path, true)?;
        let root = self.resolve_root()?;
        walk_directory(&root, relative_path, false)
    }

    pub async fn resolve_directory_async(
        &self,
        relative_path: impl Into<PathBuf>,
    ) -> Result<PathBuf, MediaPathError> {
        let root = self.clone();
        let worker_path = self.path.clone();
        let relative_path = relative_path.into();
        tokio::task::spawn_blocking(move || root.resolve_directory(relative_path))
            .await
            .map_err(|_| MediaPathError::Worker(worker_path))?
    }

    pub fn prepare_file(&self, relative_path: impl AsRef<Path>) -> Result<PathBuf, MediaPathError> {
        let relative_path = relative_path.as_ref();
        validate_relative_path(relative_path, false)?;
        let parent = relative_path
            .parent()
            .ok_or_else(|| MediaPathError::InvalidRelativePath(relative_path.to_path_buf()))?;
        let parent = self.prepare_directory(parent)?;
        let path = parent.join(
            relative_path
                .file_name()
                .ok_or_else(|| MediaPathError::InvalidRelativePath(relative_path.to_path_buf()))?,
        );
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                Err(MediaPathError::UnsafeEntry(path))
            }
            Ok(_) => Ok(path),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(path),
            Err(source) => Err(MediaPathError::Io { path, source }),
        }
    }

    pub async fn prepare_file_async(
        &self,
        relative_path: impl Into<PathBuf>,
    ) -> Result<PathBuf, MediaPathError> {
        let root = self.clone();
        let worker_path = self.path.clone();
        let relative_path = relative_path.into();
        tokio::task::spawn_blocking(move || root.prepare_file(relative_path))
            .await
            .map_err(|_| MediaPathError::Worker(worker_path))?
    }

    pub fn resolve_file(&self, relative_path: impl AsRef<Path>) -> Result<PathBuf, MediaPathError> {
        let relative_path = relative_path.as_ref();
        validate_relative_path(relative_path, false)?;
        let root = self.resolve_root()?;
        let parent = relative_path
            .parent()
            .ok_or_else(|| MediaPathError::InvalidRelativePath(relative_path.to_path_buf()))?;
        let parent = walk_directory(&root, parent, false)?;
        let path = parent.join(
            relative_path
                .file_name()
                .ok_or_else(|| MediaPathError::InvalidRelativePath(relative_path.to_path_buf()))?,
        );
        let metadata = fs::symlink_metadata(&path).map_err(|source| MediaPathError::Io {
            path: path.clone(),
            source,
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(MediaPathError::UnsafeEntry(path));
        }
        let resolved = canonicalize(&path)?;
        if !resolved.starts_with(&root) {
            return Err(MediaPathError::UnsafeEntry(path));
        }
        Ok(resolved)
    }

    pub fn resolve_optional_file(
        &self,
        relative_path: impl AsRef<Path>,
    ) -> Result<Option<PathBuf>, MediaPathError> {
        match self.resolve_file(relative_path) {
            Ok(path) => Ok(Some(path)),
            Err(MediaPathError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    pub async fn resolve_file_async(
        &self,
        relative_path: impl Into<PathBuf>,
    ) -> Result<PathBuf, MediaPathError> {
        let root = self.clone();
        let worker_path = self.path.clone();
        let relative_path = relative_path.into();
        tokio::task::spawn_blocking(move || root.resolve_file(relative_path))
            .await
            .map_err(|_| MediaPathError::Worker(worker_path))?
    }

    pub async fn resolve_optional_file_async(
        &self,
        relative_path: impl Into<PathBuf>,
    ) -> Result<Option<PathBuf>, MediaPathError> {
        let root = self.clone();
        let worker_path = self.path.clone();
        let relative_path = relative_path.into();
        tokio::task::spawn_blocking(move || root.resolve_optional_file(relative_path))
            .await
            .map_err(|_| MediaPathError::Worker(worker_path))?
    }

    fn resolve_root(&self) -> Result<PathBuf, MediaPathError> {
        validate_absolute_root(&self.path)?;
        let metadata = fs::symlink_metadata(&self.path).map_err(|source| MediaPathError::Io {
            path: self.path.clone(),
            source,
        })?;
        validate_directory_entry(&self.path, metadata)?;
        canonicalize(&self.path)
    }
}

impl From<PathBuf> for MediaRoot {
    fn from(path: PathBuf) -> Self {
        Self::new(path)
    }
}

impl From<&Path> for MediaRoot {
    fn from(path: &Path) -> Self {
        Self::new(path)
    }
}

fn validate_absolute_root(path: &Path) -> Result<(), MediaPathError> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(MediaPathError::InvalidRoot(path.to_path_buf()));
    }
    Ok(())
}

fn validate_relative_path(path: &Path, allow_empty: bool) -> Result<(), MediaPathError> {
    if (!allow_empty && path.as_os_str().is_empty())
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(MediaPathError::InvalidRelativePath(path.to_path_buf()));
    }
    Ok(())
}

fn walk_directory(
    root: &Path,
    relative_path: &Path,
    create: bool,
) -> Result<PathBuf, MediaPathError> {
    let mut current = root.to_path_buf();
    for component in relative_path.components() {
        let Component::Normal(component) = component else {
            return Err(MediaPathError::InvalidRelativePath(
                relative_path.to_path_buf(),
            ));
        };
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => validate_directory_entry(&current, metadata)?,
            Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(source) => {
                        return Err(MediaPathError::Io {
                            path: current,
                            source,
                        });
                    }
                }
                let metadata =
                    fs::symlink_metadata(&current).map_err(|source| MediaPathError::Io {
                        path: current.clone(),
                        source,
                    })?;
                validate_directory_entry(&current, metadata)?;
            }
            Err(source) => {
                return Err(MediaPathError::Io {
                    path: current,
                    source,
                });
            }
        }
    }
    let resolved = canonicalize(&current)?;
    if !resolved.starts_with(root) {
        return Err(MediaPathError::UnsafeEntry(current));
    }
    Ok(resolved)
}

fn validate_directory_entry(path: &Path, metadata: fs::Metadata) -> Result<(), MediaPathError> {
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(MediaPathError::UnsafeEntry(path.to_path_buf()));
    }
    Ok(())
}

fn canonicalize(path: &Path) -> Result<PathBuf, MediaPathError> {
    fs::canonicalize(path).map_err(|source| MediaPathError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[derive(Debug, Error)]
pub enum MediaPathError {
    #[error("media root must be an absolute path: {0}")]
    InvalidRoot(PathBuf),
    #[error("media path must be relative and contain only normal components: {0}")]
    InvalidRelativePath(PathBuf),
    #[error("media path is a symbolic link or has an unexpected entry type: {0}")]
    UnsafeEntry(PathBuf),
    #[error("failed to access media path {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("media path worker stopped unexpectedly while accessing {0}")]
    Worker(PathBuf),
}
