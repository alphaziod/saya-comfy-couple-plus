//! Real bounded F13 proof: panel preparation with a local LoRA, then one ComfyUI
//! generation. Ignored by default; uses only a temporary SQLite and no user files.
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use studio_core::{comfy::Comfy, Database};

fn comfy_dir() -> std::path::PathBuf {
    std::env::var_os("SAYA_COMFYUI_DIR").map(Into::into).unwrap_or_else(|| {
        std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("AI/ComfyUI")
    })
}
#[allow(dead_code)]
fn real_workflow() -> std::path::PathBuf {
    std::env::var_os("SAYA_REAL_UI_WORKFLOW")
        .map(Into::into)
        .unwrap_or_else(|| comfy_dir().join("user/default/workflows/ILUSTCOUPLECLEAN.json"))
}
#[allow(dead_code)]
fn report_dir() -> std::path::PathBuf {
    std::env::var_os("SAYA_REAL_REPORT_DIR").map(Into::into).unwrap_or_else(std::env::temp_dir)
}
#[allow(dead_code)]
fn skip_without_workflow() -> bool {
    let missing = !real_workflow().is_file();
    if missing {
        eprintln!("skip: workflow introuvable ({}) ; régler SAYA_COMFYUI_DIR ou SAYA_REAL_UI_WORKFLOW", real_workflow().display());
    }
    missing
}

#[test]
#[ignore = "real GPU ComfyUI generation; requires local instance"]
fn prepared_panel_with_lora_generates_real_image() {
    let ckpt = std::env::var("SAYA_REAL_CKPT")
        .unwrap_or_else(|_| "Illustrious/no tags/ilustmix_v9.safetensors".into());
    let port: u16 = std::env::var("SAYA_REAL_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8188);
    let comfy = Comfy::loopback(port).unwrap();
    let info = comfy.get("/object_info").unwrap();
    let lora = info["LoraLoader"]["input"]["required"]["lora_name"][0]
        .as_array()
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .unwrap()
        .to_owned();
    let dir = std::env::temp_dir().join(format!("saya-real-f13-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut db = Database::open(&dir.join("db.sqlite3")).unwrap();
    let graph = json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":ckpt}},
        "2":{"class_type":"LoraLoader","inputs":{"lora_name":"old.safetensors","strength_model":0.1,"strength_clip":0.1,"model":["1",0],"clip":["1",1]}},
        "3":{"class_type":"CLIPTextEncode","inputs":{"text":"old P1","clip":["2",1]}},
        "4":{"class_type":"CLIPTextEncode","inputs":{"text":"old anatomy","clip":["2",1]}},
        "5":{"class_type":"CLIPTextEncode","inputs":{"text":"old BG","clip":["2",1]}},
        "6":{"class_type":"CLIPTextEncode","inputs":{"text":"old action","clip":["2",1]}},
        "7":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},
        "8":{"class_type":"KSampler","inputs":{"seed":42,"steps":1,"cfg":1.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0,"model":["2",0],"positive":["3",0],"negative":["5",0],"latent_image":["7",0]}},
        "9":{"class_type":"VAEDecode","inputs":{"samples":["8",0],"vae":["1",2]}},
        "10":{"class_type":"SaveImage","inputs":{"images":["9",0],"filename_prefix":"saya-real-f13"}}
    });
    let workflow = db.import_workflow("F13 real", &graph.to_string()).unwrap();
    let p1 = db
        .save_character(
            None,
            "P1 real".into(),
            json!({"identity_prompt":"red apple on a table","anatomy_prompt":"object anatomy"}),
        )
        .unwrap();
    for (key, node, input) in [
        ("P1", "3", "text"),
        ("ANATOMY_P1", "4", "text"),
        ("BG", "5", "text"),
        ("ACTION", "6", "text"),
        ("QUICK_STEPS", "8", "steps"),
    ] {
        db.save_semantic_binding(&workflow, key, node, input)
            .unwrap();
    }
    let config = json!({
        "p1":{"characterId":p1},"p2":{"characterId":""},
        "bg":{"source":"fixed","text":"studio background","selected":"studio background"},
        "action":{"source":"fixed","text":"simple composition"},
        "mappings":{"P1":{"nodeId":"3","inputName":"text"},"ANATOMY_P1":{"nodeId":"4","inputName":"text"},"BG":{"nodeId":"5","inputName":"text"},"ACTION":{"nodeId":"6","inputName":"text"}},
        "quick":[{"nodeId":"8","inputName":"steps","value":"1","visible":true}],
        "lora":{"selections":[{"name":lora,"relativePath":lora,"loaderNode":"2","modelWeight":0.4,"clipWeight":0.3,"enabled":true}]}
    });
    let profile = db
        .save_prepare_profile(None, "F13 real", &workflow, "solo", &config)
        .unwrap();
    let result = db
        .prepare_universal(profile["id"].as_str().unwrap(), &info)
        .unwrap();
    let prepared = &result["prepared"];
    let snapshot: Value = serde_json::from_str(prepared["requestJson"].as_str().unwrap()).unwrap();
    assert_eq!(
        snapshot["prompt"]["3"]["inputs"]["text"],
        "red apple on a table"
    );
    assert_eq!(
        snapshot["prompt"]["5"]["inputs"]["text"],
        "studio background"
    );
    assert_eq!(snapshot["prompt"]["2"]["inputs"]["lora_name"], lora);
    assert_eq!(snapshot["prompt"]["2"]["inputs"]["strength_model"], 0.4);
    let job = prepared["jobId"].as_str().unwrap();
    db.submit_job(
        job,
        prepared["snapshotHash"].as_str().unwrap(),
        true,
        true,
        &comfy,
    )
    .unwrap();
    let start = Instant::now();
    loop {
        db.sync_jobs(&comfy, &dir.join("results")).unwrap();
        let state = db
            .jobs()
            .unwrap()
            .into_iter()
            .find(|j| j["id"] == job)
            .and_then(|j| j["state"].as_str().map(str::to_owned))
            .unwrap_or_default();
        if state == "succeeded" {
            break;
        }
        if state == "failed" {
            panic!("real generation failed");
        }
        assert!(start.elapsed() < Duration::from_secs(240), "timeout");
        std::thread::sleep(Duration::from_millis(300));
    }
    let outputs = db
        .jobs()
        .unwrap()
        .into_iter()
        .find(|j| j["id"] == job)
        .map(|j| j["outputs"].clone())
        .unwrap_or(Value::Null);
    assert!(!outputs.as_array().is_none_or(Vec::is_empty));
    std::fs::write(report_dir().join("f13-real.json"),
        json!({"job":job,"state":"succeeded","outputs":outputs,"lora":lora,"resolution":result["resolution"],"loraEdits":result["loraEdits"]}).to_string()).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
