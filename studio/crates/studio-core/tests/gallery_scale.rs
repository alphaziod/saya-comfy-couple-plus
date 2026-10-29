//! Ignored scale benchmark: synthetic metadata only, no image files, no real DB.
//! Run: cargo test -p studio-core --release --test gallery_scale -- --ignored --nocapture
use std::time::Instant;
use studio_core::{gallery::Filters, resources::process_rss, Database};

#[test]
#[ignore = "100k synthetic gallery benchmark"]
fn hundred_thousand_metadata_window_and_deep_restore() {
    let dir = std::env::temp_dir().join(format!("saya-scale-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("studio.sqlite3");
    drop(Database::open(&path).unwrap());
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute("INSERT INTO libraries(id,label,root_path,platform) VALUES('lib','Synthetic','/synthetic','linux')", []).unwrap();
    let tx = raw.unchecked_transaction().unwrap();
    for i in 0..100_000u32 {
        tx.execute(
            "INSERT INTO assets(id,library_id,relative_path,absolute_path,availability,metadata_json,metadata_state,width,height,content_sha256,favorite) VALUES(?1,'lib',?2,?3,'present','{}','read',512,512,?4,?5)",
            rusqlite::params![
                format!("a{i:06}"),
                format!("set{:02}/img{i:06}.png", i % 100),
                format!("/synthetic/set{:02}/img{i:06}.png", i % 100),
                format!("{:064x}", i),
                i % 10 == 0,
            ],
        ).unwrap();
    }
    tx.commit().unwrap();
    drop(raw);
    let db = Database::open(&path).unwrap();
    let rss0 = process_rss(std::process::id());
    let mut f = Filters::default();
    let t = Instant::now();
    let first = db.gallery_window(&f, 250).unwrap();
    let first_ms = t.elapsed().as_millis();
    assert_eq!(first.total, 100_000);
    assert_eq!(first.items.len(), 250);
    let t = Instant::now();
    f.offset = 50_000;
    let middle = db.gallery_window(&f, 250).unwrap();
    let middle_ms = t.elapsed().as_millis();
    assert_eq!(middle.items.len(), 250);
    let t = Instant::now();
    f.offset = 99_750;
    let last = db.gallery_window(&f, 250).unwrap();
    let last_ms = t.elapsed().as_millis();
    assert_eq!(last.items.len(), 250);
    let t = Instant::now();
    f.sort = "name_desc".into();
    f.offset = 50_000;
    let sorted = db.gallery_window(&f, 250).unwrap();
    let sorted_ms = t.elapsed().as_millis();
    assert_eq!(sorted.items.len(), 250);
    f.sort = "name_asc".into();
    f.favorite = true;
    f.offset = 0;
    let t = Instant::now();
    let favorites = db.gallery_window(&f, 250).unwrap();
    let filter_ms = t.elapsed().as_millis();
    assert_eq!(favorites.total, 10_000);
    f.favorite = false;
    let anchor = last.items.last().unwrap().id.clone();
    let t = Instant::now();
    let neighbors = db.gallery_neighbors(&f, &anchor, 99_999).unwrap();
    let neighbors_ms = t.elapsed().as_millis();
    assert_eq!(neighbors["total"], 100_000);
    // Windowed deep restore: resolve the anchor, then load one bounded window.
    let t = Instant::now();
    let deep_filter = Filters {
        offset: 99_500,
        ..Default::default()
    };
    let deep_window = db.gallery_window(&deep_filter, 250).unwrap();
    let deep_ms = t.elapsed().as_millis();
    assert_eq!(deep_window.items.len(), 250);
    let rss1 = process_rss(std::process::id());
    println!(
        "SCALE {}",
        serde_json::json!({
            "entries": 100_000, "firstMs": first_ms, "middleMs": middle_ms, "lastMs": last_ms,
            "sortedMs": sorted_ms, "filterMs": filter_ms, "neighborsMs": neighbors_ms,
            "deepWindowMs": deep_ms, "deepWindowSize": deep_window.items.len(),
            "rssBefore": rss0, "rssAfter": rss1,
        })
    );
    std::fs::remove_dir_all(dir).unwrap();
}
