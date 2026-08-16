use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use pixivarchive_application::installation::InstallationData;
use uuid::Uuid;

#[test]
fn installation_imports_the_legacy_cookie_key_once_with_its_identifier() {
    let media_root =
        std::env::temp_dir().join(format!("pixivarchive-installation-{}", Uuid::now_v7()));
    let data = InstallationData::new(&media_root);
    let legacy_key = [9_u8; 32];
    let encoded = URL_SAFE_NO_PAD.encode(legacy_key);

    let imported = data.prepare_with_legacy("rotated-key", &encoded).unwrap();
    assert_eq!(imported.key_id(), "rotated-key");
    assert_eq!(imported.key(), legacy_key);

    let stored = data
        .prepare_with_legacy("ignored-key", &URL_SAFE_NO_PAD.encode([3_u8; 32]))
        .unwrap();
    assert_eq!(stored, imported);

    std::fs::remove_dir_all(media_root).unwrap();
}

#[test]
fn installation_data_owns_cache_and_stable_pixiv_cookie_key() {
    let media_root =
        std::env::temp_dir().join(format!("pixivarchive-installation-{}", Uuid::now_v7()));
    let data = InstallationData::new(&media_root);

    assert_eq!(
        data.cache_root(),
        media_root.join(".pixivarchive").join("cache")
    );
    let first = data.prepare().unwrap();
    let second = data.prepare().unwrap();
    assert_eq!(first, second);
    assert_eq!(data.load_pixiv_cookie_key().unwrap(), first);
    assert_eq!(first.key_id(), "primary");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(media_root.join(".pixivarchive/pixiv-cookie.key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    std::fs::remove_dir_all(media_root).unwrap();
}

#[cfg(unix)]
#[test]
fn installation_data_rejects_a_symlinked_cookie_key() {
    use std::os::unix::fs::symlink;

    let media_root =
        std::env::temp_dir().join(format!("pixivarchive-installation-{}", Uuid::now_v7()));
    let data_root = media_root.join(".pixivarchive");
    let external_key = media_root.with_extension("key");
    std::fs::create_dir_all(&data_root).unwrap();
    std::fs::write(&external_key, [7_u8; 32]).unwrap();
    symlink(&external_key, data_root.join("pixiv-cookie.key")).unwrap();

    assert!(InstallationData::new(&media_root).prepare().is_err());

    std::fs::remove_dir_all(media_root).unwrap();
    std::fs::remove_file(external_key).unwrap();
}

#[cfg(any(unix, windows))]
#[test]
fn installation_data_rejects_a_symlinked_media_root_without_writing_outside() {
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    #[cfg(windows)]
    use std::os::windows::fs::symlink_dir as symlink;

    let media_root =
        std::env::temp_dir().join(format!("pixivarchive-installation-link-{}", Uuid::now_v7()));
    let external_root = std::env::temp_dir().join(format!(
        "pixivarchive-installation-outside-{}",
        Uuid::now_v7()
    ));
    std::fs::create_dir_all(&external_root).unwrap();
    symlink(&external_root, &media_root).unwrap();

    let result = InstallationData::new(&media_root).prepare();
    let wrote_outside = external_root.join(".pixivarchive").exists();

    #[cfg(windows)]
    std::fs::remove_dir(&media_root).unwrap();
    #[cfg(unix)]
    std::fs::remove_file(&media_root).unwrap();
    std::fs::remove_dir_all(&external_root).unwrap();

    assert!(result.is_err());
    assert!(!wrote_outside);
}

#[test]
fn installation_data_rejects_an_invalid_existing_key() {
    let media_root =
        std::env::temp_dir().join(format!("pixivarchive-installation-{}", Uuid::now_v7()));
    let data_root = media_root.join(".pixivarchive");
    std::fs::create_dir_all(&data_root).unwrap();
    std::fs::write(data_root.join("pixiv-cookie.key"), [7_u8; 31]).unwrap();

    assert!(InstallationData::new(&media_root).prepare().is_err());

    std::fs::remove_dir_all(media_root).unwrap();
}
