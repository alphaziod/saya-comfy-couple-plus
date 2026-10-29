#[test]
fn generation_destination_is_character_then_timestamp_without_creation() {
    let root = std::path::Path::new("/image");
    let path = studio_core::generation_storage::generation_destination(
        root,
        "Saya",
        "2026-10-19_0253",
        "image_0001.png",
    )
    .unwrap();
    assert_eq!(
        path,
        std::path::PathBuf::from("/image/Saya/2026-10-19_0253/image_0001.png")
    );
    assert!(!path.exists());
    assert!(studio_core::generation_storage::generation_destination(
        root,
        "../evil",
        "2026-10-19_0253",
        "x.png"
    )
    .is_err());
}
