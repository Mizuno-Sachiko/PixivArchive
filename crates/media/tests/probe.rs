mod support;

use image::{DynamicImage, GrayImage, ImageFormat, RgbaImage};
use pixivarchive_domain::media::{MediaColorMode, MediaDimensions, MediaFormat};
use pixivarchive_media::{ExpectedMedia, MediaProbe, MediaProbeLimits, ProbeError};
use std::io::Cursor;
use support::{TestDirectory, image_bytes, write_zip};

fn limits() -> MediaProbeLimits {
    MediaProbeLimits {
        max_bytes: 2 * 1024 * 1024,
        max_width: 4_096,
        max_height: 4_096,
        max_pixels: 16_000_000,
    }
}

#[test]
fn supported_pixiv_images_are_identified_from_content() {
    let directory = TestDirectory::new("probe-valid-images");
    let cases = [
        (
            ImageFormat::Jpeg,
            MediaFormat::Jpeg,
            "image/jpeg",
            "sample.jpg",
        ),
        (
            ImageFormat::Png,
            MediaFormat::Png,
            "image/png",
            "sample.png",
        ),
        (
            ImageFormat::Gif,
            MediaFormat::Gif,
            "image/gif",
            "sample.gif",
        ),
    ];
    let probe = MediaProbe::new(limits());

    for (encoder, expected_format, mime, name) in cases {
        let bytes = image_bytes(encoder, 7, 5);
        let path = directory.write(name, &bytes);
        let result = probe
            .probe(
                &path,
                &ExpectedMedia::source_image(expected_format).with_content_type(mime),
            )
            .unwrap();

        assert_eq!(result.format, expected_format);
        assert_eq!(
            result.dimensions,
            Some(MediaDimensions {
                width: 7,
                height: 5,
            })
        );
        assert_eq!(result.byte_size, bytes.len() as u64);
        assert_eq!(result.format.mime_type(), mime);
    }
}

#[test]
fn decoded_color_mode_is_reported_from_image_content() {
    let directory = TestDirectory::new("probe-color-mode");
    let grayscale = encoded_png(DynamicImage::ImageLuma8(GrayImage::new(4, 3)));
    let rgba = encoded_png(DynamicImage::ImageRgba8(RgbaImage::new(4, 3)));
    let probe = MediaProbe::new(limits());

    let grayscale_result = probe
        .probe(
            &directory.write("grayscale.png", &grayscale),
            &ExpectedMedia::source_image(MediaFormat::Png),
        )
        .unwrap();
    let rgba_result = probe
        .probe(
            &directory.write("rgba.png", &rgba),
            &ExpectedMedia::source_image(MediaFormat::Png),
        )
        .unwrap();

    assert_eq!(grayscale_result.color_mode, Some(MediaColorMode::Grayscale));
    assert_eq!(rgba_result.color_mode, Some(MediaColorMode::Rgba));
}

#[test]
fn zip_probe_requires_zip_magic_and_has_no_image_dimensions() {
    let directory = TestDirectory::new("probe-zip");
    let zip_path = directory.file("ugoira.zip");
    write_zip(&zip_path, &[("000000.jpg", b"frame")]);

    let result = MediaProbe::new(limits())
        .probe(
            &zip_path,
            &ExpectedMedia::ugoira_zip().with_content_type("application/zip"),
        )
        .unwrap();

    assert_eq!(result.format, MediaFormat::Zip);
    assert_eq!(result.dimensions, None);
}

#[test]
fn corrupt_and_spoofed_images_are_rejected() {
    let directory = TestDirectory::new("probe-invalid");
    let png = image_bytes(ImageFormat::Png, 3, 2);
    let truncated = directory.write("truncated.png", &png[..16]);
    let renamed = directory.write("renamed.jpg", &png);
    let probe = MediaProbe::new(limits());

    assert!(matches!(
        probe.probe(&truncated, &ExpectedMedia::source_image(MediaFormat::Png)),
        Err(ProbeError::InvalidImage)
    ));
    assert!(matches!(
        probe.probe(&renamed, &ExpectedMedia::source_image(MediaFormat::Jpeg)),
        Err(ProbeError::FormatMismatch {
            expected: MediaFormat::Jpeg,
            actual: MediaFormat::Png,
        })
    ));
}

#[test]
fn response_mime_size_and_dimensions_are_enforced() {
    let directory = TestDirectory::new("probe-limits");
    let jpeg = image_bytes(ImageFormat::Jpeg, 20, 10);
    let path = directory.write("sample.jpg", &jpeg);

    assert!(matches!(
        MediaProbe::new(limits()).probe(
            &path,
            &ExpectedMedia::source_image(MediaFormat::Jpeg).with_content_type("image/png"),
        ),
        Err(ProbeError::ContentTypeMismatch)
    ));

    let mut size_limited = limits();
    size_limited.max_bytes = jpeg.len() as u64 - 1;
    assert!(matches!(
        MediaProbe::new(size_limited).probe(&path, &ExpectedMedia::source_image(MediaFormat::Jpeg)),
        Err(ProbeError::ResponseTooLarge { .. })
    ));

    let mut dimension_limited = limits();
    dimension_limited.max_width = 19;
    assert!(matches!(
        MediaProbe::new(dimension_limited)
            .probe(&path, &ExpectedMedia::source_image(MediaFormat::Jpeg)),
        Err(ProbeError::DimensionsExceeded { .. })
    ));
}

fn encoded_png(image: DynamicImage) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).unwrap();
    bytes.into_inner()
}
