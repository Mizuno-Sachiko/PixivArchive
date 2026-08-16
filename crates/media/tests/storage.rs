mod support;

use bytes::Bytes;
use futures_util::{StreamExt, stream};
use image::ImageFormat;
use pixivarchive_domain::{
    media::MediaFormat,
    pixiv::{PixivUgoiraFrame, PixivUgoiraMeta},
};
use pixivarchive_media::{
    ExpectedMedia, IngestRequest, MediaProbeLimits, MediaStore, MediaStoreConfig, StorageError,
    UgoiraError, UgoiraLimits, UgoiraManifestValidator,
};
use support::{TestDirectory, directory_entries, image_bytes, write_zip};
use url::Url;

fn store(directory: &TestDirectory, max_download_bytes: u64) -> MediaStore {
    MediaStore::new(
        directory.path(),
        MediaStoreConfig {
            max_download_bytes,
            probe_limits: MediaProbeLimits {
                max_bytes: max_download_bytes,
                max_width: 4_096,
                max_height: 4_096,
                max_pixels: 16_000_000,
            },
        },
    )
}

#[tokio::test]
async fn valid_stream_is_hashed_and_promoted_after_validation() {
    let directory = TestDirectory::new("storage-valid");
    let bytes = image_bytes(ImageFormat::Png, 4, 3);
    let request = IngestRequest::new(
        "originals/pixiv/12/34/34_p0_r0001.png",
        ExpectedMedia::source_image(MediaFormat::Png).with_content_type("image/png"),
    )
    .with_content_length(bytes.len() as u64);
    let chunks = stream::iter(vec![
        Ok::<_, std::io::Error>(Bytes::copy_from_slice(&bytes[..11])),
        Ok(Bytes::copy_from_slice(&bytes[11..])),
    ]);

    let stored = store(&directory, 1024 * 1024)
        .ingest(request, chunks)
        .await
        .unwrap();

    assert_eq!(
        stored.relative_path.to_string_lossy(),
        "originals/pixiv/12/34/34_p0_r0001.png"
    );
    assert_eq!(stored.byte_size, bytes.len() as u64);
    assert_ne!(stored.sha256, [0; 32]);
    assert_eq!(std::fs::read(&stored.absolute_path).unwrap(), bytes);
    assert!(directory_entries(&directory.file("staging")).is_empty());
}

#[tokio::test]
async fn invalid_or_oversized_stream_never_reaches_the_final_path() {
    let directory = TestDirectory::new("storage-invalid");
    let relative = "originals/pixiv/12/34/34_p0_r0001.jpg";
    let invalid = vec![0xff, 0xd8, 0xff, 0x00];

    let invalid_result = store(&directory, 1024)
        .ingest(
            IngestRequest::new(relative, ExpectedMedia::source_image(MediaFormat::Jpeg)),
            stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(invalid))]),
        )
        .await;
    assert!(matches!(invalid_result, Err(StorageError::Probe(_))));
    assert!(!directory.file(relative).exists());
    assert!(directory_entries(&directory.file("staging")).is_empty());

    let oversized_relative = "originals/pixiv/12/34/34_p0_r0001.png";
    let oversized_result = store(&directory, 8)
        .ingest(
            IngestRequest::new(
                oversized_relative,
                ExpectedMedia::source_image(MediaFormat::Png),
            ),
            stream::iter(vec![
                Ok::<_, std::io::Error>(Bytes::from_static(b"12345")),
                Ok(Bytes::from_static(b"67890")),
            ]),
        )
        .await;
    assert_eq!(
        oversized_result,
        Err(StorageError::ResponseTooLarge { limit: 8 })
    );
    assert!(!directory.file(oversized_relative).exists());
    assert!(directory_entries(&directory.file("staging")).is_empty());
}

#[tokio::test]
async fn cancelling_streaming_ingest_removes_the_partial_file() {
    let directory = TestDirectory::new("storage-cancelled");
    let media_store = store(&directory, 1024 * 1024);
    let request = IngestRequest::new(
        "originals/pixiv/12/34/34_p0_r0001.png",
        ExpectedMedia::source_image(MediaFormat::Png),
    );
    let chunks =
        stream::once(async { Ok::<_, std::io::Error>(Bytes::from_static(b"partial image")) })
            .chain(stream::pending());
    let task = tokio::spawn(async move { media_store.ingest(request, chunks).await });
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while directory_entries(&directory.file("staging")).is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(directory_entries(&directory.file("staging")).is_empty());
}

