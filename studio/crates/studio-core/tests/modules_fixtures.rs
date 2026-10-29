//! F10: Detailer, Upscale and Saya Couple modules from isolated API fixtures.
use serde_json::{json, Value};
use studio_core::modules::{extract_module, ModuleBinding, ModuleConnection};
use studio_core::Database;

fn info() -> Value {
    json!({
        "LoadImage":{"input":{"required":{"image":[["source.png"]]}},"output":["IMAGE"]},
        "CheckpointLoaderSimple":{"input":{"required":{"ckpt_name":[["saya.safetensors"]]}},"output":["MODEL","CLIP","VAE"]},
        "CLIPTextEncode":{"input":{"required":{"text":["STRING"],"clip":["CLIP"]}},"output":["CONDITIONING"]},
        "FaceDetailer":{"input":{"required":{"image":["IMAGE"],"model":["MODEL"],"positive":["CONDITIONING"],"negative":["CONDITIONING"],"seed":["INT"],"steps":["INT"]}},"output":["IMAGE"]},
        "UpscaleModelLoader":{"input":{"required":{"model_name":[["4x.safetensors"]]}},"output":["UPSCALE_MODEL"]},
        "ImageUpscaleWithModel":{"input":{"required":{"upscale_model":["UPSCALE_MODEL"],"image":["IMAGE"]}},"output":["IMAGE"]},
        "SayaMultiCouple":{"input":{"required":{"pos_1":["CONDITIONING"],"pos_2":["CONDITIONING"],"neg_1":["CONDITIONING"]}},"output":["CONDITIONING"]},
        "KSampler":{"input":{"required":{"model":["MODEL"],"positive":["CONDITIONING"],"negative":["CONDITIONING"],"latent_image":["LATENT"],"seed":["INT"],"steps":["INT"]}},"output":["LATENT"]},
        "EmptyLatentImage":{"input":{"required":{"width":["INT"],"height":["INT"],"batch_size":["INT"]}},"output":["LATENT"]},
        "SaveImage":{"input":{"required":{"images":["IMAGE"],"filename_prefix":["STRING"]}},"output":[],"output_node":true}
    })
}

fn graph(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

fn ports(draft: &studio_core::modules::ModuleDraft) -> Vec<(String, String)> {
    draft
        .inputs
        .iter()
        .map(|p| (p.name.clone(), p.kind.clone()))
        .collect()
}

#[test]
fn detailer_upscale_and_couple_extract_declared_ports_and_dependencies() {
    let info = info();
    let detailer = extract_module(
        &graph(include_str!(
            "../../../scripts/fixtures/f10-detailer-api.json"
        )),
        &info,
        &["5"],
    )
    .unwrap();
    assert_eq!(
        ports(&detailer),
        vec![
            ("5.image".into(), "IMAGE".into()),
            ("5.model".into(), "MODEL".into()),
            ("5.negative".into(), "CONDITIONING".into()),
            ("5.positive".into(), "CONDITIONING".into()),
        ]
    );
    assert_eq!(detailer.outputs[0].name, "5.0");
    assert_eq!(detailer.outputs[0].kind, "IMAGE");
    assert_eq!(detailer.dependencies.classes, ["FaceDetailer"]);
    assert_eq!(
        detailer
            .parameters
            .iter()
            .find(|p| p.input == "seed")
            .unwrap()
            .value,
        "123456789012345"
    );

    let upscale = extract_module(
        &graph(include_str!(
            "../../../scripts/fixtures/f10-upscale-api.json"
        )),
        &info,
        &["3"],
    )
    .unwrap();
    assert_eq!(
        ports(&upscale),
        vec![
            ("3.image".into(), "IMAGE".into()),
            ("3.upscale_model".into(), "UPSCALE_MODEL".into())
        ]
    );
    assert_eq!(upscale.dependencies.classes, ["ImageUpscaleWithModel"]);

    let couple = extract_module(
        &graph(include_str!(
            "../../../scripts/fixtures/f10-couple-api.json"
        )),
        &info,
        &["2", "3", "4"],
    )
    .unwrap();
    assert_eq!(
        ports(&couple),
        vec![
            ("2.clip".into(), "CLIP".into()),
            ("3.clip".into(), "CLIP".into()),
            ("4.neg_1".into(), "CONDITIONING".into())
        ]
    );
    assert_eq!(couple.outputs[0].kind, "CONDITIONING");
    assert!(couple
        .parameters
        .iter()
        .any(|p| p.value == "P1 exact,  rose\nsecond"));
    assert!(couple
        .parameters
        .iter()
        .any(|p| p.value == "P2 exact,  night"));
}

#[test]
fn versioning_and_assembly_preserve_previous_versions_and_original_graph() {
    let db = Database::in_memory().unwrap();
    let info = info();
    let source = graph(include_str!(
        "../../../scripts/fixtures/f10-upscale-api.json"
    ));
    let draft = extract_module(&source, &info, &["3"]).unwrap();
    let (id, v1) = db
        .save_workflow_module(None, "Upscale 4x", "", None, "sha-up", &draft)
        .unwrap();
    let mut changed = source.clone();
    changed["8"] =
        json!({"class_type":"UpscaleModelLoader","inputs":{"model_name":"4x-other.safetensors"}});
    changed["3"]["inputs"]["upscale_model"] = json!(["8", 0]);
    let draft2 = extract_module(&changed, &info, &["3"]).unwrap();
    let (_, v2) = db
        .save_workflow_module(Some(&id), "Upscale 4x", "", None, "sha-up2", &draft2)
        .unwrap();
    assert_eq!(v2, 2);
    let old = db.module_detail(&id, Some(1), None).unwrap();
    assert_eq!(old["document"], draft.document);
    assert_eq!(db.module_detail(&id, None, None).unwrap()["version"], 2);

    let target = json!({
        "1":{"class_type":"LoadImage","inputs":{"image":"source.png"}},
        "2":{"class_type":"UpscaleModelLoader","inputs":{"model_name":"4x.safetensors"}},
        "4":{"class_type":"SaveImage","inputs":{"images":"PENDING","filename_prefix":"out"}}
    });
    let connection = ModuleConnection {
        inputs: vec![
            ModuleBinding {
                port: "3.image".into(),
                node: "1".into(),
                input: None,
                output_index: Some(0),
            },
            ModuleBinding {
                port: "3.upscale_model".into(),
                node: "2".into(),
                input: None,
                output_index: Some(0),
            },
        ],
        outputs: vec![ModuleBinding {
            port: "3.0".into(),
            node: "4".into(),
            input: Some("images".into()),
            output_index: None,
        }],
        overrides: json!([]),
    };
    let connected = db
        .connect_module(&id, v1, "up", &target, &connection, &info)
        .unwrap();
    assert_eq!(
        connected["graph"]["up::3"]["inputs"]["image"],
        json!(["1", 0])
    );
    assert_eq!(
        connected["graph"]["4"]["inputs"]["images"],
        json!(["up::3", 0])
    );
    assert_eq!(target["4"]["inputs"]["images"], "PENDING");
}
