//! Structural edits on a preserved root UI document.
use serde_json::{json, Value};
use studio_core::graph_session::{EditOp, GraphSession};

fn info() -> Value {
    serde_json::from_str(include_str!(
        "../../../scripts/fixtures/object_info-subset.json"
    ))
    .unwrap()
}

fn source() -> String {
    include_str!("../../../scripts/fixtures/v15.2-subgraph-ui.json").to_owned()
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
fn copy_paste_root_node_preserves_definitions_and_undo() {
    let mut session = GraphSession::open_file(&source(), &info(), "Root").unwrap();
    let before = session.export_ui().unwrap();
    let save = node_for(&session.view(), "2");
    let fragment = session.copy_fragment(std::slice::from_ref(&save)).unwrap();
    let pasted = session
        .paste_fragment(
            &fragment,
            studio_core::graph_editor::Point { x: 40.0, y: 50.0 },
        )
        .unwrap();
    assert_eq!(
        session.view()["document"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let after = session.export_ui().unwrap();
    assert_eq!(after["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(after["definitions"], before["definitions"]);
    let new_id = pasted["nodes"][0].as_str().unwrap();
    let raw = after["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["type"] == "SaveImage" && n["id"] != json!(2))
        .unwrap();
    assert!(raw["id"].is_number(), "UI id must remain numeric");
    assert_eq!(
        raw["widgets_values_named"]["filename_prefix"],
        "saya/api-smoke"
    );
    assert_eq!(raw["pos"][0].as_f64(), Some(940.0));
    assert_eq!(session.view()["document"]["nodes"][2]["id"], json!(new_id));
    session.apply(EditOp::Undo).unwrap();
    assert_eq!(session.export_ui().unwrap(), before);
    session.apply(EditOp::Redo).unwrap();
    assert_eq!(session.export_ui().unwrap(), after);
}

#[test]
fn add_connect_remove_root_nodes_serialize_links() {
    let mut session = GraphSession::open_file(&source(), &info(), "Root").unwrap();
    let before = session.export_ui().unwrap();
    session
        .apply(EditOp::Add {
            class_type: "SaveImage".into(),
            x: 1200.0,
            y: 100.0,
        })
        .unwrap();
    let view = session.view();
    let added = view["document"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| {
            n["classType"] == "SaveImage"
                && !view["originalIds"]
                    .as_object()
                    .unwrap()
                    .contains_key(n["id"].as_str().unwrap())
        })
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let source_node = node_for(&view, "2");
    session
        .apply(EditOp::Connect {
            from_node: source_node,
            output: 0,
            to_node: added.clone(),
            input: "images".into(),
        })
        .unwrap();
    let after = session.export_ui().unwrap();
    assert_eq!(after["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(after["links"].as_array().unwrap().len(), 2);
    let raw = after["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["type"] == "SaveImage" && n["id"] != json!(2))
        .unwrap();
    let images = raw["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "images")
        .unwrap();
    assert!(images["link"].is_number());
    assert_eq!(after["definitions"], before["definitions"]);
    session
        .apply(EditOp::Remove { nodes: vec![added] })
        .unwrap();
    assert_eq!(session.export_ui().unwrap(), before);
}