#[tokio::test]
async fn storage_rejects_unsafe_paths_and_conflicting_existing_files() {
    let directory = TestDirectory::new("storage-paths");
    let bytes = image_bytes(ImageFormat::Png, 2, 2);
    let media = ExpectedMedia::source_image(MediaFormat::Png);

    let traversal = store(&directory, 1024 * 1024)
        .ingest(
            IngestRequest::new("../outside.png", media.clone()),
            stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(bytes.clone()))]),
        )
        .await;
    assert!(matches!(traversal, Err(StorageError::InvalidRelativePath)));

    let relative = "originals/pixiv/1/2/2_p0_r0001.png";
    directory.write(relative, b"different");
    let conflict = store(&directory, 1024 * 1024)
        .ingest(
            IngestRequest::new(relative, media),
            stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(bytes))]),
        )
        .await;
    assert!(matches!(conflict, Err(StorageError::DestinationConflict)));
    assert_eq!(
        std::fs::read(directory.file(relative)).unwrap(),
        b"different"
    );
    assert!(directory_entries(&directory.file("staging")).is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn storage_rejects_destination_ancestors_that_are_symbolic_links() {
    use std::{os::unix::fs::symlink, path::PathBuf};

    let directory = TestDirectory::new("storage-symlink-root");
    let outside = TestDirectory::new("storage-symlink-outside");
    std::fs::create_dir_all(directory.file("originals")).unwrap();
    symlink(outside.path(), directory.file("originals/pixiv")).unwrap();
    let relative = PathBuf::from("originals/pixiv/1/2/2_p0_r0001.png");
    let bytes = image_bytes(ImageFormat::Png, 2, 2);

    let result = store(&directory, 1024 * 1024)
        .ingest(
            IngestRequest::new(
                relative.clone(),
                ExpectedMedia::source_image(MediaFormat::Png),
            ),
            stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(bytes))]),
        )
        .await;

    assert_eq!(
        result,
        Err(StorageError::UnsafeDestination { path: relative })
    );
    assert!(!outside.file("1/2/2_p0_r0001.png").exists());
    assert!(directory_entries(&directory.file("staging")).is_empty());
}

#[tokio::test]
async fn ugoira_manifest_is_validated_before_the_zip_reaches_its_final_path() {
    let directory = TestDirectory::new("storage-ugoira");
    let frame = image_bytes(ImageFormat::Jpeg, 2, 2);
    let source = directory.file("source.zip");
    write_zip(&source, &[("000000.jpg", &frame)]);
    let bytes = std::fs::read(source).unwrap();
    let relative = "originals/pixiv/12/34/34_ugoira_r0001.zip";
    let manifest = PixivUgoiraMeta {
        work_id: 34,
        zip_url: Url::parse("https://i.pximg.net/ugoira.zip").unwrap(),
        frame_mime_type: "image/jpeg".to_owned(),
        frames: vec![PixivUgoiraFrame {
            file: "different.jpg".to_owned(),
            delay_ms: 80,
        }],
    };

    let result = store(&directory, 1024 * 1024)
        .ingest_ugoira(
            IngestRequest::new(relative, ExpectedMedia::ugoira_zip()),
            stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(bytes))]),
            &manifest,
        )
        .await;

    assert_eq!(result, Err(StorageError::Ugoira(UgoiraError::UnknownEntry)));
    assert!(!directory.file(relative).exists());
    assert!(directory_entries(&directory.file("staging")).is_empty());
}

#[tokio::test]
async fn playback_limits_do_not_prevent_a_valid_ugoira_archive_from_being_stored() {
    let directory = TestDirectory::new("storage-ugoira-playback-limits");
    let frame = image_bytes(ImageFormat::Jpeg, 2, 2);
    let source = directory.file("source.zip");
    write_zip(&source, &[("000000.jpg", &frame), ("000001.jpg", &frame)]);
    let bytes = std::fs::read(&source).unwrap();
    let manifest = PixivUgoiraMeta {
        work_id: 35,
        zip_url: Url::parse("https://i.pximg.net/ugoira.zip").unwrap(),
        frame_mime_type: "image/jpeg".to_owned(),
        frames: vec![
            PixivUgoiraFrame {
                file: "000000.jpg".to_owned(),
                delay_ms: 80,
            },
            PixivUgoiraFrame {
                file: "000001.jpg".to_owned(),
                delay_ms: 80,
            },
        ],
    };
    assert!(matches!(
        UgoiraManifestValidator::new(UgoiraLimits {
            max_zip_bytes: 1024 * 1024,
            max_frames: 1,
            max_entry_bytes: 512 * 1024,
            max_total_expanded_bytes: 1024 * 1024,
            max_pixels_per_frame: 1_000_000,
        })
        .validate(&source, &manifest),
        Err(UgoiraError::TooManyFrames { limit: 1 })
    ));

    let relative = "originals/pixiv/12/35/35_ugoira_r0001.zip";
    let stored = store(&directory, 1024 * 1024)
        .ingest_ugoira(
            IngestRequest::new(relative, ExpectedMedia::ugoira_zip()),
            stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(bytes))]),
            &manifest,
        )
        .await
        .unwrap();

    assert_eq!(stored.relative_path.to_string_lossy(), relative);
    assert!(stored.absolute_path.is_file());
}
