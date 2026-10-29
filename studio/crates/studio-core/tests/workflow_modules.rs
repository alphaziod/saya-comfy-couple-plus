use serde_json::{json, Value};
use studio_core::modules::{
    extract_module, instantiate_module, ModuleBinding, ModuleConnection, ModuleDraft,
    MANIFEST_VERSION,
};
use studio_core::Database;

/// Fixture inspired by docs/WORKFLOWS-REELS.md: checkpoint → LoraManager chain
/// strict composite → KSampler (exact u64 seed) → VAEDecode → SaveImage.
const RAW_GRAPH: &str = r#"{
 "1": {"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"saya.safetensors"}},
 "2": {"class_type":"CLIPTextEncode","inputs":{"clip":["5",1],"text":"P1: silver hair, blue eyes"}},
 "3": {"class_type":"CLIPTextEncode","inputs":{"clip":["5",1],"text":"blurry"}},
 "4": {"class_type":"EmptyLatentImage","inputs":{"batch_size":1,"height":1024,"width":1024}},
 "5": {"class_type":"Lora Loader (LoraManager)","inputs":{"clip":["1",1],"loras":{"__value__":[{"active":true,"clipStrength":0.9,"expanded":true,"locked":false,"name":"saya-fix","strength":0.8}]},"model":["1",0],"text":"<lora:saya-fix:0.8> précis"}},
 "6": {"class_type":"KSampler","inputs":{"cfg":7.0,"denoise":1.0,"latent_image":["4",0],"model":["5",0],"negative":["3",0],"positive":["2",0],"sampler_name":"euler","scheduler":"normal","seed":123456789012345,"steps":20}},
 "7": {"class_type":"VAEDecode","inputs":{"samples":["6",0],"vae":["1",2]}},
 "8": {"class_type":"SaveImage","inputs":{"filename_prefix":"saya","images":["7",0]}}
}"#;

/// Expected canonical copy of the selected subgraph ["5","6","7"]: sorted
/// keys, compact separation, u64 seed and composite LoraManager widget
/// intact. Hand-written constant: the comparison proves that
/// the extraction rewrote, lost and normalized nothing.
const EXPECTED_DOCUMENT: &str = concat!(
    r#"{"2":{"class_type":"CLIPTextEncode","inputs":{"clip":["5",1],"text":"P1: silver hair, blue eyes"}},"#,
    r#""3":{"class_type":"CLIPTextEncode","inputs":{"clip":["5",1],"text":"blurry"}},"#,
    r#""5":{"class_type":"Lora Loader (LoraManager)","inputs":{"clip":["1",1],"loras":{"__value__":[{"active":true,"clipStrength":0.9,"expanded":true,"locked":false,"name":"saya-fix","strength":0.8}]},"model":["1",0],"text":"<lora:saya-fix:0.8> précis"}},"#,
    r#""6":{"class_type":"KSampler","inputs":{"cfg":7.0,"denoise":1.0,"latent_image":["4",0],"model":["5",0],"negative":["3",0],"positive":["2",0],"sampler_name":"euler","scheduler":"normal","seed":123456789012345,"steps":20}},"#,
    r#""7":{"class_type":"VAEDecode","inputs":{"samples":["6",0],"vae":["1",2]}}}"#
);

