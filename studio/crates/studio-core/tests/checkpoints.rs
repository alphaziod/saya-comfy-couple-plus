use serde_json::{json, Value};
use std::path::PathBuf;
use studio_core::Database;

fn fixture() -> (PathBuf, Database, String, String, Value) {
    let root = std::env::temp_dir().join(format!("saya-checkpoints-{}", uuid::Uuid::new_v4()));
    let images = root.join("images");
    std::fs::create_dir_all(&images).unwrap();
    image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]))
        .save(images.join("saved.png"))
        .unwrap();
    let mut db = Database::open(&root.join("db.sqlite3")).unwrap();
    db.index_library(&images).unwrap();
    let asset = db.gallery(&Default::default()).unwrap().items[0].id.clone();
    let graph = json!({"source":{"class_type":"SyntheticImage","inputs":{"version":"model-v1"}},"save":{"class_type":"SaveImage","inputs":{"images":["source",0],"filename_prefix":"test"}}});
    let workflow = db
        .import_workflow("checkpoint", &graph.to_string())
        .unwrap();
    let schema = json!({"SyntheticImage":{"input":{"required":{"version":["STRING"]}},"output":["IMAGE"]},"SaveImage":{"input":{"required":{"images":["IMAGE"],"filename_prefix":["STRING"]}},"output":[]}});
    let job = db.prepare_job(&workflow, &[], &schema).unwrap().job_id;
    let sql = rusqlite::Connection::open(root.join("db.sqlite3")).unwrap();
    sql.execute(
        "UPDATE generation_jobs SET state='succeeded' WHERE id=?",
        [&job],
    )
    .unwrap();
    sql.execute(
        "INSERT INTO job_outputs VALUES(?1,?2,'save',0,'saved')",
        rusqlite::params![job, asset],
    )
    .unwrap();
    (root, db, job, asset, graph)
}
#[test]
fn managed_image_checkpoint_deduplicates_verifies_and_preserves_reviews_after_restart() {
    let (root, db, job, asset, graph) = fixture();
    let store = root.join("checkpoints");
    let first = db
        .capture_image_checkpoint(&store, &job, &asset, 1024 * 1024)
        .unwrap();
    let id = first["id"].as_str().unwrap();
    assert_eq!(first["resumeCapability"], "unverified");
    assert_eq!(
        db.capture_image_checkpoint(&store, &job, &asset, 1024 * 1024)
            .unwrap()["id"],
        id
    );
    assert_eq!(
        std::fs::read_dir(&store)
            .unwrap()
            .filter(|e| e
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|x| x == "png"))
            .count(),
        1
    );
    assert!(db.check_checkpoint_dependencies(&store, id, &graph).is_ok());
    let mut changed = graph.clone();
    changed["source"]["inputs"]["version"] = json!("model-v2");
    assert!(db
        .check_checkpoint_dependencies(&store, id, &changed)
        .is_err());
    db.review_checkpoint(&store, id, true, "looks good")
        .unwrap();
    let checkpoints = db.checkpoints(&job).unwrap();
    assert_eq!(checkpoints[0]["status"], "validated");
    let file = store.join(checkpoints[0]["manifest"]["file"].as_str().unwrap());
    let bytes = std::fs::read(&file).unwrap();
    drop(db);
    let db = Database::open(&root.join("db.sqlite3")).unwrap();
    assert_eq!(db.checkpoints(&job).unwrap(), checkpoints);
    assert!(db.verify_checkpoint(&store, id).is_ok());
    std::fs::write(&file, b"corrupt").unwrap();
    assert!(db.verify_checkpoint(&store, id).is_err());
    std::fs::write(&file, bytes).unwrap();
    std::fs::rename(&file, store.join("moved.png")).unwrap();
    assert!(db.verify_checkpoint(&store, id).is_err());
    assert!(db
        .capture_image_checkpoint(&store, "foreign", &asset, 1024 * 1024)
        .is_err());
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn checkpoint_budget_orphans_rejection_and_manifest_tampering_fail_safely() {
    let (root, db, job, asset, _) = fixture();
    let store = root.join("checkpoints");
    assert!(db
        .capture_image_checkpoint(&store, &job, &asset, 1)
        .is_err());
    assert!(db.checkpoints(&job).unwrap().is_empty());
    // Durable file left before SQL publication: reuse it, never overwrite or duplicate.
    let source = root.join("images/saved.png");
    let hash = studio_core::gallery::digest(&std::fs::read(&source).unwrap());
    std::fs::copy(&source, store.join(format!("{hash}.png"))).unwrap();
    let captured = db
        .capture_image_checkpoint(&store, &job, &asset, 1024 * 1024)
        .unwrap();
    let id = captured["id"].as_str().unwrap();
    db.review_checkpoint(&store, id, false, "test reject")
        .unwrap();
    assert_eq!(db.checkpoints(&job).unwrap()[0]["status"], "rejected");
    assert!(store.join(format!("{hash}.png")).exists());
    let sql = rusqlite::Connection::open(root.join("db.sqlite3")).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT count(*) FROM human_reviews WHERE artifact_id=?",
            [id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    let mut manifest = db.verify_checkpoint(&store, id).unwrap();
    manifest["file"] = json!("../images/saved.png");
    sql.execute(
        "UPDATE pass_artifacts SET manifest_json=?2 WHERE id=?1",
        rusqlite::params![id, manifest.to_string()],
    )
    .unwrap();
    assert!(db.verify_checkpoint(&store, id).is_err());
    assert!(db.review_checkpoint(&store, id, true, "").is_err());
    assert_eq!(db.checkpoints(&job).unwrap()[0]["status"], "rejected");
    drop(sql);
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn backup_restores_managed_checkpoints_without_originals_and_rejects_incomplete_copies() {
    use studio_core::backup;
    let (root, db, job, asset, _) = fixture();
    let store = root.join("checkpoints");
    let captured = db
        .capture_image_checkpoint(&store, &job, &asset, 1024 * 1024)
        .unwrap();
    let id = captured["id"].as_str().unwrap();
    db.review_checkpoint(&store, id, true, "keep").unwrap();
    let sql = rusqlite::Connection::open(root.join("db.sqlite3")).unwrap();
    sql.execute("INSERT INTO pass_artifacts SELECT 'same-file-other-artifact',pass_attempt_id,asset_id,kind,status,manifest_json,content_sha256,resume_capability FROM pass_artifacts WHERE id=?", [id]).unwrap();
    drop(sql);
    let before = db.checkpoints(&job).unwrap();
    let file = before[0]["manifest"]["file"].as_str().unwrap();
    std::fs::write(store.join(".partial-unused"), b"interrupted").unwrap();
    let default_copy = root.join("database-only");
    let default_manifest = backup::create(&root.join("db.sqlite3"), &default_copy).unwrap();
    assert!(
        default_manifest.checkpoint_files.is_empty(),
        "pass states require an explicit backup option"
    );
    assert!(!default_copy.join("checkpoints").exists());
    let copy = root.join("backup");
    let manifest = backup::create_with_options(&root.join("db.sqlite3"), &copy, true).unwrap();
    assert_eq!(manifest.version, 3);
    assert_eq!(manifest.checkpoint_files.len(), 1);
    // Existing v2 backups remain readable with their original checkpoint contract.
    let manifest_path = copy.join("manifest.json");
    let current = std::fs::read(&manifest_path).unwrap();
    let mut legacy: serde_json::Value = serde_json::from_slice(&current).unwrap();
    legacy["version"] = json!(2);
    legacy.as_object_mut().unwrap().remove("passStatesIncluded");
    legacy.as_object_mut().unwrap().remove("passStateFiles");
    std::fs::write(&manifest_path, legacy.to_string()).unwrap();
    assert_eq!(backup::verify(&copy).unwrap().version, 2);
    std::fs::write(&manifest_path, current).unwrap();

    assert_eq!(
        std::fs::read(copy.join("checkpoints").join(file)).unwrap(),
        std::fs::read(store.join(file)).unwrap()
    );
    assert!(!copy.join("checkpoints/.partial-unused").exists());
    // The managed copy must survive loss of the original result and active store.
    std::fs::rename(root.join("images"), root.join("images-away")).unwrap();
    std::fs::rename(&store, root.join("checkpoints-away")).unwrap();
    let restored = root.join("restored");
    backup::restore(&copy, &restored).unwrap();
    let restored_db = Database::open(&restored.join("studio.sqlite3")).unwrap();
    assert_eq!(restored_db.checkpoints(&job).unwrap(), before);
    restored_db
        .verify_checkpoint(&restored.join("checkpoints"), id)
        .unwrap();
    // Missing live checkpoint makes a new backup incomplete, never a false success.
    let incomplete = root.join("incomplete");
    assert!(backup::create_with_options(&root.join("db.sqlite3"), &incomplete, true).is_err());
    assert!(!incomplete.join("manifest.json").exists());
    assert!(backup::verify(&incomplete).is_err());
    let copied_file = copy.join("checkpoints").join(file);
    let bytes = std::fs::read(&copied_file).unwrap();
    std::fs::write(&copied_file, b"damaged").unwrap();
    let bad = root.join("bad");
    assert!(backup::restore(&copy, &bad).is_err());
    assert!(!bad.exists());
    std::fs::write(&copied_file, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::{symlink, PermissionsExt};
        assert_eq!(
            std::fs::metadata(restored.join("checkpoints").join(file))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let target = root.join("outside.png");
        std::fs::rename(&copied_file, &target).unwrap();
        symlink(&target, &copied_file).unwrap();
        assert!(backup::verify(&copy).is_err());
        std::fs::remove_file(&copied_file).unwrap();
        std::fs::rename(target, &copied_file).unwrap();
    }
    let manifest_path = copy.join("manifest.json");
    let mut tampered: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    tampered["checkpointFiles"] = json!([]);
    std::fs::write(&manifest_path, tampered.to_string()).unwrap();
    assert!(backup::verify(&copy).is_err());
    drop(restored_db);
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}
