use serde_json::json;
use std::{fs, path::Path};
use studio_core::{
    gallery::{digest, Filters},
    Database,
};
fn png(path: &Path, names: &[&str], metadata: bool) {
    let mut encoder = png::Encoder::new(fs::File::create(path).unwrap(), 2, 2);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    if metadata {
        encoder.add_text_chunk("prompt".into(),json!({"model":{"inputs":{"ckpt_name":"model"}},"loras":{"inputs":{"loras":names.iter().map(|name|json!({"name":name,"active":true})).collect::<Vec<_>>()}}}).to_string()).unwrap();
    }
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[100; 12])
        .unwrap();
}
#[test]
fn exact_indexed_lora_sets_combine_and_survive_restart_without_changing_sources() {
    let root = std::env::temp_dir().join(format!("saya-loras-{}", uuid::Uuid::new_v4()));
    let images = root.join("images");
    fs::create_dir_all(&images).unwrap();
    for (name, loras) in [
        ("ab", vec!["A", "B"]),
        ("abc", vec!["A", "B", "C"]),
        ("a", vec!["A"]),
        ("empty", vec![]),
    ] {
        png(&images.join(format!("{name}.png")), &loras, true);
    }
    png(&images.join("missing.png"), &[], false);
    let original = digest(&fs::read(images.join("ab.png")).unwrap());
    let path = root.join("db.sqlite3");
    let mut db = Database::open(&path).unwrap();
    db.index_library(&images).unwrap();
    let mut filters: Filters =
        serde_json::from_value(json!({"loras":["A","B"],"loraExact":true})).unwrap();
    let exact = db.gallery(&filters).unwrap();
    assert_eq!(exact.total, 1, "exact A/B must exclude A/B/C");
    assert!(exact.items[0].path.ends_with("ab.png"));
    filters.lora_exact = false;
    assert_eq!(db.gallery(&filters).unwrap().total, 2);
    filters.lora_exact = true;
    filters.lora = "A".into();
    filters.loras.push("B".into());
    assert_eq!(
        db.gallery(&filters).unwrap().total,
        1,
        "legacy and repeated names are a set"
    );
    filters.favorite = true;
    assert_eq!(db.gallery(&filters).unwrap().total, 0);
    db.annotate_asset(&exact.items[0].id, true, "kept", "accepted")
        .unwrap();
    filters.review = "accepted".into();
    filters.checkpoint = "model".into();
    assert_eq!(db.gallery(&filters).unwrap().total, 1);
    db.set_preference("galleryFilters", serde_json::to_value(&filters).unwrap())
        .unwrap();
    drop(db);
    let db = Database::open(&path).unwrap();
    let restored: Filters =
        serde_json::from_value(db.preferences().unwrap()["galleryFilters"].clone()).unwrap();
    assert!(restored.lora_exact);
    assert_eq!(db.gallery(&restored).unwrap().total, 1);
    let none = Filters {
        lora_exact: true,
        ..Default::default()
    };
    assert_eq!(
        db.gallery(&none).unwrap().total,
        2,
        "empty means no indexed LoRA, including unknown metadata"
    );
    assert_eq!(digest(&fs::read(images.join("ab.png")).unwrap()), original);
    drop(db);
    fs::remove_dir_all(root).unwrap();
}
