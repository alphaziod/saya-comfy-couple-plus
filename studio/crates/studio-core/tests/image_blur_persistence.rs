//! F01 non-destructive blur document persistence; source bytes stay intact.
use serde_json::{json, Value};

fn doc() -> Value {
    json!({"version":1,"strokes":[
        {"id":"s1","size":64.0,"intensity":0.7,"hardness":0.5,"points":[{"x":2.0,"y":3.0},{"x":8.0,"y":9.0}]}
    ]})
}

#[test]
fn blur_document_roundtrips_and_never_touches_source() {
    let dir = std::env::temp_dir().join(format!("saya-blur-db-{}", uuid::Uuid::new_v4()));
    let images = dir.join("images");
    std::fs::create_dir_all(&images).unwrap();
    image::RgbImage::from_pixel(12, 10, image::Rgb([20, 40, 60]))
        .save(images.join("source.png"))
        .unwrap();
    let before = std::fs::read(images.join("source.png")).unwrap();
    let mut db = studio_core::Database::open(&dir.join("db.sqlite3")).unwrap();
    db.index_library(&images).unwrap();
    let asset = db.gallery(&Default::default()).unwrap().items[0].id.clone();
    assert!(db.blur_document(&asset).unwrap()["document"].is_null());
    assert!(db
        .save_blur_document(&asset, &json!({"version":1,"strokes":[{"id":"bad","size":0.0,"intensity":2.0,"hardness":0.5,"points":[]}]}))
        .is_err());
    let saved = db.save_blur_document(&asset, &doc()).unwrap();
    assert_eq!(saved["strokes"], 1);
    let stored = db.blur_document(&asset).unwrap();
    assert_eq!(stored["document"], doc());
    assert_eq!(stored["sha256"], saved["sha256"]);
    assert_eq!(std::fs::read(images.join("source.png")).unwrap(), before);
    assert_eq!(
        db.blur_document("inconnu").unwrap_err().to_string(),
        "Asset inconnu"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
