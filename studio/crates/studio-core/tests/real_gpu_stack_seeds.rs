//! Bounded real-GPU proof for LoRA stack, seed sentinels and Saya Continue/Restart.
//! Ignored by default. Isolated temp DB/results; the source workflow and the
//! ComfyUI instance are read-only. One short generation per scenario.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use studio_core::{comfy::Comfy, gallery::digest, Database};

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

fn user_checkpoints() -> PathBuf {
    comfy_dir().join("output/image/checkpoints")
}
const CKPT: &str = "Illustrious/no tags/ilustmix_v9.safetensors";
const LORA_A: &str = "Anima/style/nyalia.safetensors";
const LORA_B: &str = "Pokemon/tmjulillu.safetensors";

fn graph() -> Value {
    json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":CKPT}},
        "2":{"class_type":"LoraLoader","inputs":{"lora_name":"old-a.safetensors","strength_model":0.1,"strength_clip":0.1,"model":["1",0],"clip":["1",1]}},
        "3":{"class_type":"LoraLoader","inputs":{"lora_name":"old-b.safetensors","strength_model":0.1,"strength_clip":0.1,"model":["2",0],"clip":["2",1]}},
        "4":{"class_type":"CLIPTextEncode","inputs":{"text":"old P1","clip":["3",1]}},
        "5":{"class_type":"CLIPTextEncode","inputs":{"text":"old anatomy","clip":["3",1]}},
        "6":{"class_type":"CLIPTextEncode","inputs":{"text":"old BG","clip":["3",1]}},
        "7":{"class_type":"CLIPTextEncode","inputs":{"text":"old action","clip":["3",1]}},
        "8":{"class_type":"KSampler","inputs":{"seed":["100032",0],"steps":20,"cfg":1.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0,"model":["3",0],"positive":["4",0],"negative":["6",0],"latent_image":["9",0]}},
        "9":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},
        "10":{"class_type":"VAEDecode","inputs":{"samples":["8",0],"vae":["1",2]}},
        "11":{"class_type":"SaveImage","inputs":{"images":["10",0],"filename_prefix":"saya-gpu-stack-seeds"}},
        "100032":{"class_type":"Seed (rgthree)","inputs":{"seed":0}}
    })
}

