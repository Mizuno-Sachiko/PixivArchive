use pixivarchive_domain::media::{DerivativeFormat, MediaFormat};
use pixivarchive_media::{PathError, PixivMediaPaths};
use std::path::PathBuf;

#[test]
fn original_paths_are_stable_and_human_readable() {
    assert_eq!(
        PixivMediaPaths::original_image(123, 456, 0, 1, MediaFormat::Jpeg).unwrap(),
        PathBuf::from("originals/pixiv/123/456/456_p0_r0001.jpg")
    );
    assert_eq!(
        PixivMediaPaths::original_image(123, 456, 12, 10_002, MediaFormat::Png).unwrap(),
        PathBuf::from("originals/pixiv/123/456/456_p12_r10002.png")
    );
    assert_eq!(
        PixivMediaPaths::ugoira_zip(123, 456, 7).unwrap(),
        PathBuf::from("originals/pixiv/123/456/456_ugoira_r0007.zip")
    );
}

#[test]
fn derivative_paths_keep_the_source_revision_visible() {
    assert_eq!(
        PixivMediaPaths::waterfall_derivative(123, 456, 2, 9, DerivativeFormat::Webp,).unwrap(),
        PathBuf::from("derivatives/pixiv/123/456/456_p2_r0009_waterfall.webp")
    );
    assert_eq!(
        PixivMediaPaths::ugoira_cover(123, 456, 3, DerivativeFormat::Avif).unwrap(),
        PathBuf::from("derivatives/pixiv/123/456/456_ugoira_r0003_cover.avif")
    );
}

#[test]
fn paths_reject_invalid_source_identifiers_and_revisions() {
    assert!(matches!(
        PixivMediaPaths::original_image(0, 456, 0, 1, MediaFormat::Jpeg),
        Err(PathError::InvalidIdentifier)
    ));
    assert!(matches!(
        PixivMediaPaths::ugoira_zip(123, -1, 1),
        Err(PathError::InvalidIdentifier)
    ));
    assert!(matches!(
        PixivMediaPaths::original_image(123, 456, 0, 0, MediaFormat::Png),
        Err(PathError::InvalidRevision)
    ));
}
