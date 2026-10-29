use serde_json::{json, Value};
use studio_core::{
    couple::SplitEdit,
    scenes::{Composition, SceneManifest, ScenePerson, SceneText, SceneTextSource, SceneZone},
    workflow::Edit,
    Database,
};

/// Full six-section couple graph with one SayaSplitMask, following the
/// existing couple.rs fixture style.
fn couple_graph() -> Value {
    json!({
        "p1":{"class_type":"Text","inputs":{"text":"old p1"}},
        "p2":{"class_type":"Text","inputs":{"text":"old p2"}},
        "action":{"class_type":"Text","inputs":{"text":"old action"}},
        "bg":{"class_type":"Text","inputs":{"text":"old bg"}},
        "quality":{"class_type":"Text","inputs":{"text":"old quality"}},
        "neg":{"class_type":"Text","inputs":{"text":"old neg"}},
        "latent":{"class_type":"Latent","inputs":{}},
        "mask":{"class_type":"SayaSplitMask","inputs":{"direction":"vertical","split":50,"blur":0.0,"swap":false,"latent":["latent",0]}},
        "out":{"class_type":"Output","inputs":{"p1":["p1",0],"p2":["p2",0],"mask1":["mask",0],"mask2":["mask",1]}}
    })
}

/// Synthetic engine schema; preparation validates against it without any server.
fn schema() -> Value {
    json!({
        "Text":{"input":{"required":{"text":["STRING"]}},"output":["CONDITIONING"]},
        "Latent":{"input":{"required":{}},"output":["LATENT"]},
        "SayaSplitMask":{"input":{"required":{"direction":[["vertical","horizontal"]],"split":["INT",{"min":0,"max":100}],"blur":["FLOAT",{"min":0}],"swap":["BOOLEAN"],"latent":["LATENT"]}},"output":["MASK","MASK"]},
        "Output":{"input":{"required":{"p1":["CONDITIONING"],"p2":["CONDITIONING"],"mask1":["MASK"],"mask2":["MASK"]}},"output":[],"output_node":true}
    })
}

/// Bind every primitive input: text nodes receive their section, SayaSplitMask
/// inputs are bound as free fields exactly like the couple.rs fixture.
fn bind(db: &mut Database, id: &str, sections: &[(&str, &str)]) {
    let fields = db.workflow(id).unwrap().fields;
    let bindings: Vec<_> = fields
        .into_iter()
        .map(|f| Edit {
            section: sections
                .iter()
                .find(|(node, _)| node == &f.node_id.as_str())
                .map(|(_, section)| section.to_string()),
            node_id: f.node_id,
            input_name: f.input_name,
            value: f.value,
        })
        .collect();
    db.save_bindings(id, &bindings).unwrap();
}

fn populate(mut db: Database) -> (Database, String, String, String, String) {
    let workflow = db
        .import_workflow("Couple scenes", &couple_graph().to_string())
        .unwrap();
    bind(
        &mut db,
        &workflow,
        &[
            ("p1", "P1"),
            ("p2", "P2"),
            ("action", "ACTION"),
            ("bg", "BG"),
            ("quality", "QUALITY"),
            ("neg", "NEG"),
        ],
    );
    let c1 = db
        .save_character(
            None,
            "Mira".into(),
            json!({"identity_prompt":"exact,  tags\ntrailing line"}),
        )
        .unwrap();
    let v1 = db
        .save_variant(
            &c1,
            None,
            "Sport",
            json!({
                "P1":"(silver hair:1.2),  exact\nsecond line",
                "ACTION":"holds (a rose:1.1)\n  gently,  warm",
                "BG":"night market,  (wet:0.9)",
                "QUALITY":"",
                "NEG":""
            }),
            json!({}),
        )
        .unwrap();
    // Second sheet uses the legacy identity key and has no variant.
    let c2 = db
        .save_character(
            None,
            "Noa".into(),
            json!({"identityPrompt":"legacy,  exact"}),
        )
        .unwrap();
    (db, workflow, c1, v1, c2)
}

fn fixture() -> (Database, String, String, String, String) {
    populate(Database::in_memory().unwrap())
}