fn object(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn node_info() -> Value {
    json!({
        "CheckpointLoaderSimple":{"input":{"required":{"ckpt_name":[["saya.safetensors","other.safetensors"]]}},"output":["MODEL","CLIP","VAE"],"output_name":["MODEL","CLIP","VAE"]},
        "CLIPTextEncode":{"input":{"required":{"text":["STRING"],"clip":["CLIP"]}},"output":["CONDITIONING"]},
        "EmptyLatentImage":{"input":{"required":{"width":["INT"],"height":["INT"],"batch_size":["INT"]}},"output":["LATENT"]},
        "KSampler":{"input":{"required":{"model":["MODEL"],"seed":["INT"],"steps":["INT"],"cfg":["FLOAT"],"sampler_name":[["euler","dpmpp_2m"]],"scheduler":[["normal","karras"]],"denoise":["FLOAT"],"positive":["CONDITIONING"],"negative":["CONDITIONING"],"latent_image":["LATENT"]}},"output":["LATENT"]},
        "VAEDecode":{"input":{"required":{"samples":["LATENT"],"vae":["VAE"]}},"output":["IMAGE"]},
        "SaveImage":{"input":{"required":{"images":["IMAGE"],"filename_prefix":["STRING"]}},"output":[],"output_node":true},
        "Lora Loader (LoraManager)":{"input":{"required":{"model":["MODEL"],"text":["AUTOCOMPLETE_TEXT_LORAS"],"loras":["LORAS"]},"optional":{"__lm_autocomplete_meta_text":["STRING"]}},"output":["MODEL","CLIP","STRING","STRING"]},
        "LatentPass":{"input":{"required":{"latent":["LATENT"]}},"output":["LATENT"]},
        "ImagePass":{"input":{"required":{"image":["IMAGE"]}},"output":["IMAGE"]},
        "PreviewImage":{"input":{"required":{"images":["IMAGE"]}},"output":[],"output_node":true}
    })
}

const SELECTION: [&str; 5] = ["2", "3", "5", "6", "7"];

fn draft_selection(graph: &Value, info: &Value, selection: &[&str]) -> ModuleDraft {
    extract_module(graph, info, selection).unwrap()
}

fn expected_ports() -> Value {
    json!({
        "inputs":[
            {"name":"5.clip","kind":"CLIP","node":"5","input":"clip"},
            {"name":"5.model","kind":"MODEL","node":"5","input":"model"},
            {"name":"6.latent_image","kind":"LATENT","node":"6","input":"latent_image"},
            {"name":"7.vae","kind":"VAE","node":"7","input":"vae"},
        ],
        "outputs":[
            {"name":"7.0","kind":"IMAGE","node":"7","outputIndex":0},
        ],
    })
}

#[test]
fn extracts_module_from_api_fixture_with_derived_ports() {
    let db = Database::in_memory().unwrap();
    let graph = object(RAW_GRAPH);
    let info = node_info();
    let draft = draft_selection(&graph, &info, &SELECTION);
    assert_eq!(
        serde_json::to_value(&draft.inputs).unwrap(),
        expected_ports()["inputs"]
    );
    assert_eq!(
        serde_json::to_value(&draft.outputs).unwrap(),
        expected_ports()["outputs"]
    );
    let parameters: Vec<(String, String, String, String)> = draft
        .parameters
        .iter()
        .map(|p| {
            (
                p.node.clone(),
                p.input.clone(),
                p.kind.clone(),
                p.value.clone(),
            )
        })
        .collect();
    assert_eq!(
        parameters,
        vec![
            (
                "2".into(),
                "text".into(),
                "string".into(),
                "P1: silver hair, blue eyes".into()
            ),
            ("3".into(), "text".into(), "string".into(), "blurry".into()),
            ("6".into(), "cfg".into(), "number".into(), "7.0".into()),
            ("6".into(), "denoise".into(), "number".into(), "1.0".into()),
            (
                "6".into(),
                "sampler_name".into(),
                "string".into(),
                "euler".into()
            ),
            (
                "6".into(),
                "scheduler".into(),
                "string".into(),
                "normal".into()
            ),
            (
                "6".into(),
                "seed".into(),
                "seed".into(),
                "123456789012345".into()
            ),
            ("6".into(), "steps".into(), "integer".into(), "20".into()),
        ]
    );
    assert_eq!(
        serde_json::to_value(&draft.dependencies).unwrap(),
        json!({
            "classes":["CLIPTextEncode","KSampler","Lora Loader (LoraManager)","VAEDecode"],
            "checkpoints":[],
            "loras":["saya-fix"],
        })
    );

    let (id, v1) = db
        .save_workflow_module(
            None,
            "Détail Saya",
            "Chaîne sampling réutilisable",
            None,
            "hash-a",
            &draft,
        )
        .unwrap();
    assert_eq!(v1, 1);
    // Equivalent extraction from a saved workflow revision.
    let workflow = db.import_workflow("source", RAW_GRAPH).unwrap();
    let selection: Vec<String> = SELECTION.iter().map(|s| s.to_string()).collect();
    let (id2, v2) = db
        .save_module_from_workflow(
            &workflow,
            Some(&id),
            "Détail Saya v2",
            "",
            &selection,
            &info,
        )
        .unwrap();
    assert_eq!(id2, id);
    assert_eq!(v2, 2);

    let list = db.workflow_modules().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["name"], "Détail Saya");
    assert_eq!(list[0]["currentVersion"], 2);
    assert_eq!(list[0]["versionCount"], 2);
    assert_eq!(list[0]["inputs"], expected_ports()["inputs"]);
    let detail = db.module_detail(&id, None, Some(&info)).unwrap();
    assert_eq!(detail["version"], 2);
    assert_eq!(detail["manifest"]["manifestVersion"], MANIFEST_VERSION);
    assert_eq!(detail["manifest"]["inputs"], expected_ports()["inputs"]);
    assert_eq!(detail["nodeCount"], 5);
    assert_eq!(detail["sourceWorkflowId"], workflow);
    assert!(detail["warnings"].as_array().unwrap().is_empty());
    assert_eq!(detail["versions"].as_array().unwrap().len(), 2);
}

