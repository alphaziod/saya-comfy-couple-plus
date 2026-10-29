use serde_json::{json, Value};
use studio_core::workflow_ui_api::{convert_ui, prepare_api, save_api_revision};
use studio_core::{graph_session::GraphSession, Database};

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

fn workflow(nodes: Value, links: Value, subgraphs: Value) -> String {
    json!({
        "id": "test",
        "revision": 0,
        "nodes": nodes,
        "links": links,
        "groups": [],
        "definitions": {"subgraphs": subgraphs},
        "version": 0.4
    })
    .to_string()
}

fn node(id: u64, class: &str, mode: u64, inputs: Value, outputs: Value, named: Value) -> Value {
    json!({
        "id": id,
        "type": class,
        "mode": mode,
        "inputs": inputs,
        "outputs": outputs,
        "widgets_values_named": named,
        "widgets_values": []
    })
}

fn subgraph(
    id: &str,
    name: &str,
    inputs: Value,
    outputs: Value,
    nodes: Value,
    links: Value,
) -> Value {
    json!({
        "id": id,
        "name": name,
        "inputNode": {"id": -10},
        "outputNode": {"id": -20},
        "inputs": inputs,
        "outputs": outputs,
        "nodes": nodes,
        "links": links
    })
}

fn info() -> Value {
    json!({
        "Gen": {"input": {"required": {"text": ["STRING"]}}, "output": ["IMAGE"]},
        "SaveImage": {"input": {"required": {"images": ["IMAGE"]}}, "output": []},
        "Loader": {"input": {"required": {}}, "output": ["MODEL"]},
        "Consumer": {"input": {"required": {"model": ["MODEL"]}}, "output": []},
        "Pass": {"input": {"required": {"model": ["MODEL"]}}, "output": ["MODEL"]},
        "Enc": {"input": {"required": {"text": ["STRING"], "seed": ["INT"]}}, "output": []}
    })
}

fn simple_subgraph() -> Value {
    subgraph(
        "sg",
        "Simple",
        json!([{"name": "p", "type": "STRING", "linkIds": [10]}]),
        json!([{"name": "out", "type": "IMAGE", "linkIds": [11]}]),
        json!([node(
            2,
            "Gen",
            0,
            json!([
                {"name": "text", "type": "STRING", "widget": {"name": "text"}, "link": 10}
            ]),
            json!([{"name": "image", "type": "IMAGE"}]),
            json!({"text": "inner-default"})
        )]),
        json!([
            {"id": 10, "origin_id": -10, "origin_slot": 0, "target_id": 2, "target_slot": 0, "type": "STRING"},
            {"id": 11, "origin_id": 2, "origin_slot": 0, "target_id": -20, "target_slot": 0, "type": "IMAGE"}
        ]),
    )
}

#[test]
fn promoted_widget_and_subgraph_output_are_resolved_by_name() {
    let text = workflow(
        json!([
            json!({
                "id": 1, "type": "sg", "mode": 0,
                "inputs": [{"name": "p", "type": "STRING", "widget": {"name": "p"}, "link": null}],
                "outputs": [{"name": "out", "type": "IMAGE", "links": [20]}],
                "widgets_values_named": {"p": "  exact,\nété 🌸  "},
                "widgets_values": ["  exact,\nété 🌸  "]
            }),
            node(
                3,
                "SaveImage",
                0,
                json!([{"name": "images", "type": "IMAGE", "link": 20}]),
                json!([]),
                json!({})
            )
        ]),
        json!([[20, 1, 0, 3, 0, "IMAGE"]]),
        json!([simple_subgraph()]),
    );
    let result = convert_ui(&text, Some(&info())).unwrap();
    assert_eq!(result["report"]["complete"], true, "{result}");
    let api = &result["api"];
    assert!(api.get("1").is_none(), "instance must not be emitted");
    assert_eq!(api["1:2"]["class_type"], "Gen");
    assert_eq!(api["1:2"]["inputs"]["text"], "  exact,\nété 🌸  ");
    assert_eq!(api["3"]["inputs"]["images"], json!(["1:2", 0]));
}

