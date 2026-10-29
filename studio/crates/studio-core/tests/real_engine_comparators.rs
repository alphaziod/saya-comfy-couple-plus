//! Real two-sampler proof for sampler-step discovery and intermediate images.
use serde_json::json;
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
#[ignore = "real GPU ComfyUI generation"]
fn two_sampler_outputs_are_discovered_and_ordered() {
    let ckpt = std::env::var("SAYA_REAL_CKPT")
        .unwrap_or_else(|_| "Illustrious/no tags/ilustmix_v9.safetensors".into());
    let comfy = Comfy::loopback(8188).unwrap();
    let info = comfy.get("/object_info").unwrap();
    let dir = std::env::temp_dir().join(format!("saya-real-cmp-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut db = Database::open(&dir.join("db.sqlite3")).unwrap();
    let graph = json!({
      "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":ckpt}},
      "2":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"red apple"}},
      "3":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"blurry"}},
      "4":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},
      "5":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["4",0],"seed":1,"steps":1,"cfg":1.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0}},
      "6":{"class_type":"VAEDecode","inputs":{"samples":["5",0],"vae":["1",2]}},
      "7":{"class_type":"PreviewImage","inputs":{"images":["6",0]}},
      "8":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["5",0],"seed":2,"steps":1,"cfg":1.0,"sampler_name":"euler","scheduler":"normal","denoise":0.5}},
      "9":{"class_type":"VAEDecode","inputs":{"samples":["8",0],"vae":["1",2]}},
      "10":{"class_type":"PreviewImage","inputs":{"images":["9",0]}},
      "11":{"class_type":"SaveImage","inputs":{"images":["9",0],"filename_prefix":"cmp"}}
    });
    let workflow = db.import_workflow("compare", &graph.to_string()).unwrap();
    let prepared = db.prepare_job(&workflow, &[], &info).unwrap();
    db.submit_job(
        &prepared.job_id,
        &prepared.snapshot_hash,
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
            .find(|j| j["id"] == prepared.job_id)
            .and_then(|j| j["state"].as_str().map(str::to_owned))
            .unwrap();
        if state == "succeeded" {
            break;
        }
        if state == "failed" {
            panic!("generation failed")
        }
        assert!(start.elapsed() < Duration::from_secs(120), "timeout");
        std::thread::sleep(Duration::from_millis(300));
    }
    let report = db.sampler_steps(&prepared.job_id).unwrap();
    assert_eq!(report["steps"].as_array().unwrap().len(), 2, "{report}");
    assert_eq!(report["steps"][0]["mode"], "single");
    assert_eq!(report["steps"][1]["mode"], "compare");
    assert!(!report["steps"][0]["images"].as_array().unwrap().is_empty());
    assert!(!report["steps"][1]["images"].as_array().unwrap().is_empty());
    std::fs::write(report_dir().join("comparators-real.json"),report.to_string()).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