#[test]
fn preserves_selected_nodes_and_internal_links_byte_for_byte() {
    let db = Database::in_memory().unwrap();
    let graph = object(RAW_GRAPH);
    let draft = draft_selection(&graph, &node_info(), &SELECTION);
    // Exact canonical copy: bytes equal to the hand-written constant.
    assert_eq!(
        serde_json::to_string(&draft.document).unwrap(),
        EXPECTED_DOCUMENT
    );
    // Internal links intact, composite values not dismantled.
    let document = object(EXPECTED_DOCUMENT);
    assert_eq!(document["6"]["inputs"]["model"], json!(["5", 0]));
    assert_eq!(document["7"]["inputs"]["samples"], json!(["6", 0]));
    assert_eq!(
        document["5"]["inputs"]["loras"]["__value__"][0]["strength"],
        json!(0.8)
    );
    assert_eq!(document["6"]["inputs"]["seed"], json!(123456789012345u64));

    let (id, _) = db
        .save_workflow_module(None, "Préservation", "", None, "hash", &draft)
        .unwrap();
    let detail = db.module_detail(&id, Some(1), None).unwrap();
    // After SQLite write then re-read: same bytes.
    assert_eq!(
        serde_json::to_string(&detail["document"]).unwrap(),
        EXPECTED_DOCUMENT
    );
}

