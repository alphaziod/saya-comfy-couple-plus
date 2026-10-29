use serde_json::{json, Value};
use studio_core::{workflow_split::Boundary, Database};
fn info() -> Value {
    serde_json::from_str(include_str!(
        "../../../scripts/fixtures/object_info-subset.json"
    ))
    .unwrap()
}

fn api() -> Value {
    json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"fixture-ckpt_name.safetensors"}},
        "2":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"exact,  double\nligne"}},
        "3":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"neg"}},
        "4":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},
        "5":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["4",0],"seed":18446744073709551615u64,"steps":20,"cfg":6.5,"sampler_name":"euler","scheduler":"normal","denoise":1.0}},
        "6":{"class_type":"VAEDecode","inputs":{"samples":["5",0],"vae":["1",2]}},
        "7":{"class_type":"SaveImage","inputs":{"images":["6",0],"filename_prefix":"x"}}
    })
}

#[test]
fn explicit_image_split_retains_dependencies_prompts_and_source_atomically() {
    let dir = std::env::temp_dir().join(format!("saya-split-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let db = Database::open(&dir.join("db.sqlite")).unwrap();
    let source = api();
    let id = db.import_workflow("source", &source.to_string()).unwrap();
    let cuts = vec![Boundary {
        node: "6".into(),
        slot: 0,
    }];
    let preview = db.split_preview(&id, &cuts, "7", &info()).unwrap();
    let plans = preview["plan"]["passes"].as_array().unwrap();
    assert_eq!(plans.len(), 2);
    let first: Value = serde_json::from_str(plans[0]["apiJson"].as_str().unwrap()).unwrap();
    let second: Value = serde_json::from_str(plans[1]["apiJson"].as_str().unwrap()).unwrap();
    assert_eq!(first["2"]["inputs"]["text"], source["2"]["inputs"]["text"]);
    assert_eq!(first["5"]["inputs"]["seed"], json!(18446744073709551615u64));
    assert_eq!(second["saya_input_1"]["class_type"], "LoadImage");
    assert_eq!(second["7"]["inputs"]["images"], json!(["saya_input_1", 0]));
    assert_eq!(
        first["saya_output"]["_meta"]["sayaSplit"]["sourceWorkflow"],
        id
    );
    let hash = preview["previewHash"].as_str().unwrap();
    assert!(db
        .split_save(&id, &cuts, "7", "split", hash, false, &info())
        .is_err());
    assert!(db
        .split_save(&id, &cuts, "7", "split", "stale", true, &info())
        .is_err());
    assert!(db
        .split_save(&id, &cuts, "7", "invalid\nname", hash, true, &info())
        .is_err());
    assert_eq!(db.workflows().unwrap().len(), 1, "transaction rolled back");
    let saved = db
        .split_save(&id, &cuts, "7", "split", hash, true, &info())
        .unwrap();
    assert_eq!(db.workflows().unwrap().len(), 3);
    assert_eq!(db.pipelines().unwrap().len(), 1);
    let manifest: Value = serde_json::from_str(saved["manifestJson"].as_str().unwrap()).unwrap();
    assert_eq!(manifest["version"], 2);
    assert_eq!(db.workflow_document(&id).unwrap().1, source.to_string());
    let latent = db
        .split_preview(
            &id,
            &[Boundary {
                node: "5".into(),
                slot: 0,
            }],
            "7",
            &info(),
        )
        .unwrap();
    let latent_passes = latent["plan"]["passes"].as_array().unwrap();
    assert_eq!(latent_passes.len(), 2);
    let first_latent: Value =
        serde_json::from_str(latent_passes[0]["apiJson"].as_str().unwrap()).unwrap();
    let second_latent: Value =
        serde_json::from_str(latent_passes[1]["apiJson"].as_str().unwrap()).unwrap();
    assert_eq!(first_latent["saya_output"]["class_type"], "SaveLatent");
    assert_eq!(second_latent["saya_input_1"]["class_type"], "LoadLatent");
    assert_eq!(
        second_latent["saya_input_1"]["inputs"]["latent"],
        "saya-pipeline-awaiting-latent.latent"
    );
    assert!(db
        .split_preview(
            &id,
            &[Boundary {
                node: "2".into(),
                slot: 0
            }],
            "7",
            &info()
        )
        .unwrap_err()
        .to_string()
        .contains("CONDITIONING"));
    assert!(db
        .split_preview(&id, &[cuts[0].clone(), cuts[0].clone()], "7", &info())
        .is_err());
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn secondary_latent_dependency_is_not_silently_recomputed() {
    let dir = std::env::temp_dir().join(format!("saya-split-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let db = Database::open(&dir.join("db.sqlite")).unwrap();
    let mut source = api();
    source["8"] = source["6"].clone();
    source["9"] = json!({"class_type":"ImageBatch","inputs":{"image1":["6",0],"image2":["8",0]}});
    source["7"]["inputs"]["images"] = json!(["9", 0]);
    let mut schema = info();
    schema["ImageBatch"] =
        json!({"input":{"required":{"image1":["IMAGE"],"image2":["IMAGE"]}},"output":["IMAGE"]});
    let id = db.import_workflow("source", &source.to_string()).unwrap();
    let error = db
        .split_preview(
            &id,
            &[Boundary {
                node: "6".into(),
                slot: 0,
            }],
            "7",
            &schema,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("déjà produit"), "{error}");
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ui_reference_is_converted_and_split_with_fidelity_audit() {
    let dir = std::env::temp_dir().join(format!("saya-split-ui-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let db = Database::open(&dir.join("db.sqlite")).unwrap();
    let ui = include_str!("../../../scripts/fixtures/v15.2-subgraph-ui.json");
    let converted = studio_core::workflow_ui_api::convert_ui(ui, Some(&info())).unwrap();
    assert_eq!(converted["report"]["complete"], true);
    let api = converted["api"].clone();
    let final_node = api
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, n)| n["class_type"] == "SaveImage")
        .map(|(id, _)| id.clone())
        .unwrap();
    let source = api[&final_node]["inputs"]["images"][0]
        .as_str()
        .unwrap()
        .to_owned();
    let slot = api[&final_node]["inputs"]["images"][1].as_u64().unwrap() as usize;
    let preview = db
        .split_ui_preview(ui, &[Boundary { node: source, slot }], &final_node, &info())
        .unwrap();
    assert_eq!(preview["passes"].as_array().unwrap().len(), 2);
    assert_eq!(preview["fidelity"]["missing"], json!([]));
    assert_eq!(preview["fidelity"]["mutated"], json!([]));
    assert_eq!(preview["report"]["complete"], true);
    assert!(preview["referenceApi"].as_object().unwrap().len() >= 2);
    // The UI source is only read; no workflow revision was created.
    assert_eq!(db.workflows().unwrap().len(), 0);
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "reads the user's original UI workflow read-only via SAYA_ORIGINAL_WORKFLOW"]
fn original_ui_workflow_converts_to_a_complete_api_reference() {
    let Ok(path) = std::env::var("SAYA_ORIGINAL_WORKFLOW") else {
        eprintln!("SAYA_ORIGINAL_WORKFLOW absent ; test original ignoré");
        return;
    };
    let source = std::fs::read_to_string(&path).unwrap();
    let before = studio_core::gallery::digest(source.as_bytes());
    let converted = studio_core::workflow_ui_api::convert_ui(&source, None).unwrap();
    assert_eq!(converted["report"]["complete"], true);
    assert_eq!(converted["report"]["unresolved"], json!([]));
    assert_eq!(converted["report"]["apiNodes"], 75);
    assert_eq!(
        studio_core::gallery::digest(std::fs::read(&path).unwrap().as_slice()),
        before
    );
}
