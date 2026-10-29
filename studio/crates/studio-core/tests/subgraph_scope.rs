//! Scoped subgraph editing: enter a definition, edit inner position/params,
//! inject back into the preserved UI source without touching boundary links.
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

#[test]
fn scoped_edit_reinjects_params_and_preserves_boundary_links() {
    let mut session = GraphSession::open_file(&source(), &info(), "Scope").unwrap();
    let before = session.export_ui().unwrap();
    session.enter_subgraph("sg-1").unwrap();
    let view = session.view();
    assert_eq!(view["scope"]["name"], "Smoke subgraph");
    assert_eq!(view["document"]["nodes"].as_array().unwrap().len(), 6);
    assert_eq!(view["document"]["connections"].as_array().unwrap().len(), 8);
    assert_eq!(view["paramText"]["7"]["steps"], "20");
    session
        .apply(EditOp::SetParam {
            node: "7".into(),
            input: "steps".into(),
            value: Some("25".into()),
        })
        .unwrap();
    session
        .apply(EditOp::Move {
            moves: vec![studio_core::graph_session::NodeMove {
                node: "4".into(),
                x: 111.0,
                y: 222.0,
            }],
        })
        .unwrap();
    assert_eq!(session.view()["paramText"]["7"]["steps"], "25");
    session.apply(EditOp::Undo).unwrap();
    assert_eq!(session.view()["paramText"]["7"]["steps"], "25");
    session.apply(EditOp::Undo).unwrap();
    assert_eq!(session.view()["paramText"]["7"]["steps"], "20");
    session.apply(EditOp::Redo).unwrap();
    session.apply(EditOp::Redo).unwrap();
    assert_eq!(session.view()["paramText"]["7"]["steps"], "25");
    assert!(
        session.snapshot().is_err(),
        "scoped snapshot must be refused"
    );
    session.exit_subgraph(true).unwrap();
    assert!(session.view()["scope"].is_null());

    let after = session.export_ui().unwrap();
    let def = &after["definitions"]["subgraphs"][0];
    let sampler = def["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == json!(7))
        .unwrap();
    assert_eq!(sampler["widgets_values_named"]["steps"], 25);
    assert_eq!(sampler["pos"][0].as_f64(), Some(540.0));
    assert_eq!(sampler["pos"][1].as_f64(), Some(40.0));
    let moved = def["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == json!(4))
        .unwrap();
    assert_eq!(moved["pos"][0].as_f64(), Some(111.0));
    assert_eq!(moved["pos"][1].as_f64(), Some(222.0));
    assert_eq!(
        def["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == json!(4))
            .unwrap()["inputs"][0]["link"],
        json!(10),
        "boundary input link restored"
    );
    assert_eq!(def["links"].as_array().unwrap().len(), 10);
    assert_eq!(
        def["links"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|l| l["target_id"] == json!(-20))
            .count(),
        1
    );
    assert_eq!(before["nodes"], after["nodes"], "root nodes preserved");
    assert_eq!(before["links"], after["links"], "root links preserved");
}

#[test]
fn cancel_restores_exact_source_after_structural_edits() {
    let mut session = GraphSession::open_file(&source(), &info(), "Scope").unwrap();
    let before = session.export_ui().unwrap();
    session.enter_subgraph("sg-1").unwrap();
    session
        .apply(EditOp::SetParam {
            node: "7".into(),
            input: "cfg".into(),
            value: Some("9".into()),
        })
        .unwrap();
    session
        .apply(EditOp::Add {
            class_type: "KSampler".into(),
            x: 0.0,
            y: 0.0,
        })
        .unwrap();
    assert_eq!(
        session.view()["document"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    session.exit_subgraph(false).unwrap();
    assert_eq!(session.export_ui().unwrap(), before);
}

#[test]
fn structural_add_connect_and_remove_commit_into_definition() {
    let mut session = GraphSession::open_file(&source(), &info(), "Scope").unwrap();
    let before = session.export_ui().unwrap();
    session.enter_subgraph("sg-1").unwrap();
    session
        .apply(EditOp::Add {
            class_type: "CLIPTextEncode".into(),
            x: 600.0,
            y: 600.0,
        })
        .unwrap();
    let view = session.view();
    let added = view["document"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| {
            n["classType"] == "CLIPTextEncode" && n["id"] != json!("4") && n["id"] != json!("5")
        })
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    session
        .apply(EditOp::Connect {
            from_node: "3".into(),
            output: 1,
            to_node: added.clone(),
            input: "clip".into(),
        })
        .unwrap();
    session
        .apply(EditOp::Remove {
            nodes: vec!["5".into()],
        })
        .unwrap();
    session.exit_subgraph(true).unwrap();

    let after = session.export_ui().unwrap();
    let def = &after["definitions"]["subgraphs"][0];
    assert_eq!(def["nodes"].as_array().unwrap().len(), 6);
    let new_node = def["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["type"] == "CLIPTextEncode" && n["id"] != json!(4) && n["id"] != json!(5))
        .expect("new inner node");
    assert!(new_node["inputs"][0]["link"].is_number());
    assert_eq!(before["nodes"], after["nodes"]);
    assert_eq!(before["links"], after["links"]);
    // The updated definition must be parseable on a second entry.
    session.enter_subgraph("sg-1").unwrap();
    assert_eq!(
        session.view()["document"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    session.exit_subgraph(false).unwrap();
}

#[test]
fn paste_inside_scope_creates_a_numeric_ui_node() {
    let mut session = GraphSession::open_file(&source(), &info(), "Scope").unwrap();
    let before = session.export_ui().unwrap();
    session.enter_subgraph("sg-1").unwrap();
    let fragment = session.copy_fragment(&["5".into()]).unwrap();
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
        7
    );
    session.exit_subgraph(true).unwrap();
    let after = session.export_ui().unwrap();
    let def = &after["definitions"]["subgraphs"][0];
    assert_eq!(def["nodes"].as_array().unwrap().len(), 7);
    let ids: Vec<u64> = def["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["id"].as_u64())
        .collect();
    assert_eq!(ids.len(), 7);
    assert_eq!(ids.iter().max(), Some(&9), "{ids:?}");
    assert_eq!(before["nodes"], after["nodes"]);
    assert_eq!(before["links"], after["links"]);
    assert_eq!(pasted["nodes"].as_array().unwrap().len(), 1);
    session.enter_subgraph("sg-1").unwrap();
    assert_eq!(
        session.view()["document"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    session.exit_subgraph(false).unwrap();
}

#[test]
fn nested_fixture_exposes_container() {
    let source = include_str!("../../../scripts/fixtures/v15.2-nested-ui.json");
    let session = GraphSession::open_file(source, &info(), "Nested fixture").unwrap();
    let view = session.view();
    eprintln!("SUBS {}", view["document"]["subgraphs"]);
    assert_eq!(view["document"]["subgraphs"][0]["nodeIds"], json!(["1"]));
}

#[test]
fn nested_scopes_propagate_the_inner_definition() {
    let mut workflow: Value = serde_json::from_str(&source()).unwrap();
    let inner = workflow["definitions"]["subgraphs"][0].clone();
    let outer = json!({
        "id": "sgouter",
        "version": 1,
        "name": "Outer",
        "inputs": [],
        "outputs": [{"name": "image", "type": "IMAGE", "linkIds": [30]}],
        "nodes": [{
            "id": 10, "type": "sgimg", "pos": [200, 200], "size": [260, 140],
            "flags": {}, "order": 0, "mode": 0,
            "inputs": [], "outputs": [{"name": "image", "type": "IMAGE", "links": [30]}],
            "properties": {}, "widgets_values": ["nested hello"],
            "widgets_values_named": {"text": "nested hello"}
        }],
        "groups": [],
        "links": [{"id": 30, "origin_id": 10, "origin_slot": 0, "target_id": -20, "target_slot": 0, "type": "IMAGE"}]
    });
    workflow["definitions"]["subgraphs"] = json!([inner, outer]);
    workflow["nodes"][0]["type"] = json!("sgouter");
    workflow["nodes"][0]["inputs"] = json!([]);
    workflow["nodes"][0]["outputs"] = json!([{"name": "image", "type": "IMAGE", "links": [20]}]);
    let mut session = GraphSession::open_file(&workflow.to_string(), &info(), "Nested").unwrap();
    session.enter_subgraph("sg-1").unwrap();
    assert_eq!(session.view()["scope"]["name"], "Outer");
    assert_eq!(
        session.view()["document"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    session.enter_subgraph("sg-10").unwrap();
    assert_eq!(session.view()["scope"]["name"], "Smoke subgraph");
    assert_eq!(session.view()["paramText"]["7"]["steps"], "20");
    session
        .apply(EditOp::SetParam {
            node: "7".into(),
            input: "steps".into(),
            value: Some("31".into()),
        })
        .unwrap();
    session.exit_subgraph(true).unwrap();
    assert_eq!(session.view()["scope"]["name"], "Outer");
    session.exit_subgraph(true).unwrap();
    assert!(session.view()["scope"].is_null());
    let exported = session.export_ui().unwrap();
    let defs = exported["definitions"]["subgraphs"].as_array().unwrap();
    let inner_after = defs.iter().find(|d| d["id"] == "sgimg").unwrap();
    let sampler = inner_after["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == json!(7))
        .unwrap();
    assert_eq!(sampler["widgets_values_named"]["steps"], 31);
    let outer_after = defs.iter().find(|d| d["id"] == "sgouter").unwrap();
    assert_eq!(outer_after["nodes"][0]["type"], "sgimg");
    assert_eq!(outer_after["links"].as_array().unwrap().len(), 1);
    // Re-entering both levels parses the updated nested definitions.
    session.enter_subgraph("sg-1").unwrap();
    session.enter_subgraph("sg-10").unwrap();
    assert_eq!(session.view()["paramText"]["7"]["steps"], "31");
    session.exit_subgraph(false).unwrap();
    session.exit_subgraph(false).unwrap();
}

#[test]
fn removing_boundary_connected_node_is_refused() {
    let mut session = GraphSession::open_file(&source(), &info(), "Scope").unwrap();
    let before = session.export_ui().unwrap();
    session.enter_subgraph("sg-1").unwrap();
    session
        .apply(EditOp::Remove {
            nodes: vec!["4".into()],
        })
        .unwrap();
    let error = session.exit_subgraph(true).unwrap_err().to_string();
    assert!(error.contains("frontière"), "{error}");
    session.exit_subgraph(false).unwrap();
    assert_eq!(session.export_ui().unwrap(), before);
}

#[test]
fn promoted_container_widget_edits_reach_api_and_undo_without_changing_definition() {
    let mut session = GraphSession::open_file(&source(), &info(), "Promoted").unwrap();
    let before = session.export_ui().unwrap();
    session
        .apply(EditOp::SetParam {
            node: "1".into(),
            input: "text".into(),
            value: Some("edited promoted prompt".into()),
        })
        .unwrap();
    let edited = session.export_ui().unwrap();
    assert_eq!(
        edited["nodes"][0]["widgets_values"][0],
        "edited promoted prompt"
    );
    assert_eq!(edited["definitions"], before["definitions"]);
    assert_eq!(edited["links"], before["links"]);
    let api =
        studio_core::workflow_ui_api::prepare_api(&edited.to_string(), Some(&info())).unwrap();
    assert_eq!(
        api["api"]["1:4"]["inputs"]["text"],
        "edited promoted prompt"
    );
    session.apply(EditOp::Undo).unwrap();
    assert_eq!(session.export_ui().unwrap(), before);
    session.apply(EditOp::Redo).unwrap();
    assert_eq!(session.export_ui().unwrap(), edited);
}

#[test]
fn lora_manager_widgets_and_master_seed_roundtrip_without_losing_frontend_state() {
    let schema = json!({"Lora Loader (LoraManager)":{"input":{"required":{"model":["MODEL"],"text":["AUTOCOMPLETE_TEXT_LORAS"],"loras":["LORAS"]}},"input_order":{"required":["model","text","loras"]},"output":["MODEL","CLIP","STRING","STRING"]},"Seed (rgthree)":{"input":{"required":{"seed":["INT"]}},"input_order":{"required":["seed"]},"output":["INT"]}});
    let source = json!({"version":0.4,"nodes":[{"id":655,"type":"Lora Loader (LoraManager)","inputs":[],"outputs":[],"widgets_values":[{"version":1,"textWidgetName":"text"},"original text",[{"name":"fixture","strength":0.4,"clipStrength":0.2,"active":false}]]},{"id":100032,"type":"Seed (rgthree)","inputs":[],"outputs":[],"widgets_values":[9007199254740993_u64,"","","okay"]}],"links":[]});
    let mut session = GraphSession::open_file(&source.to_string(), &schema, "widgets").unwrap();
    assert_eq!(
        session.view()["paramText"]["100032"]["seed"],
        "9007199254740993"
    );
    session.apply(EditOp::SetParam{node:"655".into(),input:"loras".into(),value:Some(json!({"__value__":[{"name":"fixture","strength":0.65,"clipStrength":0.35,"active":true}]}).to_string())}).unwrap();
    let edited = session.export_ui().unwrap();
    assert_eq!(edited["nodes"][0]["widgets_values"][2][0]["strength"], 0.65);
    assert_eq!(edited["nodes"][0]["widgets_values"][2][0]["active"], true);
    assert_eq!(
        edited["nodes"][0]["widgets_values"][0],
        source["nodes"][0]["widgets_values"][0]
    );
    assert_eq!(edited["nodes"][1], source["nodes"][1]);
    assert!(session
        .apply(EditOp::SetParam {
            node: "655".into(),
            input: "loras".into(),
            value: Some("{\"__value__\":[{\"name\":\"bad\"}]}".into())
        })
        .is_err());
}

#[test]
fn promoted_boundary_inputs_without_widgets_do_not_shift_sibling_values() {
    let source = json!({"version":0.4,"nodes":[{"id":1,"type":"region","inputs":[{"name":"text","type":"STRING","widget":{"name":"text"}},{"name":"linked_text","type":"STRING","link":null},{"name":"blur","type":"FLOAT","widget":{"name":"blur"}}],"outputs":[],"widgets_values":["unchanged",0.2],"widgets_values_named":{"text":"unchanged","blur":0.2}}],"links":[],"definitions":{"subgraphs":[{"id":"region","name":"Region","inputs":[{"name":"text","type":"STRING"},{"name":"linked_text","type":"STRING"},{"name":"blur","type":"FLOAT"}],"nodes":[],"links":[]}]}});
    let mut session = GraphSession::open_file(&source.to_string(), &json!({}), "promoted").unwrap();
    assert_eq!(session.export_ui().unwrap(), source);
    assert_eq!(session.view()["paramText"]["1"]["blur"], "0.2");
    session
        .apply(EditOp::SetParam {
            node: "1".into(),
            input: "text".into(),
            value: Some("edited".into()),
        })
        .unwrap();
    let edited = session.export_ui().unwrap();
    assert_eq!(edited["nodes"][0]["widgets_values"][1], 0.2);
    assert_eq!(edited["nodes"][0]["widgets_values_named"]["blur"], 0.2);
    assert_eq!(edited["definitions"], source["definitions"]);
}

#[test]
fn generation_scoped_widgets_leave_session_at_root_on_success_and_failure() {
    let mut session = GraphSession::open_file(&source(), &info(), "generation").unwrap();
    let before = session.export_ui().unwrap();
    let path = vec!["sg-1".into()];
    let view = session.scoped_widget(&path, None).unwrap();
    assert_eq!(view["paramText"]["7"]["steps"], "20");
    assert_eq!(session.export_ui().unwrap(), before);
    assert!(session
        .scoped_widget(
            &path,
            Some(EditOp::SetParam {
                node: "7".into(),
                input: "steps".into(),
                value: Some("invalid".into())
            })
        )
        .is_err());
    assert_eq!(session.export_ui().unwrap(), before);
    session
        .scoped_widget(
            &path,
            Some(EditOp::SetParam {
                node: "7".into(),
                input: "steps".into(),
                value: Some("25".into()),
            }),
        )
        .unwrap();
    assert!(!session.is_scoped());
    let exported = session.export_ui().unwrap();
    assert_eq!(
        exported["definitions"]["subgraphs"][0]["nodes"][4]["widgets_values_named"]["steps"],
        25
    );
}

#[test]
fn restore_ui_retains_display_cycles_but_api_still_rejects_them() {
    let info = json!({"Relay":{"input":{"required":{"value":["STRING"]}},"output":["STRING"]}});
    let source = json!({"version":0.4,"nodes":[
        {"id":1,"type":"Relay","pos":[0,0],"inputs":[{"name":"value","type":"STRING","link":2}],"outputs":[{"name":"value","type":"STRING","links":[1]}],"widgets_values":[]},
        {"id":2,"type":"Relay","pos":[200,0],"inputs":[{"name":"value","type":"STRING","link":1}],"outputs":[{"name":"value","type":"STRING","links":[2]}],"widgets_values":[]}
    ],"links":[[1,1,0,2,0,"STRING"],[2,2,0,1,0,"STRING"]]});
    let session = GraphSession::open_file(&source.to_string(), &info, "Display cycle").unwrap();
    let snapshot = session.snapshot().unwrap();
    let restored = GraphSession::restore(
        "",
        snapshot["document"].as_str().unwrap(),
        snapshot["uiSource"].as_str(),
        None,
        snapshot["originalIds"].as_str().unwrap(),
        info.clone(),
    )
    .unwrap();
    assert_eq!(restored.export_ui().unwrap(), session.export_ui().unwrap());
    assert!(GraphSession::restore(
        "",
        snapshot["document"].as_str().unwrap(),
        None,
        None,
        "{}",
        info
    )
    .is_err());
}

#[test]
fn normal_ui_execution_expands_subgraphs_without_saving_or_changing_source() {
    let mut session = GraphSession::open_file(&source(), &info(), "Normal workflow").unwrap();
    let before = session.export_ui().unwrap();
    assert!(
        session.export().is_err(),
        "legacy exporter must reproduce the old rejection"
    );
    session.scoped_widget(&["sg-1".to_owned()], None).unwrap();
    let (api, ui) = session.prepare_execution(&info()).unwrap();
    let expected =
        studio_core::workflow_ui_api::prepare_api(&before.to_string(), Some(&info())).unwrap();
    assert_eq!(expected["report"]["complete"], true);
    assert_eq!(api, expected["api"]);
    assert_eq!(ui.unwrap(), before);
    assert_eq!(session.export_ui().unwrap(), before);
}
