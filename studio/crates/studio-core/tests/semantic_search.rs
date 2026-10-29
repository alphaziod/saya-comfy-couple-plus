//! Optional CPU-only CLIP semantic search. Ignored by default because it loads
//! the local 630M-parameter checkpoint (~a few GiB RAM); no network/GPU.
use image::RgbImage;
use studio_core::{gallery::Filters, semantic_search, Database};

#[test]
#[ignore = "loads the local CLIP checkpoint on CPU"]
fn local_clip_indexes_and_searches_then_invalidates() {
    assert!(
        semantic_search::available(),
        "local CLIP sidecar unavailable"
    );
    std::env::set_var("SAYA_SEMANTIC_IDLE_SECONDS", "2");
    let dir = std::env::temp_dir().join(format!("saya-semantic-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    RgbImage::from_pixel(32, 32, image::Rgb([220, 30, 30]))
        .save(dir.join("red.png"))
        .unwrap();
    RgbImage::from_pixel(32, 32, image::Rgb([30, 30, 220]))
        .save(dir.join("blue.png"))
        .unwrap();
    let mut db = Database::in_memory().unwrap();
    db.index_library(&dir).unwrap();
    let page = db.gallery(&Filters::default()).unwrap();
    assert_eq!(page.total, 2);
    let red = page
        .items
        .iter()
        .find(|a| a.path.ends_with("red.png"))
        .unwrap()
        .id
        .clone();
    let blue = page
        .items
        .iter()
        .find(|a| a.path.ends_with("blue.png"))
        .unwrap()
        .id
        .clone();
    let status = db.semantic_status().unwrap();
    assert_eq!(status["available"], true);
    assert_eq!(status["pending"], 2);
    let report = db.semantic_index(2).unwrap();
    assert_eq!(report["embedded"], 2, "{report}");
    let first_status = db.semantic_status().unwrap();
    assert_eq!(first_status["indexed"], 2);
    assert_eq!(first_status["sidecar"]["resident"], true);
    let launches = first_status["sidecar"]["launches"].as_u64().unwrap();
    // A second lot reuses the loaded model; the DB skips indexed vectors.
    db.semantic_invalidate().unwrap();
    let second = db.semantic_index(2).unwrap();
    assert_eq!(second["embedded"], 2);
    let reused = db.semantic_status().unwrap();
    assert_eq!(reused["sidecar"]["launches"].as_u64().unwrap(), launches);
    // Idle timeout stops the worker (memory released); next lot restarts it.
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert_eq!(db.semantic_status().unwrap()["sidecar"]["resident"], false);
    std::thread::sleep(std::time::Duration::from_millis(50));
    db.semantic_invalidate().unwrap();
    let third = db.semantic_index(2).unwrap();
    assert_eq!(third["embedded"], 2);
    let restarted = db.semantic_status().unwrap();
    assert!(restarted["sidecar"]["launches"].as_u64().unwrap() > launches);
    let result = db.semantic_search(&red, 5).unwrap();
    assert_eq!(result["matches"][0]["id"], blue);
    assert!(result["matches"][0]["score"].as_f64().unwrap().is_finite());
    assert_eq!(db.semantic_invalidate().unwrap(), 2);
    assert_eq!(db.semantic_status().unwrap()["pending"], 2);
    std::fs::remove_dir_all(dir).unwrap();
}