fn fixture_at(path: &std::path::Path) -> (Database, String, String, String, String) {
    populate(Database::open(path).unwrap())
}

fn person(character: &str, variant: Option<&str>) -> ScenePerson {
    ScenePerson {
        character_id: character.into(),
        variant_id: variant.map(str::to_owned),
        zone: None,
    }
}

fn scene_text(source: SceneTextSource, from: Option<&str>, text: &str) -> SceneText {
    SceneText {
        source,
        from: from.map(str::to_owned),
        text: text.into(),
    }
}

fn mask() -> SplitEdit {
    SplitEdit {
        node_id: "mask".into(),
        direction: "horizontal".into(),
        split: 35,
        blur: 1.5,
        swap: true,
    }
}

/// Full couple scene: variant identity, sheet identity, preset action,
/// custom decor, existing quality, custom negative, remapped mask.
fn couple_scene(workflow: &str, c1: &str, v1: &str, c2: &str) -> SceneManifest {
    SceneManifest {
        grok_source: None,
        workflow_id: workflow.into(),
        composition: Composition::Couple,
        p1: person(c1, Some(v1)),
        p2: Some(person(c2, None)),
        action: scene_text(SceneTextSource::Preset, Some("P1"), ""),
        bg: scene_text(
            SceneTextSource::Custom,
            None,
            "night market,\n(wet cobblestones:0.9)",
        ),
        quality: Some(scene_text(SceneTextSource::Workflow, None, "")),
        negative: Some(scene_text(
            SceneTextSource::Custom,
            None,
            "lowres,  worst quality",
        )),
        mask: Some(mask()),
    }
}

