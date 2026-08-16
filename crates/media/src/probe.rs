use image::GenericImageView;
use pixivarchive_domain::media::{MediaColorMode, MediaDimensions, MediaFormat, MediaKind};
use std::{fs, path::Path};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaProbeLimits {
    pub max_bytes: u64,
    pub max_width: u32,
    pub max_height: u32,
    pub max_pixels: u64,
}

impl Default for MediaProbeLimits {
    fn default() -> Self {
        Self {
            max_bytes: 256 * 1024 * 1024,
            max_width: 65_535,
            max_height: 65_535,
            max_pixels: 268_435_456,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpectedMedia {
    pub kind: MediaKind,
    pub format: MediaFormat,
    reported_content_type: Option<String>,
}

impl ExpectedMedia {
    pub fn source_image(format: MediaFormat) -> Self {
        Self {
            kind: MediaKind::SourceImage,
            format,
            reported_content_type: None,
        }
    }

    pub fn ugoira_zip() -> Self {
        Self {
            kind: MediaKind::UgoiraZip,
            format: MediaFormat::Zip,
            reported_content_type: None,
        }
    }

    pub fn derivative(format: MediaFormat) -> Self {
        Self {
            kind: MediaKind::Derivative,
            format,
            reported_content_type: None,
        }
    }

    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.reported_content_type = Some(content_type.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaProbeResult {
    pub format: MediaFormat,
    pub byte_size: u64,
    pub dimensions: Option<MediaDimensions>,
    pub color_mode: Option<MediaColorMode>,
}

#[derive(Clone, Copy, Debug)]
pub struct MediaProbe {
    limits: MediaProbeLimits,
}

impl MediaProbe {
    pub fn new(limits: MediaProbeLimits) -> Self {
        Self { limits }
    }

    pub fn probe(
        &self,
        path: &Path,
        expected: &ExpectedMedia,
    ) -> Result<MediaProbeResult, ProbeError> {
        let byte_size = fs::metadata(path)
            .map_err(|_| ProbeError::ReadFailed)?
            .len();
        if byte_size > self.limits.max_bytes {
            return Err(ProbeError::ResponseTooLarge {
                actual: byte_size,
                limit: self.limits.max_bytes,
            });
        }

        if expected
            .reported_content_type
            .as_deref()
            .is_some_and(|value| !expected.format.accepts_content_type(value))
        {
            return Err(ProbeError::ContentTypeMismatch);
        }

        let inferred = infer::get_from_path(path)
            .map_err(|_| ProbeError::ReadFailed)?
            .ok_or(ProbeError::UnknownFormat)?;
        let actual = media_format(inferred.mime_type()).ok_or(ProbeError::UnknownFormat)?;
        if actual != expected.format {
            return Err(ProbeError::FormatMismatch {
                expected: expected.format,
                actual,
            });
        }

        if actual == MediaFormat::Zip {
            return Ok(MediaProbeResult {
                format: actual,
                byte_size,
                dimensions: None,
                color_mode: None,
            });
        }

        let size = imagesize::size(path).map_err(|_| ProbeError::InvalidImage)?;
        let width = u32::try_from(size.width).map_err(|_| ProbeError::InvalidImage)?;
        let height = u32::try_from(size.height).map_err(|_| ProbeError::InvalidImage)?;
        let pixels = u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or(ProbeError::DimensionsExceeded { width, height })?;
        if width == 0
            || height == 0
            || width > self.limits.max_width
            || height > self.limits.max_height
            || pixels > self.limits.max_pixels
        {
            return Err(ProbeError::DimensionsExceeded { width, height });
        }

        let color_mode = if actual != MediaFormat::Avif {
            let decoded = image::ImageReader::open(path)
                .map_err(|_| ProbeError::ReadFailed)?
                .with_guessed_format()
                .map_err(|_| ProbeError::InvalidImage)?
                .decode()
                .map_err(|_| ProbeError::InvalidImage)?;
            if decoded.dimensions() != (width, height) {
                return Err(ProbeError::InvalidImage);
            }
            color_mode(decoded.color())
        } else {
            None
        };

        Ok(MediaProbeResult {
            format: actual,
            byte_size,
            dimensions: Some(MediaDimensions { width, height }),
            color_mode,
        })
    }
}

fn color_mode(color: image::ColorType) -> Option<MediaColorMode> {
    match color {
        image::ColorType::L8 | image::ColorType::L16 => Some(MediaColorMode::Grayscale),
        image::ColorType::La8 | image::ColorType::La16 => Some(MediaColorMode::GrayscaleAlpha),
        image::ColorType::Rgb8 | image::ColorType::Rgb16 | image::ColorType::Rgb32F => {
            Some(MediaColorMode::Rgb)
        }
        image::ColorType::Rgba8 | image::ColorType::Rgba16 | image::ColorType::Rgba32F => {
            Some(MediaColorMode::Rgba)
        }
        _ => None,
    }
}

fn media_format(mime_type: &str) -> Option<MediaFormat> {
    match mime_type {
        "image/jpeg" => Some(MediaFormat::Jpeg),
        "image/png" => Some(MediaFormat::Png),
        "image/gif" => Some(MediaFormat::Gif),
        "application/zip" => Some(MediaFormat::Zip),
        "image/webp" => Some(MediaFormat::Webp),
        "image/avif" => Some(MediaFormat::Avif),
        _ => None,
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProbeError {
    #[error("media file could not be read")]
    ReadFailed,
    #[error("media format could not be identified")]
    UnknownFormat,
    #[error("media format does not match the expected format")]
    FormatMismatch {
        expected: MediaFormat,
        actual: MediaFormat,
    },
    #[error("HTTP content type does not match the expected media format")]
    ContentTypeMismatch,
    #[error("image data is corrupt or incomplete")]
    InvalidImage,
    #[error("media response exceeds the configured limit")]
    ResponseTooLarge { actual: u64, limit: u64 },
    #[error("image dimensions exceed the configured limits")]
    DimensionsExceeded { width: u32, height: u32 },
}
