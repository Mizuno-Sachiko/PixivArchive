use crate::{
    probe::{ExpectedMedia, MediaProbe, MediaProbeLimits, ProbeError},
    root::MediaRoot,
};
use image::Pixel;
use pixivarchive_domain::media::{DerivativeFormat, MediaDimensions, MediaFormat};
use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
};
use thiserror::Error;
use tokio::{fs, process::Command};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct DerivativeGenerator {
    program: PathBuf,
    probe: MediaProbe,
    avif_available: bool,
}

impl DerivativeGenerator {
    pub fn new(
        program: impl Into<PathBuf>,
        probe_limits: MediaProbeLimits,
        avif_available: bool,
    ) -> Self {
        Self {
            program: program.into(),
            probe: MediaProbe::new(probe_limits),
            avif_available,
        }
    }

    pub async fn generate(
        &self,
        request: DerivativeRequest,
    ) -> Result<GeneratedDerivative, DerivativeError> {
        if request.max_width == 0 || !(1..=100).contains(&request.quality) {
            return Err(DerivativeError::InvalidRequest);
        }
        if request.format == DerivativeFormat::Avif && !self.avif_available {
            return Err(DerivativeError::AvifUnavailable);
        }
        let destination = request
            .destination_root
            .prepare_file_async(request.relative_path.clone())
            .await
            .map_err(|_| DerivativeError::Storage)?;
        match fs::symlink_metadata(&destination).await {
            Ok(_) => return self.existing_output(&request, &destination).await,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(DerivativeError::Storage),
        }

        let temporary = TemporaryOutput::new(temporary_path(&destination, request.format)?);
        let mut output_argument = OsString::from(temporary.path().as_os_str());
        output_argument.push(format!("[Q={},strip]", request.quality));
        let mut command = Command::new(&self.program);
        command
            .arg(&request.source)
            .arg("--size")
            .arg(request.max_width.to_string())
            .arg("--output")
            .arg(output_argument)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let status = command
            .status()
            .await
            .map_err(|_| DerivativeError::Process)?;
        if !status.success() {
            return Err(DerivativeError::Process);
        }

        let generated = self.validate_output(&request, temporary.path())?;
        match fs::hard_link(temporary.path(), &destination).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                return self.existing_output(&request, &destination).await;
            }
            Err(_) => return Err(DerivativeError::Storage),
        }
        Ok(GeneratedDerivative {
            path: destination,
            ..generated
        })
    }

    async fn existing_output(
        &self,
        request: &DerivativeRequest,
        destination: &Path,
    ) -> Result<GeneratedDerivative, DerivativeError> {
        let metadata = fs::symlink_metadata(destination)
            .await
            .map_err(|_| DerivativeError::Storage)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(DerivativeError::DestinationExists);
        }
        self.validate_output(request, destination)
            .map_err(|_| DerivativeError::DestinationExists)
    }

    fn validate_output(
        &self,
        request: &DerivativeRequest,
        path: &Path,
    ) -> Result<GeneratedDerivative, DerivativeError> {
        let result = self
            .probe
            .probe(
                path,
                &ExpectedMedia::derivative(MediaFormat::from(request.format)),
            )
            .map_err(DerivativeError::Probe)?;
        let dimensions = result.dimensions.ok_or(DerivativeError::InvalidOutput)?;
        if dimensions.width > request.max_width {
            return Err(DerivativeError::InvalidOutput);
        }
        let dominant_color = dominant_color(&request.source)?;
        Ok(GeneratedDerivative {
            path: path.to_path_buf(),
            format: request.format,
            dimensions,
            byte_size: result.byte_size,
            dominant_color,
        })
    }
}

fn dominant_color(path: &Path) -> Result<String, DerivativeError> {
    let image = image::ImageReader::open(path)
        .map_err(|_| DerivativeError::DominantColor)?
        .with_guessed_format()
        .map_err(|_| DerivativeError::DominantColor)?
        .decode()
        .map_err(|_| DerivativeError::DominantColor)?;
    let sample = image.thumbnail(64, 64).to_rgb8();
    let mut clusters: HashMap<[u8; 3], (u64, [u64; 3])> = HashMap::new();
    for pixel in sample.pixels() {
        let channels = pixel.channels();
        let rgb = [channels[0], channels[1], channels[2]];
        let key = [rgb[0] >> 3, rgb[1] >> 3, rgb[2] >> 3];
        let entry = clusters.entry(key).or_insert((0, [0; 3]));
        entry.0 += 1;
        for (total, value) in entry.1.iter_mut().zip(rgb) {
            *total += u64::from(value);
        }
    }
    let (_, (count, totals)) = clusters
        .into_iter()
        .max_by_key(|(_, (count, _))| *count)
        .ok_or(DerivativeError::DominantColor)?;
    Ok(format!(
        "#{:02x}{:02x}{:02x}",
        totals[0] / count,
        totals[1] / count,
        totals[2] / count
    ))
}

fn temporary_path(
    destination: &Path,
    format: DerivativeFormat,
) -> Result<PathBuf, DerivativeError> {
    let parent = destination
        .parent()
        .ok_or(DerivativeError::InvalidRequest)?;
    let file_name = destination
        .file_name()
        .ok_or(DerivativeError::InvalidRequest)?
        .to_string_lossy();
    Ok(parent.join(format!(
        ".{file_name}.{}.{}",
        Uuid::now_v7(),
        format.extension()
    )))
}

struct TemporaryOutput {
    path: PathBuf,
}

impl TemporaryOutput {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryOutput {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Clone, Debug)]
pub struct DerivativeRequest {
    pub source: PathBuf,
    pub destination_root: MediaRoot,
    pub relative_path: PathBuf,
    pub format: DerivativeFormat,
    pub max_width: u32,
    pub quality: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedDerivative {
    pub path: PathBuf,
    pub format: DerivativeFormat,
    pub dimensions: MediaDimensions,
    pub byte_size: u64,
    pub dominant_color: String,
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DerivativeError {
    #[error("derivative request is invalid")]
    InvalidRequest,
    #[error("AVIF derivatives are unavailable")]
    AvifUnavailable,
    #[error("derivative destination already exists")]
    DestinationExists,
    #[error("derivative process failed")]
    Process,
    #[error("derivative output storage failed")]
    Storage,
    #[error("derivative output validation failed")]
    Probe(ProbeError),
    #[error("derivative output has no dimensions")]
    InvalidOutput,
    #[error("derivative dominant color could not be calculated")]
    DominantColor,
}
