//! Bounded real Saya phase 1 proof. Test copies only; original read-only.
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
#[ignore = "real GPU; Saya phase 1 bounded"]
fn saya_phase1_collects_review_and_validates() {
    if skip_without_workflow() {
        return;
    }
    let ui = std::fs::read_to_string(real_workflow()).unwrap();
    let comfy = Comfy::loopback(8188).unwrap();
    let info = comfy.get("/object_info").unwrap();
    let phase = studio_core::saya::prepare_phase_api(
        &ui,
        &info,
        1,
        Some("123456789"),
        Some("saya-tests/phase1-isolated"),
    )
    .unwrap();
    let roots: Vec<&str> = phase["api"]
        .as_object()
        .unwrap()
        .values()
        .filter_map(|n| n["inputs"]["checkpoint_root"].as_str())
        .collect();
    assert!(
        roots.iter().all(|r| *r == "saya-tests/phase1-isolated"),
        "checkpoint roots not isolated: {roots:?}"
    );
    let mut api = phase["api"].clone();
    for (_, node) in api.as_object_mut().unwrap() {
        if let Some(inputs) = node["inputs"].as_object_mut() {
            if inputs.get("steps").is_some_and(Value::is_number) {
                inputs["steps"] = json!(1);
            }
            if inputs.get("steps_to_run").is_some_and(Value::is_number) {
                inputs["steps_to_run"] = json!(1);
            }
        }
    }
    let dir = std::env::temp_dir().join(format!("saya-phase1-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut db = Database::open(&dir.join("db.sqlite3")).unwrap();
    let wf = db
        .import_workflow("Saya phase1 copy", &api.to_string())
        .unwrap();
    let prepared = db.prepare_job(&wf, &[], &info).unwrap();
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
            .unwrap_or_default();
        if state == "succeeded" {
            break;
        }
        if state == "failed" {
            panic!("phase1 failed")
        }
        assert!(start.elapsed() < Duration::from_secs(240), "timeout");
        std::thread::sleep(Duration::from_millis(300));
    }
    let job = db
        .jobs()
        .unwrap()
        .into_iter()
        .find(|j| j["id"] == prepared.job_id)
        .unwrap();
    let outputs = job["outputs"].as_array().unwrap();
    let review = outputs
        .iter()
        .find(|o| o.get("sayaReview").is_some())
        .expect("saya review missing");
    let image = review["assetId"].as_str().unwrap().to_owned();
    let decision = db
        .saya_review_decision(&prepared.job_id, "validate", &comfy, true)
        .unwrap();
    std::fs::write(report_dir().join("saya-phase1.json"),json!({"job":prepared.job_id,"outputs":outputs,"review":review,"decision":decision,"image":image,"seedTargets":phase["seedTargets"]}).to_string()).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}
