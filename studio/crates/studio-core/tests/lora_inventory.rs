#[test]
fn lora_inventory_lists_read_only_files_and_never_writes() {
    let dir = std::env::temp_dir().join(format!("saya-lora-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(dir.join("style")).unwrap();
    std::fs::write(dir.join("style/alpha.safetensors"), b"not a real tensor").unwrap();
    std::fs::write(dir.join("style/alpha.png"), b"preview").unwrap();
    std::fs::write(dir.join("style/alpha.json"), br#"{"name":"Alpha"}"#).unwrap();
    std::fs::write(dir.join("beta.pt"), b"x").unwrap();
    let before = std::fs::read(dir.join("style/alpha.safetensors")).unwrap();
    let report = studio_core::lora_inventory::inventory(dir.to_str().unwrap()).unwrap();
    assert_eq!(report["count"], 2);
    let alpha = report["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"] == "alpha")
        .unwrap();
    assert_eq!(alpha["metadata"]["name"], "Alpha");
    assert!(alpha["preview"].as_str().unwrap().ends_with("alpha.png"));
    assert_eq!(
        std::fs::read(dir.join("style/alpha.safetensors")).unwrap(),
        before
    );
    assert!(studio_core::lora_inventory::inventory("/definitely/absent").is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn owned_metadata_sidecar_does_not_touch_model_or_existing_json() {
    let dir = std::env::temp_dir().join(format!("saya-lora-meta-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let model = dir.join("test.safetensors");
    std::fs::write(&model, b"model").unwrap();
    let existing = dir.join("test.json");
    std::fs::write(&existing, b"{\"old\":true}").unwrap();
    let saved = studio_core::lora_inventory::save_metadata(
        model.to_str().unwrap(),
        &serde_json::json!({"tags":["a"]}),
    )
    .unwrap();
    assert!(saved["path"]
        .as_str()
        .unwrap()
        .ends_with("test.metadata.json"));
    assert_eq!(std::fs::read(&model).unwrap(), b"model");
    assert_eq!(std::fs::read(&existing).unwrap(), b"{\"old\":true}");
    std::fs::remove_dir_all(dir).unwrap();
}
