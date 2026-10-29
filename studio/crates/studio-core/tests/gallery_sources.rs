use studio_core::{gallery::Filters, Database};

#[test]
fn nested_roots_keep_one_identity_and_annotations_after_restart() {
    let dir = std::env::temp_dir().join(format!("saya-v15-sources-{}", uuid::Uuid::new_v4()));
    let outer = dir.join("Images été");
    let inner = outer.join("Sous dossier");
    std::fs::create_dir_all(&inner).unwrap();
    image::RgbImage::new(2, 2)
        .save(inner.join("portrait.png"))
        .unwrap();
    let db_path = dir.join("studio.sqlite3");
    let mut db = Database::open(&db_path).unwrap();
    db.index_library(&inner).unwrap();
    let id = db.gallery(&Filters::default()).unwrap().items[0].id.clone();
    db.annotate_asset(&id, true, "Conserver exactement", "accepted")
        .unwrap();
    db.index_library(&outer).unwrap();
    let gallery = db.gallery(&Filters::default()).unwrap();
    assert_eq!(
        gallery.total, 1,
        "Nested sources must not duplicate the same physical path"
    );
    assert_eq!(gallery.items[0].id, id);
    assert_eq!(gallery.items[0].note, "Conserver exactement");
    drop(db);
    let mut db = Database::open(&db_path).unwrap();
    assert_eq!(db.index_library(&outer).unwrap().unchanged, 1);
    assert_eq!(db.gallery(&Filters::default()).unwrap().items[0].id, id);
    std::fs::remove_file(inner.join("portrait.png")).unwrap();
    db.index_library(&outer).unwrap();
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, 0);
    image::RgbImage::new(2, 2)
        .save(inner.join("portrait.png"))
        .unwrap();
    db.index_library(&outer).unwrap();
    let gallery = db.gallery(&Filters::default()).unwrap();
    assert_eq!(gallery.items[0].id, id);
    assert!(gallery.items[0].favorite);
    assert_eq!(gallery.items[0].note, "Conserver exactement");
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn removing_sources_preserves_history_and_overlapping_coverage() {
    let dir = std::env::temp_dir().join(format!("saya-v15-remove-{}", uuid::Uuid::new_v4()));
    let child = dir.join("child");
    std::fs::create_dir_all(&child).unwrap();
    let file = child.join("one.png");
    image::RgbImage::new(2, 2).save(&file).unwrap();
    let mut db = Database::in_memory().unwrap();
    let inner = db.add_library_source(&child).unwrap();
    db.index_library(&child).unwrap();
    let id = db.gallery(&Filters::default()).unwrap().items[0].id.clone();
    let outer = db.add_library_source(&dir).unwrap();
    assert_eq!(db.add_library_source(&dir).unwrap(), outer);
    db.set_library_source(&inner, false).unwrap();
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, 1);
    db.set_library_source(&outer, false).unwrap();
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, 0);
    assert_eq!(db.asset_path(&id).unwrap(), file);
    assert_eq!(db.asset_detail(&id).unwrap()["id"], id);
    db.set_library_source(&inner, true).unwrap();
    assert_eq!(db.gallery(&Filters::default()).unwrap().items[0].id, id);
    assert_eq!(db.library_sources().unwrap().len(), 2);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unavailable_source_and_last_callback_cancellation_do_not_mark_missing() {
    use std::cell::Cell;
    let dir = std::env::temp_dir().join(format!("saya-v15-inaccessible-{}", uuid::Uuid::new_v4()));
    let root = dir.join("root");
    std::fs::create_dir_all(&root).unwrap();
    for name in ["a.png", "b.png"] {
        image::RgbImage::new(2, 2).save(root.join(name)).unwrap();
    }
    let mut db = Database::open(&dir.join("db.sqlite3")).unwrap();
    db.index_library(&root).unwrap();
    let before = db.gallery(&Filters::default()).unwrap();
    std::fs::remove_file(root.join("b.png")).unwrap();
    let done = Cell::new(false);
    let r = db
        .index_library_controlled(&root, |n, t| done.set(n == t), || done.get())
        .unwrap();
    assert!(r.cancelled);
    // Cancellation itself does not infer absence. Display checks confirm it directly.
    let sql = rusqlite::Connection::open(dir.join("db.sqlite3")).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT count(*) FROM assets WHERE availability='missing'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, 1);
    std::fs::rename(&root, dir.join("offline")).unwrap();
    assert!(db.index_library(&root).is_err());
    assert_eq!(db.library_sources().unwrap()[0]["state"], "inaccessible");
    let sql = rusqlite::Connection::open(dir.join("db.sqlite3")).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT count(*) FROM assets WHERE availability='missing'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    for a in before.items {
        assert_eq!(db.asset_detail(&a.id).unwrap()["id"], a.id);
    }
    std::fs::rename(dir.join("offline"), &root).unwrap();
    db.index_library(&root).unwrap();
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, 1);
    std::fs::remove_dir_all(dir).unwrap();
}
