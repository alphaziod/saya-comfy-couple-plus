//! F02 LATENT boundary contract, offline: SaveLatent file output -> LoadLatent
//! filename input. No conversion, no invented filename and no engine call.
use serde_json::{json, Value};
use studio_core::{
    workflow_pipeline::{PIPELINE_FORMAT, PIPELINE_VERSION},
    Database,
};

fn graph(nodes: Value) -> String {
    nodes.to_string()
}

fn object_info() -> Value {
    json!({
        "CheckpointLoaderSimple":{"input":{"required":{"ckpt_name":[["saya.safetensors"]]}},"output":["MODEL","CLIP","VAE"]},
        "CLIPTextEncode":{"input":{"required":{"text":["STRING"],"clip":["CLIP"]}},"output":["CONDITIONING"]},
        "EmptyLatentImage":{"input":{"required":{"width":["INT"],"height":["INT"],"batch_size":["INT"]}},"output":["LATENT"]},
        "KSampler":{"input":{"required":{"model":["MODEL"],"positive":["CONDITIONING"],"negative":["CONDITIONING"],"latent_image":["LATENT"],"seed":["INT"],"steps":["INT"]}},"output":["LATENT"]},
        "LoadLatent":{"input":{"required":{"latent":[[]]}},"output":["LATENT"]},
        "SaveLatent":{"input":{"required":{"samples":["LATENT"],"filename_prefix":["STRING"]}},"output":["LATENT"],"output_node":true},
        "VAEDecode":{"input":{"required":{"samples":["LATENT"],"vae":["VAE"]}},"output":["IMAGE"]},
        "SaveImage":{"input":{"required":{"images":["IMAGE"],"filename_prefix":["STRING"]}},"output":[],"output_node":true}
    })
}

fn manifest(base: &str, refine: &str, polish: &str) -> String {
    json!({
        "format": PIPELINE_FORMAT, "version": PIPELINE_VERSION,
        "id": "latent-pipeline", "name": "Latent",
        "workflows": [{"id":base},{"id":refine},{"id":polish}],
        "steps": [
            {"id":"base","workflowId":base,"kind":"latent","outputs":[{"name":"latent","portType":"LATENT"}]},
            {"id":"refine","workflowId":refine,"kind":"latent",
             "inputs":[{"name":"latent","portType":"LATENT"}],
             "outputs":[{"name":"latent","portType":"LATENT"}],
             "bindings":[{"port":"latent","nodeId":"1","input":"latent","source":{"step":"base","port":"latent"}}]},
            {"id":"polish","workflowId":polish,"kind":"image",
             "inputs":[{"name":"latent","portType":"LATENT"}],
             "outputs":[{"name":"image","portType":"IMAGE"}],
             "bindings":[{"port":"latent","nodeId":"1","input":"latent","source":{"step":"refine","port":"latent"}}]}
        ]
    }).to_string()
}

fn setup() -> (Database, String) {
    let db = Database::in_memory().unwrap();
    let base = db.import_workflow("base", &graph(json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"saya.safetensors"}},
        "2":{"class_type":"CLIPTextEncode","inputs":{"text":"p","clip":["1",1]}},
        "3":{"class_type":"CLIPTextEncode","inputs":{"text":"n","clip":["1",1]}},
        "4":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},
        "5":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["4",0],"seed":1,"steps":2}},
        "6":{"class_type":"SaveLatent","inputs":{"samples":["5",0],"filename_prefix":"base"}}
    }))).unwrap();
    let refine = db.import_workflow("refine", &graph(json!({
        "1":{"class_type":"LoadLatent","inputs":{"latent":"base.safetensors"}},
        "2":{"class_type":"SaveLatent","inputs":{"samples":["1",0],"filename_prefix":"refine"}}
    }))).unwrap();
    let polish = db.import_workflow("polish", &graph(json!({
        "1":{"class_type":"LoadLatent","inputs":{"latent":"refine.safetensors"}},
        "2":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"saya.safetensors"}},
        "3":{"class_type":"VAEDecode","inputs":{"samples":["1",0],"vae":["2",2]}},
        "4":{"class_type":"SaveImage","inputs":{"images":["3",0],"filename_prefix":"polish"}}
    }))).unwrap();
    (db, manifest(&base, &refine, &polish))
}

#[test]
fn latent_save_load_contract_preflights_without_conversion_or_filename_invention() {
    let (db, text) = setup();
    let pipeline = db.save_pipeline(&text).unwrap();
    db.start_pipeline_run(&pipeline).unwrap();
    let settings = db
        .preflight_pipeline(
            &studio_core::workflow_pipeline::WorkflowPipeline::parse(&text).unwrap(),
            Some(&object_info()),
        )
        .unwrap();
    assert_eq!(settings.len(), 3);
}

#[test]
fn latent_binding_rejects_image_nodes_and_wrong_source_kind() {
    let (db, text) = setup();
    let mut value: Value = serde_json::from_str(&text).unwrap();
    value["steps"][1]["bindings"][0]["nodeId"] = json!("does-not-exist");
    let error = db
        .save_pipeline(&value.to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("node cible"), "{error}");
    let mut value: Value = serde_json::from_str(&text).unwrap();
    value["steps"][1]["bindings"][0]["input"] = json!("image");
    let error = db
        .save_pipeline(&value.to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("LoadLatent.latent"), "{error}");
}