#[test]
fn two_instances_keep_distinct_values_and_path_ids() {
    let text = workflow(
        json!([
            json!({"id": 1, "type": "sg", "mode": 0,
                "inputs": [{"name": "p", "type": "STRING", "widget": {"name": "p"}, "link": null}],
                "outputs": [{"name": "out", "type": "IMAGE", "links": [20]}],
                "widgets_values_named": {"p": "first"}}),
            json!({"id": 5, "type": "sg", "mode": 0,
                "inputs": [{"name": "p", "type": "STRING", "widget": {"name": "p"}, "link": null}],
                "outputs": [{"name": "out", "type": "IMAGE", "links": [21]}],
                "widgets_values_named": {"p": "second"}}),
            node(
                3,
                "SaveImage",
                0,
                json!([{"name": "images", "type": "IMAGE", "link": 20}]),
                json!([]),
                json!({})
            ),
            node(
                4,
                "SaveImage",
                0,
                json!([{"name": "images", "type": "IMAGE", "link": 21}]),
                json!([]),
                json!({})
            )
        ]),
        json!([[20, 1, 0, 3, 0, "IMAGE"], [21, 5, 0, 4, 0, "IMAGE"]]),
        json!([simple_subgraph()]),
    );
    let result = convert_ui(&text, Some(&info())).unwrap();
    assert_eq!(result["report"]["complete"], true, "{result}");
    assert_eq!(result["api"]["1:2"]["inputs"]["text"], "first");
    assert_eq!(result["api"]["5:2"]["inputs"]["text"], "second");
}

#[test]
fn nested_subgraphs_use_full_paths() {
    let inner = simple_subgraph();
    let outer = subgraph(
        "outer",
        "Outer",
        json!([]),
        json!([{"name": "out", "type": "IMAGE", "linkIds": [30]}]),
        json!([json!({
            "id": 2, "type": "sg", "mode": 0,
            "inputs": [{"name": "p", "type": "STRING", "widget": {"name": "p"}, "link": null}],
            "outputs": [{"name": "out", "type": "IMAGE", "links": [30]}],
            "widgets_values_named": {"p": "nested"}
        })]),
        json!([
            {"id": 30, "origin_id": 2, "origin_slot": 0, "target_id": -20, "target_slot": 0, "type": "IMAGE"}
        ]),
    );
    let text = workflow(
        json!([
            json!({"id": 1, "type": "outer", "mode": 0, "inputs": [], "outputs": [{"name": "out", "type": "IMAGE", "links": [40]}]}),
            node(
                3,
                "SaveImage",
                0,
                json!([{"name": "images", "type": "IMAGE", "link": 40}]),
                json!([]),
                json!({})
            )
        ]),
        json!([[40, 1, 0, 3, 0, "IMAGE"]]),
        json!([inner, outer]),
    );
    let result = convert_ui(&text, Some(&info())).unwrap();
    assert_eq!(result["report"]["complete"], true, "{result}");
    assert_eq!(result["api"]["1:2:2"]["inputs"]["text"], "nested");
    assert_eq!(result["api"]["3"]["inputs"]["images"], json!(["1:2:2", 0]));
}

#[test]
fn hidden_widgets_are_never_serialized_and_arrays_are_wrapped() {
    let text = workflow(
        json!([node(
            1,
            "Enc",
            0,
            json!([
                {"name": "text", "type": "STRING", "widget": {"name": "text"}, "link": null},
                {"name": "seed", "type": "INT", "widget": {"name": "seed"}, "link": null}
            ]),
            json!([]),
            json!({
                "text": "line1\nligne é 🌸",
                "seed": 18446744073709551615u64,
                "control_after_generate": "fixed",
                "🎲 New Fixed Random": "",
                "USE_LAST_SEED": "okay"
            })
        )]),
        json!([]),
        json!([]),
    );
    let result = convert_ui(&text, Some(&info())).unwrap();
    assert_eq!(result["report"]["complete"], true, "{result}");
    let inputs = &result["api"]["1"]["inputs"];
    assert_eq!(inputs["text"], "line1\nligne é 🌸");
    assert_eq!(inputs["seed"], json!(18446744073709551615u64));
    assert!(inputs.get("control_after_generate").is_none());
    assert!(inputs.get("🎲 New Fixed Random").is_none());
    assert!(inputs.get("USE_LAST_SEED").is_none());
    assert_eq!(result["report"]["hiddenWidgetsExcluded"], 3);
}

