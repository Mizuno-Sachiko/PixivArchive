use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use pixivarchive_media::{MediaPathError, MediaRoot};
use rand::{TryRngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;

const DATA_DIRECTORY: &str = ".pixivarchive";
const CACHE_DIRECTORY: &str = "cache";
const PIXIV_COOKIE_KEY_FILE: &str = "pixiv-cookie.key";
const PIXIV_COOKIE_KEY_VERSION: u8 = 1;
const DEFAULT_PIXIV_COOKIE_KEY_ID: &str = "primary";
const MAX_PIXIV_COOKIE_KEY_FILE_BYTES: u64 = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PixivCookieInstallationKey {
    key_id: String,
    key: [u8; 32],
}

impl PixivCookieInstallationKey {
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn key(&self) -> [u8; 32] {
        self.key
    }
}

#[derive(Deserialize, Serialize)]
struct StoredPixivCookieKey {
    version: u8,
    key_id: String,
    key: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallationData {
    media_root: MediaRoot,
}

impl InstallationData {
    pub fn new(media_root: &Path) -> Self {
        Self {
            media_root: MediaRoot::new(media_root),
        }
    }

    pub fn cache_root(&self) -> PathBuf {
        self.media_root
            .path()
            .join(DATA_DIRECTORY)
            .join(CACHE_DIRECTORY)
    }

    pub fn prepare(&self) -> Result<PixivCookieInstallationKey, InstallationError> {
        self.prepare_inner(None)
    }

    pub fn prepare_with_legacy(
        &self,
        key_id: &str,
        encoded_key: &str,
    ) -> Result<PixivCookieInstallationKey, InstallationError> {
        self.prepare_inner(Some((key_id, encoded_key)))
    }

    fn prepare_inner(
        &self,
        legacy: Option<(&str, &str)>,
    ) -> Result<PixivCookieInstallationKey, InstallationError> {
        self.media_root
            .prepare_directory(DATA_DIRECTORY)
            .map_err(installation_path_error)?;
        self.media_root
            .prepare_directory(Path::new(DATA_DIRECTORY).join(CACHE_DIRECTORY))
            .map_err(installation_path_error)?;
        match self.load_pixiv_cookie_key() {
            Ok(key) => Ok(key),
            Err(InstallationError::Io { source, .. })
                if source.kind() == io::ErrorKind::NotFound =>
            {
                let key = legacy
                    .map(|(key_id, encoded_key)| decode_legacy_key(key_id, encoded_key))
                    .transpose()?;
                self.create_pixiv_cookie_key(key)
            }
            Err(error) => Err(error),
        }
    }

    pub fn load_pixiv_cookie_key(&self) -> Result<PixivCookieInstallationKey, InstallationError> {
        let relative_path = Path::new(DATA_DIRECTORY).join(PIXIV_COOKIE_KEY_FILE);
        let path = self
            .media_root
            .resolve_file(&relative_path)
            .map_err(installation_path_error)?;
        let metadata = fs::metadata(&path).map_err(|source| InstallationError::Io {
            path: path.clone(),
            source,
        })?;
        if metadata.len() > MAX_PIXIV_COOKIE_KEY_FILE_BYTES {
            return Err(InstallationError::InvalidKey(path));
        }
        let contents = fs::read(&path).map_err(|source| InstallationError::Io {
            path: path.clone(),
            source,
        })?;
        let stored: StoredPixivCookieKey = serde_json::from_slice(&contents)
            .map_err(|_| InstallationError::InvalidKey(path.clone()))?;
        if stored.version != PIXIV_COOKIE_KEY_VERSION || !valid_key_id(&stored.key_id) {
            return Err(InstallationError::InvalidKey(path));
        }
        let key = URL_SAFE_NO_PAD
            .decode(stored.key)
            .map_err(|_| InstallationError::InvalidKey(path.clone()))?
            .try_into()
            .map_err(|_| InstallationError::InvalidKey(path))?;
        Ok(PixivCookieInstallationKey {
            key_id: stored.key_id,
            key,
        })
    }

    fn create_pixiv_cookie_key(
        &self,
        imported: Option<PixivCookieInstallationKey>,
    ) -> Result<PixivCookieInstallationKey, InstallationError> {
        let path = self
            .media_root
            .prepare_file(Path::new(DATA_DIRECTORY).join(PIXIV_COOKIE_KEY_FILE))
            .map_err(installation_path_error)?;
        let key = match imported {
            Some(key) => key,
            None => {
                let mut key = [0_u8; 32];
                OsRng
                    .try_fill_bytes(&mut key)
                    .map_err(|_| InstallationError::Random)?;
                PixivCookieInstallationKey {
                    key_id: DEFAULT_PIXIV_COOKIE_KEY_ID.to_owned(),
                    key,
                }
            }
        };
        let stored = StoredPixivCookieKey {
            version: PIXIV_COOKIE_KEY_VERSION,
            key_id: key.key_id.clone(),
            key: URL_SAFE_NO_PAD.encode(key.key),
        };
        let mut contents =
            serde_json::to_vec(&stored).map_err(|_| InstallationError::InvalidKey(path.clone()))?;
        contents.push(b'\n');
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = match options.open(&path) {
            Ok(file) => file,
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                return self.load_pixiv_cookie_key();
            }
            Err(source) => {
                return Err(InstallationError::Io { path, source });
            }
        };
        file.write_all(&contents)
            .and_then(|_| file.sync_all())
            .map_err(|source| InstallationError::Io {
                path: path.clone(),
                source,
            })?;
        Ok(key)
    }
}

fn decode_legacy_key(
    key_id: &str,
    encoded_key: &str,
) -> Result<PixivCookieInstallationKey, InstallationError> {
    if !valid_key_id(key_id) {
        return Err(InstallationError::InvalidLegacyKey);
    }
    let key = URL_SAFE_NO_PAD
        .decode(encoded_key)
        .map_err(|_| InstallationError::InvalidLegacyKey)?
        .try_into()
        .map_err(|_| InstallationError::InvalidLegacyKey)?;
    Ok(PixivCookieInstallationKey {
        key_id: key_id.to_owned(),
        key,
    })
}

fn valid_key_id(key_id: &str) -> bool {
    !key_id.is_empty()
        && key_id.len() <= 128
        && key_id.trim() == key_id
        && !key_id.chars().any(char::is_control)
}

fn installation_path_error(error: MediaPathError) -> InstallationError {
    match error {
        MediaPathError::Io { path, source } => InstallationError::Io { path, source },
        MediaPathError::InvalidRoot(path)
        | MediaPathError::InvalidRelativePath(path)
        | MediaPathError::UnsafeEntry(path) => InstallationError::InvalidEntry(path),
        MediaPathError::Worker(path) => InstallationError::Io {
            path,
            source: io::Error::other("media path worker stopped unexpectedly"),
        },
    }
}

#[derive(Debug, Error)]
pub enum InstallationError {
    #[error("installation path is not an owned regular entry: {0}")]
    InvalidEntry(PathBuf),
    #[error("Pixiv cookie key file is invalid: {0}")]
    InvalidKey(PathBuf),
    #[error("legacy Pixiv cookie key or key identifier is invalid")]
    InvalidLegacyKey,
    #[error("failed to access installation path {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("operating system random source is unavailable")]
    Random,
}
