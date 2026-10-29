#[test]
fn generation_store_is_character_timestamp_and_refuses_overwrite() {
    let root = std::env::temp_dir().join(format!("saya-gen-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let (run, manifest) =
        studio_core::generation_store::create(&root, "Saya", "2026-10-19_0300").unwrap();
    assert_eq!(manifest["format"], "saya-generation");
    let entry = studio_core::generation_store::record_pass(
        &run,
        "1",
        "image",
        b"png bytes",
        serde_json::json!({"seed":"42"}),
    )
    .unwrap();
    assert_eq!(
        studio_core::generation_store::load_pass(&run, "1", entry["sha256"].as_str().unwrap())
            .unwrap(),
        b"png bytes"
    );
    assert!(studio_core::generation_store::record_pass(
        &run,
        "1",
        "image",
        b"other",
        serde_json::json!({})
    )
    .is_err());
    assert_eq!(
        studio_core::generation_store::status(&run).unwrap()["passes"],
        1
    );
    assert!(studio_core::generation_store::create(&root, "Saya", "2026-10-19_0300").is_err());
    assert!(studio_core::generation_store::create(&root, "../evil", "x").is_err());
    std::fs::remove_dir_all(root).unwrap();
}
