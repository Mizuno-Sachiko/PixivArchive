use pixivarchive_media::MediaRoot;
use std::{ffi::OsStr, path::Path};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

pub async fn prepare(root: &MediaRoot) -> std::io::Result<()> {
    let exports = root
        .prepare_directory_async("exports")
        .await
        .map_err(std::io::Error::other)?;
    let avatars = root
        .prepare_directory_async("avatars")
        .await
        .map_err(std::io::Error::other)?;
    remove_matching_files(&exports, is_stale_export).await?;
    remove_matching_files(&avatars, is_stale_avatar_temporary).await?;
    verify_writable(&exports, format!("{}.zip", Uuid::now_v7())).await?;
    verify_writable(&avatars, format!(".{}.tmp", Uuid::now_v7())).await
}

async fn remove_matching_files(
    directory: &Path,
    matches: fn(&OsStr) -> bool,
) -> std::io::Result<()> {
    let mut entries = tokio::fs::read_dir(directory).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_file() && matches(&entry.file_name()) {
            tokio::fs::remove_file(entry.path()).await?;
        }
    }
    Ok(())
}

async fn verify_writable(directory: &Path, file_name: String) -> std::io::Result<()> {
    let path = directory.join(file_name);
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await?;
    file.write_all(b"pixivarchive cache probe").await?;
    file.sync_all().await?;
    drop(file);
    tokio::fs::remove_file(path).await
}

fn is_stale_export(name: &OsStr) -> bool {
    name.to_str()
        .and_then(|name| name.strip_suffix(".zip"))
        .is_some_and(|stem| Uuid::parse_str(stem).is_ok())
}

fn is_stale_avatar_temporary(name: &OsStr) -> bool {
    name.to_str()
        .and_then(|name| name.strip_prefix('.'))
        .and_then(|name| name.strip_suffix(".tmp"))
        .is_some_and(|stem| Uuid::parse_str(stem).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn prepare_removes_only_owned_temporary_files() {
        let root = std::env::temp_dir().join(format!("pixivarchive-cache-{}", Uuid::now_v7()));
        let cache_root = MediaRoot::new(&root);
        let exports = cache_root.path().join("exports");
        let avatars = cache_root.path().join("avatars");
        tokio::fs::create_dir_all(&exports).await.unwrap();
        tokio::fs::create_dir_all(&avatars).await.unwrap();
        let stale_export = exports.join(format!("{}.zip", Uuid::now_v7()));
        let stale_avatar = avatars.join(format!(".{}.tmp", Uuid::now_v7()));
        let unrelated = exports.join("keep.zip");
        tokio::fs::write(&stale_export, b"stale").await.unwrap();
        tokio::fs::write(&stale_avatar, b"stale").await.unwrap();
        tokio::fs::write(&unrelated, b"keep").await.unwrap();

        prepare(&cache_root).await.unwrap();

        assert!(!stale_export.exists());
        assert!(!stale_avatar.exists());
        assert!(unrelated.exists());
        assert_eq!(file_count(&exports).await, 1);
        assert_eq!(file_count(&avatars).await, 0);

        prepare(&cache_root).await.unwrap();
        assert_eq!(file_count(&exports).await, 1);
        assert_eq!(file_count(&avatars).await, 0);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[cfg(any(unix, windows))]
    #[tokio::test]
    async fn prepare_rejects_a_symbolic_link_without_writing_outside() {
        #[cfg(unix)]
        use std::os::unix::fs::symlink;
        #[cfg(windows)]
        use std::os::windows::fs::symlink_dir as symlink;

        let root = std::env::temp_dir().join(format!("pixivarchive-cache-link-{}", Uuid::now_v7()));
        let outside =
            std::env::temp_dir().join(format!("pixivarchive-cache-outside-{}", Uuid::now_v7()));
        tokio::fs::create_dir_all(&outside).await.unwrap();
        symlink(&outside, &root).unwrap();

        let result = prepare(&MediaRoot::new(&root)).await;
        let wrote_outside = outside.join("exports").exists() || outside.join("avatars").exists();

        #[cfg(windows)]
        tokio::fs::remove_dir(&root).await.unwrap();
        #[cfg(unix)]
        tokio::fs::remove_file(&root).await.unwrap();
        tokio::fs::remove_dir_all(&outside).await.unwrap();

        assert!(result.is_err());
        assert!(!wrote_outside);
    }

    async fn file_count(directory: &Path) -> usize {
        let mut entries = tokio::fs::read_dir(directory).await.unwrap();
        let mut count = 0;
        while entries.next_entry().await.unwrap().is_some() {
            count += 1;
        }
        count
    }
}
