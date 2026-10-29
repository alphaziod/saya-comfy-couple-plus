//! F13: universal profile/mapping persistence and preparation on a fixture
//! workflow (no engine, no real user data).
use serde_json::{json, Value};

fn comfy_dir() -> std::path::PathBuf {
    std::env::var_os("SAYA_COMFYUI_DIR").map(Into::into).unwrap_or_else(|| {
        std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("AI/ComfyUI")
    })
}
#[allow(dead_code)]
fn real_workflow() -> std::path::PathBuf {
    std::env::var_os("SAYA_REAL_UI_WORKFLOW")
        .map(Into::into)
        .unwrap_or_else(|| comfy_dir().join("user/default/workflows/ILUSTCOUPLECLEAN.json"))
}
#[allow(dead_code)]
fn report_dir() -> std::path::PathBuf {
    std::env::var_os("SAYA_REAL_REPORT_DIR").map(Into::into).unwrap_or_else(std::env::temp_dir)
}
#[allow(dead_code)]
fn skip_without_workflow() -> bool {
    let missing = !real_workflow().is_file();
    if missing {
        eprintln!("skip: workflow introuvable ({}) ; régler SAYA_COMFYUI_DIR ou SAYA_REAL_UI_WORKFLOW", real_workflow().display());
    }
    missing
}

fn info() -> Value {
    json!({
        "CheckpointLoaderSimple":{"input":{"required":{"ckpt_name":[["ck.safetensors"]]}},"output":["MODEL","CLIP","VAE"]},
        "CLIPTextEncode":{"input":{"required":{"text":["STRING"],"clip":["CLIP"]}},"output":["CONDITIONING"]},
        "EmptyLatentImage":{"input":{"required":{"width":["INT"],"height":["INT"],"batch_size":["INT"]}},"output":["LATENT"]},
        "KSampler":{"input":{"required":{"seed":["INT"],"steps":["INT"],"cfg":["FLOAT"],"sampler_name":[["euler"]],"scheduler":[["normal"]],"denoise":["FLOAT"],"model":["MODEL"],"positive":["CONDITIONING"],"negative":["CONDITIONING"],"latent_image":["LATENT"]}},"output":["LATENT"]},
        "VAEDecode":{"input":{"required":{"samples":["LATENT"],"vae":["VAE"]}},"output":["IMAGE"]},
        "SaveImage":{"input":{"required":{"images":["IMAGE"],"filename_prefix":["STRING"]}},"output":[],"output_node":true}
    })
}

fn graph() -> Value {
    json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"ck.safetensors"}},
        "2":{"class_type":"CLIPTextEncode","inputs":{"text":"P1 old","clip":["1",1]}},
        "3":{"class_type":"CLIPTextEncode","inputs":{"text":"anatomy old","clip":["1",1]}},
        "4":{"class_type":"CLIPTextEncode","inputs":{"text":"bg old","clip":["1",1]}},
        "5":{"class_type":"CLIPTextEncode","inputs":{"text":"action old","clip":["1",1]}},
        "6":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},
        "7":{"class_type":"KSampler","inputs":{"seed":1,"steps":20,"cfg":6.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0,"model":["1",0],"positive":["2",0],"negative":["4",0],"latent_image":["6",0]}},
        "8":{"class_type":"VAEDecode","inputs":{"samples":["7",0],"vae":["1",2]}},
        "9":{"class_type":"SaveImage","inputs":{"images":["8",0],"filename_prefix":"u"}}
    })
}

