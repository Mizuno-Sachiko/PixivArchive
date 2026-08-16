mod support;

use pixivarchive_media::{MediaRoot, ReflinkCloner, ReflinkError};
use support::TestDirectory;

#[test]
fn reflink_requires_identical_size_and_sha256() {
    let directory = TestDirectory::new("reflink-mismatch");
    let source = directory.write("source.bin", b"abcdefgh");
    let destination = directory.write("destination.bin", b"abcdEfgh");

    assert!(matches!(
        ReflinkCloner::new().clone_identical(
            &MediaRoot::new(directory.path()),
            source.strip_prefix(directory.path()).unwrap(),
            destination.strip_prefix(directory.path()).unwrap(),
        ),
        Err(ReflinkError::ContentMismatch)
    ));
}

#[cfg(unix)]
#[test]
fn same_filesystem_clone_is_copy_on_write_or_explicitly_unsupported() {
    use std::os::unix::fs::MetadataExt;

    let directory = TestDirectory::new("reflink");
    let source = directory.write("source.bin", b"shared media blocks");
    let destination = directory.write("destination.bin", b"shared media blocks");

    match ReflinkCloner::new().clone_identical(
        &MediaRoot::new(directory.path()),
        source.strip_prefix(directory.path()).unwrap(),
        destination.strip_prefix(directory.path()).unwrap(),
    ) {
        Ok(()) => {
            let source_inode = std::fs::metadata(&source).unwrap().ino();
            let destination_inode = std::fs::metadata(&destination).unwrap().ino();
            assert_ne!(source_inode, destination_inode);
            std::fs::write(&destination, b"private destination").unwrap();
            assert_eq!(std::fs::read(&source).unwrap(), b"shared media blocks");
        }
        Err(ReflinkError::Unsupported) => {}
        Err(other) => panic!("unexpected reflink result: {other}"),
    }
}
