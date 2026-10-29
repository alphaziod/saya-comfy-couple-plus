//! Multi-pass orchestration end to end against the loopback mock engine
//! (scripts/mock-comfy.py, synthetic outputs, no GPU): two chained IMAGE
//! passes, restart, corruption quarantine, dependency invalidation, refusals.
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::{Child, Command},
    sync::Mutex,
    time::Duration,
};
use studio_core::{
    comfy::Comfy,
    pass_states::{MemoryOutcome, PassMemoryManager},
    Database,
};

struct Mock {
    child: Child,
    port: u16,
    journal: PathBuf,
}

impl Mock {
    fn start(dir: &std::path::Path) -> Self {
        Self::start_mode(dir, false)
    }
    fn start_mode(dir: &std::path::Path, drop_response: bool) -> Self {
        let ready = dir.join("mock-port");
        let journal = dir.join("mock-journal.jsonl");
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/mock-comfy.py");
        let mut command = Command::new("python3");
        command.args([
            script.to_str().unwrap(),
            "--accept-prompts",
            "--journal",
            journal.to_str().unwrap(),
            "--ready-file",
            ready.to_str().unwrap(),
        ]);
        if drop_response {
            command.arg("--drop-prompt-response");
        }
        let mut child = command
            .spawn()
            .expect("python3 required for the mock engine");
        for _ in 0..100 {
            if let Ok(text) = std::fs::read_to_string(&ready) {
                if let Ok(port) = text.trim().parse() {
                    return Self {
                        child,
                        port,
                        journal,
                    };
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("mock engine did not start");
    }
    fn prompts(&self) -> usize {
        std::fs::read_to_string(&self.journal)
            .unwrap_or_default()
            .lines()
            .filter(|l| l.contains("\"/prompt\""))
            .count()
    }
}

impl Drop for Mock {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn manifest(base: &str, upscale: &str) -> String {
    json!({
        "format": studio_core::workflow_pipeline::PIPELINE_FORMAT,
        "version": studio_core::workflow_pipeline::PIPELINE_VERSION,
        "id": "11111111-2222-4333-8444-555555555555",
        "name": "Base puis agrandissement",
        "workflows": [{"id": base}, {"id": upscale}],
        "steps": [
            {"id": "upscale", "workflowId": upscale, "kind": "image",
             "inputs": [{"name": "image", "portType": "IMAGE"}], "outputs": [{"name": "image", "portType": "IMAGE"}],
             "bindings": [{"port": "image", "nodeId": "1", "input": "image", "source": {"step": "base", "port": "image"}}]},
            {"id": "base", "workflowId": base, "kind": "image", "outputs": [{"name": "image", "portType": "IMAGE"}]}
        ]
    })
    .to_string()
}

fn setup(dir: &std::path::Path) -> (Database, String, String) {
    let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    let base = db.import_workflow("Base", &json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"fixture-ckpt_name.safetensors"}},
        "2":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"portrait,  exact"}},
        "3":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"neg"}},
        "4":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},
        "5":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["4",0],"seed":18446744073709551615u64,"steps":20,"cfg":6.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0}},
        "6":{"class_type":"VAEDecode","inputs":{"samples":["5",0],"vae":["1",2]}},
        "7":{"class_type":"SaveImage","inputs":{"images":["6",0],"filename_prefix":"base"}}
    }).to_string()).unwrap();
    let upscale = db.import_workflow("Agrandir", &json!({
        "1":{"class_type":"LoadImage","inputs":{"image":"fixture-image.png"}},
        "2":{"class_type":"ImageScaleBy","inputs":{"image":["1",0],"upscale_method":"nearest-exact","scale_by":2.0}},
        "3":{"class_type":"SaveImage","inputs":{"images":["2",0],"filename_prefix":"up"}}
    }).to_string()).unwrap();
    (db, base, upscale)
}

fn step<'a>(run: &'a Value, id: &str) -> &'a Value {
    run["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["step"] == id)
        .unwrap()
}

#[derive(Default)]
struct RecordingMemory {
    calls: Mutex<Vec<String>>,
}

impl PassMemoryManager for RecordingMemory {
    fn backend(&self) -> &'static str {
        "recording"
    }
    fn release_after_pass(
        &self,
        _pipeline: &str,
        pass: &str,
    ) -> studio_core::Result<MemoryOutcome> {
        self.calls.lock().unwrap().push(format!("release:{pass}"));
        Ok(MemoryOutcome {
            backend: self.backend().into(),
            released: false,
            details: "aucune baisse revendiquée".into(),
        })
    }
    fn prepare_pass(&self, _pipeline: &str, pass: &str) -> studio_core::Result<MemoryOutcome> {
        self.calls.lock().unwrap().push(format!("prepare:{pass}"));
        Ok(MemoryOutcome {
            backend: self.backend().into(),
            released: false,
            details: "préparation mesurée".into(),
        })
    }
}