#[test]
fn scene_assembles_prompts_with_exact_bytes_from_sheets_variants_and_custom() {
    let (db, workflow, c1, v1, c2) = fixture();
    let manifest = couple_scene(&workflow, &c1, &v1, &c2);
    let preview = db.preview_scene(&manifest).unwrap();
    assert_eq!(preview["workflow"]["promptProfile"], "couple");
    assert_eq!(preview["composition"], "couple");
    assert_eq!(
        preview["prompts"]["P1"]["text"],
        "(silver hair:1.2),  exact\nsecond line"
    );
    assert_eq!(preview["prompts"]["P2"]["text"], "legacy,  exact");
    assert_eq!(
        preview["prompts"]["ACTION"]["text"],
        "holds (a rose:1.1)\n  gently,  warm"
    );
    assert_eq!(
        preview["prompts"]["BG"]["text"],
        "night market,\n(wet cobblestones:0.9)"
    );
    assert_eq!(preview["prompts"]["QUALITY"]["text"], "old quality");
    assert_eq!(preview["prompts"]["QUALITY"]["changed"], false);
    assert_eq!(preview["prompts"]["NEG"]["text"], "lowres,  worst quality");
    for section in ["P1", "P2", "ACTION", "BG", "NEG"] {
        assert_eq!(preview["prompts"][section]["changed"], true, "{section}");
        assert_eq!(
            preview["prompts"][section]["before"],
            format!("old {}", section.to_lowercase())
        );
    }
    let roles: Vec<_> = preview["characters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["P1", "P2"]);
    assert_eq!(preview["characters"][0]["name"], "Mira");
    assert_eq!(preview["characters"][0]["variant"]["id"], v1);
    assert!(preview["characters"][1]["variant"].is_null());
    let jobs: Vec<_> = preview["jobCharacters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["role"].as_str().unwrap(),
                p["characterId"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(jobs, [("P1", c1.as_str()), ("P2", c2.as_str())]);
    // Every planned change is listed, mask fields included; the no-op QUALITY
    // rewrite is not announced as a change.
    let changed: Vec<_> = preview["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["section"].as_str(),
                c["nodeId"].as_str().unwrap(),
                c["inputName"].as_str().unwrap(),
            )
        })
        .collect();
    assert!(changed.contains(&(Some("P1"), "p1", "text")));
    assert!(changed.contains(&(Some("NEG"), "neg", "text")));
    assert!(changed.contains(&(None, "mask", "direction")));
    assert!(changed.contains(&(None, "mask", "split")));
    assert!(changed.contains(&(None, "mask", "blur")));
    assert!(changed.contains(&(None, "mask", "swap")));
    assert_eq!(changed.len(), 9);
    assert_eq!(preview["mask"]["direction"], "horizontal");
}

#[test]
fn solo_scene_refuses_p2_and_couple_scene_requires_p2() {
    let (mut db, workflow, c1, v1, c2) = fixture();
    let couple = couple_scene(&workflow, &c1, &v1, &c2);
    // COUPLE without a second person: refused, matching couple_edits' contract
    // that every mapped identity needs a non-empty prompt.
    let mut without_p2 = couple.clone();
    without_p2.p2 = None;
    let error = db.preview_scene(&without_p2).unwrap_err().to_string();
    assert!(error.contains("COUPLE"), "{error}");
    // SOLO on a couple-profile workflow: no verified single-identity mapping.
    let mut solo_on_couple = couple.clone();
    solo_on_couple.composition = Composition::Solo;
    let error = db.preview_scene(&solo_on_couple).unwrap_err().to_string();
    assert!(error.contains("SOLO"), "{error}");
    // SOLO must not carry P2.
    let mut solo = couple.clone();
    solo.composition = Composition::Solo;
    solo.p2 = Some(person(&c2, None));
    let error = db.preview_scene(&solo).unwrap_err().to_string();
    assert!(error.contains("P2"), "{error}");
    // COUPLE on a solo-profile workflow: P2 is not mapped, no internal mode invented.
    let solo_workflow = db
        .import_workflow(
            "Solo scenes",
            &json!({"p1":{"class_type":"Text","inputs":{"text":"one"}}}).to_string(),
        )
        .unwrap();
    bind(&mut db, &solo_workflow, &[("p1", "P1")]);
    let mut couple_on_solo = couple.clone();
    couple_on_solo.workflow_id = solo_workflow.clone();
    let error = db.preview_scene(&couple_on_solo).unwrap_err().to_string();
    assert!(error.contains("P2 n'est pas mapp"), "{error}");
    // A SOLO scene restricted to the solo workflow's mappings is accepted.
    let mut solo_ok = couple.clone();
    solo_ok.composition = Composition::Solo;
    solo_ok.workflow_id = solo_workflow;
    solo_ok.p2 = None;
    solo_ok.action = scene_text(SceneTextSource::Workflow, None, "");
    solo_ok.bg = scene_text(SceneTextSource::Workflow, None, "");
    solo_ok.quality = None;
    solo_ok.negative = None;
    solo_ok.mask = None;
    let preview = db.preview_scene(&solo_ok).unwrap();
    assert_eq!(preview["workflow"]["promptProfile"], "solo");
    assert_eq!(
        preview["prompts"]["P1"]["text"],
        "(silver hair:1.2),  exact\nsecond line"
    );
    assert!(preview["prompts"].get("P2").is_none());
}

#[test]
fn scene_validation_refuses_missing_mappings_and_incompatible_choices() {
    let (mut db, workflow, c1, v1, c2) = fixture();
    let couple = couple_scene(&workflow, &c1, &v1, &c2);
    // Unconfigured workflow: nothing may be invented.
    let raw = db
        .import_workflow(
            "Raw scenes",
            &json!({"p1":{"class_type":"Text","inputs":{"text":"one"}}}).to_string(),
        )
        .unwrap();
    let mut unconfigured = couple.clone();
    unconfigured.workflow_id = raw;
    unconfigured.composition = Composition::Solo;
    unconfigured.p2 = None;
    let error = db.preview_scene(&unconfigured).unwrap_err().to_string();
    assert!(error.contains("mapping P1"), "{error}");
    // Unknown workflow.
    let mut unknown = couple.clone();
    unknown.workflow_id = "missing".into();
    assert!(db.preview_scene(&unknown).is_err());
    // Foreign variant and unknown character are refused by snapshot validation.
    let mut foreign = couple.clone();
    foreign.p1 = person(&c2, Some(&v1));
    assert!(db.preview_scene(&foreign).is_err());
    let mut missing = couple.clone();
    missing.p1 = person("nobody", None);
    assert!(db.preview_scene(&missing).is_err());
    // Custom text without a saved section mapping is refused with a diagnostic.
    let solo_workflow = db
        .import_workflow(
            "Solo mapped",
            &json!({"p1":{"class_type":"Text","inputs":{"text":"one"}}}).to_string(),
        )
        .unwrap();
    bind(&mut db, &solo_workflow, &[("p1", "P1")]);
    let mut unmapped = couple.clone();
    unmapped.workflow_id = solo_workflow.clone();
    unmapped.composition = Composition::Solo;
    unmapped.p2 = None;
    unmapped.action = scene_text(SceneTextSource::Custom, None, "a custom action");
    unmapped.bg = scene_text(SceneTextSource::Workflow, None, "");
    unmapped.quality = None;
    unmapped.negative = None;
    unmapped.mask = None;
    let error = db.preview_scene(&unmapped).unwrap_err().to_string();
    assert!(
        error.contains("ACTION") && error.contains("mapping"),
        "{error}"
    );
    // Preset diagnostics: missing role, missing variant, missing block, solo scope.
    let mut no_role = unmapped.clone();
    no_role.action = scene_text(SceneTextSource::Preset, None, "");
    assert!(db.preview_scene(&no_role).is_err());
    let mut no_variant = unmapped.clone();
    no_variant.action = scene_text(SceneTextSource::Preset, Some("P1"), "");
    no_variant.p1 = person(&c2, None);
    let error = db.preview_scene(&no_variant).unwrap_err().to_string();
    assert!(error.contains("aucune variante"), "{error}");
    // A variant without the requested section is refused, no block invented.
    let v2 = db
        .save_variant(&c1, None, "Bare", json!({"P1":"(bare:1.0)"}), json!({}))
        .unwrap();
    let mut no_block = unmapped.clone();
    no_block.p1 = person(&c1, Some(&v2));
    no_block.bg = scene_text(SceneTextSource::Preset, Some("P1"), "");
    let error = db.preview_scene(&no_block).unwrap_err().to_string();
    assert!(
        error.contains("BG") && error.contains("n'enregistre pas"),
        "{error}"
    );
    let mut from_p2 = unmapped.clone();
    from_p2.action = scene_text(SceneTextSource::Preset, Some("P2"), "");
    let error = db.preview_scene(&from_p2).unwrap_err().to_string();
    assert!(error.contains("SOLO") && error.contains("P1"), "{error}");
    // Mask diagnostics: unmapped target, invalid geometry, invalid direction.
    let mut bad_mask = couple.clone();
    bad_mask.mask.as_mut().unwrap().node_id = "p1".into();
    let error = db.preview_scene(&bad_mask).unwrap_err().to_string();
    assert!(error.contains("SayaSplitMask"), "{error}");
    let mut bad_split = couple.clone();
    bad_split.mask.as_mut().unwrap().split = 101;
    let error = db.preview_scene(&bad_split).unwrap_err().to_string();
    assert!(error.contains("SayaSplitMask"), "{error}");
    let mut bad_direction = couple.clone();
    bad_direction.mask.as_mut().unwrap().direction = "diagonal".into();
    assert!(db.preview_scene(&bad_direction).is_err());
    // The typed contract refuses unknown fields instead of ignoring them.
    let mut bogus = serde_json::to_value(&couple).unwrap();
    bogus["invented"] = json!(1);
    assert!(serde_json::from_value::<SceneManifest>(bogus).is_err());
}

#[test]
fn explicit_zones_roundtrip_without_changing_prompts_or_variants() {
    let (db, workflow, c1, v1, c2) = fixture();
    let mut manifest = couple_scene(&workflow, &c1, &v1, &c2);
    manifest.p1.zone = Some(SceneZone {
        x: 0.05,
        y: 0.1,
        width: 0.4,
        height: 0.8,
        feather: 0.12,
        enabled: true,
        z: 1,
    });
    manifest.p2.as_mut().unwrap().zone = Some(SceneZone {
        x: 0.55,
        y: 0.1,
        width: 0.4,
        height: 0.8,
        feather: 0.2,
        enabled: false,
        z: 2,
    });
    let before = db.preview_scene(&manifest).unwrap();
    let id = db.save_scene(None, "Zones", &manifest).unwrap();
    let stored = db.scene(&id).unwrap();
    assert_eq!(stored["manifest"]["p1"]["zone"]["x"], 0.05);
    assert_eq!(stored["manifest"]["p2"]["zone"]["feather"], 0.2);
    assert_eq!(stored["manifest"]["p1"]["variantId"], v1);
    let after = db
        .preview_scene(&serde_json::from_value(stored["manifest"].clone()).unwrap())
        .unwrap();
    assert_eq!(before["prompts"], after["prompts"]);
    assert_eq!(before["jobCharacters"], after["jobCharacters"]);
    assert_eq!(after["zones"]["P1"]["enabled"], true);
    assert_eq!(after["zones"]["P2"]["enabled"], false);
    assert_eq!(after["zones"]["P1"]["z"], 1);
    assert_eq!(after["zoneOrder"]["P2"], 2);

    let mut invalid = manifest.clone();
    invalid.p1.zone.as_mut().unwrap().x = 0.9;
    let error = db
        .save_scene(None, "Invalid", &invalid)
        .unwrap_err()
        .to_string();
    assert!(error.contains("Zone P1"), "{error}");
    assert!(db.scene(&id).unwrap()["manifest"]["p1"]["zone"]["x"] == 0.05);
}

#[test]
fn overlapping_zones_report_visibility_without_touching_prompts() {
    let (db, workflow, c1, v1, c2) = fixture();
    let mut manifest = couple_scene(&workflow, &c1, &v1, &c2);
    manifest.p1.zone = Some(SceneZone {
        x: 0.1,
        y: 0.1,
        width: 0.6,
        height: 0.8,
        feather: 0.0,
        enabled: true,
        z: 2,
    });
    manifest.p2.as_mut().unwrap().zone = Some(SceneZone {
        x: 0.3,
        y: 0.2,
        width: 0.6,
        height: 0.7,
        feather: 0.0,
        enabled: true,
        z: 7,
    });
    let before = db.preview_scene(&manifest).unwrap();
    let overlaps = before["overlaps"].as_array().unwrap();
    assert_eq!(overlaps.len(), 1);
    assert_eq!(overlaps[0]["visible"], "P2");
    assert!(overlaps[0]["area"].as_f64().unwrap() > 0.0);
    assert_eq!(before["zoneOrder"]["P1"], 2);
    assert_eq!(before["zoneOrder"]["P2"], 7);
    let after_prompts = db.preview_scene(&manifest).unwrap()["prompts"].clone();
    assert_eq!(before["prompts"], after_prompts);
    let mut swapped = manifest.clone();
    swapped.p1.zone.as_mut().unwrap().z = 9;
    assert_eq!(
        db.preview_scene(&swapped).unwrap()["overlaps"][0]["visible"],
        "P1"
    );
    let id = db.save_scene(None, "Overlap", &manifest).unwrap();
    let stored = db.scene(&id).unwrap();
    assert_eq!(stored["manifest"]["p2"]["zone"]["z"], 7);
}

#[test]
fn scene_manifest_roundtrip_persists_duplicates_updates_and_deletes() {
    let directory = std::env::temp_dir().join(format!("studio-scenes-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("studio.sqlite3");
    let (db, workflow, c1, v1, c2) = fixture_at(&path);
    let manifest = couple_scene(&workflow, &c1, &v1, &c2);
    let id = db.save_scene(None, "  Market scene  ", &manifest).unwrap();
    let stored = db.scene(&id).unwrap();
    assert_eq!(stored["name"], "Market scene");
    let reopened: SceneManifest = serde_json::from_value(stored["manifest"].clone()).unwrap();
    assert_eq!(reopened, manifest);
    assert_eq!(db.scenes().unwrap().len(), 1);
    // Reopen the database: the manifest survives.
    drop(db);
    let db = Database::open(&path).unwrap();
    assert_eq!(db.scene(&id).unwrap()["manifest"]["composition"], "couple");
    // Duplicate is an explicit copy under a new identity.
    let copy = db.duplicate_scene(&id).unwrap();
    assert_ne!(copy, id);
    assert_eq!(db.scene(&copy).unwrap()["name"], "Market scene (copie)");
    // Updating a copy leaves the original untouched.
    let mut edited = manifest.clone();
    edited.bg = scene_text(SceneTextSource::Custom, None, "desert canyon");
    db.save_scene(Some(&copy), "Market scene v2", &edited)
        .unwrap();
    assert_eq!(db.scene(&id).unwrap()["name"], "Market scene");
    assert_eq!(
        serde_json::from_value::<SceneManifest>(db.scene(&id).unwrap()["manifest"].clone())
            .unwrap()
            .bg
            .text,
        "night market,\n(wet cobblestones:0.9)"
    );
    // Invalid names and unknown ids are refused without partial writes.
    assert!(db.save_scene(None, "   ", &manifest).is_err());
    assert!(db.save_scene(Some("missing"), "x", &manifest).is_err());
    assert!(db.scene("missing").is_err());
    assert!(db.delete_scene("missing").is_err());
    db.delete_scene(&id).unwrap();
    assert_eq!(db.scenes().unwrap().len(), 1);
    assert!(db.scene(&id).is_err());
    drop(db);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn scene_preview_mutates_neither_workflow_nor_jobs() {
    let directory =
        std::env::temp_dir().join(format!("studio-scenes-preview-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("studio.sqlite3");
    let (db, workflow, c1, v1, c2) = fixture_at(&path);
    let manifest = couple_scene(&workflow, &c1, &v1, &c2);
    drop(db);
    // Independent read-only inspection of the persisted state.
    let witness = || -> (String, i64) {
        let sql = rusqlite::Connection::open_with_flags(
            &path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let document: String = sql
            .query_row(
                "SELECT document_json FROM workflow_revisions WHERE id=?",
                [&workflow],
                |row| row.get(0),
            )
            .unwrap();
        let jobs: i64 = sql
            .query_row("SELECT count(*) FROM generation_jobs", [], |row| row.get(0))
            .unwrap();
        (document, jobs)
    };
    let (before, zero) = witness();
    let db = Database::open(&path).unwrap();
    let first = db.preview_scene(&manifest).unwrap();
    let second = db.preview_scene(&manifest).unwrap();
    assert_eq!(first, second);
    // scene_edits is equally read-only.
    let (edits, people) = db.scene_edits(&manifest).unwrap();
    assert_eq!(edits.len(), 10);
    assert_eq!(people.len(), 2);
    drop(db);
    assert_eq!(witness(), (before, zero));
    assert_eq!(zero, 0);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn scene_prepares_runner_compatible_request_without_any_submission() {
    let directory =
        std::env::temp_dir().join(format!("studio-scenes-prepare-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let (db, workflow, c1, v1, c2) = fixture_at(&directory.join("studio.sqlite3"));
    let manifest = couple_scene(&workflow, &c1, &v1, &c2);
    let (edits, people) = db.scene_edits(&manifest).unwrap();
    let prepared = db
        .prepare_job_with_characters(&workflow, &edits, &schema(), &people)
        .unwrap();
    let request: Value = serde_json::from_str(&prepared.request_json).unwrap();
    let mut expected = couple_graph();
    expected["p1"]["inputs"]["text"] = json!("(silver hair:1.2),  exact\nsecond line");
    expected["p2"]["inputs"]["text"] = json!("legacy,  exact");
    expected["action"]["inputs"]["text"] = json!("holds (a rose:1.1)\n  gently,  warm");
    expected["bg"]["inputs"]["text"] = json!("night market,\n(wet cobblestones:0.9)");
    expected["neg"]["inputs"]["text"] = json!("lowres,  worst quality");
    expected["mask"]["inputs"]["direction"] = json!("horizontal");
    expected["mask"]["inputs"]["split"] = json!(35);
    expected["mask"]["inputs"]["blur"] = json!(1.5);
    expected["mask"]["inputs"]["swap"] = json!(true);
    assert_eq!(request["prompt"], expected);
    let roles: Vec<String> = request["characters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["role"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(roles, ["P1", "P2"]);
    // The imported workflow copy stays structurally identical and the job
    // remains a draft without any engine submission.
    let witness = rusqlite::Connection::open_with_flags(
        directory.join("studio.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let document: String = witness
        .query_row(
            "SELECT document_json FROM workflow_revisions WHERE id=?",
            [&workflow],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&document).unwrap(),
        couple_graph()
    );
    let (state, prompt_id): (String, Option<String>) = witness
        .query_row(
            "SELECT state,comfy_prompt_id FROM generation_jobs WHERE id=?",
            [prepared.job_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "draft");
    assert_eq!(prompt_id, None);
    drop(db);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn saved_scene_is_revalidated_against_current_mappings_at_preview_time() {
    let (mut db, _couple, c1, v1, _c2) = fixture();
    // Solo workflow with its own saved mapping, without P2 and without a mask.
    let workflow = db
        .import_workflow(
            "Solo market",
            &json!({
                "p1":{"class_type":"Text","inputs":{"text":"old p1"}},
                "action":{"class_type":"Text","inputs":{"text":"old action"}},
                "bg":{"class_type":"Text","inputs":{"text":"old bg"}},
                "quality":{"class_type":"Text","inputs":{"text":"old quality"}},
                "neg":{"class_type":"Text","inputs":{"text":"old neg"}}
            })
            .to_string(),
        )
        .unwrap();
    bind(
        &mut db,
        &workflow,
        &[
            ("p1", "P1"),
            ("action", "ACTION"),
            ("bg", "BG"),
            ("quality", "QUALITY"),
            ("neg", "NEG"),
        ],
    );
    let manifest = SceneManifest {
        grok_source: None,
        workflow_id: workflow.clone(),
        composition: Composition::Solo,
        p1: person(&c1, Some(&v1)),
        p2: None,
        action: scene_text(SceneTextSource::Workflow, None, ""),
        bg: scene_text(SceneTextSource::Preset, Some("P1"), ""),
        quality: None,
        negative: None,
        mask: None,
    };
    let id = db.save_scene(None, "Solo market", &manifest).unwrap();
    assert!(db.preview_scene(&manifest).is_ok());
    // The mapping is re-saved without BG: the saved scene now refuses with a diagnostic.
    let fields = db.workflow(&workflow).unwrap().fields;
    let bindings: Vec<_> = fields
        .into_iter()
        .map(|f| Edit {
            section: match f.node_id.as_str() {
                "p1" => Some("P1".into()),
                "action" => Some("ACTION".into()),
                "quality" => Some("QUALITY".into()),
                "neg" => Some("NEG".into()),
                _ => None,
            },
            node_id: f.node_id,
            input_name: f.input_name,
            value: f.value,
        })
        .collect();
    db.save_bindings(&workflow, &bindings).unwrap();
    let error = db.preview_scene(&manifest).unwrap_err().to_string();
    assert!(error.contains("BG") && error.contains("mapping"), "{error}");
    // Editing happens on a copy: switch the copy to the workflow source.
    let copy = db.duplicate_scene(&id).unwrap();
    let mut edited = manifest.clone();
    edited.bg = scene_text(SceneTextSource::Workflow, None, "");
    db.save_scene(Some(&copy), "Solo market v2", &edited)
        .unwrap();
    let preview = db.preview_scene(&edited).unwrap();
    // BG is no longer mapped at all: it leaves the preview instead of being invented.
    assert!(preview["prompts"].get("BG").is_none());
    // The original saved scene is untouched and still refuses.
    let stored: SceneManifest =
        serde_json::from_value(db.scene(&id).unwrap()["manifest"].clone()).unwrap();
    assert_eq!(stored.bg.source, SceneTextSource::Preset);
    assert!(db.preview_scene(&stored).is_err());
}

/// Every difference between the stored graph and the prepared request must be
/// one of the declared changes: nothing else in the graph may move.
fn assert_only_declared_changes(original: &Value, prepared: &Value, changes: &[Value]) {
    let mut differences = Vec::new();
    studio_core::recipes::diff_values("", original, prepared, &mut differences);
    for difference in &differences {
        let path = difference["path"].as_str().unwrap();
        // diff_values paths are `.node.inputs.input` from an empty root.
        assert!(
            changes.iter().any(|c| path
                == format!(
                    ".{}.inputs.{}",
                    c["node"].as_str().unwrap(),
                    c["input"].as_str().unwrap()
                )),
            "undeclared graph change at {path}: {difference}"
        );
    }
    assert!(
        !changes.is_empty(),
        "a scene that changes nothing proves nothing"
    );
    assert_eq!(
        differences.len(),
        changes.len(),
        "{differences:?} vs {changes:?}"
    );
}

#[test]
fn solo_and_couple_scenes_prepare_only_declared_changes() {
    let (mut db, couple_workflow, c1, v1, c2) = fixture();
    // SOLO workflow: same graph, only P1/ACTION/BG/NEG mapped (P2 node left alone).
    let solo_workflow = db
        .import_workflow("Solo prep", &couple_graph().to_string())
        .unwrap();
    bind(
        &mut db,
        &solo_workflow,
        &[
            ("p1", "P1"),
            ("action", "ACTION"),
            ("bg", "BG"),
            ("neg", "NEG"),
        ],
    );
    let mut solo = couple_scene(&solo_workflow, &c1, &v1, &c2);
    solo.composition = Composition::Solo;
    solo.p2 = None;
    solo.quality = None;
    solo.mask = None;
    for (manifest, workflow, roles) in [
        (solo, solo_workflow.clone(), vec!["P1"]),
        (
            couple_scene(&couple_workflow, &c1, &v1, &c2),
            couple_workflow.clone(),
            vec!["P1", "P2"],
        ),
    ] {
        let (edits, people) = db.scene_edits(&manifest).unwrap();
        let prepared = db
            .prepare_job_with_characters(&workflow, &edits, &schema(), &people)
            .unwrap();
        let request: Value = serde_json::from_str(&prepared.request_json).unwrap();
        assert_only_declared_changes(&couple_graph(), &request["prompt"], &prepared.changes);
        assert_eq!(
            people.iter().map(|p| p.role.as_str()).collect::<Vec<_>>(),
            roles
        );
        if roles.len() == 1 {
            assert_eq!(
                request["prompt"]["p2"]["inputs"]["text"], "old p2",
                "unmapped P2 must stay intact"
            );
            assert_eq!(
                request["prompt"]["p1"]["inputs"]["text"],
                "(silver hair:1.2),  exact\nsecond line"
            );
        }
    }
}

#[test]
fn imported_scene_keeps_source_and_refuses_unresolved_templates_in_both_paths() {
    let (db, workflow, c1, v1, c2) = fixture();
    let file = std::env::temp_dir().join(format!("saya-grok-source-{}.json", uuid::Uuid::new_v4()));
    std::fs::write(
        &file,
        json!([{"id":"scene","bg":"{accent} garden","pose":" exact\n"}]).to_string(),
    )
    .unwrap();
    let entry = studio_core::grok_scenes::parse_scenes(&file)
        .unwrap()
        .0
        .remove(0);
    let ids = db
        .grok_scene_import(
            &[studio_core::grok_scenes::SceneChoice {
                path: entry.path,
                sha256: entry.sha256,
                key: entry.key,
            }],
            true,
        )
        .unwrap();
    let mut manifest = couple_scene(&workflow, &c1, &v1, &c2);
    manifest.grok_source = Some(studio_core::scenes::GrokSceneSource {
        scene_id: ids[0].clone(),
        variant_index: 0,
    });
    manifest.bg = scene_text(SceneTextSource::Custom, None, "{accent} garden");
    assert!(db
        .preview_scene(&manifest)
        .unwrap_err()
        .to_string()
        .contains("{accent}"));
    assert!(db.scene_edits(&manifest).is_err());
    manifest.bg.text = " gold garden\n".into();
    db.preview_scene(&manifest).unwrap();
    let id = db.save_scene(None, "Imported", &manifest).unwrap();
    assert_eq!(
        db.scene(&id).unwrap()["manifest"]["grokSource"]["sceneId"],
        ids[0]
    );
    std::fs::remove_file(file).unwrap();
}