#[test]
fn reopening_a_separate_database_reads_the_same_bytes() {
    let root = std::env::temp_dir().join(format!("saya-modules-bytes-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let draft = draft_selection(&object(RAW_GRAPH), &node_info(), &SELECTION);
    let id = {
        let db = Database::open(&root.join("db.sqlite3")).unwrap();
        db.save_workflow_module(None, "Préservation", "", None, "hash", &draft)
            .unwrap()
            .0
    };
    let db = Database::open(&root.join("db.sqlite3")).unwrap();
    let detail = db.module_detail(&id, Some(1), None).unwrap();
    // After SQLite write, close and reopen: same bytes.
    assert_eq!(
        serde_json::to_string(&detail["document"]).unwrap(),
        EXPECTED_DOCUMENT
    );
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn refuses_ambiguous_cuts_with_diagnostics() {
    let info = node_info();

    // Unknown type on the input side: class missing from object_info.
    let mut unknown_class = object(RAW_GRAPH);
    unknown_class["9"] = json!({"class_type":"GhostNode","inputs":{"clip":["1",1]}});
    let error = extract_module(&unknown_class, &info, &["5", "6", "7", "9"]).unwrap_err();
    assert!(error.to_string().contains("Type inconnu"), "{error}");
    assert!(error.to_string().contains("GhostNode"), "{error}");

    // Unknown type on the output side: outputs of the class absent.
    let mut unknown_output = object(RAW_GRAPH);
    unknown_output["9"] = json!({"class_type":"GhostNode","inputs":{}});
    unknown_output["8"]["inputs"]["images"] = json!(["9", 0]);
    let error = extract_module(&unknown_output, &info, &["9"]).unwrap_err();
    assert!(error.to_string().contains("Type inconnu"), "{error}");

    // Non-convex boundary: an external node consumes an output of the module
    // then feeds the selection again.
    let mut looped = object(RAW_GRAPH);
    looped["9"] = json!({"class_type":"LatentPass","inputs":{"latent":["6",0]}});
    looped["7"]["inputs"]["samples"] = json!(["9", 0]);
    let error = extract_module(&looped, &info, &SELECTION).unwrap_err();
    assert!(error.to_string().contains("non convexe"), "{error}");
    assert!(error.to_string().contains("6.0"), "{error}");

    // Broken link: the source does not exist in the workflow.
    let mut broken = object(RAW_GRAPH);
    broken["6"]["inputs"]["positive"] = json!(["99", 0]);
    let error = extract_module(&broken, &info, &SELECTION).unwrap_err();
    assert!(error.to_string().contains("Lien brisé"), "{error}");
    assert!(error.to_string().contains("99"), "{error}");

    // Invalid selections.
    let graph = object(RAW_GRAPH);
    for (selection, expected) in [
        (vec!["5", "42"], "absent du workflow"),
        (vec!["5", "5"], "deux fois"),
        (vec![], "vide"),
    ] {
        let error = extract_module(&graph, &info, &selection).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn versions_are_immutable_and_rollback_only_moves_the_pointer() {
    let root = std::env::temp_dir().join(format!("saya-modules-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let db = Database::open(&root.join("db.sqlite3")).unwrap();
    assert!(db.status().unwrap().schema_version >= 16);
    let info = node_info();
    let graph = object(RAW_GRAPH);
    let draft_v1 = draft_selection(&graph, &info, &SELECTION);
    let (id, v1) = db
        .save_workflow_module(None, "Versionné", "", None, "hash-1", &draft_v1)
        .unwrap();
    let mut changed = object(RAW_GRAPH);
    changed["6"]["inputs"]["seed"] = json!(999u64);
    let draft_v2 = draft_selection(&changed, &info, &SELECTION);
    let (_, v2) = db
        .save_workflow_module(Some(&id), "Versionné", "", None, "hash-2", &draft_v2)
        .unwrap();
    assert_eq!((v1, v2), (1, 2));
    assert_eq!(db.module_detail(&id, None, None).unwrap()["version"], 2);

    // Versions are immutable: UPDATE and DELETE refused by triggers.
    let raw = rusqlite::Connection::open(root.join("db.sqlite3")).unwrap();
    let update = raw.execute(
        "UPDATE workflow_module_versions SET document_json='{}' WHERE module_id=? AND version=1",
        [&id],
    );
    assert!(update.is_err());
    assert!(update.unwrap_err().to_string().contains("immutable"));
    let delete = raw.execute(
        "DELETE FROM workflow_module_versions WHERE module_id=?",
        [&id],
    );
    assert!(delete.is_err());
    assert!(delete.unwrap_err().to_string().contains("immutable"));
    drop(raw);

    // Rollback only moves the pointer: v2 stays readable identically.
    db.rollback_module(&id, 1).unwrap();
    let current = db.module_detail(&id, None, None).unwrap();
    assert_eq!(current["version"], 1);
    assert_eq!(current["isCurrent"], true);
    let second = db.module_detail(&id, Some(2), None).unwrap();
    assert_eq!(second["isCurrent"], false);
    assert_eq!(
        serde_json::to_string(&second["document"]).unwrap(),
        serde_json::to_string(&draft_v2.document).unwrap()
    );
    assert!(second["document"]["6"]["inputs"]["seed"] == json!(999u64));
    assert!(db.rollback_module(&id, 7).is_err());

    // A version recorded after rollback continues the MAX+1 numbering.
    let (_, v3) = db
        .save_workflow_module(Some(&id), "Versionné", "", None, "hash-3", &draft_v1)
        .unwrap();
    assert_eq!(v3, 3);
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn compares_versions_with_exact_diff() {
    let db = Database::in_memory().unwrap();
    let info = node_info();
    let draft_v1 = draft_selection(&object(RAW_GRAPH), &info, &SELECTION);
    let (id, v1) = db
        .save_workflow_module(None, "Diff", "", None, "hash-1", &draft_v1)
        .unwrap();
    let mut changed = object(RAW_GRAPH);
    changed["6"]["inputs"]["cfg"] = json!(8.0);
    let draft_v2 = draft_selection(&changed, &info, &["2", "3", "5", "6"]);
    let (_, v2) = db
        .save_workflow_module(Some(&id), "Diff", "", None, "hash-2", &draft_v2)
        .unwrap();
    let diff = db.compare_module_versions(&id, v1, v2).unwrap();
    assert_eq!(
        diff,
        json!({
            "module":id,"a":1,"b":2,
            "nodes":{
                "added":[],
                "removed":[{"id":"7","classType":"VAEDecode"}],
                "changed":[{"id":"6","classChanged":false,"inputs":{"added":[],"removed":[],
                    "changed":[{"input":"cfg","before":json!(7.0),"after":json!(8.0)}]}}],
            },
            "ports":{
                "inputs":{"added":[],"removed":[{"name":"7.vae","kind":"VAE","node":"7","input":"vae"}],"changed":[]},
                "outputs":{"added":[{"name":"6.0","kind":"LATENT","node":"6","outputIndex":0}],
                           "removed":[{"name":"7.0","kind":"IMAGE","node":"7","outputIndex":0}],"changed":[]},
            },
            "parameters":{
                "added":[],
                "removed":[],
                "changed":[{"node":"6","input":"cfg",
                    "before":{"node":"6","input":"cfg","kind":"number","value":"7.0"},
                    "after":{"node":"6","input":"cfg","kind":"number","value":"8.0"}}],
            },
        })
    );
    // Symmetric diff: both directions can be queried without rewriting.
    let reverse = db.compare_module_versions(&id, v2, v1).unwrap();
    assert_eq!(
        reverse["nodes"]["added"],
        json!([{"id":"7","classType":"VAEDecode"}])
    );
    assert_eq!(reverse["nodes"]["removed"], json!([]));
    assert!(db.compare_module_versions(&id, 1, 9).is_err());
}

fn pipeline() -> Value {
    json!({
        "a":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"saya.safetensors"}},
        "c":{"class_type":"CLIPTextEncode","inputs":{"clip":["a",1],"text":"pipe"}},
        "d":{"class_type":"EmptyLatentImage","inputs":{"batch_size":1,"height":512,"width":512}},
        "b":{"class_type":"SaveImage","inputs":{"filename_prefix":"saya-pipe","images":"PENDING"}},
    })
}

fn input_binding(port: &str, node: &str, output_index: Option<u32>) -> ModuleBinding {
    ModuleBinding {
        port: port.into(),
        node: node.into(),
        input: None,
        output_index,
    }
}

fn full_bindings() -> (Vec<ModuleBinding>, Vec<ModuleBinding>) {
    let inputs = vec![
        input_binding("5.model", "a", Some(0)),
        input_binding("5.clip", "a", Some(1)),
        input_binding("6.latent_image", "d", Some(0)),
        input_binding("7.vae", "a", Some(2)),
    ];
    let outputs = vec![ModuleBinding {
        port: "7.0".into(),
        node: "b".into(),
        input: Some("images".into()),
        output_index: None,
    }];
    (inputs, outputs)
}

fn connection(
    inputs: &[ModuleBinding],
    outputs: &[ModuleBinding],
    overrides: Value,
) -> ModuleConnection {
    ModuleConnection {
        inputs: inputs.to_vec(),
        outputs: outputs.to_vec(),
        overrides,
    }
}

#[test]
fn connects_module_into_pipeline_with_validated_contract() {
    let db = Database::in_memory().unwrap();
    let info = node_info();
    let (id, v1) = db
        .save_workflow_module(
            None,
            "Connectable",
            "",
            None,
            "hash",
            &draft_selection(&object(RAW_GRAPH), &info, &SELECTION),
        )
        .unwrap();
    let pipeline = pipeline();
    let before = pipeline.clone();
    let (inputs, outputs) = full_bindings();
    let overrides = json!([
        {"node":"6","input":"seed","value":"987654321"},
        {"node":"6","input":"cfg","value":9.5},
    ]);
    let connected = db
        .connect_module(
            &id,
            v1,
            "m1",
            &pipeline,
            &connection(&inputs, &outputs, overrides),
            &info,
        )
        .unwrap();
    // The contract modifies neither the supplied pipeline nor the registered module.
    assert_eq!(pipeline, before);
    let graph = &connected["graph"];
    assert_eq!(graph["b"]["inputs"]["images"], json!(["m1::7", 0]));
    assert_eq!(graph["m1::6"]["inputs"]["seed"], json!(987654321u64));
    assert_eq!(graph["m1::6"]["inputs"]["cfg"], json!(9.5));
    assert_eq!(graph["m1::6"]["inputs"]["model"], json!(["m1::5", 0]));
    assert_eq!(
        graph["m1::5"]["inputs"]["loras"]["__value__"][0]["name"],
        "saya-fix"
    );
    assert_eq!(graph["m1::7"]["inputs"]["vae"], json!(["a", 2]));
    // Full engine validation ran: no missing-class warning.
    let warnings = connected["warnings"].as_array().unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(connected["connections"].as_array().unwrap().len(), 5);

    // Missing custom node: dependency listed, use warned, no refusal.
    let mut partial_info = info.clone();
    partial_info
        .as_object_mut()
        .unwrap()
        .remove("Lora Loader (LoraManager)");
    let warned = db
        .connect_module(
            &id,
            v1,
            "m1",
            &pipeline,
            &connection(&inputs, &outputs, json!([])),
            &partial_info,
        )
        .unwrap();
    let warnings = warned["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("Custom node absent")),
        "{warnings:?}"
    );
    assert!(
        warnings.iter().any(|w| w
            .as_str()
            .unwrap()
            .contains("Validation moteur complète ignorée")),
        "{warnings:?}"
    );
    // Pipeline without an output node: contract validated by types only, warned.
    let mut without_output = pipeline.clone();
    without_output.as_object_mut().unwrap().remove("b");
    let outputs_none: Vec<ModuleBinding> = Vec::new();
    let headless = db
        .connect_module(
            &id,
            v1,
            "m1",
            &without_output,
            &connection(&inputs, &outputs_none, json!([])),
            &info,
        )
        .unwrap();
    assert!(headless["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("Aucun node de sortie")));
}

#[test]
fn refuses_connection_on_incompatible_types_and_ambiguous_bindings() {
    let db = Database::in_memory().unwrap();
    let info = node_info();
    let (id, v1) = db
        .save_workflow_module(
            None,
            "Strict",
            "",
            None,
            "hash",
            &draft_selection(&object(RAW_GRAPH), &info, &SELECTION),
        )
        .unwrap();
    let connect = |pipeline: &Value,
                   inputs: &[ModuleBinding],
                   outputs: &[ModuleBinding],
                   overrides: &Value| {
        db.connect_module(
            &id,
            v1,
            "m1",
            pipeline,
            &connection(inputs, outputs, overrides.clone()),
            &info,
        )
        .map(|v| v["graph"].clone())
    };
    let pipeline = pipeline();

    // Incompatible type: pipeline LATENT output to the module's MODEL port.
    let (mut inputs, outputs) = full_bindings();
    inputs[0] = ModuleBinding {
        port: "5.model".into(),
        node: "d".into(),
        input: None,
        output_index: Some(0),
    };
    let error = connect(&pipeline, &inputs, &outputs, &json!([])).unwrap_err();
    assert!(
        error.to_string().contains("Types de lien incompatibles"),
        "{error}"
    );
    assert!(error.to_string().contains("MODEL"), "{error}");

    // Unknown port, missing port, duplicate binding.
    let (inputs, outputs) = full_bindings();
    let mut unknown = inputs.clone();
    unknown.push(input_binding("inconnu", "a", Some(0)));
    assert!(connect(&pipeline, &unknown, &outputs, &json!([])).is_err());
    let error = connect(&pipeline, &inputs[..3], &outputs, &json!([])).unwrap_err();
    assert!(
        error.to_string().contains("Port d'entrée non connecté"),
        "{error}"
    );
    let mut duplicated = inputs.clone();
    duplicated.push(inputs[0].clone());
    let error = connect(&pipeline, &duplicated, &outputs, &json!([])).unwrap_err();
    assert!(
        error.to_string().contains("connecté plusieurs fois"),
        "{error}"
    );

    // Target input already fed by a link: double feed refused.
    let mut fed = pipeline.clone();
    fed["e"] = json!({"class_type":"ImagePass","inputs":{"image":"PENDING"}});
    fed["g"] = json!({"class_type":"PreviewImage","inputs":{"images":["e",0]}});
    let fed_target = vec![ModuleBinding {
        port: "7.0".into(),
        node: "g".into(),
        input: Some("images".into()),
        output_index: None,
    }];
    let error = connect(&fed, &inputs, &fed_target, &json!([])).unwrap_err();
    assert!(
        error.to_string().contains("Entrée déjà alimentée"),
        "{error}"
    );

    // Target connected twice.
    let double_target = vec![
        ModuleBinding {
            port: "7.0".into(),
            node: "b".into(),
            input: Some("images".into()),
            output_index: None,
        },
        ModuleBinding {
            port: "7.0".into(),
            node: "b".into(),
            input: Some("images".into()),
            output_index: None,
        },
    ];
    let error = connect(&pipeline, &inputs, &double_target, &json!([])).unwrap_err();
    assert!(error.to_string().contains("connectée deux fois"), "{error}");

    // Parameter not exposed and parameter already fed internally (invariant).
    let error = connect(
        &pipeline,
        &inputs,
        &outputs,
        &json!([{"node":"6","input":"secret","value":1}]),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("Paramètre non exposé"),
        "{error}"
    );
    let manifest = db.module_detail(&id, Some(1), None).unwrap()["manifest"].clone();
    let manifest: studio_core::modules::ModuleManifest = serde_json::from_value(manifest).unwrap();
    let mut corrupted = object(EXPECTED_DOCUMENT);
    corrupted["6"]["inputs"]["cfg"] = json!(["9", 0]);
    let error = instantiate_module(
        &manifest,
        &corrupted,
        "m1",
        &pipeline,
        &connection(
            &inputs,
            &outputs,
            json!([{"node":"6","input":"cfg","value":9.0}]),
        ),
        &info,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("déjà alimenté en interne"),
        "{error}"
    );
}

#[test]
fn modules_persist_after_reopen_and_migration_stays_additive() {
    let root = std::env::temp_dir().join(format!("saya-modules-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let info = node_info();
    {
        let db = Database::open(&root.join("db.sqlite3")).unwrap();
        let workflow = db.import_workflow("source", RAW_GRAPH).unwrap();
        let selection: Vec<String> = SELECTION.iter().map(|s| s.to_string()).collect();
        let (id, v1) = db
            .save_module_from_workflow(&workflow, None, "Persistant", "desc", &selection, &info)
            .unwrap();
        let draft_v2 = draft_selection(&object(RAW_GRAPH), &info, &["2", "3", "5"]);
        db.save_workflow_module(Some(&id), "Persistant", "desc", None, "hash-2", &draft_v2)
            .unwrap();
        db.rollback_module(&id, v1).unwrap();
    }
    let db = Database::open(&root.join("db.sqlite3")).unwrap();
    assert!(db.status().unwrap().schema_version >= 16);
    let list = db.workflow_modules().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["name"], "Persistant");
    assert_eq!(list[0]["currentVersion"], 1);
    assert_eq!(list[0]["versionCount"], 2);
    let detail = db
        .module_detail(list[0]["id"].as_str().unwrap(), Some(1), Some(&info))
        .unwrap();
    assert_eq!(
        serde_json::to_string(&detail["document"]).unwrap(),
        EXPECTED_DOCUMENT
    );
    assert_eq!(detail["sourceSha256"].as_str().unwrap().len(), 64);
    let diff = db
        .compare_module_versions(list[0]["id"].as_str().unwrap(), 1, 2)
        .unwrap();
    // v2 keeps {2,3,5}: nodes 6 and 7 are absent from this version.
    assert_eq!(diff["nodes"]["removed"].as_array().unwrap().len(), 2);
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn workflow_nodes_and_typed_candidates_for_module_reuse() {
    let db = Database::in_memory().unwrap();
    let info = node_info();
    let workflow = db.import_workflow("Source", RAW_GRAPH).unwrap();
    let selection: Vec<String> = SELECTION.iter().map(|s| s.to_string()).collect();
    let (module, version) = db
        .save_module_from_workflow(&workflow, None, "Bloc", "", &selection, &info)
        .unwrap();
    let nodes = db.workflow_nodes(&workflow).unwrap();
    let ids: Vec<&str> = nodes.iter().map(|n| n["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["1", "2", "3", "4", "5", "6", "7", "8"]);
    assert_eq!(nodes[5]["classType"], "KSampler");

    let target = db
        .import_workflow(
            "Cible",
            r#"{"1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"saya.safetensors"}},
                "4":{"class_type":"EmptyLatentImage","inputs":{"batch_size":1,"height":512,"width":512}},
                "9":{"class_type":"PreviewImage","inputs":{"images":["10",0]}},
                "10":{"class_type":"ImagePass","inputs":{"image":["10",0]}},
                "11":{"class_type":"MysteryNode","inputs":{}}}"#,
        )
        .unwrap();
    let candidates = db
        .module_candidates(&module, Some(version), &target, &info)
        .unwrap();
    let port = |name: &str| {
        candidates["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["port"] == name)
            .unwrap()
            .clone()
    };
    // MODEL only from the checkpoint loader output 0; CLIP from output 1.
    assert_eq!(
        port("5.model")["sources"],
        json!([{"node":"1","outputIndex":0,"classType":"CheckpointLoaderSimple","kind":"MODEL","title":null}])
    );
    assert_eq!(port("5.clip")["sources"][0]["outputIndex"], 1);
    assert_eq!(port("6.latent_image")["sources"][0]["node"], "4");
    // IMAGE output feeds PreviewImage.images and ImagePass.image (both linked).
    let targets = &candidates["outputs"][0]["targets"];
    assert_eq!(targets.as_array().unwrap().len(), 2);
    assert!(targets
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["linked"] == true && t["kind"] == "IMAGE"));
    // Unknown classes are reported, never guessed.
    assert_eq!(
        candidates["unknownWorkflowNodes"],
        json!(["11 (MysteryNode)"])
    );
    assert!(db
        .module_candidates("missing", None, &target, &info)
        .is_err());
    assert!(db.workflow_nodes("missing").is_err());
}

#[test]
fn insert_preview_reports_added_nodes_and_rewired_inputs_without_touching_the_workflow() {
    let db = Database::in_memory().unwrap();
    let info = node_info();
    let source = db.import_workflow("Source", RAW_GRAPH).unwrap();
    let selection: Vec<String> = ["6", "7"].iter().map(|s| s.to_string()).collect();
    let (module, _) = db
        .save_module_from_workflow(&source, None, "Sampler+Decode", "", &selection, &info)
        .unwrap();
    // Target: the source graph plus a PreviewImage whose `images` input is free.
    let mut graph: Value = serde_json::from_str(RAW_GRAPH).unwrap();
    graph["9"] = json!({"class_type":"PreviewImage","inputs":{}});
    let target = db.import_workflow("Cible", &graph.to_string());
    let target = match target {
        Ok(id) => id,
        Err(error) => panic!("import refused a free input workflow: {error}"),
    };
    let before = db.workflow_nodes(&target).unwrap();
    let candidates = db.module_candidates(&module, None, &target, &info).unwrap();
    let mut inputs = Vec::new();
    for port in candidates["inputs"].as_array().unwrap() {
        let source = &port["sources"][0];
        inputs.push(ModuleBinding {
            port: port["port"].as_str().unwrap().into(),
            node: source["node"].as_str().unwrap().into(),
            input: None,
            output_index: Some(source["outputIndex"].as_u64().unwrap() as u32),
        });
    }
    let outputs = vec![ModuleBinding {
        port: "7.0".into(),
        node: "9".into(),
        input: Some("images".into()),
        output_index: None,
    }];
    let connection = ModuleConnection {
        inputs,
        outputs,
        overrides: json!([]),
    };
    let preview = db
        .module_insert_preview(&module, None, &target, "copy", &connection, &info)
        .unwrap();
    assert_eq!(preview["workflow"]["nodeCount"], 9);
    assert_eq!(preview["addedNodes"].as_array().unwrap().len(), 2);
    let changed = preview["changedInputs"].as_array().unwrap();
    assert_eq!(changed.len(), 1, "{changed:?}");
    assert_eq!(changed[0]["node"], "9");
    assert_eq!(changed[0]["input"], "images");
    assert_eq!(changed[0]["before"], Value::Null);
    // Candidates mark the free input as usable and linked ones as not rewirable.
    let targets = candidates["outputs"][0]["targets"]
        .as_array()
        .unwrap()
        .clone();
    assert!(targets
        .iter()
        .any(|t| t["node"] == "9" && t["linked"] == false));
    assert!(targets
        .iter()
        .any(|t| t["node"] == "8" && t["linked"] == true));
    // Stored workflow untouched.
    assert_eq!(db.workflow_nodes(&target).unwrap(), before);
    assert!(db
        .module_insert_preview(&module, None, &target, "bad id!", &connection, &info)
        .is_err());
}