#[test]
fn bypass_passes_matching_input_and_mute_refuses_targeted() {
    let bypassed = subgraph(
        "sg",
        "Bypass",
        json!([{"name": "model", "type": "MODEL", "linkIds": [10]}]),
        json!([{"name": "out", "type": "MODEL", "linkIds": [11]}]),
        json!([node(
            2,
            "Pass",
            4,
            json!([{"name": "model", "type": "MODEL", "link": 10}]),
            json!([{"name": "model", "type": "MODEL"}]),
            json!({})
        )]),
        json!([
            {"id": 10, "origin_id": -10, "origin_slot": 0, "target_id": 2, "target_slot": 0, "type": "MODEL"},
            {"id": 11, "origin_id": 2, "origin_slot": 0, "target_id": -20, "target_slot": 0, "type": "MODEL"}
        ]),
    );
    let text = workflow(
        json!([
            node(
                5,
                "Loader",
                0,
                json!([]),
                json!([{"name": "model", "type": "MODEL", "links": [20]}]),
                json!({})
            ),
            json!({"id": 1, "type": "sg", "mode": 0,
                "inputs": [{"name": "model", "type": "MODEL", "link": 20}],
                "outputs": [{"name": "out", "type": "MODEL", "links": [21]}]}),
            node(
                7,
                "Consumer",
                0,
                json!([{"name": "model", "type": "MODEL", "link": 21}]),
                json!([]),
                json!({})
            )
        ]),
        json!([[20, 5, 0, 1, 0, "MODEL"], [21, 1, 0, 7, 0, "MODEL"]]),
        json!([bypassed]),
    );
    let result = convert_ui(&text, Some(&info())).unwrap();
    assert_eq!(result["report"]["complete"], true, "{result}");
    assert_eq!(result["api"]["7"]["inputs"]["model"], json!(["5", 0]));
    assert!(result["api"].get("1:2").is_none(), "bypassed node emitted");

    // Same graph with a muted node: the link is removed, the refusal is targeted.
    let mut muted_text: Value = serde_json::from_str(&text).unwrap();
    muted_text["definitions"]["subgraphs"][0]["nodes"][0]["mode"] = json!(2);
    let result = convert_ui(&muted_text.to_string(), Some(&info())).unwrap();
    let report = &result["report"];
    assert_eq!(report["complete"], false, "{result}");
    assert!(result["api"]["7"]["inputs"].get("model").is_none());
    let unresolved = report["unresolved"].as_array().unwrap();
    assert!(
        unresolved.iter().any(|u| u["executionId"] == "7"),
        "{unresolved:?}"
    );
}