#[test]
fn profile_persists_maps_exact_prompts_and_detects_changes() {
    let db = studio_core::Database::in_memory().unwrap();
    let workflow = db
        .import_workflow("Universal", &graph().to_string())
        .unwrap();
    let character = db
        .save_character(
            None,
            "Alpha".into(),
            json!({"identity_prompt":"full P1,  exact\nligne","anatomy_prompt":"anatomie exacte"}),
        )
        .unwrap();
    db.save_variant(
        &character,
        None,
        "BG Garden",
        json!({"BG":"jardin  exact"}),
        json!({}),
    )
    .unwrap();
    for (key, node, input) in [
        ("P1", "2", "text"),
        ("ANATOMY_P1", "3", "text"),
        ("BG", "4", "text"),
        ("ACTION", "5", "text"),
        ("QUICK_STEPS", "7", "steps"),
    ] {
        db.save_semantic_binding(&workflow, key, node, input)
            .unwrap();
    }
    let config = json!({
        "p1":{"characterId":character,"variantId":Value::Null},
        "p2":{"characterId":""},
        "bg":{"source":"character","enabledByVariant":[],"selected":"jardin  exact"},
        "action":{"source":"fixed","text":"running","selected":null},
        "mappings":{"P1":{"nodeId":"2","inputName":"text"},"ANATOMY_P1":{"nodeId":"3","inputName":"text"},"BG":{"nodeId":"4","inputName":"text"},"ACTION":{"nodeId":"5","inputName":"text"}},
        "quick":[{"nodeId":"7","inputName":"steps","value":"30","visible":true}],
        "favorites":[character]
    });
    let profile = db
        .save_prepare_profile(None, "Pan", &workflow, "solo", &config)
        .unwrap();
    let id = profile["id"].as_str().unwrap();
    let result = db.prepare_universal(id, &info()).unwrap();
    assert_eq!(result["resolution"]["bg"], "jardin  exact");
    assert_eq!(result["resolution"]["action"], "running");
    let prepared = &result["prepared"];
    let request: Value = serde_json::from_str(prepared["requestJson"].as_str().unwrap()).unwrap();
    assert_eq!(
        request["prompt"]["2"]["inputs"]["text"],
        "full P1,  exact\nligne"
    );
    assert_eq!(request["prompt"]["3"]["inputs"]["text"], "anatomie exacte");
    assert_eq!(request["prompt"]["4"]["inputs"]["text"], "jardin  exact");
    assert_eq!(request["prompt"]["5"]["inputs"]["text"], "running");
    assert_eq!(request["prompt"]["7"]["inputs"]["steps"], 30);
    assert_eq!(db.prepare_bindings(&workflow).unwrap().len(), 5);
    // The source workflow is unchanged.
    assert_eq!(
        db.workflow_document(&workflow).unwrap().1,
        graph().to_string()
    );
    // Changing the workflow signature makes the old profile stale.
    let mut changed = graph();
    changed["9"]["inputs"]["filename_prefix"] = json!("changed");
    db.import_workflow("changed", &changed.to_string()).unwrap();
    let profile_row = db.prepare_profile(id).unwrap();
    assert_eq!(profile_row["changed"], false);
}

#[test]
fn profiles_roundtrip_reject_invalid_modes_and_duplicate_mapping_targets() {
    let db = studio_core::Database::open(
        &std::env::temp_dir().join(format!("saya-up-{}.sqlite", uuid::Uuid::new_v4())),
    )
    .unwrap();
    let workflow = db
        .import_workflow("Universal", &graph().to_string())
        .unwrap();
    let config = json!({"p1":{"characterId":""},"p2":{"characterId":""},"bg":{"source":"fixed","text":"x"},"action":{"source":"fixed","text":"y"},"mappings":{},"quick":[],"favorites":[]});
    assert!(db
        .save_prepare_profile(None, " ", &workflow, "solo", &config)
        .is_err());
    assert!(db
        .save_prepare_profile(None, "p", &workflow, "triple", &config)
        .is_err());
    let saved = db
        .save_prepare_profile(None, "Profil", &workflow, "solo", &config)
        .unwrap();
    let id = saved["id"].as_str().unwrap();
    assert_eq!(db.prepare_profile(id).unwrap()["config"], config);
    db.save_semantic_binding(&workflow, "P1", "2", "text")
        .unwrap();
    db.save_semantic_binding(&workflow, "BG", "4", "text")
        .unwrap();
    assert!(db
        .save_semantic_binding(&workflow, "OTHER", "2", "text")
        .is_err());
    assert_eq!(db.prepare_bindings(&workflow).unwrap().len(), 2);
    db.delete_prepare_profile(id).unwrap();
    assert!(db.prepare_profile(id).is_err());
}

