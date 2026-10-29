//! Real ComfyUI F02/F03 proof (ignored by default). Creates an isolated DB and
//! submits a minimal IMAGE -> LATENT -> IMAGE pipeline. No user library is used.
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use studio_core::{comfy::Comfy, resources::ComfyMemoryManager, Database};

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

fn graph1(ckpt: &str) -> Value {
    json!({
     "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":ckpt}},
     "2":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"a simple red apple, studio light"}},
     "3":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"blurry, text"}},
     "4":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},
     "5":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["4",0],"seed":12345,"steps":1,"cfg":1.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0}},
     "6":{"class_type":"VAEDecode","inputs":{"samples":["5",0],"vae":["1",2]}},
     "7":{"class_type":"SaveImage","inputs":{"images":["6",0],"filename_prefix":"saya-real-p1"}}
    })
}
fn graph2(ckpt: &str) -> Value {
    json!({
     "1":{"class_type":"LoadImage","inputs":{"image":"saya-real-p1-awaiting.png"}},
     "2":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":ckpt}},
     "3":{"class_type":"CLIPTextEncode","inputs":{"clip":["2",1],"text":"a simple red apple, studio light"}},
     "4":{"class_type":"CLIPTextEncode","inputs":{"clip":["2",1],"text":"blurry, text"}},
     "5":{"class_type":"VAEEncode","inputs":{"pixels":["1",0],"vae":["2",2]}},
     "6":{"class_type":"KSampler","inputs":{"model":["2",0],"positive":["3",0],"negative":["4",0],"latent_image":["5",0],"seed":12346,"steps":1,"cfg":1.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0}},
     "7":{"class_type":"SaveLatent","inputs":{"samples":["6",0],"filename_prefix":"saya-real-p2"}}
    })
}
fn graph3(ckpt: &str) -> Value {
    json!({
     "1":{"class_type":"LoadLatent","inputs":{"latent":"saya-real-p2-awaiting.latent"}},
     "2":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":ckpt}},
     "3":{"class_type":"CLIPTextEncode","inputs":{"clip":["2",1],"text":"a simple red apple, studio light"}},
     "4":{"class_type":"CLIPTextEncode","inputs":{"clip":["2",1],"text":"blurry, text"}},
     "5":{"class_type":"KSampler","inputs":{"model":["2",0],"positive":["3",0],"negative":["4",0],"latent_image":["1",0],"seed":12347,"steps":1,"cfg":1.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0}},
     "6":{"class_type":"VAEDecode","inputs":{"samples":["5",0],"vae":["2",2]}},
     "7":{"class_type":"SaveImage","inputs":{"images":["6",0],"filename_prefix":"saya-real-p3"}}
    })
}
fn manifest(p1: &str, p2: &str, p3: &str) -> String {
    json!({
 "format":"saya-workflow-pipeline","version":2,"id":"real-f02","name":"Real IMAGE LATENT IMAGE",
 "workflows":[{"id":p1},{"id":p2},{"id":p3}],
 "steps":[
  {"id":"p1","workflowId":p1,"kind":"image","outputs":[{"name":"image","portType":"IMAGE"}]},
  {"id":"p2","workflowId":p2,"kind":"latent","inputs":[{"name":"image","portType":"IMAGE"}],"outputs":[{"name":"latent","portType":"LATENT"}],
   "bindings":[{"port":"image","nodeId":"1","input":"image","source":{"step":"p1","port":"image"}}]},
  {"id":"p3","workflowId":p3,"kind":"image","inputs":[{"name":"latent","portType":"LATENT"}],"outputs":[{"name":"image","portType":"IMAGE"}],
   "bindings":[{"port":"latent","nodeId":"1","input":"latent","source":{"step":"p2","port":"latent"}}]}
 ]}).to_string()
}

fn vram(port: u16) -> Option<(u64, u64)> {
    let stats = Comfy::loopback(port).ok()?.get("/system_stats").ok()?;
    let d = stats["devices"].as_array()?.first()?;
    let total = d["vram_total"].as_u64()?;
    let free = d["vram_free"].as_u64()?;
    let torch = d["torch_vram_total"].as_u64().unwrap_or(0);
    Some((total.saturating_sub(free), torch))
}

fn sample_while(port: u16, stop: Arc<AtomicBool>) -> (u64, u64) {
    let mut max_used = 0u64;
    let mut max_torch = 0u64;
    while !stop.load(Ordering::Acquire) {
        if let Some((used, torch)) = vram(port) {
            max_used = max_used.max(used);
            max_torch = max_torch.max(torch);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    (max_used, max_torch)
}

#[test]
#[ignore = "real GPU ComfyUI pipeline; requires local instance"]
fn real_image_latent_image_pipeline_with_memory() {
    let ckpt = std::env::var("SAYA_REAL_CKPT")
        .unwrap_or_else(|_| "Illustrious/no tags/ilustmix_v9.safetensors".into());
    let port: u16 = std::env::var("SAYA_REAL_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8188);
    let comfy = Comfy::loopback(port).unwrap();
    let info = comfy.get("/object_info").unwrap();
    let dir = std::env::temp_dir().join(format!("saya-real-f02-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut db = Database::open(&dir.join("db.sqlite3")).unwrap();
    let w1 = db
        .import_workflow("p1", &graph1(&ckpt).to_string())
        .unwrap();
    let w2 = db
        .import_workflow("p2", &graph2(&ckpt).to_string())
        .unwrap();
    let w3 = db
        .import_workflow("p3", &graph3(&ckpt).to_string())
        .unwrap();
    let pipeline = db.save_pipeline(&manifest(&w1, &w2, &w3)).unwrap();
    db.preflight_pipeline(
        &studio_core::workflow_pipeline::WorkflowPipeline::parse(&manifest(&w1, &w2, &w3)).unwrap(),
        Some(&info),
    )
    .unwrap();
    let run = db.start_pipeline_run(&pipeline).unwrap();
    let results = dir.join("results");
    let manager = ComfyMemoryManager {
        client: Comfy::loopback(port).unwrap(),
        port,
        drm: None,
        settle: Duration::from_secs(3),
        threshold: 64 * 1024 * 1024,
    };
    let mut report = Vec::new();
    let mut samples = Vec::new();
    for pass in 0..3 {
        if pass == 2 {
            // Restart boundary: reopen the database and resume pass 3 only.
            drop(db);
            db = Database::open(&dir.join("db.sqlite3")).unwrap();
            let jobs = db.jobs().unwrap().len();
            assert_eq!(jobs, 2, "restart must not resubmit completed passes");
        }
        let before = vram(port);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let sampler = std::thread::spawn(move || sample_while(port, flag));
        let result = db
            .run_next_pass_with_memory(
                &run,
                &comfy,
                &results,
                true,
                Duration::from_secs(120),
                Some(&manager),
                true,
            )
            .unwrap();
        stop.store(true, Ordering::Release);
        let (peak_used, peak_torch) = sampler.join().unwrap();
        let after = vram(port);
        samples.push(json!({"pass":pass+1,"before":before,"peakUsed":peak_used,"peakTorch":peak_torch,"after":after,"memory":result.get("memory").cloned()}));
        report.push(result);
    }
    let state = db.pipeline_run(&run).unwrap();
    assert_eq!(state["state"], "completed", "{state}");
    std::fs::write(report_dir().join("f02-real.json"), json!({"run":state,"steps":report,"vramSamples":samples}).to_string()).unwrap();
    assert!(report
        .iter()
        .any(|r| r["memory"].to_string().contains("released")));
    std::fs::remove_dir_all(dir).unwrap();
}