#[test]
fn set_get_and_reroute_are_virtual_and_resolve_upstream() {
    let text = workflow(
        json!([
            node(
                5,
                "Loader",
                0,
                json!([]),
                json!([{"name": "model", "type": "MODEL", "links": [10, 12]}]),
                json!({})
            ),
            node(
                6,
                "SetNode",
                0,
                json!([{"name": "*", "type": "*", "link": 10}]),
                json!([{"name": "*", "type": "*"}]),
                json!({"Constant": "m"})
            ),
            node(
                7,
                "GetNode",
                0,
                json!([]),
                json!([{"name": "*", "type": "MODEL", "links": [11]}]),
                json!({"Constant": "m"})
            ),
            node(
                8,
                "Reroute",
                0,
                json!([{"name": "", "type": "*", "link": 12}]),
                json!([{"name": "", "type": "*", "links": [13]}]),
                json!({})
            ),
            node(
                9,
                "Consumer",
                0,
                json!([{"name": "model", "type": "MODEL", "link": 11}]),
                json!([]),
                json!({})
            ),
            node(
                10,
                "Consumer",
                0,
                json!([{"name": "model", "type": "MODEL", "link": 13}]),
                json!([]),
                json!({})
            )
        ]),
        json!([
            [10, 5, 0, 6, 0, "MODEL"],
            [11, 7, 0, 9, 0, "MODEL"],
            [12, 5, 0, 8, 0, "MODEL"],
            [13, 8, 0, 10, 0, "MODEL"]
        ]),
        json!([]),
    );
    let result = convert_ui(&text, Some(&info())).unwrap();
    assert_eq!(result["report"]["complete"], true, "{result}");
    assert_eq!(result["api"]["9"]["inputs"]["model"], json!(["5", 0]));
    assert_eq!(result["api"]["10"]["inputs"]["model"], json!(["5", 0]));
    assert!(result["api"].get("5").is_some(), "loader must be emitted");
    for id in ["6", "7", "8"] {
        assert!(result["api"].get(id).is_none(), "virtual {id} emitted");
    }
}

#[test]
fn missing_promoted_value_is_reported_without_inventing_one() {
    let text = workflow(
        json!([
            json!({"id": 1, "type": "sg", "mode": 0,
                "inputs": [{"name": "p", "type": "STRING", "widget": {"name": "p"}, "link": null}],
                "outputs": [{"name": "out", "type": "IMAGE", "links": [20]}]}),
            node(
                3,
                "SaveImage",
                0,
                json!([{"name": "images", "type": "IMAGE", "link": 20}]),
                json!([]),
                json!({})
            )
        ]),
        json!([[20, 1, 0, 3, 0, "IMAGE"]]),
        json!([simple_subgraph()]),
    );
    let result = convert_ui(&text, Some(&info())).unwrap();
    assert_eq!(result["report"]["complete"], false, "{result}");
    assert!(result["api"]["1:2"]["inputs"].get("text").is_none());
    let unresolved = serde_json::to_string(&result["report"]["unresolved"]).unwrap();
    assert!(unresolved.contains("1:2"), "{unresolved}");
}

#[test]
fn top_level_muted_subgraph_is_not_expanded() {
    let text = workflow(
        json!([json!({"id": 1, "type": "sg", "mode": 2,
                "inputs": [{"name": "p", "type": "STRING", "widget": {"name": "p"}, "link": null}],
                "outputs": [{"name": "out", "type": "IMAGE"}]})]),
        json!([]),
        json!([simple_subgraph()]),
    );
    let result = convert_ui(&text, Some(&info())).unwrap();
    assert_eq!(result["report"]["complete"], true, "{result}");
    assert_eq!(result["report"]["apiNodes"], 0);
    assert_eq!(result["report"]["mutedSkipped"], 1);
}

#[test]
fn cyclic_subgraph_definitions_are_rejected() {
    let a = subgraph(
        "a",
        "A",
        json!([]),
        json!([]),
        json!([{"id": 2, "type": "b", "mode": 0, "inputs": [], "outputs": []}]),
        json!([]),
    );
    let b = subgraph(
        "b",
        "B",
        json!([]),
        json!([]),
        json!([{"id": 3, "type": "a", "mode": 0, "inputs": [], "outputs": []}]),
        json!([]),
    );
    let text = workflow(
        json!([{"id": 1, "type": "a", "mode": 0, "inputs": [], "outputs": []}]),
        json!([]),
        json!([a, b]),
    );
    let error = convert_ui(&text, Some(&info())).unwrap_err().to_string();
    assert!(error.contains("circulaire"), "{error}");
}

