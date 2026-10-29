//! Interactive graph editing session: exact u64, grouped undo, typed values,
//! original ids kept on export, schema validation. Uses the scrubbed real
//! object_info subset used by the mock engine.
use serde_json::{json, Value};
use studio_core::graph_session::{EditOp, GraphSession, NodeMove};

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

fn node_for(view: &Value, original: &str) -> String {
    view["originalIds"]
        .as_object()
        .unwrap()
        .iter()
        .find(|(_, v)| v.as_str() == Some(original))
        .unwrap()
        .0
        .clone()
}

#[test]
fn session_keeps_u64_exact_groups_undo_and_exports_original_ids() {
    let info = info();
    let mut session = GraphSession::open("wf", &api(), &info, "Edition").unwrap();
    let view = session.view();
    let sampler = node_for(&view, "5");
    assert_eq!(view["paramText"][&sampler]["seed"], "18446744073709551615");
    assert_eq!(view["canUndo"], false, "initial layout is not user history");
    assert_eq!(view["exportable"], true, "{view}");
    assert!(view["document"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|n| n["position"].is_object()));
    // Unchanged export round-trips the original API graph exactly.
    assert_eq!(session.export().unwrap(), api());

    // Multi-node move is one undo step.
    let text = node_for(&view, "2");
    session
        .apply(EditOp::Move {
            moves: vec![
                NodeMove {
                    node: sampler.clone(),
                    x: 900.0,
                    y: 10.0,
                },
                NodeMove {
                    node: text.clone(),
                    x: 5.0,
                    y: 600.0,
                },
            ],
        })
        .unwrap();
    session.apply(EditOp::Undo).unwrap();
    let back = session.view();
    let position = |v: &Value, id: &str| {
        v["document"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap()["position"]
            .clone()
    };
    assert_eq!(position(&back, &sampler), position(&view, &sampler));
    assert_eq!(position(&back, &text), position(&view, &text));
    session.apply(EditOp::Redo).unwrap();
    assert_eq!(position(&session.view(), &sampler)["x"], 900.0);

    // Typed exact values: u64 seed stays exact, wrong types refused without change.
    session
        .apply(EditOp::SetParam {
            node: sampler.clone(),
            input: "seed".into(),
            value: Some("18446744073709551614".into()),
        })
        .unwrap();
    assert!(session
        .apply(EditOp::SetParam {
            node: sampler.clone(),
            input: "steps".into(),
            value: Some("vingt".into())
        })
        .is_err());
    let exported = session.export().unwrap();
    assert_eq!(
        exported["5"]["inputs"]["seed"],
        json!(18446744073709551614u64)
    );
    assert_eq!(exported["5"]["inputs"]["steps"], 20);
    assert_eq!(exported["2"]["inputs"]["text"], "exact,  double\nligne");

    // Removing a required source breaks export; undo restores it exactly.
    let latent = node_for(&view, "4");
    session
        .apply(EditOp::Remove {
            nodes: vec![latent],
        })
        .unwrap();
    assert_eq!(session.view()["exportable"], false);
    session.apply(EditOp::Undo).unwrap();
    assert_eq!(session.view()["exportable"], true);

    // Added node gets the next numeric id; incompatible link refused.
    session
        .apply(EditOp::Add {
            class_type: "PreviewImage".into(),
            x: 1200.0,
            y: 0.0,
        })
        .unwrap();
    let preview = session.view()["document"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["classType"] == "PreviewImage")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let decode = node_for(&view, "6");
    assert!(
        session
            .apply(EditOp::Connect {
                from_node: sampler.clone(),
                output: 0,
                to_node: preview.clone(),
                input: "images".into()
            })
            .is_err(),
        "LATENT into IMAGE refused"
    );
    session
        .apply(EditOp::Connect {
            from_node: decode,
            output: 0,
            to_node: preview.clone(),
            input: "images".into(),
        })
        .unwrap();
    let exported = session.export().unwrap();
    assert_eq!(exported["8"]["class_type"], "PreviewImage");
    assert_eq!(exported["8"]["inputs"]["images"], json!(["6", 0]));

    // Groups and layout are single undo steps too.
    session
        .apply(EditOp::Group {
            title: "Texte".into(),
            nodes: vec![text.clone(), node_for(&view, "3")],
        })
        .unwrap();
    assert_eq!(
        session.view()["document"]["groups"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    session.apply(EditOp::Layout).unwrap();
    session.apply(EditOp::Undo).unwrap();
    assert_eq!(position(&session.view(), &sampler)["x"], 900.0);
    assert!(session.apply(EditOp::Undo).is_ok());
    assert_eq!(
        session.view()["document"]["groups"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn unknown_classes_are_refused_at_open() {
    let mut graph = api();
    graph["9"] = json!({"class_type":"MysteryNode","inputs":{}});
    assert!(GraphSession::open("wf", &graph, &info(), "X").is_err());
}

#[test]
fn module_extraction_from_editor_selection_derives_ports_and_keeps_values() {
    use studio_core::modules::{ModuleBinding, ModuleConnection};
    use studio_core::Database;

    let info = info();
    let session = GraphSession::open("wf", &api(), &info, "Édition").unwrap();
    let view = session.view();
    let selection: Vec<String> = ["2", "3", "5", "6", "7"]
        .iter()
        .map(|original| node_for(&view, original))
        .collect();
    let draft = session.extract_module_draft(&selection).unwrap();

    // Ports named by the original ids, derived from the crossing links only.
    let inputs: Vec<String> = draft
        .inputs
        .iter()
        .map(|p| format!("{}.{}", p.node, p.input))
        .collect();
    assert_eq!(
        inputs,
        vec!["2.clip", "3.clip", "5.latent_image", "5.model", "6.vae"]
    );
    let kinds: Vec<&str> = draft.inputs.iter().map(|p| p.kind.as_str()).collect();
    assert_eq!(kinds, vec!["CLIP", "CLIP", "LATENT", "MODEL", "VAE"]);
    assert!(
        draft.outputs.is_empty(),
        "tous les consommateurs du graphe sont internes : aucune sortie attendue"
    );

    // Exact values preserved: u64 seed, exact text, internal links intact.
    assert_eq!(
        draft.document["5"]["inputs"]["seed"].to_string(),
        "18446744073709551615"
    );
    assert_eq!(draft.document["5"]["inputs"]["positive"], json!(["2", 0]));
    assert_eq!(draft.document["6"]["inputs"]["samples"], json!(["5", 0]));
    assert_eq!(
        draft.document["2"]["inputs"]["text"],
        json!("exact,  double\nligne")
    );

    // Provenance: stable fingerprint for an unchanged document, changed by an edit.
    let fingerprint = session.extract_module_fingerprint().unwrap();
    assert_eq!(session.extract_module_fingerprint().unwrap(), fingerprint);
    let mut edited = GraphSession::open("wf", &api(), &info, "Édition").unwrap();
    let edited_view = edited.view();
    edited
        .apply(EditOp::SetParam {
            node: node_for(&edited_view, "5"),
            input: "steps".into(),
            value: Some("30".into()),
        })
        .unwrap();
    assert_ne!(edited.extract_module_fingerprint().unwrap(), fingerprint);

    // Node added in the session: fresh number after the largest original id,
    // internal link rewritten in the module document.
    let mut extended = GraphSession::open("wf", &api(), &info, "Édition").unwrap();
    extended
        .apply(EditOp::Add {
            class_type: "PreviewImage".into(),
            x: 10.0,
            y: 10.0,
        })
        .unwrap();
    let preview = extended.view()["document"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["classType"] == "PreviewImage")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    extended
        .apply(EditOp::Connect {
            from_node: node_for(&extended.view(), "7"),
            output: 0,
            to_node: preview.clone(),
            input: "images".into(),
        })
        .unwrap();
    let extended_view = extended.view();
    let extended_selection = vec![node_for(&extended_view, "7"), preview];
    let extended_draft = extended.extract_module_draft(&extended_selection).unwrap();
    assert!(
        extended_draft
            .document
            .as_object()
            .unwrap()
            .contains_key("8"),
        "numéro frais attendu après le max d'origine : {}",
        extended_draft.document
    );
    assert_eq!(
        extended_draft.document["8"]["inputs"]["images"],
        json!(["7", 0])
    );
    assert!(extended_draft.outputs.is_empty());

    // Extraction of a second, smaller module (selection [5,6]): this time
    // an output crosses the boundary (6.0 feeds 7 left outside).
    let sampler_selection: Vec<String> = ["5", "6"].iter().map(|o| node_for(&view, o)).collect();
    let sampler = session.extract_module_draft(&sampler_selection).unwrap();
    let sampler_inputs: Vec<String> = sampler
        .inputs
        .iter()
        .map(|p| format!("{}.{}", p.node, p.input))
        .collect();
    assert_eq!(
        sampler_inputs,
        vec![
            "5.latent_image",
            "5.model",
            "5.negative",
            "5.positive",
            "6.vae"
        ]
    );
    assert_eq!(sampler.outputs.len(), 1);
    assert_eq!(sampler.outputs[0].name, "6.0");
    assert_eq!(sampler.outputs[0].kind, "IMAGE");

    // Re-insertion into another graph: ports linked by type, correct wires.
    let root = std::env::temp_dir().join(format!("saya-session-module-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let db = Database::open(&root.join("db.sqlite3")).unwrap();
    let (module_id, version) = db
        .save_workflow_module(
            None,
            "Sampling session",
            "",
            Some("wf"),
            &fingerprint,
            &sampler,
        )
        .unwrap();
    assert_eq!(version, 1);
    // Target: the reference graph plus a PreviewImage whose images
    // input is free (like the app's reuse window).
    let mut target_graph = api();
    target_graph["8"] = json!({"class_type": "PreviewImage", "inputs": {}});
    let target = db
        .import_workflow("cible", &target_graph.to_string())
        .unwrap();
    let binding = |port: &str, node: &str, index: u32| ModuleBinding {
        port: port.into(),
        node: node.into(),
        input: None,
        output_index: Some(index),
    };
    let connection = ModuleConnection {
        inputs: vec![
            binding("5.latent_image", "4", 0),
            binding("5.model", "1", 0),
            binding("5.negative", "3", 0),
            binding("5.positive", "2", 0),
            binding("6.vae", "1", 2),
        ],
        outputs: vec![ModuleBinding {
            port: "6.0".into(),
            node: "8".into(),
            input: Some("images".into()),
            output_index: None,
        }],
        overrides: serde_json::json!([]),
    };
    let merged = db
        .module_insert_preview(
            &module_id,
            Some(version),
            &target,
            "smoke",
            &connection,
            &info,
        )
        .unwrap();
    let graph = &merged["graph"];
    assert_eq!(
        graph["smoke::5"]["inputs"]["seed"].to_string(),
        "18446744073709551615"
    );
    assert_eq!(graph["smoke::6"]["inputs"]["vae"], json!(["1", 2]));
    assert_eq!(graph["smoke::5"]["inputs"]["positive"], json!(["2", 0]));
    assert_eq!(
        graph["7"]["inputs"]["images"],
        json!(["6", 0]),
        "le 7 d'origine reste inchangé"
    );
    assert_eq!(graph["8"]["inputs"]["images"], json!(["smoke::6", 0]));
    assert_eq!(merged["addedNodes"].as_array().unwrap().len(), 2);
    let changed = merged["changedInputs"].as_array().unwrap();
    assert_eq!(
        changed.len(),
        1,
        "seule l'entrée images libre de 8 est câblée"
    );
    assert_eq!(changed[0]["node"], "8");
    assert_eq!(changed[0]["after"], json!(["smoke::6", 0]));

    // Unknown selection refused.
    assert!(session
        .extract_module_draft(&["introuvable".to_string()])
        .is_err());
}

#[test]
fn bypass_ui_node_changes_execution_and_reactivation_restores_it() {
    let info=json!({"Source":{"input":{"required":{}},"output":["IMAGE"]},"Filter":{"input":{"required":{"image":["IMAGE"]}},"output":["IMAGE"]},"SaveImage":{"input":{"required":{"images":["IMAGE"],"filename_prefix":["STRING"]}},"output":[],"output_node":true}});
    let ui=json!({"version":0.4,"nodes":[
        {"id":1,"type":"Source","mode":0,"inputs":[],"outputs":[{"name":"image","type":"IMAGE","links":[1]}],"widgets_values":[],"pos":[0,0]},
        {"id":2,"type":"Filter","mode":0,"inputs":[{"name":"image","type":"IMAGE","link":1}],"outputs":[{"name":"image","type":"IMAGE","links":[2]}],"widgets_values":[],"pos":[0,0]},
        {"id":3,"type":"SaveImage","mode":0,"inputs":[{"name":"images","type":"IMAGE","link":2}],"outputs":[],"widgets_values":["x"],"widgets_values_named":{"filename_prefix":"x"},"pos":[0,0]}],"links":[[1,1,0,2,0,"IMAGE"],[2,2,0,3,0,"IMAGE"]],"groups":[]});
    let mut session=GraphSession::open_file(&ui.to_string(),&info,"bypass").unwrap();
    session.apply(EditOp::SetBypass{node:"2".into(),bypass:true}).unwrap();
    let source=session.export_ui().unwrap();assert_eq!(source["nodes"][1]["mode"],4);
    let (api,_)=session.prepare_execution(&info).unwrap();assert!(api.get("2").is_none());assert_eq!(api["3"]["inputs"]["images"],json!(["1",0]));
    let mut reopened=GraphSession::open_file(&source.to_string(),&info,"restored").unwrap();
    reopened.apply(EditOp::SetBypass{node:"2".into(),bypass:false}).unwrap();
    let (api,_)=reopened.prepare_execution(&info).unwrap();assert!(api.get("2").is_some());assert_eq!(api["3"]["inputs"]["images"],json!(["2",0]));
    assert_eq!(ui["nodes"][1]["mode"],0);
}

#[test]
fn bypass_survives_saya_phase_split_and_unknown_node_is_refused() {
    let info=json!({"Source":{"input":{"required":{}},"output":["IMAGE"]},"Filter":{"input":{"required":{"image":["IMAGE"]}},"output":["IMAGE"]},"SaveImage":{"input":{"required":{"images":["IMAGE"],"filename_prefix":["STRING"]}},"output":[],"output_node":true}});
    let ui=json!({"version":0.4,"nodes":[
        {"id":1,"type":"Source","mode":0,"inputs":[],"outputs":[{"name":"image","type":"IMAGE","links":[1]}],"widgets_values":[],"pos":[0,0],"properties":{"saya_phase":1}},
        {"id":2,"type":"Filter","mode":0,"inputs":[{"name":"image","type":"IMAGE","link":1}],"outputs":[{"name":"image","type":"IMAGE","links":[2]}],"widgets_values":[],"pos":[0,0],"properties":{"saya_phase":1}},
        {"id":3,"type":"SaveImage","mode":0,"inputs":[{"name":"images","type":"IMAGE","link":2}],"outputs":[],"widgets_values":["x"],"pos":[0,0],"properties":{"saya_phase":1}}],"links":[[1,1,0,2,0,"IMAGE"],[2,2,0,3,0,"IMAGE"]],"groups":[]});
    let mut session=GraphSession::open_file(&ui.to_string(),&info,"phase-bypass").unwrap();
    session.apply(EditOp::SetBypass{node:"2".into(),bypass:true}).unwrap();
    let source=session.export_ui().unwrap().to_string();
    let phase=studio_core::workflow_ui_api::convert_ui_phase(&source,Some(&info),1).unwrap();
    assert!(phase["api"].get("2").is_none(),"le node bypassé ne part pas à ComfyUI");
    assert_eq!(phase["api"]["3"]["inputs"]["images"],json!(["1",0]));
    session.apply(EditOp::SetBypass{node:"2".into(),bypass:false}).unwrap();
    let phase=studio_core::workflow_ui_api::convert_ui_phase(&session.export_ui().unwrap().to_string(),Some(&info),1).unwrap();
    assert_eq!(phase["api"]["3"]["inputs"]["images"],json!(["2",0]));
    assert!(session.apply(EditOp::SetBypass{node:"999".into(),bypass:true}).is_err());
}

#[test]
fn bypass_reaches_every_embedded_saya_preset_phase_and_restores() {
    let info=json!({"Source":{"input":{"required":{}},"output":["IMAGE"]},"Filter":{"input":{"required":{"image":["IMAGE"]}},"output":["IMAGE"]},"SaveImage":{"input":{"required":{"images":["IMAGE"],"filename_prefix":["STRING"]}},"output":[],"output_node":true}});
    let graph=|mute:i64|json!({"version":0.4,"nodes":[
        {"id":1,"type":"Source","mode":0,"inputs":[],"outputs":[{"name":"image","type":"IMAGE","links":[1]}],"widgets_values":[],"pos":[0,0]},
        {"id":2,"type":"Filter","mode":mute,"inputs":[{"name":"image","type":"IMAGE","link":1}],"outputs":[{"name":"image","type":"IMAGE","links":[2]}],"widgets_values":[],"pos":[0,0]},
        {"id":3,"type":"SaveImage","mode":0,"inputs":[{"name":"images","type":"IMAGE","link":2}],"outputs":[],"widgets_values":["x"],"pos":[0,0]}],"links":[[1,1,0,2,0,"IMAGE"],[2,2,0,3,0,"IMAGE"]],"groups":[]});
    // Six embedded passes; pass 6 already mutes the filter (mute) and must stay so.
    let mut source=graph(0);
    source["extra"]=json!({"saya_preset":{"version":1,"phases":[graph(0),graph(0),graph(0),graph(0),graph(0),graph(2)]}});
    let mut session=GraphSession::open_file(&source.to_string(),&info,"preset-bypass").unwrap();
    session.apply(EditOp::SetBypass{node:"2".into(),bypass:true}).unwrap();
    let edited=session.export_ui().unwrap();
    for phase in 1..=5 {
        let pass=studio_core::saya_preset::phase_source(&edited,phase).unwrap().unwrap();
        let api=studio_core::workflow_ui_api::convert_ui_phase(&pass.to_string(),Some(&info),phase).unwrap();
        assert!(api["api"].get("2").is_none(),"passe {phase}");
        assert_eq!(api["api"]["3"]["inputs"]["images"],json!(["1",0]));
    }
    let muted=studio_core::saya_preset::phase_source(&edited,6).unwrap().unwrap();
    assert_eq!(muted["nodes"][1]["mode"],2);
    session.apply(EditOp::SetBypass{node:"2".into(),bypass:false}).unwrap();
    let restored=studio_core::saya_preset::phase_source(&session.export_ui().unwrap(),1).unwrap().unwrap();
    let api=studio_core::workflow_ui_api::convert_ui_phase(&restored.to_string(),Some(&info),1).unwrap();
    assert_eq!(api["api"]["3"]["inputs"]["images"],json!(["2",0]));
}
