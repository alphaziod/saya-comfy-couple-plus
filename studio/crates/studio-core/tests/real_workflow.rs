//! Opt-in read-only qualification of a local API export against the live schema.
use serde_json::{json, Value};
use studio_core::{
    comfy::Comfy,
    workflow::{api_graph, validate_graph, Edit},
    Database,
};

#[test]
#[ignore = "requires SAYA_REAL_WORKFLOW and the running local ComfyUI; never submits"]
fn real_api_export_matches_live_schema_without_submission() {
    let path = std::env::var("SAYA_REAL_WORKFLOW").expect("explicit API workflow path required");
    let source = std::fs::read_to_string(&path).unwrap();
    let graph = api_graph(&source).unwrap();
    let engine = Comfy::new().unwrap();
    let schema = engine.get("/object_info").unwrap();
    validate_graph(&graph, &schema).unwrap();
    let mut db = Database::in_memory().unwrap();
    let id = db
        .import_workflow("Read-only real export qualification", &source)
        .unwrap();
    let unedited = db.prepare_job(&id, &[], &schema).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&unedited.request_json).unwrap()["prompt"],
        graph
    );
    if let Ok(path) = std::env::var("SAYA_REAL_BINDINGS") {
        let edits: Vec<Edit> =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        db.save_bindings(&id, &edits).unwrap();
        let prepared = db.prepare_job(&id, &edits, &schema).unwrap();
        let request: Value = serde_json::from_str(&prepared.request_json).unwrap();
        let mut expected = graph.clone();
        for edit in &edits {
            let original = &graph[&edit.node_id]["inputs"][&edit.input_name];
            expected[&edit.node_id]["inputs"][&edit.input_name] = if original.is_string() {
                json!(edit.value)
            } else {
                serde_json::from_str(&edit.value).unwrap()
            };
        }
        assert_eq!(request["prompt"], expected);
        assert_eq!(
            serde_json::from_str::<Value>(&db.prepare_job(&id, &[], &schema).unwrap().request_json)
                .unwrap()["prompt"],
            graph
        );
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), source);
}