#[test]
fn unnamed_widget_count_mismatch_is_reported_not_guessed() {
    let mut value = node(
        1,
        "Enc",
        0,
        json!([
            {"name": "text", "type": "STRING", "widget": {"name": "text"}, "link": null},
            {"name": "seed", "type": "INT", "widget": {"name": "seed"}, "link": null}
        ]),
        json!([]),
        json!({}),
    );
    value
        .as_object_mut()
        .unwrap()
        .remove("widgets_values_named");
    value["widgets_values"] = json!(["only-one"]);
    let text = workflow(json!([value]), json!([]), json!([]));
    let result = convert_ui(&text, None).unwrap();
    assert_eq!(result["report"]["complete"], false, "{result}");
    assert_eq!(result["api"]["1"]["inputs"], json!({}), "{result}");
    let causes = serde_json::to_string(&result["report"]["unresolved"]).unwrap();
    assert!(causes.contains("valeurs nommées"), "{causes}");
}

#[test]
fn documented_custom_widget_rules_match_frontend_serialization() {
    let text = workflow(
        json!([
            node(
                1,
                "CustomCombo",
                0,
                json!([]),
                json!([]),
                json!({"choice": "B", "option1": "A", "option2": "B", "index": 0})
            ),
            node(
                2,
                "Image Comparer (rgthree)",
                0,
                json!([]),
                json!([]),
                json!({"rgthree_comparer": {"images": [
                    {"img": {"secret": true}, "name": "A", "selected": true},
                    {"img": {}, "name": "B", "selected": false}
                ]}})
            )
        ]),
        json!([]),
        json!([]),
    );
    let result = convert_ui(&text, None).unwrap();
    assert_eq!(result["report"]["complete"], true, "{result}");
    assert_eq!(result["api"]["1"]["inputs"]["index"], 1);
    assert!(
        result["api"]["2"]["inputs"]["rgthree_comparer"]["images"][0]
            .get("img")
            .is_none()
    );
    assert_eq!(result["report"]["knownWidgetRulesApplied"], 2);
}

fn simple_root_source() -> String {
    workflow(
        json!([
            json!({
                "id": 1, "type": "sg", "mode": 0,
                "inputs": [{"name": "p", "type": "STRING", "widget": {"name": "p"}, "link": null}],
                "outputs": [{"name": "out", "type": "IMAGE", "links": [20]}],
                "widgets_values_named": {"p": "hello"}
            }),
            node(
                3,
                "SaveImage",
                0,
                json!([{"name": "images", "type": "IMAGE", "link": 20}]),
                json!([]),
                json!({})
            )
        ]),
        json!([[20, 1, 0, 3, 0, "IMAGE"]]),
        json!([simple_subgraph()]),
    )
}

#[test]
fn editor_roundtrip_preserves_subgraph_conversion() {
    let source = include_str!("../../../scripts/fixtures/v15.2-subgraph-ui.json");
    let info: Value = serde_json::from_str(include_str!(
        "../../../scripts/fixtures/object_info-subset.json"
    ))
    .unwrap();
    let session = GraphSession::open_file(source, &info, "Fixture").unwrap();
    let exported = session.export_ui().unwrap();
    let prepared = prepare_api(&exported.to_string(), Some(&info)).unwrap();
    assert_eq!(prepared["report"]["complete"], true, "{prepared}");
    assert_eq!(prepared["api"]["1:4"]["inputs"]["text"], "hello\nmonde");
    assert_eq!(
        prepared["api"]["1:7"]["inputs"]["seed"],
        json!(18446744073709551615u64)
    );
    assert_eq!(prepared["api"]["2"]["inputs"]["images"], json!(["1:8", 0]));
}

#[test]
fn native_smoke_fixture_converts_to_seven_api_nodes() {
    let source = include_str!("../../../scripts/fixtures/v15.2-subgraph-ui.json");
    let info: Value = serde_json::from_str(include_str!(
        "../../../scripts/fixtures/object_info-subset.json"
    ))
    .unwrap();
    let result = convert_ui(source, Some(&info)).unwrap();
    assert_eq!(result["report"]["complete"], true, "{result}");
    assert_eq!(result["report"]["validatedAgainstSchema"], true);
    assert_eq!(result["report"]["apiNodes"], 7);
    assert_eq!(result["report"]["subgraphDefinitions"], 1);
    assert_eq!(result["report"]["topLevelInstances"], 1);
    assert_eq!(result["api"]["1:4"]["inputs"]["text"], "hello\nmonde");
    assert_eq!(
        result["api"]["1:7"]["inputs"]["seed"],
        json!(18446744073709551615u64)
    );
    assert_eq!(result["api"]["2"]["inputs"]["images"], json!(["1:8", 0]));
    assert!(result["api"].get("1").is_none());
}

