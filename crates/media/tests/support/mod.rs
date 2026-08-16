#![allow(dead_code)]

use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};
use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

pub struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    pub fn new(label: &str) -> Self {
        let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "pixivarchive-media-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    pub fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.file(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn image_bytes(format: ImageFormat, width: u32, height: u32) -> Vec<u8> {
    let image =
        DynamicImage::ImageRgb8(ImageBuffer::from_pixel(width, height, Rgb([32, 128, 224])));
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, format).unwrap();
    output.into_inner()
}

pub fn solid_png(width: u32, height: u32, color: [u8; 3]) -> Vec<u8> {
    let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(width, height, Rgb(color)));
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, ImageFormat::Png).unwrap();
    output.into_inner()
}

pub fn rgb_png(width: u32, height: u32, pixels: &[[u8; 3]]) -> Vec<u8> {
    let image = ImageBuffer::from_fn(width, height, |x, y| {
        let index = (y * width + x) as usize;
        Rgb(pixels[index])
    });
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(image)
        .write_to(&mut output, ImageFormat::Png)
        .unwrap();
    output.into_inner()
}

pub fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = fs::File::create(path).unwrap();
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for (name, bytes) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}

pub fn write_duplicate_zip(path: &Path, name: &str, bytes: &[u8]) {
    const PLACEHOLDER: &str = "000001.jpg";
    assert_eq!(name.len(), PLACEHOLDER.len());
    write_zip(path, &[(name, bytes), (PLACEHOLDER, bytes)]);
    let mut archive = fs::read(path).unwrap();
    for offset in 0..=archive.len() - PLACEHOLDER.len() {
        if archive[offset..].starts_with(PLACEHOLDER.as_bytes()) {
            archive[offset..offset + name.len()].copy_from_slice(name.as_bytes());
        }
    }
    fs::write(path, archive).unwrap();
}

pub fn directory_entries(path: &Path) -> Vec<PathBuf> {
    if !path.exists() {
        return Vec::new();
    }
    fs::read_dir(path)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect()
}