#[test]
fn real_saya_couple_workflow_is_mapped_read_only_without_schema_claims() {
    let Ok(source) = std::fs::read_to_string(real_workflow()) else {
        return;
    };
    let before = studio_core::gallery::digest(source.as_bytes());
    let converted = studio_core::workflow_ui_api::convert_ui(&source, None).unwrap();
    assert_eq!(converted["report"]["complete"], true);
    let api = converted["api"].clone();
    let db = studio_core::Database::in_memory().unwrap();
    let workflow = db
        .import_workflow("Saya Couple real", &api.to_string())
        .unwrap();
    let p1 = db.save_character(None, "P1 real".into(), json!({"identity_prompt":"P1 real exact\nsecond","anatomy_prompt":"anatomie P1 exacte"})).unwrap();
    let p2 = db
        .save_character(
            None,
            "P2 real".into(),
            json!({"identity_prompt":"P2 real exact","anatomy_prompt":"anatomie P2 exacte"}),
        )
        .unwrap();
    db.save_variant(&p1, None, "BG P1", json!({"BG":"BG P1 exact"}), json!({}))
        .unwrap();
    db.save_variant(&p2, None, "BG P2", json!({"BG":"BG P2 exact"}), json!({}))
        .unwrap();
    for (key, node, input) in [
        ("P1", "807", "value"),
        ("ANATOMY_P1", "100301", "value"),
        ("P2", "808", "value"),
        ("ANATOMY_P2", "100302", "value"),
        ("BG", "100309", "decor_libre"),
        ("ACTION", "100300", "value"),
        ("QUICK_SEED", "100032", "seed"),
    ] {
        db.save_semantic_binding(&workflow, key, node, input)
            .unwrap();
    }
    let config = json!({
        "p1":{"characterId":p1,"variantId":Value::Null},"p2":{"characterId":p2,"variantId":Value::Null},
        "bg":{"source":"combined","enabledByVariant":[],"selected":"BG P2 exact"},
        "action":{"source":"fixed","text":"action real exact","selected":null},
        "mappings":{
            "P1":{"nodeId":"807","inputName":"value"},"ANATOMY_P1":{"nodeId":"100301","inputName":"value"},
            "P2":{"nodeId":"808","inputName":"value"},"ANATOMY_P2":{"nodeId":"100302","inputName":"value"},
            "BG":{"nodeId":"100309","inputName":"decor_libre"},"ACTION":{"nodeId":"100300","inputName":"value"}
        },
        "quick":[{"nodeId":"100032","inputName":"seed","value":"123456789012345678","visible":true}],
        "favorites":[p1,p2]
    });
    let profile = db
        .save_prepare_profile(None, "Saya Couple real", &workflow, "duo", &config)
        .unwrap();
    let result = db
        .prepare_universal(profile["id"].as_str().unwrap(), &Value::Null)
        .unwrap();
    assert_eq!(result["resolution"]["bg"], "BG P2 exact");
    let request: Value =
        serde_json::from_str(result["prepared"]["requestJson"].as_str().unwrap()).unwrap();
    assert_eq!(
        request["prompt"]["807"]["inputs"]["value"],
        "P1 real exact\nsecond"
    );
    assert_eq!(request["prompt"]["808"]["inputs"]["value"], "P2 real exact");
    assert_eq!(
        request["prompt"]["100301"]["inputs"]["value"],
        "anatomie P1 exacte"
    );
    assert_eq!(
        request["prompt"]["100302"]["inputs"]["value"],
        "anatomie P2 exacte"
    );
    assert_eq!(
        request["prompt"]["100309"]["inputs"]["decor_libre"],
        "BG P2 exact"
    );
    assert_eq!(
        request["prompt"]["100300"]["inputs"]["value"],
        "action real exact"
    );
    assert_eq!(
        request["prompt"]["100032"]["inputs"]["seed"],
        123456789012345678u64
    );
    // Unrelated custom node values and links remain exact.
    assert_eq!(
        request["prompt"]["100309"]["inputs"]["prefixe"],
        "masterpiece, best quality, very aesthetic, absurdres"
    );
    assert_eq!(request["prompt"]["654"]["inputs"]["text"], "");
    assert_eq!(request["prompt"]["100309"]["inputs"]["seed"], 0);
    assert_eq!(
        studio_core::gallery::digest(std::fs::read(real_workflow()).unwrap().as_slice()),
        before
    );
    assert_eq!(db.workflow_document(&workflow).unwrap().1, api.to_string());
}

