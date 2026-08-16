use pixivarchive_domain::media::{DerivativeFormat, MediaFormat};
use std::path::PathBuf;
use thiserror::Error;

pub struct PixivMediaPaths;

impl PixivMediaPaths {
    pub fn original_image(
        author_id: i64,
        work_id: i64,
        page_index: u32,
        revision: u64,
        format: MediaFormat,
    ) -> Result<PathBuf, PathError> {
        validate(author_id, work_id, revision)?;
        if !matches!(
            format,
            MediaFormat::Jpeg | MediaFormat::Png | MediaFormat::Gif
        ) {
            return Err(PathError::UnsupportedFormat);
        }
        Ok(original_directory(author_id, work_id).join(format!(
            "{work_id}_p{page_index}_r{revision:04}.{}",
            format.extension()
        )))
    }

    pub fn ugoira_zip(author_id: i64, work_id: i64, revision: u64) -> Result<PathBuf, PathError> {
        validate(author_id, work_id, revision)?;
        Ok(original_directory(author_id, work_id)
            .join(format!("{work_id}_ugoira_r{revision:04}.zip")))
    }

    pub fn waterfall_derivative(
        author_id: i64,
        work_id: i64,
        page_index: u32,
        revision: u64,
        format: DerivativeFormat,
    ) -> Result<PathBuf, PathError> {
        validate(author_id, work_id, revision)?;
        Ok(derivative_directory(author_id, work_id).join(format!(
            "{work_id}_p{page_index}_r{revision:04}_waterfall.{}",
            format.extension()
        )))
    }

    pub fn ugoira_cover(
        author_id: i64,
        work_id: i64,
        revision: u64,
        format: DerivativeFormat,
    ) -> Result<PathBuf, PathError> {
        validate(author_id, work_id, revision)?;
        Ok(derivative_directory(author_id, work_id).join(format!(
            "{work_id}_ugoira_r{revision:04}_cover.{}",
            format.extension()
        )))
    }
}

fn validate(author_id: i64, work_id: i64, revision: u64) -> Result<(), PathError> {
    if author_id <= 0 || work_id <= 0 {
        return Err(PathError::InvalidIdentifier);
    }
    if revision == 0 {
        return Err(PathError::InvalidRevision);
    }
    Ok(())
}

fn original_directory(author_id: i64, work_id: i64) -> PathBuf {
    PathBuf::from("originals")
        .join("pixiv")
        .join(author_id.to_string())
        .join(work_id.to_string())
}

fn derivative_directory(author_id: i64, work_id: i64) -> PathBuf {
    PathBuf::from("derivatives")
        .join("pixiv")
        .join(author_id.to_string())
        .join(work_id.to_string())
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum PathError {
    #[error("Pixiv media identifiers must be positive")]
    InvalidIdentifier,
    #[error("media revision must be positive")]
    InvalidRevision,
    #[error("format is not supported for Pixiv source media")]
    UnsupportedFormat,
}
