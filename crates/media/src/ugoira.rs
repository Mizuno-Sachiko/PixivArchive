use crate::{MediaPathError, MediaRoot};
use pixivarchive_domain::{
    media::{MediaDimensions, MediaFormat},
    pixiv::PixivUgoiraMeta,
};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use thiserror::Error;
use zip::ZipArchive;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UgoiraLimits {
    pub max_zip_bytes: u64,
    pub max_frames: usize,
    pub max_entry_bytes: u64,
    pub max_total_expanded_bytes: u64,
    pub max_pixels_per_frame: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct UgoiraManifestValidator {
    limits: UgoiraLimits,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UgoiraArchiveValidator;

impl UgoiraArchiveValidator {
    pub fn validate(&self, path: &Path, manifest: &PixivUgoiraMeta) -> Result<(), UgoiraError> {
        validated_archive(path, manifest).map(|_| ())
    }
}

impl UgoiraManifestValidator {
    pub fn new(limits: UgoiraLimits) -> Self {
        Self { limits }
    }

    pub fn validate(
        &self,
        path: &Path,
        manifest: &PixivUgoiraMeta,
    ) -> Result<ValidatedUgoiraManifest, UgoiraError> {
        let archive_size = fs::metadata(path).map_err(|_| UgoiraError::Archive)?.len();
        if archive_size > self.limits.max_zip_bytes {
            return Err(UgoiraError::ZipTooLarge {
                limit: self.limits.max_zip_bytes,
            });
        }
        if manifest.frames.len() > self.limits.max_frames {
            return Err(UgoiraError::TooManyFrames {
                limit: self.limits.max_frames,
            });
        }

        let mut archive = validated_archive(path, manifest)?;

        let expected_format = frame_format(&manifest.frame_mime_type)?;
        let mut total_expanded = 0_u64;
        let mut frames = Vec::with_capacity(archive.len());
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|_| UgoiraError::Archive)?;
            let name = entry.name().to_owned();
            let expected = manifest
                .frames
                .get(index)
                .ok_or(UgoiraError::UnknownEntry)?;
            if entry.size() > self.limits.max_entry_bytes {
                return Err(UgoiraError::EntryTooLarge {
                    entry: name,
                    limit: self.limits.max_entry_bytes,
                });
            }
            total_expanded =
                total_expanded
                    .checked_add(entry.size())
                    .ok_or(UgoiraError::ExpansionTooLarge {
                        limit: self.limits.max_total_expanded_bytes,
                    })?;
            if total_expanded > self.limits.max_total_expanded_bytes {
                return Err(UgoiraError::ExpansionTooLarge {
                    limit: self.limits.max_total_expanded_bytes,
                });
            }

            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry
                .by_ref()
                .take(self.limits.max_entry_bytes + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| UgoiraError::Archive)?;
            if bytes.len() as u64 > self.limits.max_entry_bytes {
                return Err(UgoiraError::EntryTooLarge {
                    entry: name,
                    limit: self.limits.max_entry_bytes,
                });
            }
            let actual_format = infer::get(&bytes)
                .and_then(|kind| frame_format(kind.mime_type()).ok())
                .ok_or(UgoiraError::InvalidFrame)?;
            if actual_format != expected_format {
                return Err(UgoiraError::FrameFormatMismatch);
            }
            let size = imagesize::blob_size(&bytes).map_err(|_| UgoiraError::InvalidFrame)?;
            let width = u32::try_from(size.width).map_err(|_| UgoiraError::InvalidFrame)?;
            let height = u32::try_from(size.height).map_err(|_| UgoiraError::InvalidFrame)?;
            let pixels = u64::from(width)
                .checked_mul(u64::from(height))
                .ok_or(UgoiraError::FrameDimensionsExceeded)?;
            if width == 0 || height == 0 || pixels > self.limits.max_pixels_per_frame {
                return Err(UgoiraError::FrameDimensionsExceeded);
            }
            image::load_from_memory(&bytes).map_err(|_| UgoiraError::InvalidFrame)?;
            frames.push(ValidatedUgoiraFrame {
                file: name,
                delay_ms: expected.delay_ms,
                dimensions: MediaDimensions { width, height },
            });
        }
        if frames.len() != manifest.frames.len() {
            return Err(UgoiraError::ManifestMismatch);
        }
        Ok(ValidatedUgoiraManifest {
            frame_mime_type: manifest.frame_mime_type.clone(),
            frames,
            total_expanded_bytes: total_expanded,
        })
    }

    pub fn extract_first_frame(
        &self,
        media_root: &MediaRoot,
        source_relative_path: &Path,
        manifest: &PixivUgoiraMeta,
        destination_relative_path: &Path,
    ) -> Result<ExtractedUgoiraFrame, UgoiraError> {
        let path = media_root
            .resolve_file(source_relative_path)
            .map_err(UgoiraError::from)?;
        let destination = media_root
            .prepare_file(destination_relative_path)
            .map_err(UgoiraError::from)?;
        self.extract_first_frame_from_paths(&path, manifest, &destination)
    }

    fn extract_first_frame_from_paths(
        &self,
        path: &Path,
        manifest: &PixivUgoiraMeta,
        destination: &Path,
    ) -> Result<ExtractedUgoiraFrame, UgoiraError> {
        let validated = self.validate(path, manifest)?;
        let first = validated
            .frames
            .first()
            .ok_or(UgoiraError::ManifestMismatch)?;
        let format = frame_format(&manifest.frame_mime_type)?;
        let extension = destination
            .extension()
            .and_then(|value| value.to_str())
            .ok_or(UgoiraError::FrameFormatMismatch)?;
        if !format.accepts_extension(extension) {
            return Err(UgoiraError::FrameFormatMismatch);
        }
        let mut created_destination = false;
        let extraction = (|| -> Result<(), UgoiraError> {
            let file = fs::File::open(path).map_err(|_| UgoiraError::Archive)?;
            let mut archive = ZipArchive::new(file).map_err(|_| UgoiraError::Archive)?;
            let mut entry = archive.by_index(0).map_err(|_| UgoiraError::Archive)?;
            if entry.name() != first.file {
                return Err(UgoiraError::ManifestMismatch);
            }
            let mut output = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(destination)
                .map_err(|_| UgoiraError::Archive)?;
            created_destination = true;
            let copied = std::io::copy(
                &mut entry.by_ref().take(self.limits.max_entry_bytes + 1),
                &mut output,
            )
            .map_err(|_| UgoiraError::Archive)?;
            if copied > self.limits.max_entry_bytes {
                return Err(UgoiraError::EntryTooLarge {
                    entry: first.file.clone(),
                    limit: self.limits.max_entry_bytes,
                });
            }
            output.flush().map_err(|_| UgoiraError::Archive)?;
            output.sync_all().map_err(|_| UgoiraError::Archive)
        })();
        if let Err(error) = extraction {
            if created_destination {
                let _ = fs::remove_file(destination);
            }
            return Err(error);
        }

        Ok(ExtractedUgoiraFrame {
            path: destination.to_path_buf(),
            file: first.file.clone(),
            delay_ms: first.delay_ms,
            format,
            dimensions: first.dimensions,
        })
    }
}

