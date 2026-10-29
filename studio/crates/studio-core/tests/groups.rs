//! Group geometry, undo/redo and faithful UI/definition serialization.
use serde_json::{json, Value};
use studio_core::graph_editor::GroupBounds;
use studio_core::graph_session::{EditOp, GraphSession};

fn info() -> Value {
    serde_json::from_str(include_str!(
        "../../../scripts/fixtures/object_info-subset.json"
    ))
    .unwrap()
}

fn api() -> Value {
    json!({
        "1":{"class_type":"LoadImage","inputs":{"image":"fixture.png"}},
        "2":{"class_type":"SaveImage","inputs":{"images":["1",0],"filename_prefix":"x"}}
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

fn group(view: &Value, title: &str) -> Value {
    view["document"]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["title"] == title)
        .unwrap()
        .clone()
}

#[test]
fn group_move_resize_and_undo_keep_geometry() {
    let mut session = GraphSession::open("wf", &api(), &info(), "Groups").unwrap();
    let view = session.view();
    let first = node_for(&view, "1");
    let second = node_for(&view, "2");
    session
        .apply(EditOp::Group {
            title: "Paire".into(),
            nodes: vec![first.clone(), second.clone()],
        })
        .unwrap();
    let created = group(&session.view(), "Paire");
    let id = created["id"].as_str().unwrap().to_owned();
    let before = created["bounds"].clone();
    assert!(before["width"].as_f64().unwrap() > 0.0);
    let p1 = session.view()["document"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == json!(first))
        .unwrap()["position"]
        .clone();
    session
        .apply(EditOp::MoveGroup {
            group: id.clone(),
            dx: 50.0,
            dy: 60.0,
        })
        .unwrap();
    let moved = group(&session.view(), "Paire");
    assert_eq!(
        moved["bounds"]["x"].as_f64().unwrap(),
        before["x"].as_f64().unwrap() + 50.0
    );
    let p2 = session.view()["document"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == json!(first))
        .unwrap()["position"]
        .clone();
    assert_eq!(p2["x"].as_f64().unwrap(), p1["x"].as_f64().unwrap() + 50.0);
    session
        .apply(EditOp::ResizeGroup {
            group: id.clone(),
            bounds: GroupBounds {
                x: moved["bounds"]["x"].as_f64().unwrap(),
                y: moved["bounds"]["y"].as_f64().unwrap(),
                width: moved["bounds"]["width"].as_f64().unwrap() + 120.0,
                height: moved["bounds"]["height"].as_f64().unwrap() + 80.0,
            },
        })
        .unwrap();
    let resized = group(&session.view(), "Paire");
    assert_eq!(
        resized["bounds"]["width"].as_f64().unwrap(),
        moved["bounds"]["width"].as_f64().unwrap() + 120.0
    );
    session.apply(EditOp::Undo).unwrap();
    assert_eq!(group(&session.view(), "Paire")["bounds"], moved["bounds"]);
    session.apply(EditOp::Undo).unwrap();
    assert_eq!(group(&session.view(), "Paire")["bounds"], before);
    session.apply(EditOp::Redo).unwrap();
    session.apply(EditOp::Redo).unwrap();
    assert_eq!(
        group(&session.view(), "Paire")["bounds"]["width"]
            .as_f64()
            .unwrap(),
        moved["bounds"]["width"].as_f64().unwrap() + 120.0
    );
}

#[test]
fn root_group_geometry_survives_ui_export_and_reopen() {
    let source = include_str!("../../../scripts/fixtures/v15.2-subgraph-ui.json");
    let mut session = GraphSession::open_file(source, &info(), "Root groups").unwrap();
    let save = node_for(&session.view(), "2");
    session
        .apply(EditOp::Group {
            title: "Racine".into(),
            nodes: vec![save],
        })
        .unwrap();
    let created = group(&session.view(), "Racine");
    let id = created["id"].as_str().unwrap().to_owned();
    session
        .apply(EditOp::MoveGroup {
            group: id.clone(),
            dx: 15.0,
            dy: 25.0,
        })
        .unwrap();
    let bounds = group(&session.view(), "Racine")["bounds"].clone();
    session
        .apply(EditOp::ResizeGroup {
            group: id,
            bounds: GroupBounds {
                x: bounds["x"].as_f64().unwrap(),
                y: bounds["y"].as_f64().unwrap(),
                width: bounds["width"].as_f64().unwrap() + 40.0,
                height: bounds["height"].as_f64().unwrap() + 20.0,
            },
        })
        .unwrap();
    let exported = session.export_ui().unwrap();
    let raw = exported["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["title"] == "Racine")
        .unwrap();
    assert_eq!(
        raw["bounding"][2].as_f64().unwrap(),
        bounds["width"].as_f64().unwrap() + 40.0
    );
    let reopened = GraphSession::open_file(&exported.to_string(), &info(), "Again").unwrap();
    let again = group(&reopened.view(), "Racine");
    assert_eq!(
        again["bounds"]["width"].as_f64().unwrap(),
        bounds["width"].as_f64().unwrap() + 40.0
    );
}

#[test]
fn subgraph_group_is_serialized_then_parsed_on_reentry() {
    let source = include_str!("../../../scripts/fixtures/v15.2-subgraph-ui.json");
    let mut session = GraphSession::open_file(source, &info(), "Inner group").unwrap();
    session.enter_subgraph("sg-1").unwrap();
    session
        .apply(EditOp::Group {
            title: "Interne".into(),
            nodes: vec!["6".into(), "7".into()],
        })
        .unwrap();
    let inner = group(&session.view(), "Interne");
    let id = inner["id"].as_str().unwrap().to_owned();
    session
        .apply(EditOp::MoveGroup {
            group: id.clone(),
            dx: 30.0,
            dy: 10.0,
        })
        .unwrap();
    let bounds = group(&session.view(), "Interne")["bounds"].clone();
    session
        .apply(EditOp::ResizeGroup {
            group: id,
            bounds: GroupBounds {
                x: bounds["x"].as_f64().unwrap(),
                y: bounds["y"].as_f64().unwrap(),
                width: bounds["width"].as_f64().unwrap() + 60.0,
                height: bounds["height"].as_f64().unwrap() + 30.0,
            },
        })
        .unwrap();
    session.exit_subgraph(true).unwrap();
    let exported = session.export_ui().unwrap();
    let def = &exported["definitions"]["subgraphs"][0];
    let raw = def["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["title"] == "Interne")
        .expect("group in definition");
    assert_eq!(
        raw["bounding"][2].as_f64().unwrap(),
        bounds["width"].as_f64().unwrap() + 60.0
    );
    session.enter_subgraph("sg-1").unwrap();
    let again = group(&session.view(), "Interne");
    assert_eq!(
        again["bounds"]["width"].as_f64().unwrap(),
        bounds["width"].as_f64().unwrap() + 60.0
    );
    session.exit_subgraph(false).unwrap();
}

#[test]
fn nested_subgraph_group_round_trips() {
    let source = include_str!("../../../scripts/fixtures/v15.2-nested-ui.json");
    let mut session = GraphSession::open_file(source, &info(), "Nested groups").unwrap();
    session.enter_subgraph("sg-1").unwrap();
    session.enter_subgraph("sg-10").unwrap();
    session
        .apply(EditOp::Group {
            title: "Imbriqué".into(),
            nodes: vec!["6".into(), "7".into()],
        })
        .unwrap();
    let inner = group(&session.view(), "Imbriqué");
    let id = inner["id"].as_str().unwrap().to_owned();
    let bounds = inner["bounds"].clone();
    session
        .apply(EditOp::ResizeGroup {
            group: id,
            bounds: GroupBounds {
                x: bounds["x"].as_f64().unwrap(),
                y: bounds["y"].as_f64().unwrap(),
                width: bounds["width"].as_f64().unwrap() + 35.0,
                height: bounds["height"].as_f64().unwrap() + 25.0,
            },
        })
        .unwrap();
    session.exit_subgraph(true).unwrap();
    session.exit_subgraph(true).unwrap();
    let exported = session.export_ui().unwrap();
    let defs = exported["definitions"]["subgraphs"].as_array().unwrap();
    let inner = defs.iter().find(|d| d["id"] == "sgimg").unwrap();
    let raw = inner["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["title"] == "Imbriqué")
        .expect("nested group");
    assert_eq!(
        raw["bounding"][2].as_f64().unwrap(),
        bounds["width"].as_f64().unwrap() + 35.0
    );
    assert_eq!(
        defs.iter().find(|d| d["id"] == "sgouter").unwrap()["groups"],
        json!([])
    );
    session.enter_subgraph("sg-1").unwrap();
    session.enter_subgraph("sg-10").unwrap();
    assert_eq!(
        group(&session.view(), "Imbriqué")["bounds"]["width"]
            .as_f64()
            .unwrap(),
        bounds["width"].as_f64().unwrap() + 35.0
    );
    session.exit_subgraph(false).unwrap();
    session.exit_subgraph(false).unwrap();
}
