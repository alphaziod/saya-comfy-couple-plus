use serde_json::{json, Value};
use studio_core::{
    couple::{CoupleInput, SplitEdit},
    workflow::Edit,
    Database,
};
fn fixture() -> (Database, String, Value) {
    let mut db = Database::in_memory().unwrap();
    let graph = json!({
        "p1":{"class_type":"Text","inputs":{"text":"old p1"}},"p2":{"class_type":"Text","inputs":{"text":"old p2"}},
        "action":{"class_type":"Text","inputs":{"text":"old action"}},"bg":{"class_type":"Text","inputs":{"text":"old bg"}},
        "latent":{"class_type":"Latent","inputs":{}},
        "mask":{"class_type":"SayaSplitMask","inputs":{"direction":"vertical","split":50,"blur":0.0,"swap":false,"latent":["latent",0]}},
        "out":{"class_type":"Output","inputs":{"p1":["p1",0],"p2":["p2",0],"mask1":["mask",0],"mask2":["mask",1]}}
    });
    let id = db
        .import_workflow("Couple fixture", &graph.to_string())
        .unwrap();
    let fields = db.workflow(&id).unwrap().fields;
    let bindings: Vec<_> = fields
        .into_iter()
        .map(|f| Edit {
            section: match f.node_id.as_str() {
                "p1" => Some("P1".into()),
                "p2" => Some("P2".into()),
                "action" => Some("ACTION".into()),
                "bg" => Some("BG".into()),
                _ => None,
            },
            node_id: f.node_id,
            input_name: f.input_name,
            value: f.value,
        })
        .collect();
    db.save_bindings(&id, &bindings).unwrap();
    (db, id, graph)
}
fn input() -> CoupleInput {
    serde_json::from_value(json!({"p1":"  P1, exact,  tags\n","p2":"P2, distinct","action":"shared action","bg":"background only","mask":{"nodeId":"mask","direction":"horizontal","split":35,"blur":1.5,"swap":true}})).unwrap()
}