fn validated_archive(
    path: &Path,
    manifest: &PixivUgoiraMeta,
) -> Result<ZipArchive<fs::File>, UgoiraError> {
    if manifest.frames.is_empty() {
        return Err(UgoiraError::ManifestMismatch);
    }
    let file = fs::File::open(path).map_err(|_| UgoiraError::Archive)?;
    let mut archive = ZipArchive::new(file).map_err(|_| UgoiraError::Archive)?;
    let central_directory_entries =
        central_directory_entry_count(path, archive.central_directory_start())?;
    // ZipArchive indexes entries by raw filename, so repeated names collapse into one item.
    if central_directory_entries != archive.len() {
        return Err(UgoiraError::DuplicateEntry);
    }
    if archive
        .has_overlapping_files()
        .map_err(|_| UgoiraError::Archive)?
    {
        return Err(UgoiraError::OverlappingEntries);
    }
    let mut seen = HashSet::with_capacity(archive.len());
    for index in 0..archive.len() {
        let expected = manifest
            .frames
            .get(index)
            .ok_or(UgoiraError::UnknownEntry)?;
        let entry = archive.by_index(index).map_err(|_| UgoiraError::Archive)?;
        let enclosed = entry.enclosed_name().ok_or(UgoiraError::UnsafeEntryPath)?;
        if enclosed.components().count() != 1 || !entry.is_file() {
            return Err(UgoiraError::UnsafeEntryPath);
        }
        let name = enclosed.to_str().ok_or(UgoiraError::UnsafeEntryPath)?;
        if !seen.insert(name.to_owned()) {
            return Err(UgoiraError::DuplicateEntry);
        }
        if name != expected.file {
            return Err(UgoiraError::UnknownEntry);
        }
        if entry.encrypted() {
            return Err(UgoiraError::EncryptedEntry);
        }
        if entry.size() == 0 {
            return Err(UgoiraError::InvalidFrame);
        }
    }
    if archive.len() != manifest.frames.len() {
        return Err(UgoiraError::ManifestMismatch);
    }
    Ok(archive)
}