#[test]
fn profiles_import_export_and_audit_assisted_remapping() {
    let db = studio_core::Database::in_memory().unwrap();
    let workflow = db.import_workflow("First", &graph().to_string()).unwrap();
    db.save_semantic_binding(&workflow, "P1", "2", "text")
        .unwrap();
    let config = json!({"p1":{"characterId":""},"p2":{"characterId":""},"bg":{"source":"fixed","text":"x"},"action":{"source":"fixed","text":"y"},"mappings":{"P1":{"nodeId":"2","inputName":"text"}},"quick":[],"favorites":[]});
    let saved = db
        .save_prepare_profile(None, "Portable", &workflow, "solo", &config)
        .unwrap();
    let id = saved["id"].as_str().unwrap();
    let exported = db.export_prepare_profile(id).unwrap();
    let imported = db
        .import_prepare_profile(&exported.to_string(), &workflow)
        .unwrap();
    assert_eq!(imported["config"]["mappings"]["P1"]["nodeId"], "2");
    let audit = db.audit_prepare_profile(id, &workflow).unwrap();
    assert_eq!(audit["mappings"]["P1"]["status"], "valid");
    let mut changed_graph = graph();
    changed_graph["10"] =
        json!({"class_type":"PrimitiveStringMultiline","inputs":{"value":"new P1"}});
    changed_graph["2"] =
        json!({"class_type":"PrimitiveStringMultiline","inputs":{"value":"replaced"}});
    let changed = db
        .import_workflow("Second", &changed_graph.to_string())
        .unwrap();
    let diff = db.audit_prepare_profile(id, &changed).unwrap();
    assert_eq!(diff["mappings"]["P1"]["status"], "missing");
    assert!(!diff["mappings"]["P1"]["suggestions"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(db
        .remap_prepare_profile(
            id,
            &changed,
            &json!({"P1":{"nodeId":"10","inputName":"value"}}),
            false
        )
        .is_err());
    let remapped = db
        .remap_prepare_profile(
            id,
            &changed,
            &json!({"P1":{"nodeId":"10","inputName":"value"}}),
            true,
        )
        .unwrap();
    assert_eq!(remapped["config"]["mappings"]["P1"]["nodeId"], "10");
}

#[test]
fn lora_preview_supports_simple_and_manager_loaders_and_refuses_ambiguity() {
    let db = studio_core::Database::in_memory().unwrap();
    let g = json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"ck.safetensors"}},
        "2":{"class_type":"LoraLoader","inputs":{"lora_name":"old.safetensors","strength_model":0.5,"strength_clip":0.6,"model":["1",0],"clip":["1",1]}},
        "3":{"class_type":"Lora Loader (LoraManager)","inputs":{"text":"<lora:old:0.2:0.2>"}},
        "4":{"class_type":"MysteryLoader","inputs":{"value":"x"}}
    });
    let workflow = db.import_workflow("LoRA", &g.to_string()).unwrap();
    let loaders = db.lora_loaders(&workflow).unwrap();
    assert_eq!(loaders.len(), 2);
    let preview = db.preview_lora(&workflow, &json!({"selections":[
        {"name":"alpha","relativePath":"alpha.safetensors","loaderNode":"2","modelWeight":0.8,"clipWeight":0.7,"enabled":true},
        {"name":"beta","relativePath":"beta.safetensors","loaderNode":"3","modelWeight":0.4,"clipWeight":0.4,"enabled":true}
    ]})).unwrap();
    assert_eq!(preview["previewGraph"]["2"]["inputs"]["lora_name"], "alpha");
    assert_eq!(
        preview["previewGraph"]["2"]["inputs"]["strength_model"],
        0.8
    );
    assert_eq!(preview["previewGraph"]["2"]["inputs"]["strength_clip"], 0.7);
    assert_eq!(
        preview["previewGraph"]["3"]["inputs"]["text"],
        "<lora:beta:0.4:0.4>"
    );
    assert!(db
        .preview_lora(
            &workflow,
            &json!({"selections":[
                {"name":"a","loaderNode":"4","modelWeight":1,"clipWeight":1,"enabled":true}
            ]})
        )
        .is_err());
    assert!(db
        .preview_lora(
            &workflow,
            &json!({"selections":[
                {"name":"a","loaderNode":"2","modelWeight":1,"clipWeight":1,"enabled":true},
                {"name":"b","loaderNode":"2","modelWeight":1,"clipWeight":1,"enabled":true}
            ]})
        )
        .is_err());
}