#[test]
fn memory_policy_wraps_new_passes_and_refuses_missing_manager() {
    let dir = std::env::temp_dir().join(format!("studio-memory-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let mock = Mock::start(&dir);
    let client = Comfy::loopback(mock.port).unwrap();
    let results = dir.join("results");
    let (mut db, base, upscale) = setup(&dir);
    let pipeline = db.save_pipeline(&manifest(&base, &upscale)).unwrap();
    let run = db.start_pipeline_run(&pipeline).unwrap();
    let memory = RecordingMemory::default();

    let first = db
        .run_next_pass_with_memory(
            &run,
            &client,
            &results,
            true,
            Duration::from_secs(20),
            Some(&memory),
            true,
        )
        .unwrap();
    assert_eq!(first["step"], "base");
    assert_eq!(
        memory.calls.lock().unwrap().as_slice(),
        ["prepare:base", "release:base"]
    );
    assert_eq!(first["memory"]["prepare"]["released"], false);
    assert_eq!(first["memory"]["release"]["released"], false);
    let detail = db.pipeline_run(&run).unwrap();
    let kinds: Vec<&str> = detail["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["kind"].as_str())
        .collect();
    assert!(kinds.contains(&"memory_prepare"));
    assert!(kinds.contains(&"memory_release"));

    let second = db
        .run_next_pass_with_memory(
            &run,
            &client,
            &results,
            true,
            Duration::from_secs(20),
            Some(&memory),
            true,
        )
        .unwrap();
    assert_eq!(second["step"], "upscale");
    assert_eq!(
        memory.calls.lock().unwrap().as_slice(),
        [
            "prepare:base",
            "release:base",
            "prepare:upscale",
            "release:upscale"
        ]
    );

    // A policy without a manager is refused before any engine call.
    let other = db.start_pipeline_run(&pipeline).unwrap();
    let error = db
        .run_next_pass_with_memory(
            &other,
            &client,
            &results,
            true,
            Duration::from_secs(20),
            None,
            true,
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("gestionnaire"), "{error}");
    assert_eq!(mock.prompts(), 2);
}

#[test]
fn two_image_passes_chain_resume_and_quarantine_corruption() {
    let dir = std::env::temp_dir().join(format!("studio-orchestrator-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let mock = Mock::start(&dir);
    let client = Comfy::loopback(mock.port).unwrap();
    let results = dir.join("results");
    let (mut db, base, upscale) = setup(&dir);
    let pipeline = db.save_pipeline(&manifest(&base, &upscale)).unwrap();
    let run = db.start_pipeline_run(&pipeline).unwrap();

    // No submission without explicit confirmation.
    assert!(db
        .run_next_pass(&run, &client, &results, false, Duration::from_secs(20))
        .unwrap_err()
        .to_string()
        .contains("Confirmation"));
    assert_eq!(mock.prompts(), 0);

    let memory = studio_core::resources::ComfyMemoryManager {
        client: Comfy::loopback(mock.port).unwrap(),
        port: mock.port,
        drm: None,
        settle: Duration::ZERO,
        threshold: 1,
    };
    assert!(
        db.release_pipeline_memory(&run, &memory, true).is_err(),
        "no persisted output yet"
    );
    let first = db
        .run_next_pass(&run, &client, &results, true, Duration::from_secs(20))
        .unwrap();
    assert_eq!(first["step"], "base", "DAG order, not list order");
    assert_eq!(first["state"], "completed");
    assert!(db.release_pipeline_memory(&run, &memory, false).is_err());
    let release = db.release_pipeline_memory(&run, &memory, true).unwrap();
    assert_eq!(
        release["released"], false,
        "CPU mock must not claim VRAM release"
    );
    assert!(db.pipeline_run(&run).unwrap()["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "memory_release_observed"));

    let second = db
        .run_next_pass(&run, &client, &results, true, Duration::from_secs(20))
        .unwrap();
    assert_eq!(second["step"], "upscale");
    assert_eq!(second["done"], true);
    assert_ne!(
        first["sha256"], second["sha256"],
        "the second pass consumed the first output"
    );
    assert_eq!(mock.prompts(), 2);
    // The upscale job really loaded the uploaded base output.
    let journal = std::fs::read_to_string(&mock.journal).unwrap();
    assert!(journal.contains("\"/upload/image\""));
    let upload = format!("saya_{}.png", &first["sha256"].as_str().unwrap()[..16]);
    assert!(journal.contains(&upload));

    let full_backup = dir.join("full-backup");
    studio_core::backup::create_with_options(&dir.join("studio.sqlite3"), &full_backup, true)
        .unwrap();
    let restored = dir.join("restored-backup");
    studio_core::backup::restore(&full_backup, &restored).unwrap();
    let mut restored_db = Database::open(&restored.join("studio.sqlite3")).unwrap();
    assert_eq!(
        restored_db
            .run_next_pass(
                &run,
                &client,
                &restored.join("results"),
                false,
                Duration::ZERO
            )
            .unwrap()["done"],
        true
    );
    assert_eq!(mock.prompts(), 2, "restored states need no replay");
    drop(restored_db);

    // Restart: a new connection reuses validated states, submits nothing.
    drop(db);
    let mut db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    assert_eq!(
        db.run_next_pass(&run, &client, &results, true, Duration::from_secs(20))
            .unwrap()["done"],
        true
    );
    assert_eq!(mock.prompts(), 2);

    // Corrupt the base state on disk: base and upscale are invalidated, the
    // corrupt file is quarantined and both passes run again.
    let detail = db.pipeline_run(&run).unwrap();
    let job = step(&detail, "base")["job"].as_str().unwrap().to_owned();
    let root = dir.join("pass-states");
    let corrupt = walk(&root)
        .into_iter()
        .find(|p| p.to_string_lossy().contains(&format!("base-{}", &job[..8])))
        .expect("base state file");
    std::fs::write(&corrupt, b"corrupted bytes").unwrap();
    assert!(studio_core::backup::create_with_options(
        &dir.join("studio.sqlite3"),
        &dir.join("corrupt-backup"),
        true
    )
    .is_err());

    let free_before = std::fs::read_to_string(&mock.journal)
        .unwrap()
        .matches("/free")
        .count();
    assert!(
        db.release_pipeline_memory(&run, &memory, true).is_err(),
        "corrupt output blocks release"
    );
    assert_eq!(
        std::fs::read_to_string(&mock.journal)
            .unwrap()
            .matches("/free")
            .count(),
        free_before
    );

    let rerun = db
        .run_next_pass(&run, &client, &results, true, Duration::from_secs(20))
        .unwrap();
    assert_eq!(rerun["step"], "base");
    let detail = db.pipeline_run(&run).unwrap();
    assert!(detail["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "invalidated" && e["step"] == "base"));
    assert!(detail["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "invalidated" && e["step"] == "upscale"));
    assert_eq!(
        std::fs::read(&corrupt).unwrap(),
        b"corrupted bytes",
        "corrupt state never overwritten"
    );
    assert_eq!(
        db.run_next_pass(&run, &client, &results, true, Duration::from_secs(20))
            .unwrap()["done"],
        true
    );

    // A modified dependency (workflow revision hash) invalidates downstream only.
    let raw = rusqlite::Connection::open(dir.join("studio.sqlite3")).unwrap();
    raw.execute(
        "UPDATE workflow_revisions SET source_sha256='changed' WHERE id=?",
        [&upscale],
    )
    .unwrap();
    let again = db
        .run_next_pass(&run, &client, &results, true, Duration::from_secs(20))
        .unwrap();
    assert_eq!(
        again["step"], "upscale",
        "base stays valid, only upscale reruns"
    );
    drop(raw);
    drop(db);
    drop(mock);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pipelines_refuse_unknown_or_changed_workflows_and_latent_transport() {
    let dir = std::env::temp_dir().join(format!(
        "studio-orchestrator-refuse-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&dir).unwrap();
    let (db, base, upscale) = setup(&dir);
    assert!(db.save_pipeline(&manifest(&base, "missing")).is_err());
    let mut pinned: Value = serde_json::from_str(&manifest(&base, &upscale)).unwrap();
    pinned["workflows"][0]["sourceSha256"] = json!("0".repeat(64));
    assert!(db
        .save_pipeline(&pinned.to_string())
        .unwrap_err()
        .to_string()
        .contains("modifié"));
    let mut latent: Value = serde_json::from_str(&manifest(&base, &upscale)).unwrap();
    latent["steps"][1]["kind"] = json!("latent");
    latent["steps"][1]["outputs"] = json!([{"name": "image", "portType": "LATENT"}]);
    latent["steps"][0]["inputs"] = json!([{"name": "image", "portType": "LATENT"}]);
    // Refused when saved (static guard), before any run exists.
    let error = db
        .save_pipeline(&latent.to_string())
        .unwrap_err()
        .to_string();
    assert!(error.contains("LATENT"), "{error}");
    assert_eq!(count(&dir, "SELECT count(*) FROM pipeline_definitions"), 0);
    std::fs::remove_dir_all(dir).unwrap();
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[test]
fn lost_acknowledgement_keeps_the_pass_job_across_restart_without_resubmission() {
    let dir =
        std::env::temp_dir().join(format!("studio-pipeline-lost-ack-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let mock = Mock::start_mode(&dir, true);
    let client = Comfy::loopback(mock.port).unwrap();
    let (mut db, base, upscale) = setup(&dir);
    let pipeline = db.save_pipeline(&manifest(&base, &upscale)).unwrap();
    let run = db.start_pipeline_run(&pipeline).unwrap();
    assert!(db
        .run_next_pass(
            &run,
            &client,
            &dir.join("results"),
            true,
            Duration::from_secs(1)
        )
        .is_err());
    assert_eq!(mock.prompts(), 1);
    let detail = db.pipeline_run(&run).unwrap();
    assert_eq!(
        step(&detail, "base")["state"],
        "submitted",
        "an ambiguous submission must keep its durable job association"
    );
    let job = step(&detail, "base")["job"].as_str().unwrap().to_owned();
    drop(db);
    let mut db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    let result = db
        .run_next_pass(
            &run,
            &client,
            &dir.join("results"),
            false,
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(result["state"], "completed");
    assert_eq!(step(&db.pipeline_run(&run).unwrap(), "base")["job"], job);
    assert_eq!(
        mock.prompts(),
        1,
        "recover the original job; never submit another one"
    );
    drop(db);
    drop(mock);
    std::fs::remove_dir_all(dir).unwrap();
}

fn submitted_prompts(mock: &Mock) -> Vec<Value> {
    std::fs::read_to_string(&mock.journal)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|e| e["path"] == "/prompt")
        .map(|e| e["body"]["prompt"].clone())
        .collect()
}

fn count(dir: &std::path::Path, sql: &str) -> i64 {
    rusqlite::Connection::open(dir.join("studio.sqlite3"))
        .unwrap()
        .query_row(sql, [], |r| r.get(0))
        .unwrap()
}

#[test]
fn unsupported_later_pass_settings_are_refused_before_any_submission() {
    // Astra's regression: settings on a later pass were silently ignored.
    let dir = std::env::temp_dir().join(format!(
        "studio-pipeline-preflight-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&dir).unwrap();
    let mock = Mock::start(&dir);
    let client = Comfy::loopback(mock.port).unwrap();
    let (mut db, base, upscale) = setup(&dir);
    for (key, value) in [
        ("seed", json!("18446744073709551615")),
        ("parameters", json!({"cfg": "7.5"})),
    ] {
        let mut document: Value = serde_json::from_str(&manifest(&base, &upscale)).unwrap();
        document["steps"][0][key] = value;
        // Refused when saved (static guard) or, at the latest, before submitting.
        if let Ok(pipeline) = db.save_pipeline(&document.to_string()) {
            let run = db.start_pipeline_run(&pipeline).unwrap();
            let result = db.run_next_pass(
                &run,
                &client,
                &dir.join("results"),
                true,
                Duration::from_secs(2),
            );
            assert!(
                result.is_err(),
                "unsupported later-pass {key} was silently ignored: {result:?}"
            );
        }
        assert_eq!(
            mock.prompts(),
            0,
            "validate the whole pipeline before starting the base pass"
        );
    }
    drop(db);
    drop(mock);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn preflight_applies_known_settings_exactly_and_refuses_everything_else() {
    let dir =
        std::env::temp_dir().join(format!("studio-pipeline-contract-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let mock = Mock::start(&dir);
    let client = Comfy::loopback(mock.port).unwrap();
    let (mut db, base, upscale) = setup(&dir);
    let refuse = |db: &Database, edit: &dyn Fn(&mut Value), expected: &str| {
        let before = count(&dir, "SELECT count(*) FROM pipeline_definitions");
        let mut document: Value = serde_json::from_str(&manifest(&base, &upscale)).unwrap();
        edit(&mut document);
        let error = db
            .save_pipeline(&document.to_string())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(expected),
            "expected « {expected} », got: {error}"
        );
        assert_eq!(
            count(&dir, "SELECT count(*) FROM pipeline_definitions"),
            before,
            "refused pipeline stored"
        );
    };
    // Unknown parameter, seed not applicable (upscale has no seed), wrong port
    // type, invalid manifest, declared state/resume/status, several outputs.
    refuse(
        &db,
        &|d| d["steps"][1]["parameters"] = json!({"cfg": "7"}),
        "non pris en charge",
    );
    refuse(
        &db,
        &|d| d["steps"][0]["seed"] = json!("7"),
        "seed déclarée mais non applicable",
    );
    refuse(
        &db,
        &|d| d["steps"][1]["parameters"] = json!({"9.text": "x"}),
        "introuvable",
    );
    refuse(
        &db,
        &|d| d["steps"][1]["parameters"] = json!({"5.steps": "vingt"}),
        "entier attendu",
    );
    refuse(
        &db,
        &|d| d["steps"][0]["inputs"][0]["portType"] = json!("MASK"),
        "",
    );
    refuse(
        &db,
        &|d| d["version"] = json!(3),
        "Format de pipeline inconnu",
    );
    refuse(
        &db,
        &|d| d["steps"][1]["status"] = json!("completed"),
        "statut",
    );
    refuse(
        &db,
        &|d| d["steps"][1]["state"] = json!({"path": "x.png", "portType": "IMAGE"}),
        "état intermédiaire",
    );
    refuse(
        &db,
        &|d| {
            d["steps"][1]["outputs"] =
                json!([{"name":"image","portType":"IMAGE"},{"name":"b","portType":"IMAGE"}])
        },
        "une seule sortie",
    );
    assert_eq!(mock.prompts(), 0);

    // Known settings: u64 max seed, explicit node.input and a typed number,
    // applied exactly to the submitted graph.
    let mut document: Value = serde_json::from_str(&manifest(&base, &upscale)).unwrap();
    document["steps"][1]["seed"] = json!("18446744073709551615");
    document["steps"][1]["parameters"] =
        json!({"2.text": "portrait,  exact (poids:1.3)", "5.cfg": "7.5"});
    let pipeline = db.save_pipeline(&document.to_string()).unwrap();
    let run = db.start_pipeline_run(&pipeline).unwrap();
    db.run_next_pass(
        &run,
        &client,
        &dir.join("results"),
        true,
        Duration::from_secs(20),
    )
    .unwrap();
    let sent = submitted_prompts(&mock);
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0]["5"]["inputs"]["seed"],
        json!(18446744073709551615u64)
    );
    assert_eq!(sent[0]["5"]["inputs"]["cfg"], json!(7.5));
    assert_eq!(
        sent[0]["2"]["inputs"]["text"],
        "portrait,  exact (poids:1.3)"
    );

    // Engine-schema refusal at run time (value not in the sampler list):
    // nothing is written, nothing is submitted.
    let mut bogus: Value = serde_json::from_str(&manifest(&base, &upscale)).unwrap();
    bogus["steps"][1]["parameters"] = json!({"5.sampler_name": "not-a-sampler"});
    let pipeline = db.save_pipeline(&bogus.to_string()).unwrap();
    let run = db.start_pipeline_run(&pipeline).unwrap();
    let events = count(&dir, "SELECT count(*) FROM pipeline_run_events");
    let error = db
        .run_next_pass(
            &run,
            &client,
            &dir.join("results"),
            true,
            Duration::from_secs(5),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("schéma du moteur"), "{error}");
    assert_eq!(
        submitted_prompts(&mock).len(),
        1,
        "no submission after a failed preflight"
    );
    assert_eq!(
        count(&dir, "SELECT count(*) FROM pipeline_run_events"),
        events,
        "no partial write"
    );
    assert_eq!(
        count(
            &dir,
            &format!(
                "SELECT count(*) FROM pipeline_run_steps WHERE run_id='{run}' AND state!='pending'"
            )
        ),
        0
    );
    drop(db);
    drop(mock);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn resume_does_not_bypass_the_preflight() {
    let dir = std::env::temp_dir().join(format!(
        "studio-pipeline-resume-guard-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&dir).unwrap();
    let mock = Mock::start(&dir);
    let client = Comfy::loopback(mock.port).unwrap();
    let (mut db, base, upscale) = setup(&dir);
    let hash: String = rusqlite::Connection::open(dir.join("studio.sqlite3"))
        .unwrap()
        .query_row(
            "SELECT source_sha256 FROM workflow_revisions WHERE id=?",
            [&upscale],
            |r| r.get(0),
        )
        .unwrap();
    let mut document: Value = serde_json::from_str(&manifest(&base, &upscale)).unwrap();
    document["workflows"][1]["sourceSha256"] = json!(hash);
    let pipeline = db.save_pipeline(&document.to_string()).unwrap();
    let run = db.start_pipeline_run(&pipeline).unwrap();
    assert_eq!(
        db.run_next_pass(
            &run,
            &client,
            &dir.join("results"),
            true,
            Duration::from_secs(20)
        )
        .unwrap()["step"],
        "base"
    );
    // The pinned upscale workflow changes between passes: the next call refuses.
    rusqlite::Connection::open(dir.join("studio.sqlite3"))
        .unwrap()
        .execute(
            "UPDATE workflow_revisions SET source_sha256='changed' WHERE id=?",
            [&upscale],
        )
        .unwrap();
    drop(db);
    let mut db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    let error = db
        .run_next_pass(
            &run,
            &client,
            &dir.join("results"),
            true,
            Duration::from_secs(5),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("modifié depuis l'épinglage"), "{error}");
    assert_eq!(mock.prompts(), 1, "the refused pass was not submitted");
    drop(db);
    drop(mock);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Same base graph as `setup`, with a SaveImage prefix steering the mock
/// (mock-fail: engine error; mock-slow: 3 s in queue before success).
fn base_with_prefix(db: &Database, prefix: &str) -> String {
    let document = db.workflow_document(&setup_base_id(db)).unwrap().1;
    let mut graph: Value = serde_json::from_str(&document).unwrap();
    graph["7"]["inputs"]["filename_prefix"] = json!(prefix);
    db.import_workflow(&format!("Base {prefix}"), &graph.to_string())
        .unwrap()
}

fn setup_base_id(db: &Database) -> String {
    db.workflow_document_by_name("Base").unwrap()
}

fn single(base: &str) -> String {
    json!({
        "format": studio_core::workflow_pipeline::PIPELINE_FORMAT,
        "version": studio_core::workflow_pipeline::PIPELINE_VERSION,
        "id": "22222222-3333-4444-8555-666666666666",
        "name": "Une passe",
        "workflows": [{"id": base}],
        "steps": [{"id": "base", "workflowId": base, "kind": "image", "outputs": [{"name": "image", "portType": "IMAGE"}]}]
    })
    .to_string()
}

#[test]
fn failed_pass_late_result_deleted_state_and_cancellation_have_precise_semantics() {
    let dir = std::env::temp_dir().join(format!(
        "studio-pipeline-transitions-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir(&dir).unwrap();
    let mock = Mock::start(&dir);
    let client = Comfy::loopback(mock.port).unwrap();
    let results = dir.join("results");
    let (mut db, _, _) = setup(&dir);

    // 1. Engine failure: the pass is failed, never auto-retried; an explicit
    //    confirmed call retries it with a new job.
    let failing = base_with_prefix(&db, "mock-fail");
    let run = db
        .start_pipeline_run(&db.save_pipeline(&single(&failing)).unwrap())
        .unwrap();
    assert_eq!(
        db.run_next_pass(&run, &client, &results, true, Duration::from_secs(10))
            .unwrap()["state"],
        "failed"
    );
    assert_eq!(db.pipeline_run(&run).unwrap()["state"], "failed");
    assert!(
        db.run_next_pass(&run, &client, &results, false, Duration::from_secs(1))
            .is_err(),
        "retry needs confirmation"
    );
    assert_eq!(mock.prompts(), 1);
    db.run_next_pass(&run, &client, &results, true, Duration::from_secs(10))
        .unwrap();
    assert_eq!(mock.prompts(), 2, "explicit retry = new job");

    // 2. Late result: timeout returns waiting; the resumed wait (no new
    //    confirmation, no new submission) collects the output.
    let slow = base_with_prefix(&db, "mock-slow");
    let run = db
        .start_pipeline_run(&db.save_pipeline(&single(&slow)).unwrap())
        .unwrap();
    let waiting = db
        .run_next_pass(&run, &client, &results, true, Duration::from_millis(500))
        .unwrap();
    assert_eq!(waiting["waiting"], true);
    let before = mock.prompts();
    let done = db
        .run_next_pass(&run, &client, &results, false, Duration::from_secs(20))
        .unwrap();
    assert_eq!(done["state"], "completed");
    assert_eq!(mock.prompts(), before, "resume never resubmits");
    let job = step(&db.pipeline_run(&run).unwrap(), "base")["job"]
        .as_str()
        .unwrap()
        .to_owned();
    let provenance = db.job_provenance(&job).unwrap();
    assert_eq!(
        provenance["pipeline"]["run"], run,
        "pass job provenance names its run"
    );
    assert_eq!(provenance["pipeline"]["step"], "base");

    // 3. Deleted intermediate file: the pass is invalidated and runs again.
    let detail = db.pipeline_run(&run).unwrap();
    let job = step(&detail, "base")["job"].as_str().unwrap().to_owned();
    let file = walk(&dir.join("pass-states"))
        .into_iter()
        .find(|p| p.to_string_lossy().contains(&format!("base-{}", &job[..8])))
        .unwrap();
    std::fs::remove_file(&file).unwrap();
    let again = db
        .run_next_pass(&run, &client, &results, true, Duration::from_millis(500))
        .unwrap();
    assert_eq!(again["waiting"], true, "re-executed (slow engine)");
    let events = db.pipeline_run(&run).unwrap()["events"].clone();
    assert!(events
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "invalidated"));

    // 4. Cancellation while queued: removed from the engine queue, confirmed
    //    absent, pass failed and run paused; nothing completes later.
    let prompts = mock.prompts();
    let cancel = db.cancel_pipeline_run(&run, &client, true).unwrap();
    assert_eq!(cancel["action"], "cancel_queue");
    assert_eq!(cancel["engine"]["confirmed"], true, "{cancel}");
    let detail = db.pipeline_run(&run).unwrap();
    assert_eq!(detail["state"], "paused");
    assert_eq!(step(&detail, "base")["state"], "failed");
    std::thread::sleep(Duration::from_millis(3500));
    db.sync_jobs(&client, &results).unwrap();
    assert_eq!(
        step(&db.pipeline_run(&run).unwrap(), "base")["state"],
        "failed",
        "a cancelled pass never completes later"
    );
    assert_eq!(mock.prompts(), prompts);
    assert!(db.cancel_pipeline_run(&run, &client, false).is_err());
    // Stopping without an active pass calls nothing on the engine.
    let stopped = db.cancel_pipeline_run(&run, &client, true).unwrap();
    assert_eq!(stopped["engineCalled"], false);
    drop(db);
    drop(mock);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn split_pipeline_v2_uses_existing_runner_and_resumes_with_exact_multiline_prompt() {
    use studio_core::workflow_split::Boundary;
    let dir = std::env::temp_dir().join(format!("studio-split-runner-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let mock = Mock::start(&dir);
    let client = Comfy::loopback(mock.port).unwrap();
    let (mut db, base, _) = setup(&dir);
    let info = client.get("/object_info").unwrap();
    let cuts = vec![Boundary {
        node: "6".into(),
        slot: 0,
    }];
    let preview = db.split_preview(&base, &cuts, "7", &info).unwrap();
    let saved = db
        .split_save(
            &base,
            &cuts,
            "7",
            "Split V2",
            preview["previewHash"].as_str().unwrap(),
            true,
            &info,
        )
        .unwrap();
    let mut manifest: Value =
        serde_json::from_str(saved["manifestJson"].as_str().unwrap()).unwrap();
    let text = "  (weight:1.25), été 🌸\nsecond\r\n\tend  ";
    manifest["steps"][0]["parameters"] = json!({"2.text":text});
    let pipeline = db.save_pipeline(&manifest.to_string()).unwrap();
    let run = db.start_pipeline_run(&pipeline).unwrap();
    assert!(db
        .run_next_pass(
            &run,
            &client,
            &dir.join("results"),
            false,
            Duration::from_secs(20)
        )
        .is_err());
    assert_eq!(mock.prompts(), 0);
    let first = db
        .run_next_pass(
            &run,
            &client,
            &dir.join("results"),
            true,
            Duration::from_secs(20),
        )
        .unwrap();
    assert_eq!(first["state"], "completed");
    drop(db);
    let mut db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    let second = db
        .run_next_pass(
            &run,
            &client,
            &dir.join("results"),
            true,
            Duration::from_secs(20),
        )
        .unwrap();
    assert_eq!(second["state"], "completed");
    assert_eq!(mock.prompts(), 2);
    let journal = std::fs::read_to_string(&mock.journal).unwrap();
    assert!(journal.contains("/upload/image"));
    // Request snapshot in the durable job includes exact whitespace and seed.
    let run_details = db.pipeline_run(&run).unwrap();
    let job = run_details["steps"][0]["job"].as_str().unwrap();
    let details = db.job_details(job).unwrap();
    let request: Value = serde_json::from_str(details["requestJson"].as_str().unwrap()).unwrap();
    assert_eq!(request["prompt"]["2"]["inputs"]["text"], text);
    assert_eq!(
        request["prompt"]["5"]["inputs"]["seed"],
        json!(18446744073709551615u64)
    );
    drop(db);
    drop(mock);
    std::fs::remove_dir_all(dir).unwrap();
}

fn latent_setup(dir: &std::path::Path) -> (Database, String, String) {
    let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    let base = db.import_workflow("Base latent", &json!({
        "1":{"class_type":"CheckpointLoaderSimple","inputs":{"ckpt_name":"fixture-ckpt_name.safetensors"}},
        "2":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"p"}},
        "3":{"class_type":"CLIPTextEncode","inputs":{"clip":["1",1],"text":"n"}},
        "4":{"class_type":"EmptyLatentImage","inputs":{"width":512,"height":512,"batch_size":1}},
        "5":{"class_type":"KSampler","inputs":{"model":["1",0],"positive":["2",0],"negative":["3",0],"latent_image":["4",0],"seed":18446744073709551615u64,"steps":2,"cfg":6.0,"sampler_name":"euler","scheduler":"normal","denoise":1.0}},
        "6":{"class_type":"SaveLatent","inputs":{"samples":["5",0],"filename_prefix":"base-latent"}}
    }).to_string()).unwrap();
    let refine = db.import_workflow("Refine latent", &json!({
        "1":{"class_type":"LoadLatent","inputs":{"latent":"saya-pipeline-awaiting-latent.latent"}},
        "2":{"class_type":"SaveLatent","inputs":{"samples":["1",0],"filename_prefix":"refine-latent"}}
    }).to_string()).unwrap();
    (db, base, refine)
}

fn latent_manifest(base: &str, refine: &str) -> String {
    json!({
        "format": studio_core::workflow_pipeline::PIPELINE_FORMAT,
        "version": studio_core::workflow_pipeline::PIPELINE_VERSION,
        "id": "21111111-2222-4333-8444-555555555555",
        "name": "Deux passes latent",
        "workflows": [{"id": base}, {"id": refine}],
        "steps": [
            {"id":"refine","workflowId":refine,"kind":"latent",
             "inputs":[{"name":"latent","portType":"LATENT"}],
             "outputs":[{"name":"latent","portType":"LATENT"}],
             "bindings":[{"port":"latent","nodeId":"1","input":"latent","source":{"step":"base","port":"latent"}}]},
            {"id":"base","workflowId":base,"kind":"latent","outputs":[{"name":"latent","portType":"LATENT"}]}
        ]
    }).to_string()
}

#[test]
fn two_latent_passes_transport_verified_files_and_survive_restart() {
    let dir = std::env::temp_dir().join(format!("studio-latent-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let mock = Mock::start(&dir);
    let client = Comfy::loopback(mock.port).unwrap();
    let results = dir.join("results");
    let (mut db, base, refine) = latent_setup(&dir);
    let pipeline = db.save_pipeline(&latent_manifest(&base, &refine)).unwrap();
    let run = db.start_pipeline_run(&pipeline).unwrap();
    let first = db
        .run_next_pass(&run, &client, &results, true, Duration::from_secs(20))
        .unwrap();
    assert_eq!(first["step"], "base");
    assert_eq!(first["state"], "completed");
    // A real SaveLatent container was collected and hashed.
    let sha = first["sha256"].as_str().unwrap();
    assert_eq!(sha.len(), 64);
    let detail = db.pipeline_run(&run).unwrap();
    let base_job = detail["steps"][0]["job"].as_str().unwrap();
    let recipe = db
        .create_recipe_from_job(base_job, "Pipeline latent", "")
        .unwrap();
    let content = db.recipe_detail(recipe["id"].as_str().unwrap()).unwrap()["content"].clone();
    assert_eq!(
        content["dependencies"]["passes"].as_array().unwrap().len(),
        2
    );
    assert_eq!(content["provenance"]["pipeline"]["stepId"], "base");
    assert!(content["dependencies"]["passes"][0]["api"].is_object());
    drop(db);
    let mut db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    let second = db
        .run_next_pass(&run, &client, &results, true, Duration::from_secs(20))
        .unwrap();
    assert_eq!(second["step"], "refine");
    assert_eq!(second["state"], "completed");
    assert_eq!(second["done"], true);
    assert_eq!(mock.prompts(), 2);
    let journal = std::fs::read_to_string(&mock.journal).unwrap();
    assert!(journal.contains("SaveLatent"));
    assert!(journal.contains("LoadLatent"));
    assert_eq!(db.pipeline_run(&run).unwrap()["state"], "completed");
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
