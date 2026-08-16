pub mod derivative;
pub mod download;
pub mod paths;
pub mod probe;
pub mod reflink;
pub mod root;
pub mod storage;
pub mod ugoira;

pub use derivative::{
    DerivativeError, DerivativeGenerator, DerivativeRequest, GeneratedDerivative,
};
pub use download::{DownloadError, DownloadStager, StagedDownload};
pub use paths::{PathError, PixivMediaPaths};
pub use probe::{ExpectedMedia, MediaProbe, MediaProbeLimits, MediaProbeResult, ProbeError};
pub use reflink::{ReflinkCloner, ReflinkError};
pub use root::{MediaPathError, MediaRoot};
pub use storage::{IngestRequest, MediaStore, MediaStoreConfig, StorageError, StoredMedia};
pub use ugoira::{
    ExtractedUgoiraFrame, UgoiraArchiveValidator, UgoiraError, UgoiraLimits,
    UgoiraManifestValidator, ValidatedUgoiraFrame, ValidatedUgoiraManifest,
};

pub const CRATE_NAME: &str = "pixivarchive-media";