#[test]
fn real_saya_couple_manager_lora_preview_targets_only_selected_loader() {
    let Ok(source) = std::fs::read_to_string(real_workflow()) else {
        return;
    };
    let converted = studio_core::workflow_ui_api::convert_ui(&source, None).unwrap();
    let db = studio_core::Database::in_memory().unwrap();
    let workflow = db
        .import_workflow("Saya LoRA", &converted["api"].to_string())
        .unwrap();
    let loaders = db.lora_loaders(&workflow).unwrap();
    assert!(loaders
        .iter()
        .any(|l| l["nodeId"] == "654" && l["mode"] == "manager"));
    let before = converted["api"]["663"]["inputs"]["text"].clone();
    let preview = db.preview_lora(&workflow, &json!({"selections":[
        {"name":"managed","relativePath":"managed.safetensors","loaderNode":"654","modelWeight":0.55,"clipWeight":0.45,"enabled":true}
    ]})).unwrap();
    assert_eq!(
        preview["previewGraph"]["654"]["inputs"]["text"],
        "<lora:managed:0.55:0.45>"
    );
    assert_eq!(preview["previewGraph"]["663"]["inputs"]["text"], before);
    assert_eq!(
        db.workflow_document(&workflow).unwrap().1,
        converted["api"].to_string()
    );
}

#[test]
fn universal_preparation_injects_selected_lora_patch_and_reports_it() {
    let db = studio_core::Database::in_memory().unwrap();
    let mut g = graph();
    g["10"] = json!({"class_type":"Lora Loader (LoraManager)","inputs":{"text":"<lora:old:0.2:0.2>","model":["1",0]}});
    let workflow = db.import_workflow("LoRA prep", &g.to_string()).unwrap();
    let p1 = db
        .save_character(
            None,
            "P1".into(),
            json!({"identity_prompt":"p1","anatomy_prompt":"a1"}),
        )
        .unwrap();
    db.save_variant(&p1, None, "BG", json!({"BG":"bg"}), json!({}))
        .unwrap();
    for (key, node, input) in [
        ("P1", "2", "text"),
        ("ANATOMY_P1", "3", "text"),
        ("BG", "4", "text"),
        ("ACTION", "5", "text"),
    ] {
        db.save_semantic_binding(&workflow, key, node, input)
            .unwrap();
    }
    let config = json!({
        "p1":{"characterId":p1},"p2":{"characterId":""},
        "bg":{"source":"character","enabledByVariant":[],"selected":"bg"},
        "action":{"source":"fixed","text":"run"},
        "mappings":{"P1":{"nodeId":"2","inputName":"text"},"ANATOMY_P1":{"nodeId":"3","inputName":"text"},"BG":{"nodeId":"4","inputName":"text"},"ACTION":{"nodeId":"5","inputName":"text"}},
        "quick":[],
        "lora":{"selections":[{"name":"chosen","loaderNode":"10","modelWeight":0.7,"clipWeight":0.6,"enabled":true}]}
    });
    let profile = db
        .save_prepare_profile(None, "LoRA", &workflow, "solo", &config)
        .unwrap();
    let result = db
        .prepare_universal(profile["id"].as_str().unwrap(), &Value::Null)
        .unwrap();
    let request: Value =
        serde_json::from_str(result["prepared"]["requestJson"].as_str().unwrap()).unwrap();
    assert_eq!(
        request["prompt"]["10"]["inputs"]["text"],
        "<lora:chosen:0.7:0.6>"
    );
    assert_eq!(result["loraEdits"].as_array().unwrap().len(), 1);
    assert_eq!(db.workflow_document(&workflow).unwrap().1, g.to_string());
}

