use studio_core::{gallery::Filters, Database};

fn fixture() -> (std::path::PathBuf, Database, String) {
    let root = std::env::temp_dir().join(format!("saya-v15-assets-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(root.join("images")).unwrap();
    image::RgbImage::new(2, 2)
        .save(root.join("images/a.png"))
        .unwrap();
    let mut db = Database::open(&root.join("db.sqlite3")).unwrap();
    db.index_library(&root.join("images")).unwrap();
    let id = db.gallery(&Filters::default()).unwrap().items[0].id.clone();
    (root, db, id)
}

#[test]
fn catalog_removal_survives_index_and_restore_preserves_notes() {
    let (root, mut db, id) = fixture();
    db.annotate_asset(&id, true, "Note à garder", "accepted")
        .unwrap();
    db.set_catalog_removed(std::slice::from_ref(&id), true)
        .unwrap();
    db.index_library(&root.join("images")).unwrap();
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, 0);
    let removed = db
        .gallery(&Filters {
            availability: "removed".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(removed.items[0].id, id);
    assert_eq!(removed.items[0].note, "Note à garder");
    assert!(root.join("images/a.png").is_file());
    assert!(db
        .set_catalog_removed(&[id.clone(), "unknown".into()], false)
        .is_err());
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, 0);
    db.set_catalog_removed(std::slice::from_ref(&id), false)
        .unwrap();
    assert!(db.gallery(&Filters::default()).unwrap().items[0].favorite);
    std::fs::remove_file(root.join("images/a.png")).unwrap();
    db.refresh_asset_presence(&id).unwrap();
    assert_eq!(
        db.gallery(&Filters {
            availability: "missing".into(),
            ..Default::default()
        })
        .unwrap()
        .items[0]
            .id,
        id
    );
    assert_eq!(db.asset_detail(&id).unwrap()["availability"], "missing");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn trash_requires_confirmation_and_unchanged_regular_target() {
    let (root, db, id) = fixture();
    let plan = db.prepare_asset_trash(std::slice::from_ref(&id)).unwrap();
    assert!(db.trash_assets(&plan, false).is_err());
    let mut forged = plan.clone();
    forged[0].path = root.join("other.png").to_string_lossy().into();
    assert!(db.trash_assets(&forged, true).is_err());
    image::RgbImage::new(4, 4)
        .save(root.join("images/a.png"))
        .unwrap();
    assert!(db.trash_assets(&plan, true).is_err());
    assert!(root.join("images/a.png").is_file());
    #[cfg(unix)]
    {
        std::fs::rename(root.join("images/a.png"), root.join("outside.png")).unwrap();
        std::os::unix::fs::symlink(root.join("outside.png"), root.join("images/a.png")).unwrap();
        assert!(db.prepare_asset_trash(std::slice::from_ref(&id)).is_err());
        assert!(root.join("outside.png").is_file());
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn references_pass_states_and_active_index_lock_protect_files() {
    let (root, mut db, id) = fixture();
    let sql = rusqlite::Connection::open(root.join("db.sqlite3")).unwrap();
    sql.execute(
        "INSERT INTO characters(id,name,identity_json) VALUES('c','Test','{}')",
        [],
    )
    .unwrap();
    sql.execute(
        "INSERT INTO character_references(character_id,asset_id,position) VALUES('c',?1,0)",
        [&id],
    )
    .unwrap();
    assert!(db
        .prepare_asset_trash(std::slice::from_ref(&id))
        .unwrap_err()
        .to_string()
        .contains("référence"));
    sql.execute("DELETE FROM character_references", []).unwrap();
    let plan = db.prepare_asset_trash(std::slice::from_ref(&id)).unwrap();
    let second = Database::open(&root.join("db.sqlite3")).unwrap();
    let mut refused = false;
    db.index_library_progress(&root.join("images"), |_, _| {
        assert!(second.trash_assets(&plan, true).is_err());
        refused = true;
    })
    .unwrap();
    assert!(refused);
    assert!(root.join("images/a.png").is_file());
    std::fs::create_dir(root.join("pass-states")).unwrap();
    image::RgbImage::new(2, 2)
        .save(root.join("pass-states/protected.png"))
        .unwrap();
    db.index_library(&root.join("pass-states")).unwrap();
    let protected = db
        .gallery(&Filters {
            search: "protected".into(),
            ..Default::default()
        })
        .unwrap()
        .items
        .remove(0)
        .id;
    assert!(db
        .prepare_asset_trash(&[protected])
        .unwrap_err()
        .to_string()
        .contains("protégé"));
    std::fs::remove_dir_all(root).unwrap();
}
