#[test]
fn two_illustrious_models_fit_but_third_or_large_unloads() {
    let two = serde_json::json!({"1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"Illustrious-A.safetensors"}},"2":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"Illustrious-B.safetensors"}}});
    assert_eq!(
        studio_core::vram_policy::plan(&two, Some(16000)).unwrap()["shouldUnloadBeforePass"],
        false
    );
    let three = serde_json::json!({"1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"Illustrious-A.safetensors"}},"2":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"Illustrious-B.safetensors"}},"3":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"Illustrious-C.safetensors"}}});
    assert_eq!(
        studio_core::vram_policy::plan(&three, Some(16000)).unwrap()["shouldUnloadBeforePass"],
        true
    );
    let large = serde_json::json!({"1":{"class_type":"UNETLoader","inputs":{"unet_name":"hidream.safetensors"}}});
    assert_eq!(
        studio_core::vram_policy::plan(&large, Some(8000)).unwrap()["shouldUnloadBeforePass"],
        true
    );
}