#[test]
#[ignore = "real local ComfyUI object_info; no submission"]
fn saya_couple_real_object_info_preflight_and_universal_preparation() {
    let info_path = std::env::var_os("SAYA_REAL_OBJECT_INFO").map(std::path::PathBuf::from).unwrap_or_else(|| report_dir().join("object_info.json"));
    let Ok(info_text) = std::fs::read_to_string(info_path) else {
        return;
    };
    let info: Value = serde_json::from_str(&info_text).unwrap();
    let source = std::fs::read_to_string(real_workflow())
    .unwrap();
    let converted = studio_core::workflow_ui_api::convert_ui(&source, Some(&info)).unwrap();
    assert_eq!(converted["report"]["complete"], true);
    studio_core::workflow::validate_graph(&converted["api"], &info).unwrap();
    let db = studio_core::Database::in_memory().unwrap();
    let workflow = db
        .import_workflow("Saya real schema", &converted["api"].to_string())
        .unwrap();
    let p1 = db
        .save_character(
            None,
            "P1".into(),
            json!({"identity_prompt":"P1 exact","anatomy_prompt":"A1 exact"}),
        )
        .unwrap();
    let p2 = db
        .save_character(
            None,
            "P2".into(),
            json!({"identity_prompt":"P2 exact","anatomy_prompt":"A2 exact"}),
        )
        .unwrap();
    db.save_variant(&p1, None, "bg1", json!({"BG":"bg1"}), json!({}))
        .unwrap();
    db.save_variant(&p2, None, "bg2", json!({"BG":"bg2"}), json!({}))
        .unwrap();
    for (key, node, input) in [
        ("P1", "807", "value"),
        ("ANATOMY_P1", "100301", "value"),
        ("P2", "808", "value"),
        ("ANATOMY_P2", "100302", "value"),
        ("BG", "100309", "decor_libre"),
        ("ACTION", "100300", "value"),
    ] {
        db.save_semantic_binding(&workflow, key, node, input)
            .unwrap();
    }
    let config = json!({
        "p1":{"characterId":p1},"p2":{"characterId":p2},
        "bg":{"source":"combined","enabledByVariant":[],"selected":"bg2"},
        "action":{"source":"fixed","text":"real action"},
        "mappings":{"P1":{"nodeId":"807","inputName":"value"},"ANATOMY_P1":{"nodeId":"100301","inputName":"value"},"P2":{"nodeId":"808","inputName":"value"},"ANATOMY_P2":{"nodeId":"100302","inputName":"value"},"BG":{"nodeId":"100309","inputName":"decor_libre"},"ACTION":{"nodeId":"100300","inputName":"value"}},
        "quick":[]
    });
    let profile = db
        .save_prepare_profile(None, "Saya real", &workflow, "duo", &config)
        .unwrap();
    let result = db
        .prepare_universal(profile["id"].as_str().unwrap(), &info)
        .unwrap();
    let request: Value =
        serde_json::from_str(result["prepared"]["requestJson"].as_str().unwrap()).unwrap();
    assert_eq!(request["prompt"]["807"]["inputs"]["value"], "P1 exact");
    assert_eq!(request["prompt"]["100309"]["inputs"]["decor_libre"], "bg2");
}

