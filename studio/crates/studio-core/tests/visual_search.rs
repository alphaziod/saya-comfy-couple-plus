//! Offline visual-search features: vectors, similarity, bounded backfill and
//! nearest-neighbour ranking. Synthetic images only.
use image::RgbImage;
use studio_core::{
    gallery::Filters,
    visual_search::{extract_file, features_from_rgb, similarity},
    Database,
};

fn red() -> RgbImage {
    RgbImage::from_pixel(64, 64, image::Rgb([230, 20, 20]))
}
fn blue() -> RgbImage {
    RgbImage::from_pixel(64, 64, image::Rgb([20, 20, 230]))
}

#[test]
fn vectors_roundtrip_and_similarity_is_deterministic() {
    let a = features_from_rgb(&red()).unwrap();
    let b = features_from_rgb(&blue()).unwrap();
    assert_eq!(a, a.clone());
    assert!(similarity(&a, &a).overall > 0.99);
    let cross = similarity(&a, &b);
    assert!(cross.overall < similarity(&a, &a).overall);
    assert!(cross.color < 0.2, "{cross:?}");
    let bytes = a.to_bytes();
    assert_eq!(studio_core::visual_search::VECTOR_BYTES, bytes.len());
    assert_eq!(
        studio_core::visual_search::FeatureVector::from_bytes(&bytes),
        Some(a)
    );
    let mut bad = bytes.clone();
    bad[0] = 99;
    assert_eq!(
        studio_core::visual_search::FeatureVector::from_bytes(&bad),
        None
    );
    assert_eq!(
        studio_core::visual_search::FeatureVector::from_bytes(&bytes[..10]),
        None
    );
    assert!(features_from_rgb(&RgbImage::new(32, 32)).is_err());
}

#[test]
fn backfill_ranks_neighbours_and_invalidation_reopens_work() {
    let dir = std::env::temp_dir().join(format!("saya-visual-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    red().save(dir.join("red.png")).unwrap();
    blue().save(dir.join("blue.png")).unwrap();
    let mut db = Database::in_memory().unwrap();
    db.index_library(&dir).unwrap();
    let page = db.gallery(&Filters::default()).unwrap();
    assert_eq!(page.total, 2);
    let red_id = page
        .items
        .iter()
        .find(|a| a.path.ends_with("red.png"))
        .unwrap()
        .id
        .clone();
    let blue_id = page
        .items
        .iter()
        .find(|a| a.path.ends_with("blue.png"))
        .unwrap()
        .id
        .clone();
    assert_eq!(
        extract_file(&dir.join("red.png")).unwrap().to_bytes().len(),
        studio_core::visual_search::VECTOR_BYTES
    );
    // Indexing extracts features at import time; backfill is for pending images.
    assert_eq!(db.visual_pending(0).unwrap()["count"], 0);
    let result = db.visual_search(&red_id, 10).unwrap();
    assert_eq!(result["query"]["vectorized"], true);
    assert_eq!(result["compared"], 1);
    assert_eq!(result["matches"][0]["id"], blue_id);
    assert!(result["matches"][0]["score"].as_f64().unwrap() < 0.99);
    assert_eq!(db.visual_invalidate().unwrap(), 2);
    assert_eq!(db.visual_pending(0).unwrap()["count"], 2);
    let report = db.visual_backfill(Some(10), |_, _| {}, || false).unwrap();
    assert_eq!(report["vectorized"], 2);
    assert_eq!(report["remaining"], 0);
    assert_eq!(db.visual_pending(0).unwrap()["count"], 0);
    db.visual_invalidate().unwrap();
    let unvectorized = db.visual_search(&red_id, 5).unwrap();
    assert_eq!(unvectorized["query"]["vectorized"], false);
    assert_eq!(unvectorized["compared"], 0);
    std::fs::remove_dir_all(dir).unwrap();
}
