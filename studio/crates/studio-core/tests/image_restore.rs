use serde_json::json;

#[test]
fn png_prompt_seed_and_ui_workflow_are_exposed_without_writing() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/restore.png");
    let before = std::fs::read(&path).unwrap();
    let result = studio_core::image_restore::inspect(&path).unwrap();
    assert_eq!(result["hasPrompt"], json!(true));
    assert_eq!(result["hasWorkflow"], json!(true));
    assert_eq!(result["fields"]["seeds"][0]["value"], json!("42"));
    assert!(
        result["fields"]["texts"]
            .as_array()
            .is_some_and(|v| !v.is_empty()),
        "texts: {}",
        result["fields"]
    );
    assert_eq!(result["width"], json!(64));
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn selective_restore_preserves_u64_and_unselected_inputs() {
    let source = serde_json::json!({"1":{"class_type":"KSampler","inputs":{"seed":9007199254740993u64,"steps":30}}});
    let target = serde_json::json!({"1":{"class_type":"KSampler","inputs":{"seed":1,"steps":10}}});
    let patches = studio_core::image_restore::selected_patches(
        &source,
        &target,
        &[("1".into(), "seed".into())],
    )
    .unwrap();
    assert_eq!(
        patches,
        vec![serde_json::json!({"node":"1","input":"seed","value":9007199254740993u64})]
    );
    assert_eq!(target["1"]["inputs"]["steps"], 10);
    assert!(studio_core::image_restore::selected_patches(
        &source,
        &target,
        &[("1".into(), "missing".into())]
    )
    .is_err());
    assert!(studio_core::image_restore::selected_patches(
        &source,
        &serde_json::json!({"1":{"class_type":"Other","inputs":{"seed":1}}}),
        &[("1".into(), "seed".into())]
    )
    .is_err());
}

#[test]
fn selective_restore_accepts_lora_values_but_never_graph_connections() {
    let source = json!({"1":{"class_type":"Lora Loader (LoraManager)","inputs":{"loras":[{"name":"A","strength":0.65}],"model":["2",0]}}});
    let target = json!({"1":{"class_type":"Lora Loader (LoraManager)","inputs":{"loras":[],"model":["3",0]}}});
    assert!(studio_core::image_restore::selected_patches(
        &source,
        &target,
        &[("1".into(), "loras".into())]
    )
    .is_ok());
    assert!(studio_core::image_restore::selected_patches(
        &source,
        &target,
        &[("1".into(), "model".into())]
    )
    .is_err());
}

#[test]
fn temporary_edits_preserve_exact_seeds_links_and_original() {
    use studio_core::image_restore::{editable_inputs, edited_temporary_graph, TemporaryEdit};
    let graph = json!({"1":{"class_type":"KSampler","inputs":{"seed":18446744073709551615u64,"steps":20,"model":["2",0]}},"2":{"class_type":"Text","inputs":{"text":"original,  exact\ntext"}}});
    assert_eq!(editable_inputs(&graph).len(), 3);
    assert_eq!(edited_temporary_graph(&graph, &[]).unwrap(), graph);
    let changed = edited_temporary_graph(
        &graph,
        &[TemporaryEdit {
            node: "1".into(),
            input: "steps".into(),
            value: "7".into(),
        }],
    )
    .unwrap();
    assert_eq!(changed["1"]["inputs"]["steps"], 7);
    assert_eq!(changed["1"]["inputs"]["seed"], 18446744073709551615u64);
    assert_eq!(
        changed["1"]["inputs"]["model"],
        graph["1"]["inputs"]["model"]
    );
    assert_eq!(graph["1"]["inputs"]["steps"], 20);
    for (input, value) in [("model", "3"), ("steps", "[\"2\",0]"), ("absent", "1")] {
        assert!(edited_temporary_graph(
            &graph,
            &[TemporaryEdit {
                node: "1".into(),
                input: input.into(),
                value: value.into()
            }]
        )
        .is_err());
    }
}

#[test]
fn temporary_execution_is_not_a_saved_workflow() {
    let db = studio_core::Database::in_memory().unwrap();
    let graph = json!({"1":{"class_type":"Output","inputs":{"text":"exact prompt"}}});
    let info = json!({"Output":{"input":{"required":{"text":["STRING",{}]}},"output":[],"output_node":true}});
    assert!(db.workflows().unwrap().is_empty());
    let job = db.prepare_temporary_image(&graph, &info, None).unwrap();
    assert!(db.workflows().unwrap().is_empty());
    assert_eq!(db.jobs().unwrap().len(), 1);
    let snapshot: serde_json::Value = serde_json::from_str(&job.request_json).unwrap();
    assert_eq!(snapshot["prompt"], graph);
    assert!(db.prepare_temporary_image(&json!({}), &info, None).is_err());
    assert_eq!(db.jobs().unwrap().len(), 1);
}

#[test]
fn imported_workflow_activates_all_scopes_without_touching_widgets_or_source() {
    let source = json!({"nodes":[{"id":1,"mode":2,"type":"sg","properties":{"saya_phase":2,"saya_original_mode":4},"widgets_values":[18446744073709551615u64,"exact prompt"]}],"definitions":{"subgraphs":[{"id":"sg","nodes":[{"id":2,"mode":4,"widgets_values":[false]}]}]}});
    let mut source = source;
    source["extra"] = json!({"saya_preset":{"version":1,"phases":[{"nodes":[{"id":3,"mode":4,"properties":{"saya_original_mode":4}}]}]}});
    let mut copy = source.clone();
    studio_core::image_restore::activate_workflow(&mut copy);
    assert_eq!(copy["nodes"][0]["mode"], 0);
    assert_eq!(
        copy["extra"]["saya_preset"]["phases"][0]["nodes"][0]["mode"],
        0
    );
    assert_eq!(
        copy["extra"]["saya_preset"]["phases"][0]["nodes"][0]["properties"]["saya_original_mode"],
        0
    );
    assert_eq!(copy["nodes"][0]["properties"]["saya_original_mode"], 0);
    assert_eq!(copy["definitions"]["subgraphs"][0]["nodes"][0]["mode"], 0);
    assert_eq!(
        copy["nodes"][0]["widgets_values"],
        source["nodes"][0]["widgets_values"]
    );
    assert_eq!(
        copy["definitions"]["subgraphs"][0]["nodes"][0]["widgets_values"],
        json!([false])
    );
    assert_eq!(source["nodes"][0]["mode"], 2);
    assert!(studio_core::image_restore::is_saya_workflow(&copy));
    assert!(!studio_core::image_restore::is_saya_workflow(
        &json!({"nodes":[{"type":"KSampler"}]})
    ));
}
