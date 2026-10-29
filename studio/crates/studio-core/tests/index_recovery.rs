use serde_json::json;
use studio_core::{gallery::Filters, Database};

#[test]
fn interrupted_index_resumes_idempotently_and_keeps_errors_and_identity() {
    let dir = std::env::temp_dir().join(format!("saya-index-recovery-{}", uuid::Uuid::new_v4()));
    let images = dir.join("images");
    std::fs::create_dir_all(&images).unwrap();
    for i in 0..4 {
        image::RgbImage::from_pixel(2, 2, image::Rgb([i, 30, 40]))
            .save(images.join(format!("{i}.png")))
            .unwrap();
    }
    std::fs::write(images.join("bad.png"), b"invalid image").unwrap();
    std::fs::write(images.join("journal.jsonl"), b"\xff").unwrap();
    let path = dir.join("db.sqlite3");
    let mut db = Database::open(&path).unwrap();
    let character = db
        .save_character(
            None,
            "Keep".into(),
            json!({"identity_prompt":"exact,  tags\n"}),
        )
        .unwrap();
    let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        db.index_library_progress(&images, |done, _| {
            if done == 2 {
                panic!("synthetic interruption")
            }
        })
        .unwrap();
    }));
    assert!(stopped.is_err());
    let unfinished = db.latest_index_run().unwrap();
    assert_eq!(unfinished["completed"], 2);
    assert_eq!(unfinished["state"], "indexing");
    drop(db);
    let mut db = Database::open(&path).unwrap();
    assert_eq!(db.latest_index_run().unwrap(), unfinished);
    let report = db.index_library(&images).unwrap();
    assert_eq!(
        (report.indexed, report.unchanged, report.error_count),
        (2, 2, 2)
    );
    let last = db.latest_index_run().unwrap();
    assert_eq!(last["state"], "completed");
    assert_eq!(last["completed"], 5);
    assert_eq!(
        db.index_errors(last["id"].as_i64().unwrap(), 0)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, 4);
    assert_eq!(db.characters().unwrap()[0].id, character);
    assert_eq!(
        db.characters().unwrap()[0].identity["identity_prompt"],
        "exact,  tags\n"
    );
    let report = db.index_library(&images).unwrap();
    assert_eq!(
        (report.indexed, report.unchanged, report.error_count),
        (0, 4, 2)
    );
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT state FROM index_runs WHERE id=?",
            [unfinished["id"].as_i64().unwrap()],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "interrupted"
    );
    assert_eq!(
        sql.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(sql);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn simultaneous_indexers_are_refused_without_relabeling_the_active_run() {
    let dir = std::env::temp_dir().join(format!("saya-index-lock-{}", uuid::Uuid::new_v4()));
    let images = dir.join("images");
    std::fs::create_dir_all(&images).unwrap();
    image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3]))
        .save(images.join("one.png"))
        .unwrap();
    let path = dir.join("db.sqlite3");
    let mut first = Database::open(&path).unwrap();
    let mut second = Database::open(&path).unwrap();
    let mut refused = false;
    first
        .index_library_progress(&images, |done, _| {
            if done == 0 {
                let before = second.latest_index_run().unwrap();
                assert!(second
                    .index_library(&images)
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("déjà en cours"));
                assert_eq!(second.latest_index_run().unwrap(), before);
                refused = true;
            }
        })
        .unwrap();
    assert!(refused);
    assert_eq!(second.index_library(&images).unwrap().unchanged, 1);
    drop(first);
    drop(second);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cancellation_keeps_committed_images_and_never_marks_unvisited_files_missing() {
    use std::cell::Cell;
    let dir = std::env::temp_dir().join(format!("saya-index-cancel-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    for i in 0..3 {
        image::RgbImage::from_pixel(2, 2, image::Rgb([i, 2, 3]))
            .save(dir.join(format!("{i}.png")))
            .unwrap();
    }
    let mut db = Database::in_memory().unwrap();
    db.index_library(&dir).unwrap();
    let before = db.gallery(&Filters::default()).unwrap().total;
    std::fs::rename(dir.join("2.png"), dir.join("renamed.png")).unwrap();
    let done = Cell::new(0);
    let report = db
        .index_library_controlled(&dir, |n, _| done.set(n), || done.get() == 1)
        .unwrap();
    assert!(report.cancelled);
    assert_eq!(report.unchanged, 1);
    assert_eq!(db.latest_index_run().unwrap()["state"], "interrupted");
    assert_eq!(
        db.gallery(&Filters {
            availability: "missing".into(),
            ..Default::default()
        })
        .unwrap()
        .total,
        0
    );
    // The cancelled scan inferred nothing; displaying the old name now confirms its absence.
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, before - 1);
    assert!(!db.index_library(&dir).unwrap().cancelled);
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, 3);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn formats_corrupt_metadata_duplicates_and_unreadable_files_are_accounted_for() {
    let dir = std::env::temp_dir().join(format!("saya-index-formats-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let image = image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]));
    for name in ["plain.png", "photo.jpg", "image.webp"] {
        image.save(dir.join(name)).unwrap();
    }
    std::fs::copy(dir.join("plain.png"), dir.join("copy.png")).unwrap();
    let mut encoder = png::Encoder::new(
        std::fs::File::create(dir.join("invalid-metadata.png")).unwrap(),
        2,
        2,
    );
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .add_text_chunk("prompt".into(), "{broken".into())
        .unwrap();
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&[10; 12])
        .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(dir.join("plain.png"), dir.join("alias.png")).unwrap();
    let mut db = Database::in_memory().unwrap();
    let report = db.index_library(&dir).unwrap();
    assert_eq!(report.indexed, 5);
    assert_eq!(report.error_count, 0);
    assert_eq!(
        db.gallery(&Filters {
            metadata_state: "invalid".into(),
            ..Default::default()
        })
        .unwrap()
        .total,
        1
    );
    assert_eq!(
        db.gallery(&Filters {
            metadata_state: "absent".into(),
            ..Default::default()
        })
        .unwrap()
        .total,
        4
    );
    assert_eq!(
        db.gallery(&Filters {
            duplicates: true,
            ..Default::default()
        })
        .unwrap()
        .total,
        2
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let denied = dir.join("denied.png");
        std::fs::copy(dir.join("plain.png"), &denied).unwrap();
        std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o000)).unwrap();
        let permissions_enforced = std::fs::read(&denied).is_err();
        let report = db.index_library(&dir).unwrap();
        std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o600)).unwrap();
        if permissions_enforced {
            assert_eq!(report.error_count, 1);
            assert_eq!(db.gallery(&Filters::default()).unwrap().total, 5);
        } else {
            eprintln!("Permission refusal not qualified: test process can read mode 000");
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
