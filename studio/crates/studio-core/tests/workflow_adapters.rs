use serde_json::{json, Value};
use studio_core::{
    workflow::{validate_graph, Edit},
    Database,
};

fn fixture() -> (Value, Value) {
    let graph = json!({
        "model":{"class_type":"Loader","inputs":{}},
        "lora":{"class_type":"Lora Loader (LoraManager)","inputs":{
            "model":["model",0],"clip":["model",1],"text":"", "__lm_autocomplete_meta_text":"",
            "loras":{"__value__":[{"name":"sample","strength":"0.85","clipStrength":0.5,"active":true,"expanded":false,"locked":false}]}}},
        "out":{"class_type":"Output","inputs":{"model":["lora",0]}}
    });
    let info = json!({
        "Loader":{"input":{"required":{}},"output":["MODEL","CLIP"]},
        "Lora Loader (LoraManager)":{"input":{"required":{"model":["MODEL"],"text":["AUTOCOMPLETE_TEXT_LORAS"],"loras":["LORAS"]},"optional":{}},"output":["MODEL","CLIP","STRING","STRING"]},
        "Output":{"input":{"required":{"model":["MODEL"]}},"output":[],"output_node":true}
    });
    (graph, info)
}

#[test]
fn preparation_preserves_composite_weights_links_and_unedited_values() {
    let (mut graph, mut info) = fixture();
    graph["text"] =
        json!({"class_type":"Text","inputs":{"text":"(exact tags:1.2),  spacing\nline"}});
    info["Text"] = json!({"input":{"required":{"text":["STRING"]}},"output":["STRING"]});
    let mut db = Database::in_memory().unwrap();
    let id = db
        .import_workflow("Composite preservation", &graph.to_string())
        .unwrap();
    assert!(db
        .workflow(&id)
        .unwrap()
        .fields
        .iter()
        .all(|f| f.node_id != "lora"));
    let edit = Edit {
        node_id: "text".into(),
        input_name: "text".into(),
        value: "(exact tags:1.2),  spacing\nline\nnew tag".into(),
        section: Some("P1".into()),
    };
    db.save_bindings(&id, std::slice::from_ref(&edit)).unwrap();
    let prepared = db
        .prepare_job(&id, std::slice::from_ref(&edit), &info)
        .unwrap();
    let request: Value = serde_json::from_str(&prepared.request_json).unwrap();
    let mut expected = graph.clone();
    expected["text"]["inputs"]["text"] = json!(edit.value);
    assert_eq!(request["prompt"], expected);
    assert!(prepared.removed.is_empty());
    let unedited = db.prepare_job(&id, &[], &info).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&unedited.request_json).unwrap()["prompt"],
        graph
    );
}

#[test]
fn explicit_lora_widget_contract_preserves_graph_and_rejects_unsupported_shapes() {
    let (graph, info) = fixture();
    let original = graph.clone();
    validate_graph(&graph, &info).unwrap();
    assert_eq!(graph, original);
    for invalid in [
        json!(null),
        json!([]),
        json!({"__value__":"invalid"}),
        json!({"__value__":[["model",0]]}),
        json!({"__value__":[],"other":true}),
    ] {
        let mut bad = graph.clone();
        bad["lora"]["inputs"]["loras"] = invalid;
        assert!(validate_graph(&bad, &info).is_err());
    }
    for (key, value) in [
        ("strength", json!("NaN")),
        ("clipStrength", json!("inf")),
        ("active", json!("yes")),
        ("name", json!("")),
    ] {
        let mut bad = graph.clone();
        bad["lora"]["inputs"]["loras"]["__value__"][0][key] = value;
        assert!(validate_graph(&bad, &info).is_err(), "{key}");
    }
    for (key, value) in [
        ("unknown", json!("")),
        ("clip", json!(["model", 0])),
        ("text", json!(17)),
    ] {
        let mut bad = graph.clone();
        bad["lora"]["inputs"][key] = value;
        assert!(validate_graph(&bad, &info).is_err(), "{key}");
    }
    let mut changed_schema = info.clone();
    changed_schema["Lora Loader (LoraManager)"]["input"]["required"]["loras"] = json!(["NEW_TYPE"]);
    assert!(validate_graph(&graph, &changed_schema).is_err());
    let mut cycle = graph.clone();
    cycle["lora"]["inputs"]["clip"] = json!(["lora", 1]);
    assert!(validate_graph(&cycle, &info).is_err());
}
