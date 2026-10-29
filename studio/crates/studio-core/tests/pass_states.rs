//! Pass-state store: save/reload round-trip, integrity, orphans, retention and
//! cleanup safety. Every fixture lives under the OS temporary directory; the
//! user database and library are never touched.
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
use studio_core::pass_states::{
    apply_cleanup, audit_store, evaluate_cleanup, init_store, load_state, open_store,
    pass_boundary, resize_budget, save_state, set_retention, store_summary, verify_state,
    CleanupPlan, Issue, MemoryOutcome, NewState, NoopPassMemoryManager, PassMemoryManager,
    Retention, RetentionPolicy, StateKind, StatusPolicy,
};

fn temp_root() -> PathBuf {
    std::env::temp_dir().join(format!("saya-pass-states-{}", uuid::Uuid::new_v4()))
}

fn png_bytes() -> Vec<u8> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]))
        .write_to(&mut buffer, image::ImageFormat::Png)
        .unwrap();
    buffer.into_inner()
}

fn state<'a>(pipeline: &'a str, pass: &'a str, step: &'a str, bytes: &'a [u8]) -> NewState<'a> {
    NewState {
        pipeline,
        pass,
        producer_step: step,
        kind: if step.starts_with("latent") {
            StateKind::Latent
        } else {
            StateKind::Image
        },
        retention: Retention::Normal,
        bytes,
        max_state_bytes: 128 * 1024 * 1024,
    }
}

fn ephemeral<'a>(pipeline: &'a str, pass: &'a str, step: &'a str, bytes: &'a [u8]) -> NewState<'a> {
    let mut value = state(pipeline, pass, step, bytes);
    value.retention = Retention::Ephemeral;
    value
}

fn pinned<'a>(pipeline: &'a str, pass: &'a str, step: &'a str, bytes: &'a [u8]) -> NewState<'a> {
    let mut value = state(pipeline, pass, step, bytes);
    value.retention = Retention::Pinned;
    value
}

/// Simulation helper: rewrite the registry timestamps, like an old store would
/// carry them. The recomputed file paths stay valid, so the store stays open.
fn backdate(root: &Path, sha256: &str, secs: u64) {
    let path = root.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for entry in manifest["entries"].as_array_mut().unwrap() {
        if entry["sha256"] == json!(sha256) {
            entry["createdAtSecs"] = json!(secs);
            entry["createdAt"] = json!(format!("backdated-{secs}"));
        }
    }
    std::fs::write(&path, manifest.to_string()).unwrap();
}

fn entry_count(root: &Path) -> usize {
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    manifest["entries"].as_array().unwrap().len()
}

fn file_path(root: &Path, file: &str) -> PathBuf {
    root.join(file)
}

