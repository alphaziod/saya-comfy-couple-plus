//! T2 proof (real runtime, CPU only): the full cycle
//! `engine_sandbox_edit` — dry-run → apply in the CLI sandbox →
//! `test quick` green → clean `rollback` — leaves the live file
//! `data/actions/` byte-identical (sha256 before = after) and the sandbox receives
//! then cancels the modification. No GPU, no request: solo_cli.py in
//! a subprocess, PYTHONHASHSEED=0.
//!
//! Requires the real engine: `GENEREZ_ROOT` (default `generez::default_root()`,
//! i.e. `SAYA_ENGINE_DIR` or the configured engine directory).

use std::path::PathBuf;
use studio_core::generez;

#[test]
#[ignore = "moteur GROK BUILD réel : lancer explicitement sur copie isolée"]
fn sandbox_edit_cycle_keeps_live_file_identical_and_cancels_sandbox_change() {
    let root = std::env::var_os("GENEREZ_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(generez::default_root);
    let cli = root.join("solo_cli.py");
    assert!(
        cli.is_file(),
        "moteur generez introuvable : {} (régler GENEREZ_ROOT)",
        cli.display()
    );
    assert!(
        root.join("data").join("actions").is_dir(),
        "data/actions introuvable sous {}",
        root.display()
    );

    // Harmless schema patch (`note` is a validated field; the required `add` stays
    // carried by the existing action).
    let patch = serde_json::json!({ "note": "preuve T2 studio-core — sandbox, rollback" });

    let report =
        generez::engine_sandbox_edit(&root, "seins", &patch).expect("cycle engine_sandbox_edit");

    assert_eq!(
        report["dry_run_ok"],
        serde_json::Value::Bool(true),
        "{report}"
    );
    assert_eq!(
        report["test_quick_ok"],
        serde_json::Value::Bool(true),
        "{report}"
    );
    assert!(
        !report["rollback_restored"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "rollback sans sauvegarde restaurée : {report}"
    );

    // The live file is identical before/after the full cycle.
    assert_eq!(
        report["live_unchanged"],
        serde_json::Value::Bool(true),
        "{report}"
    );
    assert_eq!(
        report["live_sha256_before"], report["live_sha256_after"],
        "sha256 du fichier vivant avant/après diffèrent : {report}"
    );

    // The sandbox did receive the modification (apply changed the byte)…
    assert_eq!(
        report["sandbox_received"],
        serde_json::Value::Bool(true),
        "{report}"
    );
    assert_ne!(
        report["sandbox_sha256_before"], report["sandbox_sha256_after_apply"],
        "apply sans effet dans la sandbox : {report}"
    );
    // … then cancelled it (rollback = original content).
    assert_eq!(
        report["sandbox_cancelled"],
        serde_json::Value::Bool(true),
        "{report}"
    );
    assert_eq!(
        report["sandbox_sha256_before"], report["sandbox_sha256_after_rollback"],
        "sandbox non restaurée après rollback : {report}"
    );
    // The sandbox is a faithful copy of the live one at the start.
    assert_eq!(
        report["sandbox_sha256_before"], report["live_sha256_before"],
        "copie sandbox ≠ vivant au départ : {report}"
    );

    // Independent re-read at time t: the live file is still
    // the one in the report (no deferred write).
    let live_file = PathBuf::from(report["live_file"].as_str().expect("live_file"));
    let now = generez::sha256_file(&live_file).expect("relecture vivant");
    assert_eq!(
        now,
        report["live_sha256_after"].as_str().expect("sha256"),
        "fichier vivant modifié après le cycle : {report}"
    );
}