#[test]
fn one_master_seed_is_applied_to_every_seed_input() {
    let db = studio_core::Database::in_memory().unwrap();
    let mut g = graph();
    g["7"]["inputs"]["seed"] = json!(7);
    g["10"] = json!({"class_type":"KSampler","inputs":{"seed":99,"noise_seed":5,"steps":1,"cfg":1.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0,"model":["1",0],"positive":["2",0],"negative":["4",0],"latent_image":["6",0]}});
    let workflow = db.import_workflow("Seed", &g.to_string()).unwrap();
    let p1 = db
        .save_character(
            None,
            "P1".into(),
            json!({"identity_prompt":"p1","anatomy_prompt":"a1"}),
        )
        .unwrap();
    db.save_variant(&p1, None, "BG", json!({"BG":"bg"}), json!({}))
        .unwrap();
    for (key, node, input) in [
        ("P1", "2", "text"),
        ("ANATOMY_P1", "3", "text"),
        ("BG", "4", "text"),
        ("ACTION", "5", "text"),
    ] {
        db.save_semantic_binding(&workflow, key, node, input)
            .unwrap();
    }
    let config = json!({"p1":{"characterId":p1},"p2":{"characterId":""},"bg":{"source":"character","selected":"bg"},"action":{"source":"fixed","text":"run"},"masterSeed":"18446744073709551615","mappings":{"P1":{"nodeId":"2","inputName":"text"},"ANATOMY_P1":{"nodeId":"3","inputName":"text"},"BG":{"nodeId":"4","inputName":"text"},"ACTION":{"nodeId":"5","inputName":"text"}},"quick":[]});
    let profile = db
        .save_prepare_profile(None, "Seed", &workflow, "solo", &config)
        .unwrap();
    let result = db
        .prepare_universal(profile["id"].as_str().unwrap(), &Value::Null)
        .unwrap();
    let request: Value =
        serde_json::from_str(result["prepared"]["requestJson"].as_str().unwrap()).unwrap();
    assert_eq!(
        request["prompt"]["7"]["inputs"]["seed"],
        18446744073709551615u64
    );
    assert_eq!(
        request["prompt"]["10"]["inputs"]["seed"],
        18446744073709551615u64
    );
    assert_eq!(
        request["prompt"]["10"]["inputs"]["noise_seed"],
        18446744073709551615u64
    );
    assert_eq!(result["resolution"]["masterSeed"], "18446744073709551615");
}

#[test]
#[ignore = "reads local Saya workflow/object_info; no GPU submission"]
fn saya_phase_apis_route_custom_nodes_and_review_gate() {
    let info_path = std::env::var_os("SAYA_REAL_OBJECT_INFO").map(std::path::PathBuf::from).unwrap_or_else(|| report_dir().join("object_info.json"));
    let Ok(info_text) = std::fs::read_to_string(info_path) else {
        return;
    };
    let info: Value = serde_json::from_str(&info_text).unwrap();
    let source = std::fs::read_to_string(real_workflow())
    .unwrap();
    for phase in 1..=3 {
        let converted =
            studio_core::workflow_ui_api::convert_ui_phase(&source, Some(&info), phase).unwrap();
        assert_eq!(converted["report"]["complete"], true, "phase {phase}");
        studio_core::workflow::validate_graph(&converted["api"], &info).unwrap();
        let classes: Vec<String> = converted["api"]
            .as_object()
            .unwrap()
            .values()
            .filter_map(|n| n["class_type"].as_str().map(str::to_owned))
            .collect();
        if phase == 1 {
            assert!(classes.iter().any(|c| c == "SayaImageGenerationReview"));
        } else {
            assert!(!classes.iter().any(|c| c == "SayaImageGenerationReview"));
            assert!(classes
                .iter()
                .any(|c| c == &format!("SayaImagePhase{phase}Load")));
            assert!(classes
                .iter()
                .any(|c| c == &format!("SayaImagePhase{phase}Stop")));
        }
    }
}

