use serde_json::json;
use studio_core::{gallery::Filters, Database};
#[test]
fn missing_primary_stays_identified_until_explicit_valid_replacement() {
    let dir = std::env::temp_dir().join(format!("saya-portrait-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    for name in ["a.png", "b.png"] {
        image::RgbImage::new(2, 2).save(dir.join(name)).unwrap();
    }
    let mut db = Database::in_memory().unwrap();
    db.index_library(&dir).unwrap();
    let character = db.save_character(None, "Test".into(), json!({})).unwrap();
    let images = db.gallery(&Filters::default()).unwrap().items;
    for a in &images {
        db.add_reference(&character, None, &a.id).unwrap();
    }
    db.primary_reference(&character, None, &images[0].id)
        .unwrap();
    std::fs::rename(dir.join("a.png"), dir.join("a.moved")).unwrap();
    let sheet = db.character_sheet(&character).unwrap();
    assert_eq!(sheet["portrait"], images[0].id);
    assert_eq!(sheet["portraitStatus"]["state"], "missing");
    assert!(db.choose_portrait(&character, &images[0].id).is_err());
    db.index_library(&dir).unwrap();
    assert_eq!(
        db.character_sheet(&character).unwrap()["portrait"],
        images[0].id,
        "Scan must not silently substitute another portrait"
    );
    let options = db.portrait_candidates(&character, 0).unwrap();
    assert_eq!(options["items"].as_array().unwrap().len(), 2);
    db.choose_portrait(&character, &images[1].id).unwrap();
    let sheet = db.character_sheet(&character).unwrap();
    assert_eq!(sheet["portrait"], images[1].id);
    assert_eq!(sheet["portraitStatus"]["state"], "present");
    assert_eq!(
        sheet["references"].as_array().unwrap().len(),
        2,
        "Old reference preserved"
    );
    assert!(!db.reference_history(&character, None).unwrap().is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}
