//! Persisted editor sessions: snapshot, reload, source preservation.
use serde_json::{json, Value};
use studio_core::graph_session::{EditOp, GraphSession};
use studio_core::Database;

fn info() -> Value {
    serde_json::from_str(include_str!(
        "../../../scripts/fixtures/object_info-subset.json"
    ))
    .unwrap()
}

fn api() -> Value {
    json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"fixture-ckpt_name.safetensors"}},
        "2":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"persisted\nprompt"}},
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

fn save(db: &Database, id: &str, snapshot: &Value) {
    db.save_editor_session(
        id,
        snapshot["name"].as_str().unwrap(),
        snapshot["workflow"].as_str().unwrap(),
        snapshot["sourceFormat"].as_str().unwrap(),
        snapshot["document"].as_str().unwrap(),
        snapshot["uiSource"].as_str(),
        snapshot["originalApi"].as_str(),
        snapshot["originalIds"].as_str().unwrap(),
    )
    .unwrap();
}

fn restore(db: &Database, id: &str, info: Value) -> GraphSession {
    let record = db.editor_session(id).unwrap();
    GraphSession::restore(
        record["workflow"].as_str().unwrap(),
        record["document"].as_str().unwrap(),
        record["uiSource"].as_str(),
        record["originalApi"].as_str(),
        record["originalIds"].as_str().unwrap(),
        info,
    )
    .unwrap()
}

#[test]
fn api_session_roundtrips_with_edits_and_history_reset() {
    let dir = std::env::temp_dir().join(format!("saya-session-api-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let db = Database::open(&dir.join("db.sqlite")).unwrap();
    let mut session = GraphSession::open("wf", &api(), &info(), "Persisted API").unwrap();
    let moved = node_for(&session.view(), "2");
    session
        .apply(EditOp::Move {
            moves: vec![studio_core::graph_session::NodeMove {
                node: moved.clone(),
                x: 777.0,
                y: 888.0,
            }],
        })
        .unwrap();
    session
        .apply(EditOp::StyleNode {
            node: moved.clone(),
            title: "Nom persisté".into(),
            color: Some("#336699".into()),
        })
        .unwrap();
    save(&db, "session-api", &session.snapshot().unwrap());
    let restored = restore(&db, "session-api", info());
    let view = restored.view();
    assert_eq!(view["sourceFormat"], "comfy_api");
    let node = view["document"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == json!(moved))
        .unwrap();
    assert_eq!(node["position"]["x"], json!(777.0));
    assert_eq!(view["canUndo"], false, "history must not survive restart");
    let list = db.editor_sessions().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["name"], "Persisted API");
    db.delete_editor_session("session-api").unwrap();
    assert!(db.editor_sessions().unwrap().is_empty());
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ui_session_roundtrips_and_preserves_source() {
    let dir = std::env::temp_dir().join(format!("saya-session-ui-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let db = Database::open(&dir.join("db.sqlite")).unwrap();
    let source = include_str!("../../../scripts/fixtures/v15.2-subgraph-ui.json");
    let mut session = GraphSession::open_file(source, &info(), "Persisted UI").unwrap();
    let view = session.view();
    let root = view["document"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["classType"] == json!("SaveImage"))
        .unwrap()["id"]
        .clone();
    session
        .apply(EditOp::Move {
            moves: vec![studio_core::graph_session::NodeMove {
                node: root.as_str().unwrap().to_owned(),
                x: 64.0,
                y: 96.0,
            }],
        })
        .unwrap();
    let expected = session.export_ui().unwrap();
    save(&db, "session-ui", &session.snapshot().unwrap());
    let restored = restore(&db, "session-ui", info());
    assert_eq!(restored.view()["sourceFormat"], "comfy_ui");
    assert_eq!(restored.export_ui().unwrap(), expected);
    assert_eq!(
        restored.export_ui().unwrap()["definitions"]["subgraphs"][0]["id"],
        "sgimg",
        "subgraph source must be preserved"
    );
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
