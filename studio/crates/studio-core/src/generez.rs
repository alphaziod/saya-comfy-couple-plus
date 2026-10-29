//! Slice T2 — data editing through the generez engine's sandbox CLI.
//!
//! Absolute rule: the app NEVER writes a generez file directly. Each
//! step goes through `solo_cli.py` as a direct subprocess (no shell, so
//! the path with a space "GROK BUILD" is safe) with `PYTHONHASHSEED=0`.
//!
//! Write isolation is the CLI's data sandbox: `SOLO_DATA_DIR`
//! redirects `data/` to a copy (`solo_cmds/common.py:29`, documented in
//! `docs/GUIDE_ISOLATION.md`). The write steps (apply, rollback) and
//! `test quick` run only on this copy; the read steps
//! (show, dry-run) run on the live data and write nothing.
//!
//! Cycle: dry-run (read, live data) → apply in the sandbox →
//! green `test quick` → clean `rollback`. The returned report carries the before/after
//! sha256 proving that the live file stayed byte-identical and
//! that the sandbox received then reverted the change.
//!
//! Never touched: `deepseek/preuves/refactor/*` nor `reference/` (no step
//! below reads or writes these paths).

use crate::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Default generez engine root (path containing a space).
pub fn default_root() -> PathBuf {
    crate::paths::engine_dir()
}

const LIMIT: usize = 16_000_000;
const TIMEOUT: Duration = Duration::from_secs(120);

fn read_bounded(reader: impl Read) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::new();
    reader.take((LIMIT + 1) as u64).read_to_end(&mut data)?;
    Ok(data)
}

/// Hexadecimal sha256 of a file.
pub fn sha256_file(path: &Path) -> Result<String> {
    crate::fsutil::file_sha256(path)
        .map_err(|e| format!("sha256_file {}: {e}", path.display()).into())
}

/// Names never copied nor counted: CLI write backups and lock
/// (same exclusions as the engine's sandbox, `moteur/sandbox.py:29`).
fn skipped(name: &str) -> bool {
    name == "_backups" || name == ".solo.lock"
}

fn walk_files(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if skipped(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            walk_files(&path, out)?;
        } else if path.is_file() {
            out.push(path);
        }
    }
    Ok(())
}

/// Stable sha256 of a tree (sorted relative paths + contents).
pub fn sha256_tree(root: &Path) -> Result<String> {
    let mut files = Vec::new();
    walk_files(root, &mut files)?;
    files.sort();
    let mut hasher = Sha256::new();
    for path in files {
        let rel = path.strip_prefix(root)?;
        hasher.update(rel.to_string_lossy().as_bytes());
        hasher.update(b"\0");
        hasher.update(&fs::read(&path)?);
        hasher.update(b"\0");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Data copy for the sandbox (same exclusion as the engine sandbox).
fn copy_data_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        if skipped(&entry.file_name().to_string_lossy()) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_data_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// App working folder (patch + data sandbox), removed on Drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Result<Self> {
        let dir = std::env::temp_dir().join(format!(
            "studio-generez-t2-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs `python3 -B solo_cli.py <args> --json` (never a shell: the "GROK BUILD"
/// path with a space stays a single argv). Always PYTHONHASHSEED=0;
/// `SOLO_DATA_DIR` only for the sandbox steps, never inherited for the
/// steps on live data.
fn run_cli(cli: &Path, cwd: &Path, sandbox_data: Option<&Path>, args: &[&str]) -> Result<Value> {
    let mut command = Command::new("python3");
    command.arg("-B").arg(cli).args(args).arg("--json");
    command.current_dir(cwd);
    command.env("PYTHONHASHSEED", "0");
    // isolation: no inherited PYTHONPATH/PYTHONHOME, no leftover SOLO_DATA_DIR
    command
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONHOME")
        .env_remove("SOLO_DATA_DIR");
    if let Some(dir) = sandbox_data {
        command.env("SOLO_DATA_DIR", dir);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| format!("spawn solo_cli {args:?} : {e}"))?;
    let stdout = child.stdout.take().ok_or("stdout solo_cli absent")?;
    let stderr = child.stderr.take().ok_or("stderr solo_cli absent")?;
    let output = thread::spawn(move || read_bounded(stdout));
    let errors = thread::spawn(move || read_bounded(stderr));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < TIMEOUT => thread::sleep(Duration::from_millis(25)),
            other => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!(
                    "solo_cli {args:?} interrompu (délai 120 s ou erreur processus) : {other:?}"
                ));
            }
        }
    };
    let output = output
        .join()
        .map_err(|_| "Lecture solo_cli interrompue")??;
    let errors = errors
        .join()
        .map_err(|_| "Lecture solo_cli interrompue")??;
    let status = status?;
    if !status.success() {
        return Err(format!(
            "solo_cli {args:?} a échoué (code {:?}) : {}",
            status.code(),
            String::from_utf8_lossy(&errors[..errors.len().min(4000)])
        )
        .into());
    }
    if output.len() > LIMIT {
        return Err(format!("sortie solo_cli {args:?} supérieure à 16 Mo").into());
    }
    let value: Value = serde_json::from_slice(&output)
        .map_err(|e| format!("JSON solo_cli {args:?} invalide : {e}"))?;
    if value.get("ok") != Some(&Value::Bool(true)) {
        return Err(format!("solo_cli {args:?} a refusé : {}", value).into());
    }
    Ok(value)
}

