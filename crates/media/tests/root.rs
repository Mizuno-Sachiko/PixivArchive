mod support;

use pixivarchive_media::{MediaPathError, MediaRoot};
use support::TestDirectory;

#[cfg(any(unix, windows))]
#[test]
fn media_root_rejects_symlinked_files_and_directories() {
    let directory = TestDirectory::new("root-symlinks");
    let media_root = directory.file("media");
    let external_directory = directory.file("outside");
    let external_file = directory.write("outside/source.bin", b"outside media");
    std::fs::create_dir_all(&media_root).unwrap();

    let linked_file = media_root.join("source.bin");
    let linked_directory = media_root.join("staging");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&external_file, &linked_file).unwrap();
        std::os::unix::fs::symlink(&external_directory, &linked_directory).unwrap();
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(&external_file, &linked_file).unwrap();
        std::os::windows::fs::symlink_dir(&external_directory, &linked_directory).unwrap();
    }

    let root = MediaRoot::new(&media_root);
    let resolved_file = root.resolve_file("source.bin");
    assert!(
        matches!(&resolved_file, Err(MediaPathError::UnsafeEntry(_))),
        "unexpected result: {resolved_file:?}"
    );
    let prepared_file = root.prepare_file("source.bin");
    assert!(
        matches!(&prepared_file, Err(MediaPathError::UnsafeEntry(_))),
        "unexpected result: {prepared_file:?}"
    );
    let prepared_directory = root.prepare_directory("staging/derived");
    assert!(
        matches!(&prepared_directory, Err(MediaPathError::UnsafeEntry(_))),
        "unexpected result: {prepared_directory:?}"
    );
    assert!(!external_directory.join("derived").exists());

    #[cfg(windows)]
    {
        std::fs::remove_file(linked_file).unwrap();
        std::fs::remove_dir(linked_directory).unwrap();
    }
}
