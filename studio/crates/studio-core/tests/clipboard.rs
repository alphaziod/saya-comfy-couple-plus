//! Copy/paste, duplication and multi-selection on interactive sessions.
use serde_json::{json, Value};
use studio_core::graph_editor::Point;
use studio_core::graph_session::{EditOp, GraphSession};

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

fn nodes(view: &Value) -> &Vec<Value> {
    view["document"]["nodes"].as_array().unwrap()
}

fn node<'a>(view: &'a Value, id: &str) -> &'a Value {
    nodes(view)
        .iter()
        .find(|n| n["id"].as_str() == Some(id))
        .unwrap()
}

#[test]
fn copy_paste_preserves_values_links_and_is_one_undo_step() {
    let mut session = GraphSession::open("wf", &api(), &info(), "Clipboard").unwrap();
    let before = session.view();
    let checkpoint = node_for(&before, "1");
    let clip = node_for(&before, "2");
    let fragment = session
        .copy_fragment(&[checkpoint.clone(), clip.clone()])
        .unwrap();
    assert_eq!(fragment["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(fragment["connections"].as_array().unwrap().len(), 1);
    assert_eq!(nodes(&before).len(), 7, "copy must not mutate");
    let before_links = before["document"]["connections"].as_array().unwrap().len();

    let pasted = session
        .paste_fragment(&fragment, Point { x: 50.0, y: 60.0 })
        .unwrap();
    let new_ids: Vec<String> = pasted["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(new_ids.len(), 2);
    assert!(new_ids
        .iter()
        .all(|id| !before["originalIds"].as_object().unwrap().contains_key(id)));

    let after = pasted["view"].clone();
    assert_eq!(nodes(&after).len(), 9);
    assert_eq!(
        after["document"]["connections"].as_array().unwrap().len(),
        before_links + 1
    );
    let new_checkpoint = nodes(&after)
        .iter()
        .find(|n| {
            n["classType"] == json!("CheckpointLoaderSimple")
                && !before["originalIds"]
                    .as_object()
                    .unwrap()
                    .contains_key(n["id"].as_str().unwrap())
        })
        .unwrap();
    let old_checkpoint = node(&before, &checkpoint);
    assert_eq!(
        new_checkpoint["position"]["x"].as_f64().unwrap(),
        old_checkpoint["position"]["x"].as_f64().unwrap() + 50.0
    );
    let new_clip = nodes(&after)
        .iter()
        .find(|n| {
            n["classType"] == json!("CLIPTextEncode")
                && n["params"]["text"] == json!("exact,  double\nligne")
                && !before["originalIds"]
                    .as_object()
                    .unwrap()
                    .contains_key(n["id"].as_str().unwrap())
        })
        .unwrap();
    assert_eq!(new_clip["params"]["text"], json!("exact,  double\nligne"));
    let linked = after["document"]["connections"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["from"]["node"] == new_checkpoint["id"] && c["to"]["node"] == new_clip["id"])
        .expect("pasted internal connection");
    assert_eq!(linked["to"]["input"], "clip");

    session.apply(EditOp::Undo).unwrap();
    let undone = session.view();
    assert_eq!(nodes(&undone).len(), 7);
    assert_eq!(
        undone["document"]["connections"].as_array().unwrap().len(),
        before_links
    );
    session.apply(EditOp::Redo).unwrap();
    assert_eq!(nodes(&session.view()).len(), 9);
}

#[test]
fn invalid_fragment_is_rejected_without_partial_write() {
    let mut session = GraphSession::open("wf", &api(), &info(), "Clipboard").unwrap();
    let view = session.view();
    let fragment = session
        .copy_fragment(&[node_for(&view, "1"), node_for(&view, "2")])
        .unwrap();
    let mut broken = fragment.clone();
    broken["connections"][0]["from"]["node"] = json!("ghost");
    assert!(session
        .paste_fragment(&broken, Point { x: 10.0, y: 10.0 })
        .is_err());
    assert_eq!(nodes(&session.view()).len(), 7);
}

#[test]
fn multi_node_move_undoes_and_redoes_as_one_batch() {
    let mut session = GraphSession::open("wf", &api(), &info(), "Multi").unwrap();
    let view = session.view();
    let first = node_for(&view, "1");
    let second = node_for(&view, "2");
    let p1 = node(&view, &first)["position"].clone();
    let p2 = node(&view, &second)["position"].clone();
    session
        .apply(EditOp::Move {
            moves: vec![
                studio_core::graph_session::NodeMove {
                    node: first.clone(),
                    x: 120.0,
                    y: 140.0,
                },
                studio_core::graph_session::NodeMove {
                    node: second.clone(),
                    x: 220.0,
                    y: 240.0,
                },
            ],
        })
        .unwrap();
    let moved = session.view();
    assert_eq!(node(&moved, &first)["position"]["x"], json!(120.0));
    assert_eq!(node(&moved, &second)["position"]["y"], json!(240.0));
    session.apply(EditOp::Undo).unwrap();
    let undone = session.view();
    assert_eq!(node(&undone, &first)["position"], p1);
    assert_eq!(node(&undone, &second)["position"], p2);
    session.apply(EditOp::Redo).unwrap();
    assert_eq!(node(&session.view(), &first)["position"]["x"], json!(120.0));
}
