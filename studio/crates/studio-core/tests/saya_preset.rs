use serde_json::{json, Value};
use std::path::PathBuf;
use studio_core::saya_preset::{assemble, phase_source};
fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../presets/passes")
}
fn first() -> Value {
    serde_json::from_slice(
        &std::fs::read(folder().join("ILUSTCOUPLE_01_PASSE_1_Generation.json")).unwrap(),
    )
    .unwrap()
}
#[test]
fn seven_files_become_one_self_contained_editable_preset() {
    let original = first();
    let mut work = original.clone();
    let prompt = work["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|n| n["id"] == 807)
        .unwrap();
    prompt["widgets_values"] = json!(["  identité\nexacte  "]);
    let preset = assemble(&work, &folder()).unwrap();
    assert_eq!(
        preset["extra"]["saya_preset"]["phases"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
    for id in [100033, 100050, 100055, 100057, 2674, 100061] {
        assert!(
            preset["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|n| n["id"] == id),
            "phase node {id}"
        );
    }
    assert_eq!(
        assemble(&preset, &PathBuf::from("/absent")).unwrap(),
        preset
    );
    for phase in 1..=6 {
        let projected = phase_source(&preset, phase).unwrap().unwrap();
        if let Some(prompt) = projected["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == 807)
        {
            assert_eq!(prompt["widgets_values"], json!(["  identité\nexacte  "]));
        }
        assert!(projected["extra"].get("saya_preset").is_none());
    }
    assert_eq!(first(), original, "source file must remain untouched");
}
#[test]
fn later_phase_edits_survive_projection_and_roundtrip() {
    let mut preset = assemble(&first(), &folder()).unwrap();
    let def = preset["definitions"]["subgraphs"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|d| d["name"].as_str().is_some_and(|s| s.contains("Phase 3")))
        .unwrap();
    let id = def["id"].clone();
    def["nodes"][0]["widgets_values"] = json!(["changed phase 3"]);
    let serialized = serde_json::to_string(&preset).unwrap();
    let restored: Value = serde_json::from_str(&serialized).unwrap();
    let projected = phase_source(&restored, 3).unwrap().unwrap();
    assert_eq!(
        projected["definitions"]["subgraphs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == id)
            .unwrap()["nodes"][0]["widgets_values"],
        json!(["changed phase 3"])
    );
}
#[test]
fn incomplete_preset_is_refused() {
    assert!(assemble(&first(), &PathBuf::from("/absent")).is_err());
    assert_eq!(
        assemble(&json!({"nodes":[]}), &PathBuf::from("/absent")).unwrap(),
        json!({"nodes":[]})
    );
}

#[test]
#[ignore = "requires an object_info capture, no GPU"]
fn real_preset_api_parity_and_scoped_edit_survive_editor_restart() {
    use studio_core::{
        graph_session::{EditOp, GraphSession},
        saya::prepare_phase_api,
    };
    let info: Value =
        serde_json::from_slice(&std::fs::read(std::env::var("SAYA_OBJECT_INFO").unwrap()).unwrap())
            .unwrap();
    let preset = assemble(&first(), &folder()).unwrap();
    let full =
        std::fs::read_to_string(folder().parent().unwrap().join("ILUSTCOUPLECLEAN.json")).unwrap();
    let mut editor = GraphSession::open_file(&preset.to_string(), &info, "preset").unwrap();
    let exported = editor.export_ui().unwrap();
    assert_eq!(
        exported["extra"]["saya_preset"],
        preset["extra"]["saya_preset"]
    );
    fn reachable(api: &Value, info: &Value) -> Value {
        let mut pending: Vec<String> = api
            .as_object()
            .unwrap()
            .iter()
            .filter(|(_, n)| info[n["class_type"].as_str().unwrap()]["output_node"] == true)
            .map(|(k, _)| k.clone())
            .collect();
        assert!(!pending.is_empty());
        let mut live = serde_json::Map::new();
        while let Some(id) = pending.pop() {
            if live.contains_key(&id) {
                continue;
            }
            let mut n = api[&id].clone();
            for v in n["inputs"].as_object().unwrap().values() {
                if let Some(a) = v
                    .as_array()
                    .filter(|a| a.len() == 2 && a[0].is_string() && a[1].is_number())
                {
                    pending.push(a[0].as_str().unwrap().to_owned());
                }
            }
            n.as_object_mut().unwrap().remove("_meta");
            live.insert(id, n);
        }
        Value::Object(live)
    }
    for phase in 1..=6 {
        let actual = prepare_phase_api(
            &exported.to_string(),
            &info,
            phase,
            Some("12345"),
            Some("saya-tests/preset"),
        )
        .unwrap();
        let reference = prepare_phase_api(
            &full,
            &info,
            phase,
            Some("12345"),
            Some("saya-tests/preset"),
        )
        .unwrap();
        assert_eq!(
            reachable(&actual["api"], &info),
            reachable(&reference["api"], &info),
            "phase {phase}"
        );
    }
    let containers = editor.view()["document"]["subgraphs"].clone();
    let container = containers
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["nodeIds"].as_array().unwrap().contains(&json!("100055")))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let scoped = editor.scoped_widget(&[container.clone()], None).unwrap();
    assert_eq!(scoped["paramText"]["100088"]["steps"], "12");
    let changed = editor
        .scoped_widget(
            &[container],
            Some(EditOp::SetParam {
                node: "100088".into(),
                input: "steps".into(),
                value: Some("7".into()),
            }),
        )
        .unwrap();
    assert_eq!(changed["paramText"]["100088"]["steps"], "7");
    let snapshot = editor.snapshot().unwrap();
    let restored = GraphSession::restore(
        snapshot["workflow"].as_str().unwrap_or(""),
        snapshot["document"].as_str().unwrap(),
        snapshot["uiSource"].as_str(),
        snapshot["originalApi"].as_str(),
        snapshot["originalIds"].as_str().unwrap(),
        info.clone(),
    )
    .unwrap();
    let actual = prepare_phase_api(
        &restored.export_ui().unwrap().to_string(),
        &info,
        3,
        None,
        Some("saya-tests/preset"),
    )
    .unwrap();
    assert_eq!(actual["api"]["100055:100088"]["inputs"]["steps"], 7);
}