#[test]
fn master_seed_only_targets_rgthree_seed_node_and_accepts_sentinel() {
    let db = studio_core::Database::in_memory().unwrap();
    let mut g = graph();
    g["11"] = json!({"class_type":"Seed (rgthree)","inputs":{"seed":7}});
    let workflow = db.import_workflow("Seed sentinel", &g.to_string()).unwrap();
    let p1 = db
        .save_character(
            None,
            "P1".into(),
            json!({"identity_prompt":"p1","anatomy_prompt":"a1"}),
        )
        .unwrap();
    db.save_variant(&p1, None, "BG", json!({"BG":"bg"}), json!({}))
        .unwrap();
    for (key, node, input) in [
        ("P1", "2", "text"),
        ("ANATOMY_P1", "3", "text"),
        ("BG", "4", "text"),
        ("ACTION", "5", "text"),
    ] {
        db.save_semantic_binding(&workflow, key, node, input)
            .unwrap();
    }
    let config = json!({"p1":{"characterId":p1},"p2":{"characterId":""},"bg":{"source":"character","selected":"bg"},"action":{"source":"fixed","text":"run"},"masterSeed":"-1","mappings":{"P1":{"nodeId":"2","inputName":"text"},"ANATOMY_P1":{"nodeId":"3","inputName":"text"},"BG":{"nodeId":"4","inputName":"text"},"ACTION":{"nodeId":"5","inputName":"text"}},"quick":[]});
    let profile = db
        .save_prepare_profile(None, "Seed sentinel", &workflow, "solo", &config)
        .unwrap();
    let result = db
        .prepare_universal(profile["id"].as_str().unwrap(), &Value::Null)
        .unwrap();
    let request: Value =
        serde_json::from_str(result["prepared"]["requestJson"].as_str().unwrap()).unwrap();
    assert_eq!(request["prompt"]["11"]["inputs"]["seed"], json!(-1));
    assert_eq!(request["prompt"]["7"]["inputs"]["seed"], json!(1));
}

#[test]
fn temporary_prompts_and_negative_do_not_change_character() {
    let db = studio_core::Database::in_memory().unwrap();
    let mut g = graph();
    g["10"] =
        json!({"class_type":"CLIPTextEncode","inputs":{"text":"old negative","clip":["1",1]}});
    g["7"]["inputs"]["negative"] = json!(["10", 0]);
    let wf = db.import_workflow("Temporary", &g.to_string()).unwrap();
    let ch = db
        .save_character(
            None,
            "Original".into(),
            json!({"identity_prompt":"original identity","anatomy_prompt":"original anatomy"}),
        )
        .unwrap();
    let before = db.character_sheet(&ch).unwrap();
    for (key, node) in [
        ("P1", "2"),
        ("ANATOMY_P1", "3"),
        ("BG", "4"),
        ("ACTION", "5"),
        ("NEG", "10"),
    ] {
        db.save_semantic_binding(&wf, key, node, "text").unwrap();
    }
    let config = json!({"p1":{"characterId":ch},"p2":{"characterId":""},
      "promptOverrides":{"P1":"temporary  identity\nexact","ANATOMY_P1":"temporary anatomy"},
      "negative":"temporary negative", "bg":{"source":"fixed","text":"garden"},"action":{"source":"fixed","text":"standing"},
      "mappings":{"P1":{"nodeId":"2","inputName":"text"},"ANATOMY_P1":{"nodeId":"3","inputName":"text"},"BG":{"nodeId":"4","inputName":"text"},"ACTION":{"nodeId":"5","inputName":"text"},"NEG":{"nodeId":"10","inputName":"text"}},"quick":[]});
    let p = db
        .save_prepare_profile(None, "Temporary", &wf, "solo", &config)
        .unwrap();
    let r = db
        .prepare_universal(p["id"].as_str().unwrap(), &info())
        .unwrap();
    let request: Value =
        serde_json::from_str(r["prepared"]["requestJson"].as_str().unwrap()).unwrap();
    assert_eq!(
        request["prompt"]["2"]["inputs"]["text"],
        "temporary  identity\nexact"
    );
    assert_eq!(
        request["prompt"]["3"]["inputs"]["text"],
        "temporary anatomy"
    );
    assert_eq!(
        request["prompt"]["10"]["inputs"]["text"],
        "temporary negative"
    );
    assert_eq!(db.character_sheet(&ch).unwrap(), before);
    assert_eq!(db.workflow_document(&wf).unwrap().1, g.to_string());
}
