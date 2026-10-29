#[test]
fn staged_downloads_require_confirmation_digest_and_never_overwrite() {
    let dir = std::env::temp_dir().join(format!("saya-download-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(dir.join("dest")).unwrap();
    let staging = dir.join("staging.bin");
    std::fs::write(&staging, b"synthetic model bytes").unwrap();
    let sha = studio_core::gallery::digest(b"synthetic model bytes");
    let dest = dir.join("dest/model.safetensors");
    assert!(
        studio_core::downloads::commit_download(&staging, &dest, Some(&sha), Some(21), false)
            .is_err()
    );
    assert!(
        studio_core::downloads::commit_download(&staging, &dest, Some("00"), Some(21), true)
            .is_err()
    );
    assert!(!dest.exists());
    let saved =
        studio_core::downloads::commit_download(&staging, &dest, Some(&sha), Some(21), true)
            .unwrap();
    assert_eq!(saved["sha256"], sha);
    assert_eq!(std::fs::read(&dest).unwrap(), b"synthetic model bytes");
    assert!(
        studio_core::downloads::commit_download(&staging, &dest, Some(&sha), Some(21), true)
            .is_err()
    );
    std::fs::remove_dir_all(dir).unwrap();
}
