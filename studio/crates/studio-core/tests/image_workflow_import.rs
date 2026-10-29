//! Real ComfyUI PNG workflow-UI conversion. Ignored by default because it reads
//! local outputs and a large object_info snapshot; no GPU submission.
use serde_json::Value;

#[test]
#[ignore = "reads local ComfyUI output and object_info; no submission"]
fn png_ui_workflow_converts_to_executable_api() {
    let image = "/tmp/saya-image-ui-current.png";
    let info_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-results/gpu-real/object_info.json");
    let image = std::path::Path::new(image);
    let inspected = studio_core::image_restore::inspect(image).unwrap();
    assert_eq!(inspected["hasWorkflow"], Value::Bool(true));
    let workflow = inspected["workflow"].as_object().expect("workflow UI");
    let source = Value::Object(workflow.clone()).to_string();
    let info_text = std::fs::read_to_string(info_path).expect("object_info snapshot");
    let info: Value = serde_json::from_str(&info_text).unwrap();
    let prepared = studio_core::workflow_ui_api::prepare_api(&source, Some(&info)).unwrap();
    eprintln!("report={}", prepared["report"]);
    assert_eq!(prepared["report"]["complete"], Value::Bool(true));
    assert!(prepared["api"].as_object().is_some_and(|m| !m.is_empty()));
}