#[test]
fn save_reload_round_trip_survives_reopen_and_enforces_budget() {
    let root = temp_root();
    let image = png_bytes();
    let latent: Vec<u8> = (0..64u32).map(|i| (i * 7 % 251) as u8).collect();
    init_store(&root, 1024 * 1024).unwrap();
    let saved = save_state(
        &root,
        &state("saya-couple", "base", "SaveImage:10:0", &image),
    )
    .unwrap();
    assert_eq!(saved.pipeline, "saya-couple");
    assert_eq!(saved.kind, StateKind::Image);
    assert_eq!(saved.bytes, image.len() as u64);
    assert!(saved.file.starts_with("data/saya-couple/base/"));
    assert!(saved.file.ends_with(".png"));
    let saved_latent = save_state(
        &root,
        &state("saya-couple", "refiner", "latent:samples", &latent),
    )
    .unwrap();
    assert!(saved_latent.file.ends_with(".safetensors"));

    // Round-trip: exact bytes back, hash verified twice (on disk then in memory).
    let (reloaded, bytes) = load_state(&root, "saya-couple", "base", &saved.sha256).unwrap();
    assert_eq!(bytes, image);
    assert_eq!(reloaded.producer_step, "SaveImage:10:0");
    assert_eq!(studio_core::gallery::digest(&bytes), reloaded.sha256);
    verify_state(&root, "saya-couple", "refiner", &saved_latent.sha256).unwrap();

    // Deduplication: identical content in the same pass returns the same entry.
    let duplicate = save_state(
        &root,
        &state("saya-couple", "base", "SaveImage:10:0", &image),
    )
    .unwrap();
    assert_eq!(duplicate, saved);
    assert_eq!(entry_count(&root), 2);

    // Reopen: the registry alone is enough to find and verify states again.
    let reopened = open_store(&root).unwrap();
    assert_eq!(reopened.entries.len(), 2);
    assert!(load_state(&root, "saya-couple", "base", &saved.sha256).is_ok());
    let summary = store_summary(&root).unwrap();
    assert_eq!(summary["entries"], 2);
    assert_eq!(summary["byKind"]["image"], 1);
    assert_eq!(summary["byKind"]["latent"], 1);
    assert_eq!(summary["byRetention"]["normal"], 2);

    // Budget: the first large state fits, the second exceeds the declared
    // budget and is refused with nothing deleted.
    let big = vec![0u8; 700 * 1024];
    let big_alt = vec![1u8; 700 * 1024];
    save_state(&root, &state("saya-couple", "base", "big-1", &big)).unwrap();
    assert_eq!(entry_count(&root), 3);
    assert!(save_state(&root, &state("saya-couple", "base", "big-2", &big_alt)).is_err());
    assert_eq!(entry_count(&root), 3);
    assert!(audit_store(&root).unwrap().is_empty());
    // Budget resize below the registered volume is refused; raising is fine.
    assert!(resize_budget(&root, 512 * 1024).is_err());
    let resized = resize_budget(&root, 2 * 1024 * 1024).unwrap();
    assert_eq!(resized.budget_bytes, 2 * 1024 * 1024);
    save_state(&root, &state("saya-couple", "base", "big-2", &big_alt)).unwrap();
    assert_eq!(entry_count(&root), 4);

    // Bad inputs refuse without touching the store.
    assert!(save_state(&root, &state("bad/pipeline", "base", "s", &image)).is_err());
    assert!(save_state(&root, &state("saya-couple", "bad.pass", "s", &image)).is_err());
    assert!(save_state(&root, &state("saya-couple", "base", "", &image)).is_err());
    assert_eq!(entry_count(&root), 4);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn corruption_size_divergence_and_missing_state_are_detected() {
    let root = temp_root();
    let image = png_bytes();
    init_store(&root, 1024 * 1024).unwrap();
    let saved = save_state(&root, &state("pipe", "only", "step", &image)).unwrap();
    let path = file_path(&root, &saved.file);

    // Corruption of identical length: hash divergence detected.
    let mut damaged = image.clone();
    damaged[0] ^= 0xff;
    std::fs::write(&path, &damaged).unwrap();
    assert!(verify_state(&root, "pipe", "only", &saved.sha256).is_err());
    assert!(load_state(&root, "pipe", "only", &saved.sha256).is_err());
    assert_eq!(
        audit_store(&root).unwrap(),
        vec![Issue::Corrupt {
            label: saved.label()
        }]
    );

    // Truncation: size divergence detected.
    std::fs::write(&path, &image[..image.len() - 1]).unwrap();
    assert!(verify_state(&root, "pipe", "only", &saved.sha256).is_err());
    assert_eq!(
        audit_store(&root).unwrap(),
        vec![Issue::SizeMismatch {
            label: saved.label(),
            expected: image.len() as u64,
            actual: (image.len() - 1) as u64,
        }]
    );

    // Disappearance: entry without file detected.
    std::fs::remove_file(&path).unwrap();
    assert!(verify_state(&root, "pipe", "only", &saved.sha256).is_err());
    assert_eq!(
        audit_store(&root).unwrap(),
        vec![Issue::Missing {
            label: saved.label()
        }]
    );

    // Unknown hash and wrong pass are clean refusals, not panics.
    assert!(verify_state(&root, "pipe", "only", &"a".repeat(64)).is_err());
    assert!(verify_state(&root, "pipe", "other", &saved.sha256).is_err());
    assert!(load_state(&root, "../escape", "only", &saved.sha256).is_err());

    // Restored bytes verify again; a symlink at the managed path is refused.
    std::fs::write(&path, &image).unwrap();
    verify_state(&root, "pipe", "only", &saved.sha256).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = temp_root().parent().unwrap().join("outside-target.png");
        std::fs::write(&outside, b"victim").unwrap();
        std::fs::remove_file(&path).unwrap();
        symlink(&outside, &path).unwrap();
        assert!(verify_state(&root, "pipe", "only", &saved.sha256).is_err());
        assert!(load_state(&root, "pipe", "only", &saved.sha256).is_err());
        assert_eq!(
            audit_store(&root).unwrap(),
            vec![Issue::Symlink {
                file: saved.file.clone()
            }]
        );
        assert_eq!(std::fs::read(&outside).unwrap(), b"victim");
        std::fs::remove_file(&outside).unwrap();
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn audit_detects_orphans_and_partial_residuals_without_touching_them() {
    let root = temp_root();
    let image = png_bytes();
    init_store(&root, 1024 * 1024).unwrap();
    let saved = save_state(&root, &state("pipe", "pass", "step", &image)).unwrap();
    let dir = root.join(&saved.file).parent().unwrap().join("nested");
    std::fs::create_dir_all(&dir).unwrap();

    // Unregistered file inside a managed pass folder.
    let orphan = dir.join(format!("{}.png", studio_core::gallery::digest(b"stray")));
    std::fs::write(&orphan, b"stray").unwrap();
    // Interrupted write residue, for states and for the registry itself.
    std::fs::write(dir.join(".partial-interrupted"), b"half").unwrap();
    std::fs::write(root.join(".manifest.json.part-crash"), b"half").unwrap();
    std::fs::write(root.join("foreign.txt"), b"user note").unwrap();

    let issues = audit_store(&root).unwrap();
    assert!(issues.contains(&Issue::Orphan {
        file: orphan
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }));
    assert!(issues.contains(&Issue::Orphan {
        file: "foreign.txt".into()
    }));
    assert!(issues.contains(&Issue::PartialResidual {
        file: "data/pipe/pass/nested/.partial-interrupted".into()
    }));
    assert!(issues.contains(&Issue::PartialResidual {
        file: ".manifest.json.part-crash".into()
    }));
    assert_eq!(issues.len(), 4);

    // Detection only: every reported file still exists, managed state intact.
    assert!(orphan.exists());
    assert!(root.join(".manifest.json.part-crash").exists());
    verify_state(&root, "pipe", "pass", &saved.sha256).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cleanup_selects_per_status_policy_and_pinned_states_survive() {
    let root = temp_root();
    // Distinct contents so every state has its own hash, like real passes.
    let base_image = png_bytes();
    let mut image = base_image.clone();
    image[0] ^= 1;
    let mut newer = base_image.clone();
    newer[0] ^= 2;
    let resume_bytes: Vec<u8> = vec![9u8; 32];
    let scratch_bytes: Vec<u8> = vec![8u8; 32];
    init_store(&root, 1024 * 1024).unwrap();
    let oldest = save_state(&root, &state("pipe", "p1", "s1", &image)).unwrap();
    let middle = save_state(&root, &state("pipe", "p2", "s2", &newer)).unwrap();
    let newest = save_state(&root, &state("pipe", "p3", "s3", &base_image)).unwrap();
    let resume = save_state(&root, &pinned("pipe", "resume", "keep", &resume_bytes)).unwrap();
    let ephemeral = save_state(&root, &ephemeral("pipe", "scratch", "s4", &scratch_bytes)).unwrap();
    backdate(&root, &oldest.sha256, 1000);
    backdate(&root, &middle.sha256, 2000);
    backdate(&root, &newest.sha256, 8600);
    backdate(&root, &resume.sha256, 4000);
    backdate(&root, &ephemeral.sha256, 2500);

    // Count bound per status: oldest overflow selected, pinned protected.
    let policy = RetentionPolicy {
        ephemeral: StatusPolicy::default(),
        normal: StatusPolicy {
            max_total_bytes: None,
            max_age_secs: None,
            max_count: Some(2),
        },
    };
    let plan = evaluate_cleanup(&root, &policy, 10_000).unwrap();
    assert_eq!(
        plan.selected.iter().map(|e| e.label()).collect::<Vec<_>>(),
        vec![oldest.label()]
    );
    assert!(plan.protected.contains(&resume.label()));

    // Age bound: pinned is protected even when far too old.
    let policy = RetentionPolicy {
        ephemeral: StatusPolicy::default(),
        normal: StatusPolicy {
            max_total_bytes: None,
            max_age_secs: Some(1500),
            max_count: None,
        },
    };
    let plan = evaluate_cleanup(&root, &policy, 10_000).unwrap();
    let selected: Vec<_> = plan.selected.iter().map(|e| e.label()).collect();
    assert!(selected.contains(&oldest.label()));
    assert!(selected.contains(&middle.label()));
    assert!(!selected.contains(&newest.label()));
    assert!(!selected.contains(&resume.label()));

    // Size bound on ephemerals: selection happens inside the status only.
    let policy = RetentionPolicy {
        ephemeral: StatusPolicy {
            max_total_bytes: Some(0),
            max_age_secs: None,
            max_count: None,
        },
        normal: StatusPolicy::default(),
    };
    let plan = evaluate_cleanup(&root, &policy, 10_000).unwrap();
    assert_eq!(
        plan.selected.iter().map(|e| e.label()).collect::<Vec<_>>(),
        vec![ephemeral.label()]
    );

    // Empty policy selects nothing.
    let plan = evaluate_cleanup(&root, &RetentionPolicy::default(), 10_000).unwrap();
    assert!(plan.selected.is_empty());
    assert_eq!(plan.protected.len(), 1);

    // Apply the age plan: selected files disappear, pinned and newest survive,
    // the registry matches the disk, and a reopened store still verifies.
    let policy = RetentionPolicy {
        ephemeral: StatusPolicy::default(),
        normal: StatusPolicy {
            max_total_bytes: None,
            max_age_secs: Some(1500),
            max_count: None,
        },
    };
    let plan = evaluate_cleanup(&root, &policy, 10_000).unwrap();
    let report = apply_cleanup(&root, &plan).unwrap();
    assert_eq!(report.removed.len(), 2);
    assert_eq!(report.freed_bytes, oldest.bytes + middle.bytes);
    assert!(load_state(&root, "pipe", "resume", &resume.sha256).is_ok());
    assert!(load_state(&root, "pipe", "p3", &newest.sha256).is_ok());
    assert!(load_state(&root, "pipe", "p1", &oldest.sha256).is_err());
    assert_eq!(entry_count(&root), 3);
    assert!(audit_store(&root).unwrap().is_empty());
    drop(open_store(&root));
    assert!(open_store(&root).is_ok());

    // A hand-crafted plan that targets a pinned state is refused, file intact.
    let hostile = CleanupPlan {
        selected: vec![resume.clone()],
        protected: vec![],
        freed_bytes: resume.bytes,
    };
    assert!(apply_cleanup(&root, &hostile).is_err());
    verify_state(&root, "pipe", "resume", &resume.sha256).unwrap();

    // Pinning a normal state moves it out of cleanup reach; unpinning returns it.
    let updated = set_retention(&root, "pipe", "p3", &newest.sha256, Retention::Pinned).unwrap();
    assert_eq!(updated.retention, Retention::Pinned);
    let plan = evaluate_cleanup(
        &root,
        &RetentionPolicy {
            normal: StatusPolicy {
                max_total_bytes: Some(0),
                max_age_secs: None,
                max_count: None,
            },
            ephemeral: StatusPolicy::default(),
        },
        10_000,
    )
    .unwrap();
    assert!(plan.selected.is_empty());
    assert!(plan.protected.contains(&newest.label()));
    let untyped = set_retention(&root, "pipe", "p3", &newest.sha256, Retention::Normal).unwrap();
    assert_eq!(untyped.retention, Retention::Normal);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn cleanup_and_saving_refuse_symlinks_foreign_roots_and_escapes() {
    let root = temp_root();
    let image = png_bytes();
    init_store(&root, 1024 * 1024).unwrap();
    let saved = save_state(&root, &state("pipe", "pass", "step", &image)).unwrap();
    let outside = temp_root().parent().unwrap().join("cleanup-victim.png");
    std::fs::write(&outside, b"outside").unwrap();

    // A symlink planted on the managed path blocks cleanup of that entry and
    // the target outside the root is never touched.
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let path = file_path(&root, &saved.file);
        std::fs::remove_file(&path).unwrap();
        symlink(&outside, &path).unwrap();
        let plan = CleanupPlan {
            selected: vec![saved.clone()],
            protected: vec![],
            freed_bytes: saved.bytes,
        };
        assert!(apply_cleanup(&root, &plan).is_err());
        assert_eq!(std::fs::read(&outside).unwrap(), b"outside");
        assert_eq!(entry_count(&root), 1);
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, &image).unwrap();
    }

    // Path escapes through names are refused before anything is written.
    assert!(save_state(&root, &state("a/../b", "pass", "s", &image)).is_err());
    assert!(save_state(&root, &state("pipe", "..", "s", &image)).is_err());
    assert_eq!(entry_count(&root), 1);

    // A foreign directory without this store's manifest is never adopted.
    let foreign = temp_root();
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::write(foreign.join("user-file.txt"), b"precious").unwrap();
    assert!(open_store(&foreign).is_err());
    assert!(save_state(&foreign, &state("pipe", "pass", "s", &image)).is_err());
    assert!(audit_store(&foreign).is_err());
    assert!(evaluate_cleanup(&foreign, &RetentionPolicy::default(), 0).is_err());
    assert!(apply_cleanup(&foreign, &CleanupPlan::default()).is_err());
    assert!(init_store(&foreign, 1024 * 1024).is_err());
    assert_eq!(
        std::fs::read(foreign.join("user-file.txt")).unwrap(),
        b"precious"
    );

    // A symlinked root is refused everywhere.
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let real = temp_root();
        std::fs::create_dir_all(&real).unwrap();
        let link = temp_root();
        symlink(&real, &link).unwrap();
        assert!(init_store(&link, 1024 * 1024).is_err());
        assert!(open_store(&link).is_err());
        assert!(save_state(&link, &state("pipe", "pass", "s", &image)).is_err());
        std::fs::remove_dir_all(real).unwrap();
        std::fs::remove_file(link).unwrap();
    }

    // Tampered registry entries pointing outside the store are refused on open.
    let manifest_path = root.join("manifest.json");
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["entries"][0]["file"] = json!("../../outside.png");
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();
    assert!(open_store(&root).is_err());
    assert!(audit_store(&root).is_err());
    assert!(apply_cleanup(
        &root,
        &CleanupPlan {
            selected: vec![saved.clone()],
            protected: vec![],
            freed_bytes: saved.bytes
        }
    )
    .is_err());
    assert!(outside.exists());
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_file(outside).unwrap();
}

struct RecordingManager {
    calls: Mutex<Vec<String>>,
}

impl RecordingManager {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }
    fn recorded(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl PassMemoryManager for RecordingManager {
    fn backend(&self) -> &'static str {
        "recording"
    }
    fn release_after_pass(&self, pipeline: &str, pass: &str) -> studio_core::Result<MemoryOutcome> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("release:{pipeline}/{pass}"));
        Ok(MemoryOutcome {
            backend: "recording".into(),
            released: true,
            details: "test double".into(),
        })
    }
    fn prepare_pass(&self, pipeline: &str, pass: &str) -> studio_core::Result<MemoryOutcome> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("prepare:{pipeline}/{pass}"));
        Ok(MemoryOutcome {
            backend: "recording".into(),
            released: false,
            details: "test double".into(),
        })
    }
}