#[test]
fn api_revision_requires_confirmation_schema_and_unchanged_hash() {
    let dir = std::env::temp_dir().join(format!("saya-api-save-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let db = Database::open(&dir.join("db.sqlite")).unwrap();
    let source = simple_root_source();
    let prepared = prepare_api(&source, Some(&info())).unwrap();
    assert_eq!(prepared["report"]["complete"], true, "{prepared}");
    assert_eq!(prepared["report"]["validatedAgainstSchema"], true);
    assert_eq!(prepared["api"]["1:2"]["inputs"]["text"], "hello");
    let hash = prepared["previewHash"].as_str().unwrap();
    assert!(save_api_revision(&db, &source, Some(&info()), "API", hash, false).is_err());
    assert!(save_api_revision(&db, &source, None, "API", hash, true).is_err());
    assert!(save_api_revision(&db, &source, Some(&info()), "API", "stale", true).is_err());
    assert!(db.workflows().unwrap().is_empty());
    let saved =
        save_api_revision(&db, &source, Some(&info()), "API convertie", hash, true).unwrap();
    assert_eq!(saved["nodeCount"], 2);
    assert_eq!(db.workflows().unwrap().len(), 1);
    let (_, text) = db
        .workflow_document(saved["workflowId"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&text).unwrap(),
        prepared["api"],
        "stored revision differs from preview"
    );
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn incomplete_conversion_cannot_create_an_api_revision() {
    let dir = std::env::temp_dir().join(format!("saya-api-refuse-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let db = Database::open(&dir.join("db.sqlite")).unwrap();
    let text = workflow(
        json!([
            json!({"id": 1, "type": "sg", "mode": 0,
                "inputs": [{"name": "p", "type": "STRING", "widget": {"name": "p"}, "link": null}],
                "outputs": [{"name": "out", "type": "IMAGE", "links": [20]}]}),
            node(
                3,
                "SaveImage",
                0,
                json!([{"name": "images", "type": "IMAGE", "link": 20}]),
                json!([]),
                json!({})
            )
        ]),
        json!([[20, 1, 0, 3, 0, "IMAGE"]]),
        json!([simple_subgraph()]),
    );
    let prepared = prepare_api(&text, Some(&info())).unwrap();
    assert_eq!(prepared["report"]["complete"], false);
    let hash = prepared["previewHash"].as_str().unwrap();
    let error = save_api_revision(&db, &text, Some(&info()), "API", hash, true)
        .unwrap_err()
        .to_string();
    assert!(error.contains("incomplète"), "{error}");
    assert!(db.workflows().unwrap().is_empty());
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "lecture seule du workflow original ; opt-in"]
fn real_original_workflow_converts_without_lost_required_inputs() {
    if skip_without_workflow() {
        return;
    }
    let path = real_workflow();
    let text = std::fs::read_to_string(&path).unwrap();
    let before = text.clone();
    let info = std::env::var("SAYA_REAL_OBJECT_INFO")
        .ok()
        .map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap());
    let result = convert_ui(&text, info.as_ref()).unwrap();
    if let Ok(path) = std::env::var("SAYA_DUMP_API") {
        std::fs::write(path, result["api"].to_string()).unwrap();
    }
    let report = &result["report"];
    assert_eq!(report["complete"], true, "{report}");
    assert!(report["apiNodes"].as_u64().unwrap() > 60, "{report}");
    assert!(
        report["subgraphDefinitions"].as_u64().unwrap() == 24,
        "{report}"
    );
    assert!(
        report["topLevelInstances"].as_u64().unwrap() == 16,
        "{report}"
    );
    assert!(report["nestedInstances"].as_u64().unwrap() == 9, "{report}");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        before,
        "source modified"
    );
}
