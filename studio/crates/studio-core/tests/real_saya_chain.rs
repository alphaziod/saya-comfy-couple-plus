//! Bounded real Saya phase chain, isolated output root and Saya-owned state copies.
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use studio_core::{comfy::Comfy, generation_store, Database};

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

fn short(api: &mut Value) {
    for (_, node) in api.as_object_mut().unwrap() {
        if let Some(i) = node["inputs"].as_object_mut() {
            if i.get("dimensions").is_some() {
                i["dimensions"] = json!("CivitAI Portrait Safe - 832x1216");
            }
            if let Some(name) = i.get("unet_name").and_then(Value::as_str) {
                if name.contains("Q5_1") {
                    i["unet_name"] = json!(name.replace("Q5_1", "Q5_K_M"));
                }
            }
        }
    }
}

fn state(db: &Database, job: &str) -> String {
    db.jobs()
        .unwrap()
        .into_iter()
        .find(|j| j["id"] == job)
        .and_then(|j| j["state"].as_str().map(str::to_owned))
        .unwrap_or_default()
}

#[test]
#[ignore = "real GPU; isolated Saya phase chain"]
fn saya_phase_chain_isolated_and_copied_to_store() {
    if skip_without_workflow() {
        return;
    }
    let ui = std::fs::read_to_string(real_workflow()).unwrap();
    let comfy = Comfy::loopback(8188).unwrap();
    let info = comfy.get("/object_info").unwrap();
    let root = format!("saya-tests/chain-{}", uuid::Uuid::new_v4());
    let dir = std::env::temp_dir().join(format!("saya-chain-db-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store_root = dir.join("generations");
    let (store, _) = generation_store::create(&store_root, "Saya", "chain").unwrap();
    let mut db = Database::open(&dir.join("db.sqlite3")).unwrap();
    let mut report = Vec::new();
    let mut validated: Option<PathBuf> = None;
    let mut vram_events = Vec::new();
    for phase in 1..=3 {
        if phase == 3 {
            let before = comfy.get("/system_stats").ok();
            let free = before
                .as_ref()
                .and_then(|s| s["devices"].as_array())
                .and_then(|d| d.first())
                .and_then(|d| d["vram_free"].as_u64())
                .unwrap_or(0);
            let forced = comfy.force_unload_models(12_000).unwrap();
            vram_events.push(json!({"phase":phase,"reason":"phase3 forced Saya unload before active pass","freeBeforeMib":free/1_048_576,"forced":forced}));
        }
        let prepared =
            studio_core::saya::prepare_phase_api(&ui, &info, phase, Some("424242"), Some(&root))
                .unwrap();
        let mut api = prepared["api"].clone();
        short(&mut api);
        let wf = db
            .import_workflow(&format!("Saya phase {phase}"), &api.to_string())
            .unwrap();
        let job = db.prepare_job(&wf, &[], &info).unwrap();
        db.submit_job(&job.job_id, &job.snapshot_hash, true, true, &comfy)
            .unwrap();
        let started = Instant::now();
        loop {
            db.sync_jobs(&comfy, &dir.join("results")).unwrap();
            let s = state(&db, &job.job_id);
            if s == "succeeded" {
                break;
            }
            if s == "failed" {
                panic!("phase {phase} failed");
            }
            assert!(
                started.elapsed() < Duration::from_secs(900),
                "phase {phase} timeout"
            );
            std::thread::sleep(Duration::from_millis(300));
        }
        let output_root = comfy_dir().join("output").join(&root);
        let candidates: Vec<PathBuf> = std::fs::read_dir(&output_root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("png"))
            .collect();
        let image = candidates
            .iter()
            .find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.contains(&format!("phase_{phase}")))
            })
            .or_else(|| candidates.first())
            .ok_or("phase image not found")
            .unwrap();
        let bytes = std::fs::read(image).unwrap();
        generation_store::record_pass(&store, &format!("phase-{phase}"), "image", &bytes,
            json!({"phase":phase,"job":job.job_id,"seed":"424242","source":image.to_string_lossy()})).unwrap();
        if phase == 1 {
            let review = db
                .jobs()
                .unwrap()
                .into_iter()
                .find(|j| j["id"] == job.job_id)
                .and_then(|j| {
                    j["outputs"]
                        .as_array()
                        .and_then(|o| o.iter().find(|x| x["sayaReview"].is_array()).cloned())
                })
                .ok_or("review missing")
                .unwrap();
            db.saya_review_decision(&job.job_id, "validate", &comfy, true)
                .unwrap();
            validated = Some(
                PathBuf::from(review["sayaReview"][0]["candidate_path"].as_str().unwrap())
                    .with_file_name("phase_1_base.png"),
            );
        }
        report.push(json!({"phase":phase,"job":job.job_id,"image":image.to_string_lossy(),"storePass":format!("phase-{phase}")}));
    }
    assert!(validated.is_some_and(|p| p.is_file()));
    assert_eq!(generation_store::status(&store).unwrap()["passes"], 3);
    std::fs::write(report_dir().join("saya-chain.json"),
        json!({"root":root,"masterSeed":"424242","phases":report,"vramEvents":vram_events,"store":store.to_string_lossy()}).to_string()).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
