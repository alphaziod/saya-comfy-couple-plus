use serde_json::{json, Value};
use studio_core::graph_session::{EditOp, GraphSession, NodeMove};
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
fn api_ui_roundtrip_widgets_links_groups_exact_seed_and_metadata() {
    let info = info();
    let mut source = api();
    source["5"]["extension"] = json!({"unknown":"preserved"});
    let session = GraphSession::open("wf", &source, &info, "API").unwrap();
    assert_eq!(session.export().unwrap(), source);
    let mut ui = session.export_ui().unwrap();
    assert_eq!(ui["nodes"].as_array().unwrap().len(), 7);
    ui["groups"] = json!([{"title":"Groupe conservé","bounding":[-1,-1,9000,9000],"color":"#123456","extension":{"keep":true}}]);
    ui["extra"]["unknown"] = json!({"nested":[1,2,3]});
    let mut loaded = GraphSession::open_file(&ui.to_string(), &info, "UI").unwrap();
    assert!(
        loaded.view()["exportable"].as_bool().unwrap(),
        "{:?}",
        loaded.view()["exportError"]
    );
    assert_eq!(loaded.export_ui().unwrap(), ui);
    let exported = loaded.export().unwrap();
    for (id, n) in source.as_object().unwrap() {
        assert_eq!(exported[id]["inputs"], n["inputs"]);
    }
    loaded
        .apply(EditOp::SetParam {
            node: "5".into(),
            input: "seed".into(),
            value: Some("18446744073709551614".into()),
        })
        .unwrap();
    loaded
        .apply(EditOp::SetParam {
            node: "2".into(),
            input: "text".into(),
            value: Some("  poids:1.2,\nété 🌸  ".into()),
        })
        .unwrap();
    loaded
        .apply(EditOp::Move {
            moves: vec![NodeMove {
                node: "2".into(),
                x: 123.0,
                y: 456.0,
            }],
        })
        .unwrap();
    let edited = loaded.export_ui().unwrap();
    assert_eq!(edited["groups"], ui["groups"]);
    assert_eq!(edited["extra"], ui["extra"]);
    assert_eq!(
        loaded.export().unwrap()["5"]["inputs"]["seed"],
        json!(18446744073709551614u64)
    );
    assert_eq!(
        loaded.export().unwrap()["2"]["inputs"]["text"],
        "  poids:1.2,\nété 🌸  "
    );
    loaded
        .apply(EditOp::Remove {
            nodes: vec!["2".into()],
        })
        .unwrap();
    assert!(!loaded.view()["document"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["id"] == json!("2")));
    loaded.apply(EditOp::Undo).unwrap();
    let again = GraphSession::open_file(&edited.to_string(), &info, "Again").unwrap();
    assert_eq!(again.export().unwrap(), loaded.export().unwrap());
}
#[test]
fn unknown_custom_subgraph_dynamic_and_inconsistent_links_never_silently_convert() {
    let info = info();
    let source = GraphSession::open("wf", &api(), &info, "API")
        .unwrap()
        .export_ui()
        .unwrap();
    for mutation in ["custom", "subgraph", "dynamic", "mode", "named"] {
        let mut ui = source.clone();
        match mutation {
            "custom" => ui["nodes"][4]["type"] = json!("UninstalledCustom"),
            "subgraph" => {
                ui["nodes"][4]["type"] = json!("sub-id");
                ui["definitions"] = json!({"subgraphs":[{"id":"sub-id","name":"Subgraph réel","nodes":[{"opaque":true}],"widgets":[{"promoted":true}]}]});
            }
            "dynamic" => ui["nodes"][4]["widgets_values"]
                .as_array_mut()
                .unwrap()
                .push(json!("unmapped")),
            "mode" => ui["nodes"][4]["mode"] = json!(4),
            _ => ui["nodes"][4]["widgets_values_named"]["seed"] = json!(1),
        }
        let loaded = GraphSession::open_file(&ui.to_string(), &info, "UI").unwrap();
        assert!(loaded.export().is_err(), "{mutation}");
        assert_eq!(loaded.export_ui().unwrap(), ui, "{mutation}");
        assert!(!loaded.view()["diagnostics"].as_array().unwrap().is_empty());
    }
    let mut broken = source.clone();
    broken["links"].as_array_mut().unwrap().pop();
    assert!(GraphSession::open_file(&broken.to_string(), &info, "Broken").is_err());
    let mut no_order = info.clone();
    no_order["KSampler"]
        .as_object_mut()
        .unwrap()
        .remove("input_order");
    let loaded = GraphSession::open_file(&source.to_string(), &no_order, "UI").unwrap();
    assert!(loaded.export().is_err());
    assert_eq!(loaded.export_ui().unwrap(), source);
}
#[test]
#[ignore = "Lecture seule du workflow personnel ; opt-in"]
fn real_saya_couple_is_visible_and_original_preserved() {
    if skip_without_workflow() {
        return;
    }
    let path = real_workflow();
    let text = std::fs::read_to_string(path).unwrap();
    let source: Value = serde_json::from_str(&text).unwrap();
    let loaded = GraphSession::open_file(&text, &json!({}), "Saya Couple").unwrap();
    assert_eq!(
        loaded.view()["document"]["nodes"].as_array().unwrap().len(),
        109
    );
    assert_eq!(
        loaded.view()["document"]["connections"]
            .as_array()
            .unwrap()
            .len(),
        211
    );
    assert_eq!(loaded.export_ui().unwrap(), source);
    assert!(loaded.export().is_err());
    assert!(!loaded.view()["document"]["subgraphs"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn named_frontend_seed_control_keeps_sampler_widgets_editable() {
    let schema = json!({"Sampler":{"input":{"required":{"seed":["INT",{}],"steps":["INT",{}]}},"input_order":{"required":["seed","steps"]},"output":[]}});
    let source = json!({"nodes":[{"id":1,"type":"Sampler","pos":[0,0],"size":[200,200],"mode":0,"inputs":[],"outputs":[],"widgets_values":[9007199254740993u64,"fixed",12],"widgets_values_named":{"seed":9007199254740993u64,"control_after_generate":"fixed","steps":12}}],"links":[],"groups":[]});
    let mut editor = GraphSession::open_file(&source.to_string(), &schema, "seed control").unwrap();
    assert_eq!(editor.view()["paramText"]["1"]["steps"], "12");
    editor
        .apply(EditOp::SetParam {
            node: "1".into(),
            input: "steps".into(),
            value: Some("7".into()),
        })
        .unwrap();
    let edited = editor.export_ui().unwrap();
    assert_eq!(
        edited["nodes"][0]["widgets_values"],
        json!([9007199254740993u64, "fixed", 7])
    );
    assert_eq!(edited["nodes"][0]["widgets_values_named"]["steps"], 7);
    let mut contradictory = source.clone();
    contradictory["nodes"][0]["widgets_values_named"]["control_after_generate"] =
        json!("randomize");
    let blocked =
        GraphSession::open_file(&contradictory.to_string(), &schema, "contradiction").unwrap();
    assert!(blocked.view()["paramText"]["1"].get("steps").is_none());
    assert_eq!(blocked.export_ui().unwrap(), contradictory);
}
