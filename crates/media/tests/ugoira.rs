mod support;

use image::ImageFormat;
use pixivarchive_domain::pixiv::{PixivUgoiraFrame, PixivUgoiraMeta};
use pixivarchive_media::{MediaRoot, UgoiraError, UgoiraLimits, UgoiraManifestValidator};
use support::{TestDirectory, image_bytes, write_duplicate_zip, write_zip};
use url::Url;

fn meta(files: &[(&str, u32)]) -> PixivUgoiraMeta {
    PixivUgoiraMeta {
        work_id: 100,
        zip_url: Url::parse("https://i.pximg.net/ugoira.zip").unwrap(),
        frame_mime_type: "image/jpeg".to_owned(),
        frames: files
            .iter()
            .map(|(file, delay_ms)| PixivUgoiraFrame {
                file: (*file).to_owned(),
                delay_ms: *delay_ms,
            })
            .collect(),
    }
}

fn limits() -> UgoiraLimits {
    UgoiraLimits {
        max_zip_bytes: 1024 * 1024,
        max_frames: 10,
        max_entry_bytes: 512 * 1024,
        max_total_expanded_bytes: 2 * 1024 * 1024,
        max_pixels_per_frame: 1_000_000,
    }
}

#[test]
fn manifest_preserves_frame_order_delays_and_dimensions() {
    let directory = TestDirectory::new("ugoira-valid");
    let first = image_bytes(ImageFormat::Jpeg, 4, 3);
    let second = image_bytes(ImageFormat::Jpeg, 5, 2);
    let path = directory.file("valid.zip");
    write_zip(&path, &[("000000.jpg", &first), ("000001.jpg", &second)]);

    let validated = UgoiraManifestValidator::new(limits())
        .validate(&path, &meta(&[("000000.jpg", 80), ("000001.jpg", 120)]))
        .unwrap();

    assert_eq!(validated.frames[0].file, "000000.jpg");
    assert_eq!(validated.frames[0].delay_ms, 80);
    assert_eq!(validated.frames[0].dimensions.width, 4);
    assert_eq!(validated.frames[1].file, "000001.jpg");
    assert_eq!(validated.frames[1].delay_ms, 120);
    assert_eq!(validated.frames[1].dimensions.height, 2);
}

#[test]
fn unsafe_duplicate_and_unknown_entries_are_rejected() {
    let directory = TestDirectory::new("ugoira-names");
    let frame = image_bytes(ImageFormat::Jpeg, 2, 2);
    let validator = UgoiraManifestValidator::new(limits());

    let traversal = directory.file("traversal.zip");
    write_zip(&traversal, &[("../000000.jpg", &frame)]);
    assert!(matches!(
        validator.validate(&traversal, &meta(&[("000000.jpg", 80)])),
        Err(UgoiraError::UnsafeEntryPath)
    ));

    let duplicate = directory.file("duplicate.zip");
    write_duplicate_zip(&duplicate, "000000.jpg", &frame);
    assert_eq!(
        validator.validate(&duplicate, &meta(&[("000000.jpg", 80)])),
        Err(UgoiraError::DuplicateEntry)
    );

    let unknown = directory.file("unknown.zip");
    write_zip(
        &unknown,
        &[("000000.jpg", &frame), ("notes.txt", b"unexpected")],
    );
    assert!(matches!(
        validator.validate(&unknown, &meta(&[("000000.jpg", 80)])),
        Err(UgoiraError::UnknownEntry)
    ));
}

#[test]
fn frame_count_entry_size_and_expansion_limits_are_enforced() {
    let directory = TestDirectory::new("ugoira-limits");
    let frame = image_bytes(ImageFormat::Jpeg, 2, 2);
    let path = directory.file("frames.zip");
    write_zip(&path, &[("000000.jpg", &frame), ("000001.jpg", &frame)]);
    let manifest = meta(&[("000000.jpg", 80), ("000001.jpg", 80)]);

    let mut frame_limits = limits();
    frame_limits.max_frames = 1;
    assert!(matches!(
        UgoiraManifestValidator::new(frame_limits).validate(&path, &manifest),
        Err(UgoiraError::TooManyFrames { limit: 1 })
    ));

    let mut entry_limits = limits();
    entry_limits.max_entry_bytes = frame.len() as u64 - 1;
    assert!(matches!(
        UgoiraManifestValidator::new(entry_limits).validate(&path, &manifest),
        Err(UgoiraError::EntryTooLarge { .. })
    ));

    let mut total_limits = limits();
    total_limits.max_total_expanded_bytes = frame.len() as u64 * 2 - 1;
    assert!(matches!(
        UgoiraManifestValidator::new(total_limits).validate(&path, &manifest),
        Err(UgoiraError::ExpansionTooLarge { .. })
    ));
}

#[test]
fn validated_first_frame_can_be_extracted_for_cover_and_analysis_jobs() {
    let directory = TestDirectory::new("ugoira-cover-source");
    let first = image_bytes(ImageFormat::Jpeg, 4, 3);
    let second = image_bytes(ImageFormat::Jpeg, 5, 2);
    let archive = directory.file("source.zip");
    let extracted = directory.file("staging/first.jpg");
    write_zip(&archive, &[("000000.jpg", &first), ("000001.jpg", &second)]);
    let manifest = meta(&[("000000.jpg", 80), ("000001.jpg", 120)]);

    let frame = UgoiraManifestValidator::new(limits())
        .extract_first_frame(
            &MediaRoot::new(directory.path()),
            archive.strip_prefix(directory.path()).unwrap(),
            &manifest,
            extracted.strip_prefix(directory.path()).unwrap(),
        )
        .unwrap();

    assert_eq!(
        std::fs::canonicalize(&frame.path).unwrap(),
        std::fs::canonicalize(&extracted).unwrap()
    );
    assert_eq!(frame.file, "000000.jpg");
    assert_eq!(frame.delay_ms, 80);
    assert_eq!(frame.dimensions.width, 4);
    assert_eq!(std::fs::read(frame.path).unwrap(), first);
}

#[test]
fn first_frame_extraction_preserves_an_existing_destination() {
    let directory = TestDirectory::new("ugoira-cover-existing");
    let first = image_bytes(ImageFormat::Jpeg, 4, 3);
    let archive = directory.file("source.zip");
    let extracted = directory.write("staging/first.jpg", b"existing");
    write_zip(&archive, &[("000000.jpg", &first)]);
    let manifest = meta(&[("000000.jpg", 80)]);

    let result = UgoiraManifestValidator::new(limits()).extract_first_frame(
        &MediaRoot::new(directory.path()),
        archive.strip_prefix(directory.path()).unwrap(),
        &manifest,
        extracted.strip_prefix(directory.path()).unwrap(),
    );

    assert_eq!(result, Err(UgoiraError::Archive));
    assert_eq!(std::fs::read(extracted).unwrap(), b"existing");
}