/// Edits ONE action of `data/actions/` through the sandbox CLI, never writing
/// the live file.
///
/// `patch` is a partial JSON patch (fields of the schema `data/actions/schema.json`).
/// Returns the full report (sha256 before/after, steps, restoration);
/// fails if the live file changed, if the sandbox did not receive the
/// modification, if it was not reverted or if `test quick` is not green.
pub fn engine_sandbox_edit(root: &Path, action_id: &str, patch: &Value) -> Result<Value> {
    if !root.is_dir() {
        return Err(format!("racine moteur absente : {}", root.display()).into());
    }
    let root = root.canonicalize()?;
    let cli = root.join("solo_cli.py");
    if !cli.is_file() {
        return Err(format!("solo_cli.py introuvable : {}", cli.display()).into());
    }
    let data_root = root.join("data");
    let actions_root = data_root.join("actions");
    if !actions_root.is_dir() {
        return Err(format!("data/actions introuvable : {}", actions_root.display()).into());
    }
    let work = TempDir::new()?;
    let patch_file = work.path().join("patch.json");
    fs::write(&patch_file, serde_json::to_string_pretty(patch)?)?;
    let patch_arg = patch_file.to_string_lossy().into_owned();

    // 1) identification (read-only, live data): family file
    let show = run_cli(&cli, &root, None, &["action", "show", action_id])?;
    let rel = show["fichier"]
        .as_str()
        .ok_or_else(|| format!("action show {} sans fichier", action_id))?
        .to_string();
    let live_file = data_root.join(&rel);
    if !live_file.starts_with(&actions_root) {
        return Err(format!(
            "fichier annoncé hors data/actions : {}",
            live_file.display()
        )
        .into());
    }

    // live fingerprints BEFORE the cycle
    let live_before = sha256_file(&live_file)?;
    let tree_before = sha256_tree(&data_root)?;

    // 2) dry-run (read-only, live data): nothing written
    let dry = run_cli(
        &cli,
        &root,
        None,
        &[
            "action",
            "patch",
            action_id,
            "--file",
            &patch_arg,
            "--dry-run",
        ],
    )?;
    if dry["dry_run"] != Value::Bool(true) {
        return Err("dry-run non confirmé par la CLI".into());
    }

    // 3) data sandbox: copy, then CLI writes only
    let sandbox = work.path().join("sandbox-data");
    copy_data_dir(&data_root, &sandbox)?;
    let sandbox_file = sandbox.join(&rel);
    let sandbox_before = sha256_file(&sandbox_file)?;
    if sandbox_before != live_before {
        return Err("la copie sandbox ne correspond pas au fichier vivant".into());
    }

    // 4) apply in the sandbox
    let apply = run_cli(
        &cli,
        &root,
        Some(&sandbox),
        &[
            "action", "patch", action_id, "--file", &patch_arg, "--apply", "--yes",
        ],
    )?;
    let sandbox_after_apply = sha256_file(&sandbox_file)?;

    // 5) green test quick in the sandbox
    let quick = run_cli(&cli, &root, Some(&sandbox), &["test", "quick"])?;
    let test_quick_ok = quick["level"].as_str() == Some("quick");

    // 6) clean rollback (restores the backup created by the apply)
    let rollback = run_cli(&cli, &root, Some(&sandbox), &["rollback"])?;
    let restored = rollback["restored"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let sandbox_after_rollback = sha256_file(&sandbox_file)?;
    let tree_after = sha256_tree(&data_root)?;
    let live_after = sha256_file(&live_file)?;

    let live_unchanged = live_after == live_before;
    let received = sandbox_after_apply != sandbox_before;
    let cancelled = sandbox_after_rollback == sandbox_before;
    if !live_unchanged {
        return Err(format!(
            "FICHIER VIVANT MODIFIÉ par le cycle : {} {live_before} → {live_after}",
            live_file.display()
        )
        .into());
    }
    if !received {
        return Err(format!(
            "la sandbox n'a pas reçu la modification : {} identique après apply",
            sandbox_file.display()
        )
        .into());
    }
    if !cancelled {
        return Err(format!(
            "rollback incomplet : {} {sandbox_before} → {sandbox_after_rollback}",
            sandbox_file.display()
        )
        .into());
    }
    if !test_quick_ok {
        return Err("test quick non vert dans la sandbox".into());
    }

    Ok(json!({
        "ok": true,
        "action_id": action_id,
        "engine_root": root,
        "cli": cli,
        "fichier": rel,
        "live_file": live_file,
        "sandbox_data_dir": sandbox,
        "sandbox_file": sandbox_file,
        "dry_run_ok": true,
        "test_quick_ok": test_quick_ok,
        "rollback_restored": restored,
        "apply_backup": apply["backup"].clone(),
        "live_sha256_before": live_before,
        "live_sha256_after": live_after,
        "live_unchanged": live_unchanged,
        "data_tree_sha256_before": tree_before,
        "data_tree_sha256_after": tree_after,
        "sandbox_sha256_before": sandbox_before,
        "sandbox_sha256_after_apply": sandbox_after_apply,
        "sandbox_sha256_after_rollback": sandbox_after_rollback,
        "sandbox_received": received,
        "sandbox_cancelled": cancelled,
        "dry_run_diff": dry["diff"].clone(),
    }))
}