fn state(db: &Database, job: &str) -> String {
    db.jobs()
        .unwrap()
        .into_iter()
        .find(|j| j["id"] == job)
        .and_then(|j| j["state"].as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn wait_job(db: &mut Database, comfy: &Comfy, results: &Path, job: &str, limit: u64) {
    let start = Instant::now();
    loop {
        db.sync_jobs(comfy, results).unwrap();
        let s = state(db, job);
        if s == "succeeded" {
            return;
        }
        if s == "failed" {
            panic!("job {job} failed");
        }
        assert!(
            start.elapsed() < Duration::from_secs(limit),
            "job {job} timeout after {limit}s (state={s})"
        );
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn executed_prompt(comfy: &Comfy, job: &str) -> Value {
    let start = Instant::now();
    loop {
        let history = comfy.get(&format!("/history/{job}")).unwrap();
        let prompt = history[job]["prompt"][2].clone();
        if prompt.is_object() {
            return prompt;
        }
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "history entry missing for {job}"
        );
        std::thread::sleep(Duration::from_millis(300));
    }
}

fn prompt_id(db: &Database, job: &str) -> String {
    db.jobs()
        .unwrap()
        .into_iter()
        .find(|j| j["id"] == job)
        .and_then(|j| j["promptId"].as_str().map(str::to_owned))
        .unwrap_or_else(|| panic!("promptId missing for {job}"))
}

fn seed_values(api: &Value) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for (node, value) in api.as_object().into_iter().flatten() {
        for (input, v) in value["inputs"].as_object().into_iter().flatten() {
            if (input == "seed" || input == "noise_seed") && v.is_number() {
                out.push((format!("{node}.{input}"), v.clone()));
            }
        }
    }
    out
}

fn setup(db: &Database, master_seed: &str) -> String {
    let workflow = db
        .import_workflow("GPU stack seeds", &graph().to_string())
        .unwrap();
    let p1 = db
        .save_character(
            None,
            "P1 GPU".into(),
            json!({"identity_prompt":"red apple on a table","anatomy_prompt":"object anatomy"}),
        )
        .unwrap();
    for (key, node, input) in [
        ("P1", "4", "text"),
        ("ANATOMY_P1", "5", "text"),
        ("BG", "6", "text"),
        ("ACTION", "7", "text"),
        ("QUICK_STEPS", "8", "steps"),
    ] {
        db.save_semantic_binding(&workflow, key, node, input)
            .unwrap();
    }
    let config = json!({
        "p1":{"characterId":p1},"p2":{"characterId":""},
        "bg":{"source":"fixed","text":"studio background","selected":"studio background"},
        "action":{"source":"fixed","text":"simple composition"},
        "mappings":{"P1":{"nodeId":"4","inputName":"text"},"ANATOMY_P1":{"nodeId":"5","inputName":"text"},"BG":{"nodeId":"6","inputName":"text"},"ACTION":{"nodeId":"7","inputName":"text"}},
        "quick":[{"nodeId":"8","inputName":"steps","value":"1","visible":true}],
        "masterSeed":master_seed,
        "lora":{"selections":[
            {"name":LORA_A,"relativePath":LORA_A,"loaderNode":"2","modelWeight":0.4,"clipWeight":0.3,"enabled":true},
            {"name":LORA_B,"relativePath":LORA_B,"loaderNode":"3","modelWeight":0.55,"clipWeight":0.45,"enabled":true}
        ]}
    });
    db.save_prepare_profile(None, "GPU stack seeds", &workflow, "solo", &config)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn run_scenario(master_seed: &str, expected: Value, name: &str) {
    let comfy = Comfy::loopback(8188).unwrap();
    let info = comfy.get("/object_info").unwrap();
    let dir = std::env::temp_dir().join(format!("saya-gpu-{name}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut db = Database::open(&dir.join("db.sqlite3")).unwrap();
    let profile = setup(&db, master_seed);
    let result = db.prepare_universal(&profile, &info).unwrap();
    let prepared = &result["prepared"];
    let submitted: Value = serde_json::from_str(prepared["requestJson"].as_str().unwrap()).unwrap();
    let api = &submitted["prompt"];
    assert_eq!(api["2"]["inputs"]["lora_name"], LORA_A);
    assert_eq!(api["2"]["inputs"]["strength_model"], 0.4);
    assert_eq!(api["2"]["inputs"]["strength_clip"], 0.3);
    assert_eq!(api["3"]["inputs"]["lora_name"], LORA_B);
    assert_eq!(api["3"]["inputs"]["strength_model"], 0.55);
    assert_eq!(api["3"]["inputs"]["strength_clip"], 0.45);
    assert_eq!(api["100032"]["inputs"]["seed"], expected);
    let job = prepared["jobId"].as_str().unwrap().to_owned();
    db.submit_job(
        &job,
        prepared["snapshotHash"].as_str().unwrap(),
        true,
        true,
        &comfy,
    )
    .unwrap();
    wait_job(&mut db, &comfy, &dir.join("results"), &job, 300);
    let prompt = prompt_id(&db, &job);
    let executed = executed_prompt(&comfy, &prompt);
    assert_eq!(
        executed["2"]["inputs"], api["2"]["inputs"],
        "lora A not executed"
    );
    assert_eq!(
        executed["3"]["inputs"], api["3"]["inputs"],
        "lora B not executed"
    );
    assert_eq!(
        executed["100032"]["inputs"]["seed"], expected,
        "seed not executed"
    );
    assert_eq!(executed["8"]["inputs"]["steps"], 1);
    println!(
        "RESULT {}",
        json!({"scenario":name,"job":job,"state":state(&db,&job),"seedExpected":expected,
        "promptId":prompt,"executedSeeds":seed_values(&executed),"executedLoras":{
            "2":{"name":executed["2"]["inputs"]["lora_name"],"model":executed["2"]["inputs"]["strength_model"],"clip":executed["2"]["inputs"]["strength_clip"]},
            "3":{"name":executed["3"]["inputs"]["lora_name"],"model":executed["3"]["inputs"]["strength_model"],"clip":executed["3"]["inputs"]["strength_clip"]}},
        "steps":executed["8"]["inputs"]["steps"]})
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "real GPU ComfyUI; one 1-step generation; stack + fixed seed executed prompt"]
fn lora_stack_and_fixed_seed_reach_executed_prompt() {
    if skip_without_workflow() {
        return;
    }
    run_scenario("424242", json!(424242), "stack-fixed");
}

#[test]
#[ignore = "real GPU ComfyUI; one 1-step generation; -1 sentinel executed prompt"]
fn seed_sentinel_minus_one_reaches_execution() {
    if skip_without_workflow() {
        return;
    }
    run_scenario("-1", json!(-1), "seed-minus-one");
}

fn job_error(db: &Database, job: &str) -> Value {
    let error = db
        .jobs()
        .unwrap()
        .into_iter()
        .find(|j| j["id"] == job)
        .map(|j| j["error"].clone())
        .unwrap_or(Value::Null);
    let messages = error["messages"].as_array().cloned().unwrap_or_default();
    let detail = messages
        .iter()
        .find(|m| m[0] == "execution_error")
        .map(|m| m[1].clone())
        .unwrap_or(Value::Null);
    json!({"node":detail["node_id"],"type":detail["node_type"],"exception":detail["exception_message"]})
}

fn wait_phase_outcome(
    db: &mut Database,
    comfy: &Comfy,
    results: &Path,
    run: &str,
    phase: u32,
    limit: u64,
) -> (String, Value, Value) {
    let start = Instant::now();
    loop {
        let value = saya_phase(db, comfy, results, run);
        let job = value["phases"][phase.to_string()]["job"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let state = db
            .jobs()
            .unwrap()
            .into_iter()
            .find(|j| j["id"] == job)
            .and_then(|j| j["state"].as_str().map(str::to_owned))
            .unwrap_or_default();
        if state == "succeeded" {
            return (state, value, Value::Null);
        }
        if state == "failed" {
            return (state, value, job_error(db, &job));
        }
        assert!(
            start.elapsed() < Duration::from_secs(limit),
            "saya phase {phase} timeout after {limit}s (state={state})"
        );
        std::thread::sleep(Duration::from_millis(700));
    }
}

fn saya_phase(db: &mut Database, comfy: &Comfy, results: &Path, run: &str) -> Value {
    db.saya_status(run, comfy, results).unwrap()
}

fn wait_phase(
    db: &mut Database,
    comfy: &Comfy,
    results: &Path,
    run: &str,
    phase: u32,
    limit: u64,
) -> Value {
    let start = Instant::now();
    loop {
        let value = saya_phase(db, comfy, results, run);
        let state = value["phases"][phase.to_string()]["state"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if state == "succeeded" {
            return value;
        }
        if state == "failed" {
            panic!("saya phase {phase} failed: {value}");
        }
        assert!(
            start.elapsed() < Duration::from_secs(limit),
            "saya phase {phase} timeout after {limit}s (state={state})"
        );
        std::thread::sleep(Duration::from_millis(700));
    }
}

fn chain_seeds(api: &Value, expected: u64, label: &str) -> Vec<(String, Value)> {
    let seeds = seed_values(api);
    assert!(
        !seeds.is_empty(),
        "{label}: no numeric seed reached /history"
    );
    assert!(
        seeds.iter().all(|(_, v)| v.as_u64() == Some(expected)),
        "{label} seed mismatch: {seeds:?}"
    );
    seeds
}

fn assert_isolated_phase(api: &Value, phase: u32, root: &str) {
    let mut loads = 0;
    let mut stops = 0;
    for (id, node) in api.as_object().into_iter().flatten() {
        for (input, value) in node["inputs"].as_object().into_iter().flatten() {
            if input == "checkpoint_root" {
                assert_eq!(
                    value.as_str(),
                    Some(root),
                    "phase {phase} node {id} checkpoint_root"
                );
            }
        }
        match node["class_type"].as_str().unwrap_or_default() {
            "SayaImagePhaseCheckpointLoad" => {
                assert_eq!(
                    node["inputs"]["checkpoint_root"], root,
                    "phase {phase} load {id} root"
                );
                let choice = node["inputs"]["phase"].as_str().unwrap_or_default();
                assert!(
                    choice.starts_with(&format!("{phase} ")),
                    "phase {phase} load {id} choice {choice}"
                );
                loads += 1;
            }
            "SayaImagePhaseCheckpointStop" => {
                assert_eq!(
                    node["inputs"]["checkpoint_root"], root,
                    "phase {phase} stop {id} root"
                );
                stops += 1;
            }
            _ => {}
        }
    }
    assert_eq!(
        loads,
        usize::from(phase >= 2),
        "phase {phase} must expose exactly one load node"
    );
    if phase >= 2 {
        assert!(stops >= 1, "phase {phase} stop missing");
    }
}

/// ComfyUI caches sampler nodes by input hash. A reused master seed would make
/// phase 1 return `execution_cached` instead of a real generation, so require
/// the samplers to be absent from the cached-node list of the run's history.
fn assert_fresh_execution(comfy: &Comfy, prompt: &str, label: &str, require_all: bool) {
    let history = comfy.get(&format!("/history/{prompt}")).unwrap();
    let entry = &history[prompt];
    let cached: Vec<String> = entry["status"]["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m[0] == "execution_cached")
        .filter_map(|m| m[1]["nodes"].as_array())
        .flatten()
        .filter_map(|n| n.as_str().map(str::to_owned))
        .collect();
    let samplers: Vec<String> = entry["prompt"][2]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, n)| {
            n["class_type"]
                .as_str()
                .is_some_and(|c| c.contains("KSampler"))
        })
        .map(|(id, _)| id.clone())
        .collect();
    assert!(!samplers.is_empty(), "{label}: no sampler in the graph");
    let executed: Vec<&String> = samplers.iter().filter(|id| !cached.contains(id)).collect();
    assert!(
        !executed.is_empty(),
        "{label}: every sampler was served from ComfyUI cache, not GPU (cached={cached:?})"
    );
    if require_all {
        assert_eq!(
            executed.len(),
            samplers.len(),
            "{label}: some samplers were cached: {:?}",
            samplers
                .iter()
                .filter(|id| cached.contains(id))
                .collect::<Vec<_>>()
        );
    }
}

fn executed_unets(api: &Value) -> Vec<String> {
    api.as_object()
        .into_iter()
        .flatten()
        .filter(|(_, n)| n["class_type"] == "UnetLoaderGGUF")
        .filter_map(|(_, n)| n["inputs"]["unet_name"].as_str().map(str::to_owned))
        .collect()
}

fn phase_stem(phase: u32) -> &'static str {
    match phase {
        1 => "phase_1_base",
        2 => "phase_2_hires",
        3 => "phase_3_refiner",
        4 => "phase_4_predetail",
        5 => "phase_5_detailers",
        6 => "phase_6_final",
        _ => panic!("unsupported phase {phase}"),
    }
}

fn phase_file(root: &str, phase: u32, extension: &str) -> PathBuf {
    comfy_dir().join("output")
        .join(root)
        .join(format!("{}.{extension}", phase_stem(phase)))
}

/// Final artifact proof: the custom node's validated PNG + manifest sidecar.
fn validated_manifest(root: &str, phase: u32, seed: u64) -> Value {
    let image = phase_file(root, phase, "png");
    let manifest_path = phase_file(root, phase, "json");
    assert!(
        image.is_file() && image.metadata().unwrap().len() > 0,
        "phase {phase} validated image missing: {}",
        image.display()
    );
    let manifest: Value = serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["phase"], phase, "phase {phase} manifest phase");
    assert_eq!(
        manifest["status"], "validated",
        "phase {phase} manifest status"
    );
    assert_eq!(manifest["seed"], seed, "phase {phase} manifest seed");
    manifest
}

fn vram_free(comfy: &Comfy) -> u64 {
    comfy
        .get("/system_stats")
        .ok()
        .and_then(|s| {
            s["devices"]
                .as_array()
                .and_then(|d| d.first())
                .and_then(|d| d["vram_free"].as_u64())
        })
        .unwrap_or(0)
        / 1_048_576
}

/// Content snapshot (relative path, size, mtime ns, sha256) of one tree.
fn snapshot_tree(root: &Path) -> Vec<(String, u64, u128, String)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let metadata = std::fs::metadata(&path).unwrap();
            let mtime = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            out.push((
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                metadata.len(),
                mtime,
                digest(&std::fs::read(&path).unwrap()),
            ));
        }
    }
    out.sort();
    out
}

#[test]
#[ignore = "real GPU ComfyUI; Saya Couple phase 1 -> Continue 2 -> 3 -> 4 -> 5 -> 6"]
fn saya_continue_phase_chain_keeps_master_seed() {
    if skip_without_workflow() {
        return;
    }
    let comfy = Comfy::loopback(8188).unwrap();
    let queue = comfy.get("/queue").unwrap();
    assert_eq!(
        queue["queue_running"].as_array().map(Vec::len).unwrap_or(0),
        0,
        "engine busy"
    );
    assert_eq!(
        queue["queue_pending"].as_array().map(Vec::len).unwrap_or(0),
        0,
        "engine busy"
    );
    let user_before = snapshot_tree(&user_checkpoints());
    let source_before = digest(&std::fs::read(real_workflow()).unwrap());
    let work = std::env::temp_dir().join(format!("saya-gpu-chain-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&work).unwrap();
    let results = work.join("results");
    let mut db = Database::open(&work.join("db.sqlite3")).unwrap();
    let isolated = format!("saya-tests/chain-{}", uuid::Uuid::new_v4());
    let isolated_dir = comfy_dir().join("output").join(&isolated);
    assert!(!isolated_dir.exists(), "isolated root not fresh");
    // 15.92 GiB card: the workflow's phase-3 loader is Q5_1 (13.5 GB) and OOMs;
    // the recorded feasible variant is Q5_K_M (13.0 GB). Only a temp copy differs.
    let source_text = std::fs::read_to_string(real_workflow()).unwrap();
    let variant = source_text.replace("hidream-i1-full-Q5_1.gguf", "hidream-i1-full-Q5_K_M.gguf");
    assert_ne!(variant, source_text, "phase 3 Q5_1 loader not found");
    let source_copy = work.join("ILUSTCOUPLECLEAN-Q5_K_M.json");
    std::fs::write(&source_copy, &variant).unwrap();
    // Unique master seed: ComfyUI caches sampler nodes by input hash, so a reused
    // seed would return execution_cached instead of a real GPU generation.
    let seed_value =
        (u64::from_le_bytes(uuid::Uuid::new_v4().as_bytes()[8..16].try_into().unwrap())
            & 0x0000_FFFF_FFFF_FFFF)
            .max(1);
    let seed = seed_value.to_string();
    let started = db
        .saya_start(
            source_copy.to_str().unwrap(),
            Some(seed.as_str()),
            &isolated,
            &comfy,
            &results,
        )
        .unwrap();
    let run = started["id"].as_str().unwrap().to_owned();
    let phase1 = started["phases"]["1"]["job"].as_str().unwrap().to_owned();
    let timer = Instant::now();
    let value = wait_phase(&mut db, &comfy, &results, &run, 1, 900);
    let phase1_seconds = timer.elapsed().as_secs();
    assert_eq!(value["masterSeed"], seed);
    assert!(value["phases"]["1"]["review"].is_array(), "review missing");
    assert!(
        !value["phases"]["1"]["asset"].is_null(),
        "phase 1 asset missing"
    );
    let prompt1 = prompt_id(&db, &phase1);
    let executed1 = executed_prompt(&comfy, &prompt1);
    assert_fresh_execution(&comfy, &prompt1, "phase 1", true);
    let seeds1 = chain_seeds(&executed1, seed_value, "phase 1");
    assert_isolated_phase(&executed1, 1, &isolated);
    let candidate1 = phase_file(&isolated, 1, "candidate.png");
    assert!(
        candidate1.is_file() && candidate1.metadata().unwrap().len() > 0,
        "phase 1 review candidate missing: {}",
        candidate1.display()
    );
    println!(
        "RESULT {}",
        json!({"scenario":"saya-phase1","run":run,"job":phase1,"seed":value["masterSeed"],
        "promptId":prompt1,"review":true,"executedSeeds":seeds1,"isolatedRoot":isolated,
        "checkpoint":"phase_1_base.candidate.png","candidate":candidate1,"seconds":phase1_seconds})
    );

    let timer = Instant::now();
    let continued = db.saya_continue(&run, &comfy, &results).unwrap();
    assert_eq!(continued["reviewValidated"], true);
    let phase2 = continued["phases"]["2"]["job"].as_str().unwrap().to_owned();
    assert_ne!(phase2, phase1, "continue did not submit phase 2");
    // Phase 1 validation is the Continue decision: the validated pair appears now.
    validated_manifest(&isolated, 1, seed_value);
    let (state2, value2, error2) = wait_phase_outcome(&mut db, &comfy, &results, &run, 2, 900);
    assert_eq!(state2, "succeeded", "phase 2 failed: {error2} {value2}");
    let phase2_seconds = timer.elapsed().as_secs();
    let prompt2 = prompt_id(&db, &phase2);
    let executed2 = executed_prompt(&comfy, &prompt2);
    assert_fresh_execution(&comfy, &prompt2, "phase 2", false);
    let seeds2 = chain_seeds(&executed2, seed_value, "phase 2");
    assert_isolated_phase(&executed2, 2, &isolated);
    let manifest2 = validated_manifest(&isolated, 2, seed_value);
    println!(
        "RESULT {}",
        json!({"scenario":"saya-continue-phase2","run":run,"phase1":phase1,"phase2":phase2,
        "reviewValidated":true,"promptId":prompt2,"state":state2,"executedSeeds":seeds2,
        "checkpoint":"phase_2_hires.png","transaction":manifest2["transaction_uuid"],"seconds":phase2_seconds})
    );

    let vram_before3 = vram_free(&comfy);
    let timer = Instant::now();
    let continued3 = db.saya_continue(&run, &comfy, &results).unwrap();
    let phase3 = continued3["phases"]["3"]["job"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(phase3, phase2, "continue did not submit phase 3");
    let (state3, value3, error3) = wait_phase_outcome(&mut db, &comfy, &results, &run, 3, 1200);
    let phase3_seconds = timer.elapsed().as_secs();
    assert_eq!(state3, "succeeded", "phase 3 failed: {error3} {value3}");
    let vram_after3 = vram_free(&comfy);
    let prompt3 = prompt_id(&db, &phase3);
    let executed3 = executed_prompt(&comfy, &prompt3);
    assert_fresh_execution(&comfy, &prompt3, "phase 3", false);
    let seeds3 = chain_seeds(&executed3, seed_value, "phase 3");
    assert_isolated_phase(&executed3, 3, &isolated);
    let unets = executed_unets(&executed3);
    assert!(
        unets.iter().any(|name| name.contains("Q5_K_M")),
        "phase 3 unet not executed: {unets:?}"
    );
    let manifest3 = validated_manifest(&isolated, 3, seed_value);
    println!(
        "RESULT {}",
        json!({"scenario":"saya-continue-phase3","run":run,"phase2":phase2,"phase3":phase3,
        "promptId":prompt3,"state":state3,"executedSeeds":seeds3,"unets":unets,
        "checkpoint":"phase_3_refiner.png","transaction":manifest3["transaction_uuid"],
        "vramFreeMibBefore":vram_before3,"vramFreeMibAfter":vram_after3,"seconds":phase3_seconds})
    );

    // Phase 4 is the last phase whose graph converts completely from this
    // source; the stop node promotes its own validated checkpoint.
    let vram_before4 = vram_free(&comfy);
    let timer = Instant::now();
    let continued4 = db.saya_continue(&run, &comfy, &results).unwrap();
    let phase4 = continued4["phases"]["4"]["job"]
        .as_str()
        .expect("continue did not submit phase 4")
        .to_owned();
    assert_ne!(phase4, phase3, "continue did not submit phase 4");
    let (state4, value4, error4) = wait_phase_outcome(&mut db, &comfy, &results, &run, 4, 1800);
    let phase4_seconds = timer.elapsed().as_secs();
    assert_eq!(state4, "succeeded", "phase 4 failed: {error4} {value4}");
    let vram_after4 = vram_free(&comfy);
    let prompt4 = prompt_id(&db, &phase4);
    let executed4 = executed_prompt(&comfy, &prompt4);
    assert_fresh_execution(&comfy, &prompt4, "phase 4", false);
    let seeds4 = chain_seeds(&executed4, seed_value, "phase 4");
    assert_isolated_phase(&executed4, 4, &isolated);
    let manifest4 = validated_manifest(&isolated, 4, seed_value);
    println!(
        "RESULT {}",
        json!({"scenario":"saya-continue-phase4","run":run,"phase3":phase3,"phase4":phase4,
        "promptId":prompt4,"state":state4,"executedSeeds":seeds4,
        "checkpoint":"phase_4_predetail.png","transaction":manifest4["transaction_uuid"],
        "vramFreeMibBefore":vram_before4,"vramFreeMibAfter":vram_after4,"seconds":phase4_seconds})
    );

    // Phases 5-6: the converter activates their owned model hubs on a private
    // copy (the saved UI keeps them muted), so Continue submits them like every
    // other gate. Phase 5 runs all detailers; phase 6 finalizes.
    let mut previous = phase4.clone();
    for phase in 5..=6u32 {
        let before = vram_free(&comfy);
        let timer = Instant::now();
        let continued = db.saya_continue(&run, &comfy, &results).unwrap();
        let job = continued["phases"][phase.to_string()]["job"]
            .as_str()
            .unwrap_or_else(|| panic!("continue did not submit phase {phase}"))
            .to_owned();
        assert_ne!(job, previous, "continue did not submit phase {phase}");
        let (state, value_phase, error) =
            wait_phase_outcome(&mut db, &comfy, &results, &run, phase, 2400);
        let seconds = timer.elapsed().as_secs();
        assert_eq!(
            state, "succeeded",
            "phase {phase} failed: {error} {value_phase}"
        );
        let after = vram_free(&comfy);
        let prompt = prompt_id(&db, &job);
        let executed = executed_prompt(&comfy, &prompt);
        assert_fresh_execution(&comfy, &prompt, &format!("phase {phase}"), false);
        let seeds = chain_seeds(&executed, seed_value, &format!("phase {phase}"));
        assert_isolated_phase(&executed, phase, &isolated);
        let manifest = validated_manifest(&isolated, phase, seed_value);
        println!(
            "RESULT {}",
            json!({"scenario":format!("saya-continue-phase{phase}"),"run":run,
            "phase":phase,"job":job,"promptId":prompt,"state":state,
            "executedSeeds":seeds,"checkpoint":format!("{}.png",phase_stem(phase)),
            "transaction":manifest["transaction_uuid"],"vramFreeMibBefore":before,
            "vramFreeMibAfter":after,"seconds":seconds})
        );
        previous = job;
    }
    let completed = saya_phase(&mut db, &comfy, &results, &run);
    assert_eq!(completed["state"], "completed");
    for phase in 1..=6u32 {
        assert_eq!(
            completed["phases"][phase.to_string()]["state"],
            "succeeded",
            "phase {phase} not succeeded in the final run state"
        );
    }
    assert_eq!(
        snapshot_tree(&user_checkpoints()),
        user_before,
        "user checkpoints changed during the chain"
    );
    assert_eq!(
        digest(&std::fs::read(real_workflow()).unwrap()),
        source_before,
        "source workflow changed"
    );

    println!(
        "RESULT {}",
        json!({"scenario":"saya-chain-summary","run":run,"root":isolated,
        "userCheckpointsUnchanged":true,"sourceUnchanged":true,
        "validatedPhases":[1,2,3,4,5,6],
        "files":(1..=6u32)
            .map(|phase| format!("{}.png", phase_stem(phase)))
            .collect::<Vec<_>>(),
        "chainState":completed["state"]})
    );
}

#[test]
#[ignore = "real ComfyUI object_info only; no generation; phase 2 must read the isolated root"]
fn saya_phase2_loads_from_isolated_root() {
    if skip_without_workflow() {
        return;
    }
    let comfy = Comfy::loopback(8188).unwrap();
    let info = comfy.get("/object_info").unwrap();
    let ui = std::fs::read_to_string(real_workflow()).unwrap();
    let root = "saya-tests/gpu-stack-seeds-continue";
    let phase =
        studio_core::saya::prepare_phase_api(&ui, &info, 2, Some("424242"), Some(root)).unwrap();
    let api = phase["api"].as_object().unwrap();
    let loads: Vec<(&String, &Value)> = api
        .iter()
        .filter(|(_, node)| node["class_type"] == "SayaImagePhaseCheckpointLoad")
        .collect();
    assert_eq!(
        loads.len(),
        1,
        "phase 2 must expose exactly one configurable load node"
    );
    let (id, load) = loads[0];
    assert_eq!(load["inputs"]["phase"], "2 — HIRES / USDU");
    assert_eq!(load["inputs"]["detailer"], "none");
    assert_eq!(load["inputs"]["checkpoint_root"], root);
    assert!(
        !api.values()
            .any(|n| n["class_type"] == "SayaImagePhase2Load"),
        "fixed phase 2 loader still hardcodes image/checkpoints"
    );
    let stops: Vec<&Value> = api
        .values()
        .filter(|n| n["class_type"] == "SayaImagePhaseCheckpointStop")
        .collect();
    assert!(
        !stops.is_empty(),
        "phase 2 stop must keep writing the isolated root"
    );
    assert!(stops.iter().all(|n| n["inputs"]["checkpoint_root"] == root));
    println!(
        "RESULT {}",
        json!({"scenario":"saya-phase2-graph","loadNode":id,"phase":load["inputs"]["phase"],
        "root":load["inputs"]["checkpoint_root"],"stops":stops.len()})
    );
}

#[test]
#[ignore = "real ComfyUI object_info only; no generation; phase 1..6 graphs"]
fn saya_all_phase_graphs_use_isolated_root() {
    if skip_without_workflow() {
        return;
    }
    let comfy = Comfy::loopback(8188).unwrap();
    let info = comfy.get("/object_info").unwrap();
    let ui = std::fs::read_to_string(real_workflow())
        .unwrap()
        .replace("hidream-i1-full-Q5_1.gguf", "hidream-i1-full-Q5_K_M.gguf");
    let root = "saya-tests/gpu-stack-seeds-6phases";
    let mut phases = Vec::new();
    for phase in 1..=6u32 {
        let prepared =
            studio_core::saya::prepare_phase_api(&ui, &info, phase, Some("424242"), Some(root))
                .unwrap_or_else(|error| panic!("phase {phase}: {error}"));
        assert_eq!(prepared["report"]["complete"], true, "phase {phase}");
        assert_isolated_phase(&prepared["api"], phase, root);
        let seeds = chain_seeds(&prepared["api"], 424242, &format!("phase {phase} graph"));
        phases.push(
            json!({"phase":phase,"nodes":prepared["report"]["apiNodes"],"seeds":seeds.len()}),
        );
    }
    println!(
        "RESULT {}",
        json!({"scenario":"saya-all-phases-graph","root":root,"phases":phases})
    );
}

#[test]
#[ignore = "real ComfyUI object_info only; no generation; phase 3 graph preflight"]
fn saya_phase3_graph_uses_isolated_root() {
    if skip_without_workflow() {
        return;
    }
    let comfy = Comfy::loopback(8188).unwrap();
    let info = comfy.get("/object_info").unwrap();
    let ui = std::fs::read_to_string(real_workflow())
        .unwrap()
        .replace("hidream-i1-full-Q5_1.gguf", "hidream-i1-full-Q5_K_M.gguf");
    let root = "saya-tests/gpu-stack-seeds-phase3";
    let phase =
        studio_core::saya::prepare_phase_api(&ui, &info, 3, Some("424242"), Some(root)).unwrap();
    assert_eq!(phase["report"]["complete"], true);
    assert_isolated_phase(&phase["api"], 3, root);
    let seeds = chain_seeds(&phase["api"], 424242, "phase 3 graph");
    let unets = executed_unets(&phase["api"]);
    assert!(
        unets.iter().any(|name| name.contains("Q5_K_M")),
        "phase 3 unet: {unets:?}"
    );
    println!(
        "RESULT {}",
        json!({"scenario":"saya-phase3-graph","seedTargets":seeds.len(),"unets":unets,
        "root":root,"complete":phase["report"]["complete"]})
    );
}
