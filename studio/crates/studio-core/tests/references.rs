use serde_json::json;
use studio_core::Database;
#[test]
fn reference_scopes_primary_order_and_links_survive_restart_without_touching_images() {
    let dir = std::env::temp_dir().join(format!("studio-references-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let images = dir.join("images");
    std::fs::create_dir(&images).unwrap();
    for i in 0..3 {
        image::RgbImage::from_pixel(2, 2, image::Rgb([i, 20, 30]))
            .save(images.join(format!("{i}.png")))
            .unwrap();
    }
    let before = (0..3)
        .map(|i| std::fs::read(images.join(format!("{i}.png"))).unwrap())
        .collect::<Vec<_>>();
    let path = dir.join("studio.sqlite3");
    let mut db = Database::open(&path).unwrap();
    db.index_library(&images).unwrap();
    let assets = db.gallery(&Default::default()).unwrap().items;
    let a = db.save_character(None, "A".into(), json!({})).unwrap();
    let b = db.save_character(None, "B".into(), json!({})).unwrap();
    let variant = db
        .save_variant(&a, None, "Variant A", json!({"P1":"exact"}), json!({}))
        .unwrap();
    db.add_reference(&a, None, &assets[0].id).unwrap();
    db.add_reference(&a, None, &assets[1].id).unwrap();
    db.add_reference(&a, Some(&variant), &assets[2].id).unwrap();
    assert!(db.add_reference(&b, Some(&variant), &assets[0].id).is_err());
    db.primary_reference(&a, None, &assets[1].id).unwrap();
    db.reorder_references(&a, None, &[assets[1].id.clone(), assets[0].id.clone()])
        .unwrap();
    let stable = db.references(&a).unwrap();
    assert!(db
        .reorder_references(&a, None, &[assets[0].id.clone(), assets[0].id.clone()])
        .is_err());
    assert_eq!(db.references(&a).unwrap(), stable);
    db.remove_reference(&a, None, &assets[0].id).unwrap();
    assert_eq!(
        db.all_characters(false)
            .unwrap()
            .iter()
            .find(|c| c.id == a)
            .unwrap()
            .image_ids
            .len(),
        2
    );
    drop(db);
    let db = Database::open(&path).unwrap();
    let refs = db.references(&a).unwrap();
    let main = refs.iter().find(|r| r["variantId"].is_null()).unwrap();
    assert_eq!(main["assetId"], assets[1].id);
    assert_eq!(main["primary"], true);
    assert_eq!(refs.iter().filter(|r| r["variantId"] == variant).count(), 1);
    db.set_character_archived(&a, true).unwrap();
    assert!(db.primary_reference(&a, None, &assets[1].id).is_err());
    assert_eq!(db.references(&a).unwrap(), refs);
    drop(db);
    for (i, bytes) in before.into_iter().enumerate() {
        assert_eq!(
            std::fs::read(images.join(format!("{i}.png"))).unwrap(),
            bytes
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v6_legacy_links_migrate_without_guessing_primary_and_variant_refs_survive_identity_restore() {
    let dir = std::env::temp_dir().join(format!("studio-ref-migration-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("studio.sqlite3");
    let old = rusqlite::Connection::open(&path).unwrap();
    for sql in [
        include_str!("../../../migrations/0001_initial.sql"),
        include_str!("../../../migrations/0002_functional.sql"),
        include_str!("../../../migrations/0003_gallery.sql"),
        include_str!("../../../migrations/0004_character_history.sql"),
        include_str!("../../../migrations/0005_prompt_proposals.sql"),
        include_str!("../../../migrations/0006_generation_provenance.sql"),
    ] {
        old.execute_batch(sql).unwrap();
    }
    old.execute_batch("PRAGMA user_version=6; INSERT INTO characters(id,name,identity_json) VALUES('a','A','{}');INSERT INTO libraries(id,label,root_path,platform) VALUES('l','Synthetic','/synthetic','linux');INSERT INTO assets(id,library_id,relative_path) VALUES('one','l','one.png'),('two','l','two.png'),('three','l','three.png'); INSERT INTO asset_characters VALUES('one','a','reference','human'),('two','a','reference','human'),('one','a','depicted','inferred');").unwrap();
    drop(old);
    let db = Database::open(&path).unwrap();
    let refs = db.references("a").unwrap();
    assert_eq!(refs.len(), 2);
    assert!(refs.iter().all(|r| r["primary"] == false));
    assert_eq!(refs[0]["position"], 0);
    let variant = db
        .save_variant("a", None, "Historical", json!({"P1":"exact"}), json!({}))
        .unwrap();
    db.add_reference("a", Some(&variant), "three").unwrap();
    db.primary_reference("a", Some(&variant), "three").unwrap();
    assert!(db
        .primary_reference("a", Some(&variant), "missing")
        .is_err());
    assert_eq!(
        db.references("a")
            .unwrap()
            .iter()
            .find(|r| r["assetId"] == "three")
            .unwrap()["primary"],
        true
    );
    db.restore_character("a", 1).unwrap();
    let refs = db.references("a").unwrap();
    let historical = refs.iter().find(|r| r["assetId"] == "three").unwrap();
    assert_eq!(historical["historicalVariant"], true);
    assert_eq!(historical["variantName"], "Historical");
    db.remove_reference("a", None, "one").unwrap();
    assert!(db.all_characters(false).unwrap()[0]
        .image_ids
        .contains(&"one".to_string()));
    db.associate("a", "three", false).unwrap();
    assert!(!db
        .references("a")
        .unwrap()
        .iter()
        .any(|r| r["assetId"] == "three"));
    let backups = std::fs::read_dir(dir.join("backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let saved = rusqlite::Connection::open(&backups[0]).unwrap();
    assert_eq!(
        saved
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        6
    );
    drop(saved);
    drop(db);
    let db = Database::open(&path).unwrap();
    assert_eq!(db.references("a").unwrap().len(), 1);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn portable_references_preserve_scope_order_primary_and_reject_loss_atomically() {
    let dir = std::env::temp_dir().join(format!("studio-ref-portable-{}", uuid::Uuid::new_v4()));
    let images = dir.join("images");
    std::fs::create_dir_all(&images).unwrap();
    for i in 0..3 {
        image::RgbImage::from_pixel(2, 2, image::Rgb([i, 20, 30]))
            .save(images.join(format!("{i}.png")))
            .unwrap();
    }
    let mut source = Database::open(&dir.join("source.sqlite3")).unwrap();
    source.index_library(&images).unwrap();
    let assets = source.gallery(&Default::default()).unwrap().items;
    let id = source
        .save_character(None, "Source".into(), json!({"P1":"exact,  tags\nkept"}))
        .unwrap();
    let v = source
        .save_variant(&id, None, "Night", json!({"P1":"night"}), json!({}))
        .unwrap();
    source.add_reference(&id, None, &assets[0].id).unwrap();
    source.add_reference(&id, None, &assets[1].id).unwrap();
    source.primary_reference(&id, None, &assets[1].id).unwrap();
    source
        .reorder_references(&id, None, &[assets[1].id.clone(), assets[0].id.clone()])
        .unwrap();
    source.add_reference(&id, Some(&v), &assets[0].id).unwrap();
    source
        .primary_reference(&id, Some(&v), &assets[0].id)
        .unwrap();
    // A depicted output is not automatically a visual reference.
    let raw = rusqlite::Connection::open(dir.join("source.sqlite3")).unwrap();
    raw.execute(
        "INSERT INTO asset_characters VALUES(?1,?2,'depicted','inferred')",
        rusqlite::params![assets[2].id, id],
    )
    .unwrap();
    drop(raw);
    let exported = source.export_character(&id).unwrap();
    assert_eq!(exported["version"], 2);
    assert_eq!(exported["references"].as_array().unwrap().len(), 3);
    let mut target = Database::open(&dir.join("target.sqlite3")).unwrap();
    assert!(target
        .import_character_document(&exported.to_string())
        .is_err());
    assert!(target.characters().unwrap().is_empty());
    target.index_library(&images).unwrap();
    let imported = target
        .import_character_document(&exported.to_string())
        .unwrap();
    let imported_v = target.variants(&imported).unwrap()[0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(v, imported_v);
    let refs = target.references(&imported).unwrap();
    assert_eq!(refs.len(), 3);
    let base = refs
        .iter()
        .filter(|r| r["variantId"].is_null())
        .collect::<Vec<_>>();
    assert_eq!(base[0]["primary"], true);
    let variant_ref = refs.iter().find(|r| r["variantId"] == imported_v).unwrap();
    assert_eq!(variant_ref["assetId"], base[1]["assetId"]);
    assert_eq!(variant_ref["primary"], true);
    for mutation in 0..5 {
        let mut bad = exported.clone();
        match mutation {
            0 => bad["references"][1]["position"] = bad["references"][0]["position"].clone(),
            1 => bad["references"][1]["primary"] = json!(true),
            2 => bad["references"][2]["variantId"] = json!("foreign"),
            3 => bad["references"][0]["hash"] = json!("0".repeat(64)),
            _ => bad["references"][2]["historicalVariant"] = json!(true),
        }
        assert!(
            target.import_character_document(&bad.to_string()).is_err(),
            "mutation {mutation}"
        );
        assert_eq!(target.characters().unwrap().len(), 1);
    }
    // Removing the active variant by restoring revision one retains historical scope.
    source.restore_character(&id, 1).unwrap();
    let historical = source.export_character(&id).unwrap();
    let h = target
        .import_character_document(&historical.to_string())
        .unwrap();
    assert!(target.variants(&h).unwrap().is_empty());
    assert_eq!(
        target
            .references(&h)
            .unwrap()
            .iter()
            .filter(|r| r["historicalVariant"] == true)
            .count(),
        1
    );
    assert_eq!(source.references(&id).unwrap().len(), 3);
    drop(target);
    let target = Database::open(&dir.join("target.sqlite3")).unwrap();
    assert_eq!(target.references(&imported).unwrap(), refs);
    let legacy = json!({"format":"saya-character","version":1,"name":"Legacy","identity":{},"variants":[],"referenceHashes":[]});
    assert!(target
        .import_character_document(&legacy.to_string())
        .is_ok());
    drop(target);
    drop(source);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn portable_duplicate_content_is_not_collapsed_or_multiplied() {
    let dir = std::env::temp_dir().join(format!("studio-ref-copies-{}", uuid::Uuid::new_v4()));
    let images = dir.join("images");
    std::fs::create_dir_all(&images).unwrap();
    image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3]))
        .save(images.join("a.png"))
        .unwrap();
    std::fs::copy(images.join("a.png"), images.join("b.png")).unwrap();
    let mut db = Database::open(&dir.join("db.sqlite3")).unwrap();
    db.index_library(&images).unwrap();
    let assets = db.gallery(&Default::default()).unwrap().items;
    let id = db.save_character(None, "Copies".into(), json!({})).unwrap();
    for asset in &assets {
        db.add_reference(&id, None, &asset.id).unwrap();
    }
    let portable = db.export_character(&id).unwrap();
    let imported = db.import_character_document(&portable.to_string()).unwrap();
    assert_eq!(db.references(&imported).unwrap().len(), 2);
    let partial = dir.join("partial");
    std::fs::create_dir(&partial).unwrap();
    std::fs::copy(images.join("a.png"), partial.join("a.png")).unwrap();
    let mut other = Database::open(&dir.join("other.sqlite3")).unwrap();
    other.index_library(&partial).unwrap();
    assert!(other
        .import_character_document(&portable.to_string())
        .unwrap_err()
        .to_string()
        .contains("copies indexées insuffisantes"));
    assert!(other.characters().unwrap().is_empty());
    let mut duplicate = portable.clone();
    duplicate["references"][1]["assetKey"] = duplicate["references"][0]["assetKey"].clone();
    assert!(other
        .import_character_document(&duplicate.to_string())
        .is_err());
    // A legacy reference with no recorded hash cannot be silently dropped on export.
    let raw = rusqlite::Connection::open(dir.join("db.sqlite3")).unwrap();
    raw.execute(
        "UPDATE assets SET content_sha256=NULL WHERE id=?",
        [&assets[0].id],
    )
    .unwrap();
    drop(raw);
    assert!(db.export_character(&id).is_err());
    drop(other);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reference_audit_is_atomic_immutable_and_survives_archive_and_restart() {
    let dir = std::env::temp_dir().join(format!("studio-ref-audit-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("db.sqlite3");
    let db = Database::open(&path).unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch("INSERT INTO libraries(id,label,root_path,platform) VALUES('l','Test','/synthetic','linux'); INSERT INTO assets(id,library_id,relative_path) VALUES('a','l','a.png'),('b','l','b.png');").unwrap();
    let id = db.save_character(None, "Audit".into(), json!({})).unwrap();
    assert!(db.reference_history(&id, None).unwrap().is_empty());
    db.add_reference(&id, None, "a").unwrap();
    db.add_reference(&id, None, "b").unwrap();
    db.primary_reference(&id, None, "b").unwrap();
    db.reorder_references(&id, None, &["b".into(), "a".into()])
        .unwrap();
    let history = db.reference_history(&id, None).unwrap();
    assert_eq!(history.len(), 4);
    assert_eq!(history[0]["operation"], "order");
    assert_eq!(history[0]["changes"]["before"].as_array().unwrap().len(), 2);
    db.add_reference(&id, None, "a").unwrap();
    db.primary_reference(&id, None, "b").unwrap();
    assert!(db
        .reorder_references(&id, None, &["a".into(), "a".into()])
        .is_err());
    assert!(db.primary_reference(&id, None, "missing").is_err());
    assert_eq!(db.reference_history(&id, None).unwrap(), history);
    db.set_character_archived(&id, true).unwrap();
    assert!(db.remove_reference(&id, None, "a").is_err());
    db.set_character_archived(&id, false).unwrap();
    assert_eq!(db.reference_history(&id, None).unwrap(), history);
    db.associate(&id, "a", false).unwrap();
    let latest = db.reference_history(&id, None).unwrap();
    assert_eq!(latest.len(), 5);
    assert_eq!(latest[0]["changes"]["before"].as_array().unwrap().len(), 1);
    assert!(latest[0]["changes"]["after"].as_array().unwrap().is_empty());
    assert!(raw
        .execute(
            "UPDATE character_reference_history SET operation='forged'",
            []
        )
        .is_err());
    assert!(raw
        .execute("DELETE FROM character_reference_history", [])
        .is_err());
    drop(raw);
    drop(db);
    let db = Database::open(&path).unwrap();
    assert_eq!(db.reference_history(&id, None).unwrap(), latest);
    assert_eq!(
        db.reference_history(&id, latest[0]["id"].as_i64()).unwrap(),
        history
    );
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn v7_reference_migration_keeps_snapshot_and_readable_backup() {
    let dir = std::env::temp_dir().join(format!("studio-ref-v7-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("db.sqlite3");
    let old = rusqlite::Connection::open(&path).unwrap();
    for sql in [
        include_str!("../../../migrations/0001_initial.sql"),
        include_str!("../../../migrations/0002_functional.sql"),
        include_str!("../../../migrations/0003_gallery.sql"),
        include_str!("../../../migrations/0004_character_history.sql"),
        include_str!("../../../migrations/0005_prompt_proposals.sql"),
        include_str!("../../../migrations/0006_generation_provenance.sql"),
        include_str!("../../../migrations/0007_character_references.sql"),
    ] {
        old.execute_batch(sql).unwrap();
    }
    old.execute_batch("PRAGMA user_version=7;INSERT INTO characters(id,name,identity_json) VALUES('c','Keep','{}');INSERT INTO libraries(id,label,root_path,platform) VALUES('l','Test','/synthetic','linux');INSERT INTO assets(id,library_id,relative_path,availability) VALUES('a','l','a.png','missing');INSERT INTO character_references(character_id,variant_id,variant_name,asset_id,position,is_primary) VALUES('c','historical','Old variant','a',9,1);").unwrap();
    drop(old);
    let db = Database::open(&path).unwrap();
    // Parallel migrations go beyond 16: number never frozen.
    assert!(db.status().unwrap().schema_version >= 16);
    let audit = db.reference_history("c", None).unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0]["operation"], "migration");
    assert_eq!(audit[0]["changes"]["after"][0]["position"], 9);
    assert_eq!(audit[0]["changes"]["after"][0]["primary"], true);
    let refs = db.references("c").unwrap();
    assert_eq!(refs[0]["availability"], "missing");
    assert_eq!(refs[0]["historicalVariant"], true);
    let backups = std::fs::read_dir(dir.join("backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let backup = rusqlite::Connection::open(&backups[0]).unwrap();
    assert_eq!(
        backup
            .query_row("pragma user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        7
    );
    assert_eq!(
        backup
            .query_row("SELECT position FROM character_references", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        9
    );
    drop(backup);
    drop(db);
    let db = Database::open(&path).unwrap();
    assert_eq!(db.reference_history("c", None).unwrap(), audit);
    assert_eq!(db.references("c").unwrap(), refs);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn multiple_roles_for_one_asset_never_duplicate_filtered_gallery_rows() {
    let root = std::env::temp_dir().join(format!("saya-multiple-roles-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let path = root.join("db.sqlite3");
    drop(Database::open(&path).unwrap());
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("INSERT INTO libraries(id,label,root_path,platform) VALUES('lib','Synthetic','/synthetic','linux'); INSERT INTO characters(id,name,identity_json) VALUES('a','A','{}'); INSERT INTO assets(id,library_id,relative_path,availability) VALUES('one','lib','one.png','present'); INSERT INTO asset_characters VALUES('one','a','depicted','inferred'),('one','a','reference','human'); INSERT INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES('wf','Synthetic','hash','comfy_api','{}'); INSERT INTO generation_jobs(id,workflow_revision_id,seed,state,request_snapshot_json) VALUES('job','wf','1','succeeded','{}'); INSERT INTO job_characters VALUES('job','P1','a','variant','{}'); INSERT INTO job_outputs VALUES('job','one','out',0,'saved');").unwrap();
    image::RgbImage::new(2, 2)
        .save(root.join("one.png"))
        .unwrap();
    sql.execute(
        "UPDATE libraries SET root_path=? WHERE id='lib'",
        [root.to_str().unwrap()],
    )
    .unwrap();
    sql.execute(
        "UPDATE assets SET absolute_path=? WHERE id='one'",
        [root.join("one.png").to_str().unwrap()],
    )
    .unwrap();
    drop(sql);
    let db = Database::open(&path).unwrap();
    let filters = studio_core::gallery::Filters {
        characters: vec!["a".into()],
        variant: "variant".into(),
        job: "job".into(),
        ..Default::default()
    };
    let page = db.gallery(&filters).unwrap();
    assert_eq!(
        (page.total, page.items.len()),
        (1, 1),
        "SQLite {}",
        rusqlite::version()
    );
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}
