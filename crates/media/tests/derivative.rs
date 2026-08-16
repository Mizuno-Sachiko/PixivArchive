mod support;

use pixivarchive_domain::media::{DerivativeFormat, MediaDimensions};
use pixivarchive_media::{
    DerivativeError, DerivativeGenerator, DerivativeRequest, MediaProbeLimits,
};
use std::path::PathBuf;
use support::{TestDirectory, solid_png};

#[tokio::test]
async fn vipsthumbnail_generates_a_bounded_webp_without_shell_parsing() {
    let directory = TestDirectory::new("derivative-webp");
    let source = directory.write("source.png", &solid_png(320, 180, [20, 80, 200]));
    let output = directory.file("thumbnail.webp");
    let generator = DerivativeGenerator::new(
        "vipsthumbnail",
        MediaProbeLimits {
            max_bytes: 4 * 1024 * 1024,
            max_width: 1_024,
            max_height: 1_024,
            max_pixels: 1_048_576,
        },
        false,
    );

    let generated = generator
        .generate(DerivativeRequest {
            source,
            destination_root: directory.path().into(),
            relative_path: PathBuf::from("thumbnail.webp"),
            format: DerivativeFormat::Webp,
            max_width: 120,
            quality: 82,
        })
        .await
        .unwrap();

    assert_eq!(generated.path, output);
    assert_eq!(generated.format, DerivativeFormat::Webp);
    assert_eq!(generated.dominant_color, "#1450c8");
    assert_eq!(
        generated.dimensions,
        MediaDimensions {
            width: 120,
            height: 68,
        }
    );
    assert!(generated.byte_size > 0);
}

#[tokio::test]
async fn avif_generation_requires_a_successful_deployment_probe() {
    let directory = TestDirectory::new("derivative-avif-gate");
    let source = directory.write("source.png", &solid_png(20, 20, [255, 0, 0]));
    let generator = DerivativeGenerator::new("vipsthumbnail", MediaProbeLimits::default(), false);

    let result = generator
        .generate(DerivativeRequest {
            source,
            destination_root: directory.path().into(),
            relative_path: PathBuf::from("thumbnail.avif"),
            format: DerivativeFormat::Avif,
            max_width: 20,
            quality: 80,
        })
        .await;

    assert!(matches!(result, Err(DerivativeError::AvifUnavailable)));
}

#[tokio::test]
async fn invalid_process_output_is_removed_before_it_reaches_the_final_path() {
    let directory = TestDirectory::new("derivative-invalid-output");
    let script = directory.write(
        "fake-thumbnail.sh",
        br#"output="${4%%[*}"
printf 'not-an-image' > "$output"
"#,
    );
    let output = directory.file("thumbnail.webp");
    let generator = DerivativeGenerator::new("sh", MediaProbeLimits::default(), false);

    let result = generator
        .generate(DerivativeRequest {
            source: script,
            destination_root: directory.path().into(),
            relative_path: PathBuf::from("thumbnail.webp"),
            format: DerivativeFormat::Webp,
            max_width: 120,
            quality: 82,
        })
        .await;

    assert!(matches!(result, Err(DerivativeError::Probe(_))));
    assert!(!output.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_generation_terminates_the_external_thumbnail_process() {
    let directory = TestDirectory::new("derivative-cancel");
    let pid_marker = directory.file("process-pid");
    let finished_marker = directory.file("process-finished");
    let script = directory.write(
        "slow-thumbnail.sh",
        format!(
            "printf '%s' \"$$\" > '{}'\nsleep 30\nprintf finished > '{}'\n",
            pid_marker.display(),
            finished_marker.display()
        )
        .as_bytes(),
    );
    let generator = DerivativeGenerator::new("sh", MediaProbeLimits::default(), false);
    let destination_root = directory.path().to_path_buf();
    let task = tokio::spawn(async move {
        generator
            .generate(DerivativeRequest {
                source: script,
                destination_root: destination_root.into(),
                relative_path: PathBuf::from("thumbnail.webp"),
                format: DerivativeFormat::Webp,
                max_width: 120,
                quality: 82,
            })
            .await
    });

    let pid = wait_for_process_pid(&pid_marker).await;
    task.abort();
    let _ = task.await;
    wait_for_process_exit(pid).await;

    assert!(!finished_marker.exists());
}

#[cfg(unix)]
async fn wait_for_process_pid(marker: &std::path::Path) -> u32 {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Ok(value) = tokio::fs::read_to_string(marker).await
                && let Ok(pid) = value.parse()
            {
                return pid;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("thumbnail process did not publish its PID")
}

#[cfg(unix)]
async fn wait_for_process_exit(pid: u32) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let running = tokio::process::Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await
                .is_ok_and(|status| status.success());
            if !running {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("thumbnail process remained alive after cancellation");
}

#[cfg(unix)]
#[tokio::test]
async fn generation_rejects_destination_ancestors_that_are_symbolic_links() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::new("derivative-symlink-root");
    let outside = TestDirectory::new("derivative-symlink-outside");
    let source = directory.write("source.png", &solid_png(20, 20, [255, 0, 0]));
    symlink(outside.path(), directory.file("derivatives")).unwrap();
    let generator = DerivativeGenerator::new("vipsthumbnail", MediaProbeLimits::default(), false);

    let result = generator
        .generate(DerivativeRequest {
            source,
            destination_root: directory.path().into(),
            relative_path: PathBuf::from("derivatives/thumbnail.webp"),
            format: DerivativeFormat::Webp,
            max_width: 20,
            quality: 80,
        })
        .await;

    assert_eq!(result, Err(DerivativeError::Storage));
    assert!(!outside.file("thumbnail.webp").exists());
}
