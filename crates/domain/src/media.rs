use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MediaFormat {
    #[serde(rename = "jpg")]
    Jpeg,
    Png,
    Gif,
    Zip,
    Webp,
    Avif,
}

impl MediaFormat {
    pub fn from_db_value(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "png" => Some(Self::Png),
            "gif" => Some(Self::Gif),
            "zip" => Some(Self::Zip),
            "webp" => Some(Self::Webp),
            "avif" => Some(Self::Avif),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Gif => "gif",
            Self::Zip => "zip",
            Self::Webp => "webp",
            Self::Avif => "avif",
        }
    }

    pub fn accepts_extension(self, extension: &str) -> bool {
        let extension = extension.trim_start_matches('.').to_ascii_lowercase();
        match self {
            Self::Jpeg => matches!(extension.as_str(), "jpg" | "jpeg"),
            _ => extension == self.extension(),
        }
    }

    pub fn accepts_content_type(self, content_type: &str) -> bool {
        let content_type = content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        match self {
            Self::Jpeg => matches!(content_type.as_str(), "image/jpeg" | "image/jpg"),
            Self::Png => content_type == "image/png",
            Self::Gif => content_type == "image/gif",
            Self::Zip => {
                matches!(
                    content_type.as_str(),
                    "application/zip" | "application/x-zip-compressed"
                )
            }
            Self::Webp => content_type == "image/webp",
            Self::Avif => content_type == "image/avif",
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Gif => "image/gif",
            Self::Zip => "application/zip",
            Self::Webp => "image/webp",
            Self::Avif => "image/avif",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaColorMode {
    Grayscale,
    GrayscaleAlpha,
    Rgb,
    Rgba,
}

impl MediaColorMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Grayscale => "grayscale",
            Self::GrayscaleAlpha => "grayscale_alpha",
            Self::Rgb => "rgb",
            Self::Rgba => "rgba",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    SourceImage,
    UgoiraZip,
    Derivative,
}

impl MediaKind {
    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "source_image" => Some(Self::SourceImage),
            "ugoira_zip" => Some(Self::UgoiraZip),
            "derivative" => Some(Self::Derivative),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DerivativeFormat {
    Webp,
    Avif,
}

impl DerivativeFormat {
    pub fn from_db_value(value: &str) -> Option<Self> {
        match value {
            "webp" => Some(Self::Webp),
            "avif" => Some(Self::Avif),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Webp => "webp",
            Self::Avif => "avif",
        }
    }
}

impl From<DerivativeFormat> for MediaFormat {
    fn from(format: DerivativeFormat) -> Self {
        match format {
            DerivativeFormat::Webp => Self::Webp,
            DerivativeFormat::Avif => Self::Avif,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MediaDimensions {
    pub width: u32,
    pub height: u32,
}