#[test]
fn solo_editor_uses_only_explicit_p1_mapping_without_touching_p2_nodes() {
    let (mut db, id, original) = fixture();
    let fields = db.workflow(&id).unwrap().fields;
    let bindings: Vec<Edit> = fields
        .into_iter()
        .filter(|f| f.enabled && f.section.as_deref() != Some("P2"))
        .map(|f| Edit {
            node_id: f.node_id,
            input_name: f.input_name,
            value: f.value,
            section: f.section,
        })
        .collect();
    db.save_bindings(&id, &bindings).unwrap();
    let inspection = db.couple_inspection(&id).unwrap();
    assert_eq!(inspection["promptProfile"], "solo");
    assert!(!inspection["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str().unwrap().contains("Mapping P2")));
    let mut data = input();
    assert!(db.couple_edits(&id, &data).is_err());
    data.p2.clear();
    let edits = db.couple_edits(&id, &data).unwrap();
    assert!(!edits
        .iter()
        .any(|e| e.node_id == "p2" || e.section.as_deref() == Some("P2")));
    assert_eq!(
        db.workflow(&id)
            .unwrap()
            .fields
            .iter()
            .find(|f| f.node_id == "p2")
            .unwrap()
            .value,
        original["p2"]["inputs"]["text"]
    );
    data.p1.clear();
    assert!(db.couple_edits(&id, &data).is_err());
}
#[test]
fn separate_prompts_and_mask_preserve_every_link_and_original() {
    let (db, id, original) = fixture();
    let input = input();
    let edits = db.couple_edits(&id, &input).unwrap();
    let mut result = original.clone();
    for edit in edits {
        result[&edit.node_id]["inputs"][&edit.input_name] = match edit.input_name.as_str() {
            "split" | "blur" | "swap" => serde_json::from_str(&edit.value).unwrap(),
            _ => json!(edit.value),
        };
    }
    assert_eq!(result["p1"]["inputs"]["text"], input.p1);
    assert_eq!(result["p2"]["inputs"]["text"], input.p2);
    assert_eq!(result["action"]["inputs"]["text"], input.action);
    assert_eq!(result["bg"]["inputs"]["text"], input.bg);
    assert_eq!(
        result["mask"]["inputs"]["latent"],
        original["mask"]["inputs"]["latent"]
    );
    assert_eq!(result["out"], original["out"]);
    assert_eq!(
        db.workflow(&id)
            .unwrap()
            .fields
            .iter()
            .find(|f| f.node_id == "p1")
            .unwrap()
            .value,
        "old p1"
    );
    assert_eq!(
        db.couple_inspection(&id).unwrap()["masks"][0]["nodeId"],
        "mask"
    );
}
#[test]
fn refuses_missing_roles_unapproved_masks_and_invalid_geometry() {
    let (mut db, id, _) = fixture();
    let mut data = input();
    data.mask.as_mut().unwrap().split = 101;
    assert!(db.couple_edits(&id, &data).is_err());
    data.mask = Some(SplitEdit {
        node_id: "p1".into(),
        direction: "vertical".into(),
        split: 50,
        blur: 0.0,
        swap: false,
    });
    assert!(db.couple_edits(&id, &data).is_err());
    data.mask = None;
    data.p2.clear();
    assert!(db.couple_edits(&id, &data).is_err());
    db.save_bindings(&id, &[]).unwrap();
    assert!(db.couple_edits(&id, &input()).is_err());
}

fn schema() -> Value {
    json!({
        "Text":{"input":{"required":{"text":["STRING"]}},"output":["CONDITIONING"]},
        "Latent":{"input":{"required":{}},"output":["LATENT"]},
        "SayaSplitMask":{"input":{"required":{"direction":[["vertical","horizontal"]],"split":["INT",{"min":0,"max":100}],"blur":["FLOAT",{"min":0}],"swap":["BOOLEAN"],"latent":["LATENT"]}},"output":["MASK","MASK"]},
        "Output":{"input":{"required":{"p1":["CONDITIONING"],"p2":["CONDITIONING"],"mask1":["MASK"],"mask2":["MASK"]}},"output":[],"output_node":true}
    })
}
#[test]
fn prepared_snapshot_changes_only_explicit_inputs() {
    let (db, id, original) = fixture();
    let data = input();
    let edits = db.couple_edits(&id, &data).unwrap();
    let prepared = db.prepare_job(&id, &edits, &schema()).unwrap();
    let request: Value = serde_json::from_str(&prepared.request_json).unwrap();
    let mut expected = original.clone();
    for (node, text) in [
        ("p1", &data.p1),
        ("p2", &data.p2),
        ("action", &data.action),
        ("bg", &data.bg),
    ] {
        expected[node]["inputs"]["text"] = json!(text);
    }
    expected["mask"]["inputs"]["direction"] = json!("horizontal");
    expected["mask"]["inputs"]["split"] = json!(35);
    expected["mask"]["inputs"]["blur"] = json!(1.5);
    expected["mask"]["inputs"]["swap"] = json!(true);
    assert_eq!(request["prompt"], expected);
    let untouched = db.prepare_job(&id, &[], &schema()).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&untouched.request_json).unwrap()["prompt"],
        original
    );
    let mut no_mask = input();
    no_mask.mask = None;
    let prepared = db
        .prepare_job(&id, &db.couple_edits(&id, &no_mask).unwrap(), &schema())
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&prepared.request_json).unwrap()["prompt"]["mask"],
        original["mask"]
    );
}
#[test]
fn rejects_linked_mask_control_missing_p2_and_invalid_slots() {
    let (mut db, _, mut graph) = fixture();
    graph["mask"]["inputs"]["split"] = json!(["latent", 0]);
    let id = db
        .import_workflow("Linked split", &graph.to_string())
        .unwrap();
    let edits: Vec<_> = db
        .workflow(&id)
        .unwrap()
        .fields
        .into_iter()
        .map(|f| Edit {
            section: match f.node_id.as_str() {
                "p1" => Some("P1".into()),
                "p2" => Some("P2".into()),
                "action" => Some("ACTION".into()),
                "bg" => Some("BG".into()),
                _ => None,
            },
            node_id: f.node_id,
            input_name: f.input_name,
            value: f.value,
        })
        .collect();
    db.save_bindings(&id, &edits).unwrap();
    assert!(db.couple_edits(&id, &input()).is_err());
    let (mut db, id, _) = fixture();
    let bindings: Vec<_> = db
        .workflow(&id)
        .unwrap()
        .fields
        .into_iter()
        .filter(|f| f.section.as_deref() != Some("P2"))
        .map(|f| Edit {
            section: f.section,
            node_id: f.node_id,
            input_name: f.input_name,
            value: f.value,
        })
        .collect();
    db.save_bindings(&id, &bindings).unwrap();
    assert!(db.couple_edits(&id, &input()).is_err());
    assert!(!db.couple_inspection(&id).unwrap()["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str().unwrap().contains("P2")));
    let (db, id, _) = fixture();
    let mut info = schema();
    info["SayaSplitMask"]["output"] = json!(["MASK"]);
    assert!(db
        .prepare_job(&id, &db.couple_edits(&id, &input()).unwrap(), &info)
        .is_err());
}

#[test]
fn float_schema_accepts_fractional_blur_when_original_literal_is_integer() {
    let (mut db, _, mut graph) = fixture();
    graph["mask"]["inputs"]["blur"] = json!(0);
    let id = db
        .import_workflow("Integer serialized float", &graph.to_string())
        .unwrap();
    let fields = db.workflow(&id).unwrap().fields;
    let bindings: Vec<_> = fields
        .into_iter()
        .map(|f| Edit {
            section: match f.node_id.as_str() {
                "p1" => Some("P1".into()),
                "p2" => Some("P2".into()),
                "action" => Some("ACTION".into()),
                "bg" => Some("BG".into()),
                _ => None,
            },
            node_id: f.node_id,
            input_name: f.input_name,
            value: f.value,
        })
        .collect();
    db.save_bindings(&id, &bindings).unwrap();
    let prepared = db
        .prepare_job(&id, &db.couple_edits(&id, &input()).unwrap(), &schema())
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&prepared.request_json).unwrap()["prompt"]["mask"]["inputs"]
            ["blur"],
        1.5
    );
}

