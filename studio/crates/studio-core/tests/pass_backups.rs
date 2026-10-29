use std::fs;
use studio_core::{
    backup,
    pass_states::{self, NewState, Retention, StateKind},
    Database,
};

#[test]
fn optional_pass_store_roundtrips_without_originals_and_rejects_corruption() {
    let root = std::env::temp_dir().join(format!("saya-optional-backup-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let source = root.join("studio.sqlite3");
    let db = Database::open(&source).unwrap();
    let store = root.join("pass-states");
    pass_states::init_store(&store, 1024 * 1024).unwrap();
    let entry = pass_states::save_state(
        &store,
        &NewState {
            pipeline: "pipe",
            pass: "base",
            producer_step: "synthetic",
            kind: StateKind::Image,
            retention: Retention::Pinned,
            bytes: b"synthetic-state",
            max_state_bytes: 1024,
        },
    )
    .unwrap();
    fs::write(store.join("orphan"), b"not registered").unwrap();
    let source_before = fs::read(store.join("manifest.json")).unwrap();
    let default = root.join("default");
    let manifest = backup::create(&source, &default).unwrap();
    assert_eq!(manifest.version, 3);
    assert!(!manifest.pass_states_included);
    assert!(manifest.pass_state_files.is_empty());
    assert!(!default.join("pass-states").exists());
    let estimate = backup::estimate(&source, true).unwrap();
    assert_eq!(estimate["files"].as_array().unwrap().len(), 3); // DB, store manifest, state
    let full = root.join("full");
    let manifest = backup::create_with_options(&source, &full, true).unwrap();
    assert!(manifest.pass_states_included);
    assert_eq!(manifest.pass_state_files.len(), 2);
    assert!(!full.join("pass-states/orphan").exists());
    assert_eq!(
        fs::read(store.join("manifest.json")).unwrap(),
        source_before
    );
    fs::rename(&store, root.join("source-away")).unwrap();
    let restored = root.join("restored");
    backup::restore(&full, &restored).unwrap();
    let (_, bytes) =
        pass_states::load_state(&restored.join("pass-states"), "pipe", "base", &entry.sha256)
            .unwrap();
    assert_eq!(bytes, b"synthetic-state");
    assert!(Database::open(&restored.join("studio.sqlite3")).is_ok());
    fs::write(full.join("pass-states").join(&entry.file), b"tamper").unwrap();
    assert!(backup::restore(&full, &root.join("bad")).is_err());
    assert!(!root.join("bad").exists());
    drop(db);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn pass_backup_rejects_symlinked_parent_and_ignores_states_without_opt_in() {
    let root = std::env::temp_dir().join(format!("saya-symlink-backup-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let source = root.join("studio.sqlite3");
    let db = Database::open(&source).unwrap();
    let store = root.join("pass-states");
    pass_states::init_store(&store, 1024 * 1024).unwrap();
    pass_states::save_state(
        &store,
        &NewState {
            pipeline: "pipe",
            pass: "base",
            producer_step: "synthetic",
            kind: StateKind::Latent,
            retention: Retention::Pinned,
            bytes: b"opaque-latent",
            max_state_bytes: 1024,
        },
    )
    .unwrap();
    fs::rename(store.join("data"), root.join("external-data")).unwrap();
    std::os::unix::fs::symlink(root.join("external-data"), store.join("data")).unwrap();
    assert!(backup::estimate(&source, true).is_err());
    assert!(backup::create_with_options(&source, &root.join("refused"), true).is_err());
    assert!(!root.join("refused/manifest.json").exists());
    assert!(backup::create(&source, &root.join("sqlite-only")).is_ok());
    drop(db);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn tampered_manifest_paths_are_refused_without_writing_outside() {
    let root = std::env::temp_dir().join(format!("saya-backup-traversal-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let source = root.join("studio.sqlite3");
    let db = Database::open(&source).unwrap();
    let store = root.join("pass-states");
    pass_states::init_store(&store, 1024 * 1024).unwrap();
    pass_states::save_state(
        &store,
        &NewState {
            pipeline: "pipe",
            pass: "base",
            producer_step: "synthetic",
            kind: StateKind::Image,
            retention: Retention::Pinned,
            bytes: b"state",
            max_state_bytes: 1024,
        },
    )
    .unwrap();
    let full = root.join("full");
    backup::create_with_options(&source, &full, true).unwrap();
    let manifest_path = full.join("manifest.json");
    let original = fs::read_to_string(&manifest_path).unwrap();
    for (field, value) in [
        ("passStateFiles", "../../outside.txt"),
        ("checkpointFiles", "../escape.png"),
    ] {
        let mut manifest: serde_json::Value = serde_json::from_str(&original).unwrap();
        let entry = serde_json::json!({"file": value, "sha256": "0".repeat(64), "byteSize": 5});
        match manifest[field].as_array_mut() {
            Some(list) if !list.is_empty() => list[0] = entry,
            _ => manifest[field] = serde_json::json!([entry]),
        }
        fs::write(&manifest_path, manifest.to_string()).unwrap();
        assert!(backup::verify(&full).is_err(), "{field} traversal accepted");
        let target = root.join(format!("restore-{field}"));
        assert!(backup::restore(&full, &target).is_err());
        assert!(!target.exists(), "partial restore left behind");
        assert!(
            !root.join("outside.txt").exists()
                && !root.join("escape.png").exists()
                && !full.parent().unwrap().join("escape.png").exists()
        );
    }
    fs::write(&manifest_path, &original).unwrap();
    assert!(
        backup::verify(&full).is_ok(),
        "untampered backup still valid"
    );
    drop(db);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dependency_report_lists_missing_external_and_state_dependencies_read_only() {
    let root = std::env::temp_dir().join(format!("saya-backup-deps-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let library = root.join("library");
    fs::create_dir(&library).unwrap();
    fs::write(library.join("present.png"), b"x").unwrap();
    let source = root.join("studio.sqlite3");
    let db = Database::open(&source).unwrap();
    let workflow = db.import_workflow("W", r#"{"1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"modele.safetensors"}},"2":{"class_type":"SaveImage","inputs":{"images":["1",0],"filename_prefix":"x"}}}"#).unwrap();
    let store = root.join("pass-states");
    pass_states::init_store(&store, 1024 * 1024).unwrap();
    let job = "0123456789abcdef-job";
    let entry = pass_states::save_state(
        &store,
        &NewState {
            pipeline: "run-r1",
            pass: "base-01234567",
            producer_step: "job",
            kind: StateKind::Image,
            retention: Retention::Pinned,
            bytes: b"state",
            max_state_bytes: 1024,
        },
    )
    .unwrap();
    let raw = rusqlite::Connection::open(&source).unwrap();
    raw.execute_batch(&format!(
        "         INSERT INTO libraries(id,label,root_path,platform) VALUES('lib','L','{}','linux');
         INSERT INTO assets(id,library_id,relative_path,availability) VALUES('a','lib','present.png','present'),('b','lib','gone.png','present');
         INSERT INTO generation_jobs(id,workflow_revision_id,seed,state,request_snapshot_json) VALUES('{job}','{workflow}','1','succeeded','{{}}');
         INSERT INTO pipeline_definitions(id,name,manifest_json,manifest_sha256) VALUES('p','P','{{}}','h');
         INSERT INTO pipeline_runs(id,pipeline_id,manifest_sha256) VALUES('r1','p','h');
         INSERT INTO pipeline_run_steps(run_id,step_id,position,state,job_id,output_kind,output_sha256) VALUES('r1','base',0,'completed','{job}','image','{}');",
        library.display(), entry.sha256
    )).unwrap();
    drop(raw);
    let minimal = root.join("minimal");
    backup::create(&source, &minimal).unwrap();
    let before = fs::read(minimal.join("studio.sqlite3")).unwrap();
    let report = backup::dependency_report(&minimal).unwrap();
    assert_eq!(report["imagesChecked"], 2);
    assert_eq!(report["missingImages"].as_array().unwrap().len(), 1);
    assert!(report["missingImages"][0]
        .as_str()
        .unwrap()
        .ends_with("gone.png"));
    assert_eq!(
        report["missingPipelineStates"][0]["step"], "base",
        "states not included by default"
    );
    assert!(report["externalModels"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m == "ckpt_name: modele.safetensors"));
    assert_eq!(report["selfContained"], false);
    assert_eq!(report["complete"], false);
    assert_eq!(
        fs::read(minimal.join("studio.sqlite3")).unwrap(),
        before,
        "report never writes the copy"
    );
    let full = root.join("full");
    backup::create_with_options(&source, &full, true).unwrap();
    assert_eq!(
        backup::dependency_report(&full).unwrap()["missingPipelineStates"],
        serde_json::json!([])
    );
    drop(db);
    fs::remove_dir_all(root).unwrap();
}