#[test]
fn pass_boundary_saves_to_disk_before_release_and_noop_never_claims_a_release() {
    let root = temp_root();
    let image = png_bytes();
    let latent: Vec<u8> = vec![1, 2, 3];
    init_store(&root, 1024 * 1024).unwrap();
    let manager = RecordingManager::new();

    // States are saved and verified before the release call happens.
    let (saved, outcome) = pass_boundary(
        &manager,
        &root,
        "saya-couple",
        "base",
        &[
            state("saya-couple", "base", "SaveImage:10:0", &image),
            state("saya-couple", "base", "latent:samples", &latent),
        ],
    )
    .unwrap();
    assert_eq!(saved.len(), 2);
    assert!(outcome.released);
    assert_eq!(
        manager.recorded(),
        vec!["release:saya-couple/base".to_owned()]
    );
    verify_state(&root, "saya-couple", "base", &saved[0].sha256).unwrap();
    assert!(audit_store(&root).unwrap().is_empty());

    // A failing save aborts the boundary: no release is ever requested.
    let refused = RecordingManager::new();
    let mismatch = state("other-pipeline", "base", "wrong", &image);
    assert!(pass_boundary(&refused, &root, "saya-couple", "base", &[mismatch]).is_err());
    assert!(refused.recorded().is_empty());
    let oversized = NewState {
        pipeline: "saya-couple",
        pass: "base",
        producer_step: "too-big",
        kind: StateKind::Image,
        retention: Retention::Normal,
        bytes: &vec![0u8; 2 * 1024 * 1024],
        max_state_bytes: 128 * 1024 * 1024,
    };
    assert!(pass_boundary(&refused, &root, "saya-couple", "base", &[oversized]).is_err());
    assert!(refused.recorded().is_empty());

    // The shipped no-op backend never claims a release.
    let noop = NoopPassMemoryManager;
    assert_eq!(noop.backend(), "noop");
    let released = noop.release_after_pass("saya-couple", "base").unwrap();
    let prepared = noop.prepare_pass("saya-couple", "refiner").unwrap();
    assert!(!released.released);
    assert!(!prepared.released);
    assert!(released.details.contains("aucune libération"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn database_derives_the_store_root_from_its_own_location() {
    let root = temp_root();
    std::fs::create_dir_all(&root).unwrap();
    let db = studio_core::Database::open(&root.join("db.sqlite3")).unwrap();
    let derived = db.pass_state_root().unwrap();
    assert_eq!(derived, root.join("pass-states"));
    assert!(derived.starts_with(&root));
    init_store(&derived, 1024 * 1024).unwrap();
    let image = png_bytes();
    let saved = save_state(&derived, &state("pipe", "pass", "s", &image)).unwrap();
    assert!(derived.join(&saved.file).exists());
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn duplicate_save_revalidates_file_kind_and_requested_pin() {
    // V12 audit regression (repros/g4): a deduplicated save used to return the
    // registered entry without checking its file or the requested retention,
    // so `pass_boundary` released memory over a missing state.
    let root = temp_root();
    let latent: Vec<u8> = b"synthetic opaque bytes; no actual latent".to_vec();
    init_store(&root, 1024 * 1024).unwrap();

    // Missing duplicate: error, and the boundary never releases.
    let saved = save_state(&root, &ephemeral("p", "base", "latent:a", &latent)).unwrap();
    std::fs::remove_file(file_path(&root, &saved.file)).unwrap();
    assert!(save_state(&root, &ephemeral("p", "base", "latent:a", &latent)).is_err());
    let manager = RecordingManager::new();
    assert!(pass_boundary(
        &manager,
        &root,
        "p",
        "base",
        &[ephemeral("p", "base", "latent:a", &latent)]
    )
    .is_err());
    assert!(manager.recorded().is_empty());

    // Corrupt duplicate: same refusal, nothing overwritten.
    let other: Vec<u8> = b"second synthetic state".to_vec();
    let saved = save_state(&root, &ephemeral("p", "base", "latent:b", &other)).unwrap();
    std::fs::write(file_path(&root, &saved.file), b"tampered bytes here!!!").unwrap();
    assert!(save_state(&root, &ephemeral("p", "base", "latent:b", &other)).is_err());
    assert_eq!(
        std::fs::read(file_path(&root, &saved.file)).unwrap(),
        b"tampered bytes here!!!"
    );

    // Kind mismatch on identical bytes is refused explicitly.
    let shared: Vec<u8> = b"same bytes, different declared kind".to_vec();
    save_state(&root, &state("p", "refiner", "latent:c", &shared)).unwrap();
    assert!(save_state(&root, &state("p", "refiner", "image:c", &shared)).is_err());

    // Pin request on an ephemeral duplicate is promoted durably and survives reopen.
    let pin: Vec<u8> = b"state that must become resumable".to_vec();
    let first = save_state(&root, &ephemeral("p", "detail", "latent:d", &pin)).unwrap();
    assert_eq!(first.retention, Retention::Ephemeral);
    let promoted = save_state(&root, &pinned("p", "detail", "latent:d", &pin)).unwrap();
    assert_eq!(promoted.retention, Retention::Pinned);
    open_store(&root).unwrap();
    assert_eq!(
        verify_state(&root, "p", "detail", &first.sha256)
            .unwrap()
            .retention,
        Retention::Pinned
    );
    // An ephemeral duplicate never downgrades an existing pin.
    let again = save_state(&root, &ephemeral("p", "detail", "latent:d", &pin)).unwrap();
    assert_eq!(again.retention, Retention::Pinned);
    assert_eq!(
        verify_state(&root, "p", "detail", &first.sha256)
            .unwrap()
            .retention,
        Retention::Pinned
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn safetensors_latent() -> Vec<u8> {
    let header = br#"{"__metadata__":{"format":"pt"},"samples":{"dtype":"F32","shape":[1,4,8,8],"data_offsets":[0,1024]}}"#;
    let mut bytes = (header.len() as u64).to_le_bytes().to_vec();
    bytes.extend_from_slice(header);
    bytes.extend(std::iter::repeat_n(0u8, 1024));
    bytes
}

#[test]
fn latent_state_is_a_real_safetensors_container_not_converted() {
    let root = temp_root();
    init_store(&root, 16 * 1024 * 1024).unwrap();
    let bytes = safetensors_latent();
    let header_len = u64::from_le_bytes(bytes[..8].try_into().unwrap()) as usize;
    let header: Value = serde_json::from_slice(&bytes[8..8 + header_len]).unwrap();
    assert_eq!(header["samples"]["dtype"], "F32");
    assert_eq!(header["samples"]["shape"], json!([1, 4, 8, 8]));
    let entry = save_state(
        &root,
        &state("saya-couple", "latent-pass", "latent:samples", &bytes),
    )
    .unwrap();
    assert!(entry.file.ends_with(".safetensors"));
    assert_eq!(entry.kind, StateKind::Latent);
    verify_state(&root, "saya-couple", "latent-pass", &entry.sha256).unwrap();
    let disk = std::fs::read(root.join(&entry.file)).unwrap();
    assert_eq!(disk, bytes, "latent bytes must be preserved exactly");
    std::fs::remove_dir_all(root).unwrap();
}