fn central_directory_entry_count(path: &Path, start: u64) -> Result<usize, UgoiraError> {
    const CENTRAL_DIRECTORY_HEADER: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
    const FIXED_HEADER_REMAINDER: usize = 42;

    let mut file = fs::File::open(path).map_err(|_| UgoiraError::Archive)?;
    file.seek(SeekFrom::Start(start))
        .map_err(|_| UgoiraError::Archive)?;
    let mut count = 0_usize;
    loop {
        let mut signature = [0_u8; 4];
        file.read_exact(&mut signature)
            .map_err(|_| UgoiraError::Archive)?;
        if signature != CENTRAL_DIRECTORY_HEADER {
            return Ok(count);
        }
        let mut header = [0_u8; FIXED_HEADER_REMAINDER];
        file.read_exact(&mut header)
            .map_err(|_| UgoiraError::Archive)?;
        let name_length = u16::from_le_bytes([header[24], header[25]]);
        let extra_length = u16::from_le_bytes([header[26], header[27]]);
        let comment_length = u16::from_le_bytes([header[28], header[29]]);
        let variable_length =
            u64::from(name_length) + u64::from(extra_length) + u64::from(comment_length);
        file.seek(SeekFrom::Current(variable_length as i64))
            .map_err(|_| UgoiraError::Archive)?;
        count = count.checked_add(1).ok_or(UgoiraError::Archive)?;
    }
}

fn frame_format(content_type: &str) -> Result<MediaFormat, UgoiraError> {
    match content_type {
        "image/jpeg" => Ok(MediaFormat::Jpeg),
        "image/png" => Ok(MediaFormat::Png),
        _ => Err(UgoiraError::UnsupportedFrameFormat),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedUgoiraManifest {
    pub frame_mime_type: String,
    pub frames: Vec<ValidatedUgoiraFrame>,
    pub total_expanded_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedUgoiraFrame {
    pub file: String,
    pub delay_ms: u32,
    pub dimensions: MediaDimensions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtractedUgoiraFrame {
    pub path: PathBuf,
    pub file: String,
    pub delay_ms: u32,
    pub format: MediaFormat,
    pub dimensions: MediaDimensions,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum UgoiraError {
    #[error("Ugoira ZIP could not be read")]
    Archive,
    #[error("Ugoira ZIP exceeds the configured compressed size limit")]
    ZipTooLarge { limit: u64 },
    #[error("Ugoira ZIP contains too many frames")]
    TooManyFrames { limit: usize },
    #[error("Ugoira ZIP entry path is unsafe")]
    UnsafeEntryPath,
    #[error("Ugoira ZIP contains a duplicate entry")]
    DuplicateEntry,
    #[error("Ugoira ZIP contains an unknown or out-of-order entry")]
    UnknownEntry,
    #[error("Ugoira ZIP contains encrypted data")]
    EncryptedEntry,
    #[error("Ugoira ZIP entries overlap")]
    OverlappingEntries,
    #[error("Ugoira frame exceeds the configured entry size limit")]
    EntryTooLarge { entry: String, limit: u64 },
    #[error("Ugoira expanded data exceeds the configured limit")]
    ExpansionTooLarge { limit: u64 },
    #[error("Ugoira manifest does not match the ZIP entries")]
    ManifestMismatch,
    #[error("Ugoira frame format is unsupported")]
    UnsupportedFrameFormat,
    #[error("Ugoira frame format does not match the manifest")]
    FrameFormatMismatch,
    #[error("Ugoira frame is corrupt")]
    InvalidFrame,
    #[error("Ugoira frame dimensions exceed the configured limit")]
    FrameDimensionsExceeded,
    #[error("Ugoira source or destination path is outside the owned media root")]
    UnsafeMediaPath,
}

impl From<MediaPathError> for UgoiraError {
    fn from(error: MediaPathError) -> Self {
        match error {
            MediaPathError::InvalidRoot(_)
            | MediaPathError::InvalidRelativePath(_)
            | MediaPathError::UnsafeEntry(_) => Self::UnsafeMediaPath,
            MediaPathError::Io { .. } | MediaPathError::Worker(_) => Self::Archive,
        }
    }
}
