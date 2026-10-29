use serde_json::json;
use std::fs;
use studio_core::{backup, Database};
#[test]
fn verified_backup_captures_wal_and_restores_without_overwrite() {
    let root = std::env::temp_dir().join(format!("saya-backup-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let source = root.join("active.sqlite3");
    let mut db = Database::open(&source).unwrap();
    let id = db
        .save_character(
            None,
            "Conserver".into(),
            json!({"identity_prompt":"(exact:1.2),  tags\nline"}),
        )
        .unwrap();
    db.set_preference(
        "galleryFilters",
        json!({"loraExact":true,"loras":["A","B"]}),
    )
    .unwrap();
    let workflow = db
        .import_workflow(
            "snapshot",
            &json!({"1":{"class_type":"Text","inputs":{"text":"exact prompt"}}}).to_string(),
        )
        .unwrap();
    let edit = studio_core::workflow::Edit {
        node_id: "1".into(),
        input_name: "text".into(),
        value: "exact prompt".into(),
        section: Some("P1".into()),
    };
    db.save_bindings(&workflow, std::slice::from_ref(&edit))
        .unwrap();
    let job = db
        .prepare_job(
            &workflow,
            &[edit],
            &json!({"Text":{"input":{"required":{"text":["STRING",{}]}},"output":[]}}),
        )
        .unwrap();
    let job_before = db.job_details(&job.job_id).unwrap();
    let copy = root.join("backup");
    let manifest = backup::create(&source, &copy).unwrap();
    // Parallel migrations may exceed 16: never freeze the number.
    assert!(manifest.schema_version >= 16, "{}", manifest.schema_version);
    assert_eq!(manifest.database_file, "studio.sqlite3");
    assert!(source.with_extension("sqlite3-wal").exists());
    let restore = root.join("restored");
    backup::restore(&copy, &restore).unwrap();
    let restored = Database::open(&restore.join("studio.sqlite3")).unwrap();
    assert_eq!(restored.characters().unwrap()[0].id, id);
    assert_eq!(restored.job_details(&job.job_id).unwrap(), job_before);
    assert_eq!(
        restored.characters().unwrap()[0].identity["identity_prompt"],
        "(exact:1.2),  tags\nline"
    );
    assert_eq!(
        restored.preferences().unwrap()["galleryFilters"]["loraExact"],
        true
    );
    // Legacy v1 remains readable without managed files; no migration of its bytes.
    let legacy = root.join("legacy");
    backup::create(&source, &legacy).unwrap();
    let legacy_path = legacy.join("manifest.json");
    let mut legacy_manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&legacy_path).unwrap()).unwrap();
    legacy_manifest["version"] = json!(1);
    legacy_manifest
        .as_object_mut()
        .unwrap()
        .remove("checkpointFiles");
    fs::write(&legacy_path, legacy_manifest.to_string()).unwrap();
    assert_eq!(backup::verify(&legacy).unwrap().version, 1);
    backup::restore(&legacy, &root.join("legacy-restored")).unwrap();
    assert!(backup::create(&source, &copy).is_err());
    assert!(backup::restore(&copy, &restore).is_err());
    assert!(backup::create(&root.join("missing.sqlite3"), &root.join("missing-copy")).is_err());
    assert!(!root.join("missing.sqlite3").exists());
    db.save_character(None, "After snapshot".into(), json!({}))
        .unwrap();
    assert_eq!(restored.characters().unwrap().len(), 1);
    assert_eq!(db.characters().unwrap().len(), 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&copy).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(copy.join("studio.sqlite3"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    drop(restored);
    drop(db);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn corrupt_manifest_or_database_never_becomes_a_restoration() {
    let root = std::env::temp_dir().join(format!("saya-backup-reject-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let source = root.join("source.sqlite3");
    let db = Database::open(&source).unwrap();
    let copy = root.join("backup");
    backup::create(&source, &copy).unwrap();
    let manifest_path = copy.join("manifest.json");
    let original = fs::read(&manifest_path).unwrap();
    let mut manifest: serde_json::Value = serde_json::from_slice(&original).unwrap();
    manifest["databaseFile"] = json!("../source.sqlite3");
    fs::write(&manifest_path, manifest.to_string()).unwrap();
    assert!(backup::restore(&copy, &root.join("bad-path")).is_err());
    assert!(!root.join("bad-path").exists());
    manifest = serde_json::from_slice(&original).unwrap();
    manifest["version"] = json!(99);
    fs::write(&manifest_path, manifest.to_string()).unwrap();
    assert!(backup::restore(&copy, &root.join("future-format")).is_err());
    fs::write(&manifest_path, &original).unwrap();
    fs::write(copy.join("studio.sqlite3-wal"), b"unexpected journal").unwrap();
    assert!(backup::verify(&copy).is_err());
    fs::remove_file(copy.join("studio.sqlite3-wal")).unwrap();
    fs::write(&manifest_path, &original).unwrap();
    fs::write(copy.join("studio.sqlite3"), b"corrupt").unwrap();
    assert!(backup::restore(&copy, &root.join("corrupt")).is_err());
    assert!(!root.join("corrupt").exists());
    let unrelated = root.join("other.sqlite3");
    rusqlite::Connection::open(&unrelated)
        .unwrap()
        .execute_batch("CREATE TABLE other(value TEXT);")
        .unwrap();
    assert!(backup::create(&unrelated, &root.join("other-copy")).is_err());
    assert!(!root.join("other-copy").exists());
    let future = rusqlite::Connection::open(&source).unwrap();
    future.pragma_update(None, "user_version", 99).unwrap();
    assert!(backup::create(&source, &root.join("future-schema")).is_err());
    assert!(!root.join("future-schema").exists());
    drop(future);
    drop(db);
    fs::remove_dir_all(root).unwrap();
}

fn table_contents(path: &std::path::Path) -> std::collections::BTreeMap<String, Vec<String>> {
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let names:Vec<String>=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<std::result::Result<_,_>>().unwrap();
    let mut result = std::collections::BTreeMap::new();
    for name in names {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = statement.column_count();
        let mut rows: Vec<String> = statement
            .query_map([], |row| {
                Ok((0..columns)
                    .map(|c| format!("{:?}", row.get_ref(c).unwrap()))
                    .collect::<Vec<_>>()
                    .join("|"))
            })
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        rows.sort();
        result.insert(name, rows);
    }
    result
}
#[test]
fn stopped_application_restore_preserves_every_table_and_missing_image_references() {
    let root = std::env::temp_dir().join(format!("saya-backup-tables-{}", uuid::Uuid::new_v4()));
    let images = root.join("images");
    fs::create_dir_all(&images).unwrap();
    let image = images.join("fixture.png");
    image::RgbImage::new(2, 2).save(&image).unwrap();
    let source = root.join("active.sqlite3");
    let mut db = Database::open(&source).unwrap();
    db.index_library(&images).unwrap();
    let asset = db.gallery(&Default::default()).unwrap().items[0].id.clone();
    let character = db
        .save_character(
            None,
            "Missing reference".into(),
            json!({"identity_prompt":"exact"}),
        )
        .unwrap();
    let variant = db
        .save_variant(
            &character,
            None,
            "variant",
            json!({"P1":"exact"}),
            json!({}),
        )
        .unwrap();
    db.add_reference(&character, Some(&variant), &asset)
        .unwrap();
    db.annotate_asset(&asset, true, "kept", "accepted").unwrap();
    fs::rename(&image, root.join("fixture-away.png")).unwrap();
    db.index_library(&images).unwrap();
    let references = db.references(&character).unwrap();
    assert_eq!(references.len(), 1);
    assert_eq!(references[0]["availability"], "missing");
    drop(db); // Closed application, checkpointed WAL; previous test covers a live WAL.
    let before = table_contents(&source);
    // Historical floor only: only migration 0010 remains reserved
    // to a parallel worktree; the tested invariant is the exact preservation
    // of content by backup/restore, never a frozen number.
    assert!(
        before["schema_migrations"].len() >= 9,
        "{}",
        before["schema_migrations"].len()
    );
    assert!(before.len() > 20);
    let copy = root.join("backup");
    backup::create(&source, &copy).unwrap();
    assert_eq!(table_contents(&copy.join("studio.sqlite3")), before);
    let restored = root.join("restored");
    backup::restore(&copy, &restored).unwrap();
    assert_eq!(table_contents(&restored.join("studio.sqlite3")), before);
    let db = Database::open(&restored.join("studio.sqlite3")).unwrap();
    assert_eq!(db.references(&character).unwrap(), references);
    assert_eq!(table_contents(&restored.join("studio.sqlite3")), before);
    assert!(root.join("fixture-away.png").is_file());
    drop(db);
    fs::remove_dir_all(root).unwrap();
}