#[test]
fn optional_shared_roles_are_never_invented_or_silently_discarded() {
    for coupled in [false, true] {
        let (mut db, id, original) = fixture();
        let bindings: Vec<_> = db
            .workflow(&id)
            .unwrap()
            .fields
            .into_iter()
            .filter(|f| {
                f.section.as_deref() == Some("P1")
                    || (coupled && f.section.as_deref() == Some("P2"))
            })
            .map(|f| Edit {
                section: f.section,
                node_id: f.node_id,
                input_name: f.input_name,
                value: f.value,
            })
            .collect();
        db.save_bindings(&id, &bindings).unwrap();
        let mut data = input();
        data.mask = None;
        if !coupled {
            data.p2.clear();
        }
        assert!(db.couple_edits(&id, &data).is_err());
        data.action.clear();
        data.bg.clear();
        let edits = db.couple_edits(&id, &data).unwrap();
        assert_eq!(edits.len(), if coupled { 2 } else { 1 });
        let prepared = db.prepare_job(&id, &edits, &schema()).unwrap();
        let mut expected = original;
        expected["p1"]["inputs"]["text"] = json!(data.p1);
        if coupled {
            expected["p2"]["inputs"]["text"] = json!(data.p2);
        }
        assert_eq!(
            serde_json::from_str::<Value>(&prepared.request_json).unwrap()["prompt"],
            expected
        );
        assert!(db.couple_inspection(&id).unwrap()["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| !v.as_str().unwrap().contains("Mapping")));
    }
}
