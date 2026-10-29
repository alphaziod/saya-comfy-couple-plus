use serde_json::json;
use std::{fs, path::PathBuf};
use studio_core::{
    comfy::Comfy,
    gallery::{digest, image_data, Filters},
    workflow::{api_graph, Edit},
    Database,
};
fn folder() -> PathBuf {
    let p = std::env::temp_dir().join(format!("saya-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&p).unwrap();
    p
}
fn png(path: &std::path::Path) {
    let file = fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, 2, 2);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.add_text_chunk("prompt".into(),json!({"1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"checkpoint-A"}},"2":{"class_type":"Lora Loader (LoraManager)","inputs":{"loras":{"__value__":[{"name":"lora-A","active":true},{"name":"disabled","active":false}]}}}}).to_string()).unwrap();
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[120; 12]).unwrap();
}
#[test]
fn gallery_crud_associations_filters_and_restart() {
    let root = folder();
    let images = root.join("images");
    fs::create_dir(&images).unwrap();
    let image = images.join("original.png");
    png(&image);
    let original = digest(&fs::read(&image).unwrap());
    let path = root.join("studio.sqlite3");
    let mut db = Database::open(&path).unwrap();
    let report = db.index_library(&images).unwrap();
    assert_eq!(report.indexed, 1);
    assert_eq!(db.index_library(&images).unwrap().unchanged, 1);
    let all = db.gallery(&Filters::default()).unwrap();
    let asset = &all.items[0].id;
    assert_eq!(
        db.gallery(&Filters {
            checkpoint: "checkpoint-A".into(),
            lora: "lora-A".into(),
            ..Default::default()
        })
        .unwrap()
        .total,
        1
    );
    assert_eq!(
        db.gallery(&Filters {
            lora: "disabled".into(),
            ..Default::default()
        })
        .unwrap()
        .total,
        0
    );
    let id = db
        .save_character(None, "Test character".into(), json!({"traits":"blue hair"}))
        .unwrap();
    db.associate(&id, asset, true).unwrap();
    db.associate(&id, asset, true).unwrap();
    db.save_character(
        Some(id.clone()),
        "Renamed".into(),
        json!({"traits":"blue hair, green eyes"}),
    )
    .unwrap();
    db.set_preference("galleryFilters", json!({"character":id}))
        .unwrap();
    assert!(
        image_data(&db.asset_path(asset).unwrap(), &root.join("cache"), false)
            .unwrap()
            .starts_with("data:image/jpeg;base64,")
    );
    drop(db);
    let db = Database::open(&path).unwrap();
    assert_eq!(db.characters().unwrap()[0].image_ids.len(), 1);
    assert_eq!(db.characters().unwrap()[0].name, "Renamed");
    assert_eq!(db.preferences().unwrap()["galleryFilters"]["character"], id);
    assert_eq!(
        db.gallery(&Filters {
            character: id.clone(),
            ..Default::default()
        })
        .unwrap()
        .total,
        1
    );
    db.associate(&id, asset, false).unwrap();
    assert!(db.characters().unwrap()[0].image_ids.is_empty());
    db.delete_character(&id).unwrap();
    assert!(db.characters().unwrap().is_empty());
    assert_eq!(digest(&fs::read(&image).unwrap()), original);
    drop(db);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn migration_backs_up_v1_before_changing_it() {
    let root = folder();
    let path = root.join("studio.sqlite3");
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch(include_str!("../../../migrations/0001_initial.sql"))
        .unwrap();
    old.execute_batch("PRAGMA user_version=1;INSERT INTO characters(id,name,identity_json) VALUES('saved','Keep me','{}')").unwrap();
    drop(old);
    let db = Database::open(&path).unwrap();
    // Parallel migrations go beyond 16: number never frozen.
    assert!(db.status().unwrap().schema_version >= 16);
    assert_eq!(db.characters().unwrap()[0].name, "Keep me");
    let backups: Vec<_> = fs::read_dir(root.join("backups")).unwrap().collect();
    assert_eq!(backups.len(), 1);
    let backup = rusqlite::Connection::open(backups[0].as_ref().unwrap().path()).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        backup
            .query_row("SELECT name FROM characters", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "Keep me"
    );
    drop(backup);
    drop(db);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn workflow_preserves_original_requires_mapping_and_confirmation() {
    let mut db = Database::in_memory().unwrap();
    let graph = json!({"1":{"class_type":"CLIPTextEncode","inputs":{"text":"QUALITY: good, blue eyes, red hair","clip":["2",0]}},"2":{"class_type":"Source","inputs":{}}});
    let text = graph.to_string();
    let id = db.import_workflow("Fixture", &text).unwrap();
    assert!(api_graph("{\"nodes\":[]}").is_err());
    let edit = Edit {
        section: Some("P1".into()),
        node_id: "1".into(),
        input_name: "text".into(),
        value: "QUALITY: good, blue eyes".into(),
    };
    let info = json!({"CLIPTextEncode":{"input":{"required":{"text":["STRING",{}],"clip":["CLIP"]}},"output":[]},"Source":{"input":{"required":{}},"output":["CLIP"]}});
    assert!(db
        .prepare_job(&id, std::slice::from_ref(&edit), &info)
        .is_err());
    db.save_bindings(&id, std::slice::from_ref(&edit)).unwrap();
    let draft = db.prepare_job(&id, &[edit], &info).unwrap();
    assert_eq!(draft.removed.len(), 1);
    assert_eq!(
        db.workflow(&id).unwrap().fields[0].section.as_deref(),
        Some("P1")
    );
    assert!(draft.request_json.contains("blue eyes"));
    assert!(draft.removed[0].contains("red hair"));
    assert_eq!(
        db.workflow(&id).unwrap().fields[0].value,
        "QUALITY: good, blue eyes, red hair"
    );
    let client = Comfy::new().unwrap();
    assert!(db
        .submit_job(&draft.job_id, &draft.snapshot_hash, false, false, &client)
        .is_err());
    assert!(db
        .submit_job(&draft.job_id, &draft.snapshot_hash, true, false, &client)
        .is_err());
    assert_eq!(db.jobs().unwrap()[0]["state"], "draft");
}
#[cfg(unix)]
#[test]
fn image_symlinks_cannot_escape_library() {
    use std::os::unix::fs::symlink;
    let root = folder();
    let inside = root.join("inside");
    fs::create_dir(&inside).unwrap();
    png(&root.join("outside.png"));
    symlink(root.join("outside.png"), inside.join("link.png")).unwrap();
    let mut db = Database::in_memory().unwrap();
    db.index_library(&inside).unwrap();
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, 0);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn gallery_filters_duplicates_missing_and_metadata_states() {
    let root = folder();
    let images = root.join("images");
    fs::create_dir(&images).unwrap();
    png(&images.join("a.png"));
    fs::copy(images.join("a.png"), images.join("b.png")).unwrap();
    image::RgbImage::new(2, 2)
        .save(images.join("c.png"))
        .unwrap();
    let mut db = Database::in_memory().unwrap();
    db.index_library(&images).unwrap();
    let filter = |v| serde_json::from_value::<Filters>(v).unwrap();
    assert_eq!(
        db.gallery(&filter(json!({"duplicates":true})))
            .unwrap()
            .total,
        2
    );
    assert_eq!(
        db.gallery(&filter(json!({"loras":["lora-A","missing"]})))
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        db.gallery(&filter(
            json!({"loras":["lora-A"],"metadataState":"read","search":"a.png"})
        ))
        .unwrap()
        .total,
        1
    );
    assert_eq!(
        db.gallery(&filter(json!({"metadataState":"absent"})))
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        db.gallery(&filter(json!({"sort":"name_desc"})))
            .unwrap()
            .items[0]
            .path,
        "c.png"
    );
    assert!(db
        .gallery(&filter(json!({"sort":"DROP TABLE assets"})))
        .is_err());
    fs::remove_file(images.join("a.png")).unwrap();
    db.index_library(&images).unwrap();
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, 2);
    assert_eq!(
        db.gallery(&filter(json!({"duplicates":true})))
            .unwrap()
            .total,
        0
    );
    png(&images.join("a.png"));
    db.index_library(&images).unwrap();
    assert_eq!(db.gallery(&Filters::default()).unwrap().total, 3);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn clip_only_lora_is_an_active_resource() {
    let (_, loras) = studio_core::gallery::resources(
        &json!({"1":{"inputs":{"lora_name":"clip-only","strength_model":0,"strength_clip":1}},"2":{"inputs":{"lora_name":"disabled","strength_model":0,"strength_clip":0}}}),
    );
    assert_eq!(loras, vec!["clip-only"]);
}

#[test]
fn annotations_survive_reindex_restart_and_concurrent_reads() {
    let root = folder();
    let images = root.join("images");
    fs::create_dir(&images).unwrap();
    png(&images.join("a.png"));
    let path = root.join("studio.sqlite3");
    let mut db = Database::open(&path).unwrap();
    db.index_library(&images).unwrap();
    let id = db.gallery(&Filters::default()).unwrap().items[0].id.clone();
    db.annotate_asset(&id, true, "keep exactly: blue eyes, red hair", "accepted")
        .unwrap();
    assert!(db.annotate_asset(&id, false, "", "bad-state").is_err());
    assert!(db.annotate_asset("missing", false, "", "accepted").is_err());
    let reader = Database::open(&path).unwrap();
    let mut observations = 0;
    db.index_library_progress(&images, |_, _| {
        assert_eq!(reader.gallery(&Filters::default()).unwrap().total, 1);
        observations += 1;
    })
    .unwrap();
    assert!(observations >= 2);
    drop(reader);
    drop(db);
    let db = Database::open(&path).unwrap();
    let result = db
        .gallery(&serde_json::from_value(json!({"favorite":true,"review":"accepted"})).unwrap())
        .unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].note, "keep exactly: blue eyes, red hair");
    assert_eq!(
        db.gallery(&serde_json::from_value(json!({"review":"rejected"})).unwrap())
            .unwrap()
            .total,
        0
    );
    drop(db);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn migration_from_v2_preserves_rows_and_backs_up_once() {
    let root = folder();
    let path = root.join("studio.sqlite3");
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch(include_str!("../../../migrations/0001_initial.sql"))
        .unwrap();
    old.execute_batch(include_str!("../../../migrations/0002_functional.sql"))
        .unwrap();
    old.execute_batch("PRAGMA user_version=2; INSERT INTO characters(id,name,identity_json) VALUES('c','Preserve','{\"tags\":\"literal,  unchanged\"}'); INSERT INTO preferences VALUES('selectedTab','\"characters\"');").unwrap();
    drop(old);
    let db = Database::open(&path).unwrap();
    assert_eq!(
        db.characters().unwrap()[0].identity["tags"],
        "literal,  unchanged"
    );
    assert_eq!(db.preferences().unwrap()["selectedTab"], "characters");
    drop(db);
    drop(Database::open(&path).unwrap());
    assert_eq!(fs::read_dir(root.join("backups")).unwrap().count(), 1);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn character_history_archive_variants_and_portable_roundtrip() {
    let root = folder();
    let path = root.join("studio.sqlite3");
    let db = Database::open(&path).unwrap();
    let id=db.save_character(None,"Original".into(),json!({"identity_prompt":"blue eyes,  red hair","loras":[{"name":"A","trigger":"exact tag"}]})).unwrap();
    db.save_variant(
        &id,
        None,
        "Night",
        json!({"P1":"blue eyes,  red hair","BG":"night"}),
        json!({"lighting":"night"}),
    )
    .unwrap();
    let revision = db.character_history(&id).unwrap()[0]["revision"]
        .as_u64()
        .unwrap();
    db.save_character(
        Some(id.clone()),
        "Edited".into(),
        json!({"identity_prompt":"changed"}),
    )
    .unwrap();
    db.set_character_archived(&id, true).unwrap();
    assert!(db.characters().unwrap().is_empty());
    assert_eq!(db.all_characters(true).unwrap().len(), 1);
    db.set_character_archived(&id, false).unwrap();
    db.restore_character(&id, revision as u32).unwrap();
    assert_eq!(
        db.characters().unwrap()[0].identity["identity_prompt"],
        "blue eyes,  red hair"
    );
    let export = db.export_character(&id).unwrap();
    let imported = db.import_character_document(&export.to_string()).unwrap();
    assert_ne!(id, imported);
    assert_eq!(
        db.variants(&imported).unwrap()[0]["promptBlocks"]["BG"],
        "night"
    );
    let before = db.characters().unwrap().len();
    let mut bad = export.clone();
    bad["variants"][0]["promptBlocks"]["P1"] = json!(12);
    assert!(db.import_character_document(&bad.to_string()).is_err());
    assert_eq!(db.characters().unwrap().len(), before);
    drop(db);
    let db = Database::open(&path).unwrap();
    assert!(db.character_history(&id).unwrap().len() >= 5);
    assert_eq!(db.variants(&id).unwrap().len(), 1);
    drop(db);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn character_generator_enforces_constraints_and_is_repeatable() {
    use studio_core::characters::suggest_character;
    let constraints =
        json!({"world":"fantasy","role":"scholar","species":"elf","palette":"silver"});
    let a = suggest_character(&constraints, "18446744073709551615").unwrap();
    assert_eq!(
        a,
        suggest_character(&constraints, "18446744073709551615").unwrap()
    );
    let prompt = a["identity"]["identity_prompt"].as_str().unwrap();
    assert!(
        prompt.contains("pointy ears") && prompt.contains("silver hair") && prompt.contains("robe")
    );
    assert!(!prompt.contains("spacesuit"));
    assert_eq!(a["constraints"], constraints);
    assert!(suggest_character(
        &json!({"world":"modern","role":"scholar","species":"android","palette":"silver"}),
        "1"
    )
    .is_err());
    assert!(suggest_character(&constraints, "-1").is_err());
    assert!(suggest_character(&json!({"world":"invalid"}), "1").is_err());
}

#[test]
fn comfy_connection_refuses_malformed_queue_and_reads_model_choices() {
    use studio_core::comfy::{describe_engine, model_choices};
    let stats = json!({"system":{"comfyui_version":"test"}});
    assert!(describe_engine(&stats, &json!({})).is_err());
    assert!(describe_engine(&stats, &json!({"queue_running":[[1]],"queue_pending":[]})).is_err());
    let status = describe_engine(
        &stats,
        &json!({"queue_running":[[7,"prompt-1",{},{}]],"queue_pending":[]}),
    )
    .unwrap();
    assert_eq!(status["running"], 1);
    assert_eq!(status["queue"]["running"][0]["promptId"], "prompt-1");
    let models=model_choices(&json!({"CheckpointLoaderSimple":{"input":{"required":{"ckpt_name":[["B","A"],{}]}}},"LoraLoader":{"input":{"required":{"lora_name":[["L"],{}]}}}})).unwrap();
    assert_eq!(models["checkpoints"], json!(["A", "B"]));
    assert_eq!(models["loras"], json!(["L"]));
    assert!(model_choices(&json!(null)).is_err());
}

#[test]
fn workflow_validation_rejects_bad_types_choices_and_cycles() {
    use studio_core::workflow::validate_graph;
    let info = json!({"Source":{"input":{"required":{"steps":["INT",{"min":1,"max":10}],"sampler":[["euler","heun"],{}]}},"output":["LATENT"]},"Sink":{"input":{"required":{"latent":["LATENT"]}},"output":[],"output_node":true}});
    let graph = json!({"1":{"class_type":"Source","inputs":{"steps":5,"sampler":"euler"}},"2":{"class_type":"Sink","inputs":{"latent":["1",0]}}});
    validate_graph(&graph, &info).unwrap();
    for bad in [json!("5"), json!(0), json!(1.2)] {
        let mut g = graph.clone();
        g["1"]["inputs"]["steps"] = bad;
        assert!(validate_graph(&g, &info).is_err());
    }
    let mut g = graph.clone();
    g["1"]["inputs"]["sampler"] = json!("missing");
    assert!(validate_graph(&g, &info).is_err());
    let mut g = graph.clone();
    g["2"]["inputs"]["latent"] = json!(["1", 10]);
    assert!(validate_graph(&g, &info).is_err());
    let mut g = graph.clone();
    g["2"]["inputs"]["latent"] = json!("not a latent");
    assert!(validate_graph(&g, &info).is_err());
    let graph = json!({"1":{"class_type":"Loop","inputs":{"input":["2",0]}},"2":{"class_type":"Loop","inputs":{"input":["1",0]}}});
    let info = json!({"Loop":{"input":{"required":{"input":["LATENT"]}},"output":["LATENT"],"output_node":true}});
    assert!(validate_graph(&graph, &info).is_err());
}

#[test]
fn mappings_cannot_alias_sections_and_snapshot_keeps_u64_seed() {
    let mut db = Database::in_memory().unwrap();
    let id=db.import_workflow("Exact", &json!({"1":{"class_type":"Fixture","inputs":{"text":"original","seed":18446744073709551615u64}}}).to_string()).unwrap();
    let edit = Edit {
        section: Some("P1".into()),
        node_id: "1".into(),
        input_name: "text".into(),
        value: "original, exact".into(),
    };
    db.save_bindings(&id, std::slice::from_ref(&edit)).unwrap();
    let mut alias = edit.clone();
    alias.section = Some("P2".into());
    assert!(db.save_bindings(&id, &[edit.clone(), alias]).is_err());
    assert_eq!(
        db.workflow(&id)
            .unwrap()
            .fields
            .iter()
            .find(|f| f.input_name == "text")
            .unwrap()
            .section
            .as_deref(),
        Some("P1")
    );
    let info = json!({"Fixture":{"input":{"required":{"text":["STRING"],"seed":["INT",{"min":0}]}},"output":[],"output_node":true}});
    let draft = db.prepare_job(&id, &[edit], &info).unwrap();
    assert!(draft.request_json.contains("18446744073709551615"));
    assert_eq!(
        db.workflow(&id)
            .unwrap()
            .fields
            .iter()
            .find(|f| f.input_name == "text")
            .unwrap()
            .value,
        "original"
    );
}
