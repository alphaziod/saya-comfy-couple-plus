//! Optional CPU-only semantic image search.
//!
//! Reuses the already-installed ComfyUI Python venv and local CLIP ViT-H/14
//! checkpoint through `scripts/f09-semantic-embed.py`. No network, no download,
//! no GPU. Vectors live in a separate table/version from the perceptual
//! `image_features` index; disabling or invalidating this index never touches
//! perceptual search.
use crate::{Database, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

pub const MODEL: &str = "clip-vit-h-14-comfyui";
pub const VERSION: u32 = 1;
pub const DIM: usize = 1024;
const BYTES: usize = DIM * 4;
const BATCH: u32 = 32;

fn configured(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}
fn python() -> Option<PathBuf> {
    configured("SAYA_SEMANTIC_PYTHON")
}
fn model() -> Option<PathBuf> {
    configured("SAYA_SEMANTIC_MODEL")
}
/// Explicit env, installed resource, then development checkout. No model or
/// Python path is hardcoded.
fn script() -> Option<PathBuf> {
    if let Some(path) = configured("SAYA_SEMANTIC_SCRIPT") {
        return Some(path);
    }
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("sidecars/f09-semantic-embed.py"));
            candidates.push(dir.join("../resources/sidecars/f09-semantic-embed.py"));
            candidates.push(dir.join("../share/saya-studio/sidecars/f09-semantic-embed.py"));
        }
    }
    #[cfg(debug_assertions)]
    candidates.push(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../sidecars/f09-semantic-embed.py"),
    );
    candidates.into_iter().find(|path| path.is_file())
}
pub fn available() -> bool {
    python().is_some() && model().is_some() && script().is_some()
}

/// Read-only startup diagnostics: paths, Python imports and worker state.
/// Never loads the CLIP model and never writes anything.
pub fn diagnose() -> Value {
    let mut errors: Vec<String> = Vec::new();
    let python_path = python();
    let model_path = model();
    let script_path = script();
    if python_path.is_none() {
        errors.push("SAYA_SEMANTIC_PYTHON absent ou non régulier".into());
    }
    if model_path.is_none() {
        errors.push("SAYA_SEMANTIC_MODEL absent ou non régulier".into());
    }
    if script_path.is_none() {
        errors.push("script sidecar absent".into());
    }
    let mut imports = Value::Null;
    if let Some(python) = &python_path {
        let code = "import json, torch, transformers, safetensors, PIL; print(json.dumps({'torch': torch.__version__, 'transformers': transformers.__version__, 'safetensors': safetensors.__version__, 'pillow': PIL.__version__}))";
        match Command::new(python).arg("-c").arg(code).output() {
            Ok(out) if out.status.success() => {
                imports = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
            }
            Ok(out) => errors.push(format!(
                "imports Python échoués : {}",
                String::from_utf8_lossy(&out.stderr).trim()
            )),
            Err(error) => errors.push(format!("Python non exécutable : {error}")),
        }
    }
    json!({
        "ok": errors.is_empty(),
        "available": available(),
        "python": python_path.map(|p| p.display().to_string()),
        "model": model_path.map(|p| p.display().to_string()),
        "script": script_path.map(|p| p.display().to_string()),
        "imports": imports,
        "errors": errors,
        "sidecar": sidecar_status(),
    })
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    pid: u32,
    key: String,
    last_used: Instant,
}

#[derive(Default)]
struct WorkerManager {
    worker: Option<Worker>,
    launches: u64,
    crashes: u64,
    last_exit: Option<String>,
    last_rss: Option<u64>,
}

const POISONED: &str = "Verrou du sidecar sémantique empoisonné (panic précédente)";

static WORKER: OnceLock<Mutex<WorkerManager>> = OnceLock::new();
fn manager() -> &'static Mutex<WorkerManager> {
    WORKER.get_or_init(|| Mutex::new(WorkerManager::default()))
}

fn idle_seconds() -> f64 {
    std::env::var("SAYA_SEMANTIC_IDLE_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120.0)
}

fn config_key() -> String {
    format!(
        "{}|{}|{}",
        python()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        model().map(|p| p.display().to_string()).unwrap_or_default(),
        script()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    )
}

fn spawn_worker(state: &mut WorkerManager) -> Result<()> {
    if let Some(mut old) = state.worker.take() {
        let _ = old.child.kill();
        let _ = old.child.wait();
    }
    let python = python().ok_or("SAYA_SEMANTIC_PYTHON absent")?;
    let script = script().ok_or("Script sémantique introuvable")?;
    let model = model().ok_or("SAYA_SEMANTIC_MODEL absent")?;
    let mut child = Command::new(&python)
        .arg(&script)
        .arg("--serve")
        .arg("--model")
        .arg(&model)
        .arg("--threads")
        .arg("4")
        .arg("--batch-size")
        .arg("16")
        .arg("--idle-seconds")
        .arg(idle_seconds().to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Démarrage sidecar sémantique échoué : {error}"))?;
    let stdin = child.stdin.take().ok_or("stdin sidecar absent")?;
    let stdout = child.stdout.take().ok_or("stdout sidecar absent")?;
    let pid = child.id();
    state.worker = Some(Worker {
        child,
        stdin,
        stdout: BufReader::new(stdout),
        pid,
        key: config_key(),
        last_used: Instant::now(),
    });
    state.launches += 1;
    Ok(())
}

fn stop_worker(state: &mut WorkerManager, reason: &str) {
    if let Some(mut worker) = state.worker.take() {
        if let Some(rss) = crate::resources::process_rss(worker.pid) {
            state.last_rss = Some(rss);
        }
        let _ = worker.child.kill();
        let _ = worker.child.wait();
    }
    state.last_exit = Some(reason.to_owned());
}

fn sidecar_status() -> Value {
    // A poisoned lock (worker panicked) is reported instead of panicking every caller.
    let Ok(mut state) = manager().lock() else {
        return json!({"resident": false, "error": POISONED});
    };
    let mut resident = false;
    let mut pid = None;
    let mut rss = None;
    if state
        .worker
        .as_ref()
        .is_some_and(|worker| worker.last_used.elapsed().as_secs_f64() >= idle_seconds())
    {
        stop_worker(&mut state, "inactivité");
    }
    if let Some(worker) = state.worker.as_mut() {
        match worker.child.try_wait() {
            Ok(Some(status)) => {
                state.last_exit = Some(format!("arrêt inattendu: {status}"));
                state.crashes += 1;
                state.worker = None;
            }
            Ok(None) => {
                resident = true;
                pid = Some(worker.pid);
                rss = crate::resources::process_rss(worker.pid);
                if rss.is_some() {
                    state.last_rss = rss;
                }
            }
            Err(error) => {
                state.last_exit = Some(format!("état sidecar illisible: {error}"));
                state.worker = None;
            }
        }
    }
    json!({
        "resident": resident,
        "pid": pid,
        "rss": rss,
        "lastRss": state.last_rss,
        "launches": state.launches,
        "crashes": state.crashes,
        "lastExit": state.last_exit,
        "idleSeconds": idle_seconds(),
    })
}

fn sidecar_embed(paths: &[String]) -> Result<Value> {
    for attempt in 0..2 {
        let request = json!({"paths": paths});
        let line = serde_json::to_string(&request)? + "\n";
        let mut state = manager().lock().map_err(|_| POISONED)?;
        if state
            .worker
            .as_ref()
            .is_some_and(|worker| worker.key != config_key())
        {
            stop_worker(&mut state, "configuration changée");
        }
        if state
            .worker
            .as_ref()
            .is_some_and(|worker| worker.last_used.elapsed().as_secs_f64() >= idle_seconds())
        {
            stop_worker(&mut state, "inactivité");
        }
        if state.worker.is_none() {
            spawn_worker(&mut state)?;
        }
        let Some(worker) = state.worker.as_mut() else {
            return Err("Sidecar sémantique absent".into());
        };
        let result = (|| -> Result<Value> {
            worker.stdin.write_all(line.as_bytes())?;
            worker.stdin.flush()?;
            let mut response = String::new();
            let read = worker.stdout.read_line(&mut response)?;
            if read == 0 {
                return Err("Sidecar sémantique terminé".into());
            }
            Ok(serde_json::from_str(&response)?)
        })();
        if result.is_ok() {
            if let Some(worker) = state.worker.as_mut() {
                worker.last_used = Instant::now();
            }
        }
        match result {
            Ok(value) => return Ok(value),
            Err(error) => {
                if attempt == 0 {
                    stop_worker(&mut state, &format!("crash: {error}"));
                    state.crashes += 1;
                } else {
                    return Err(format!("Sidecar sémantique indisponible : {error}").into());
                }
            }
        }
    }
    Err("Sidecar sémantique indisponible".into())
}

fn encode(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(BYTES);
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn decode(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() != BYTES {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            // Infallible: chunks_exact yields 4-byte slices only.
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect(),
    )
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        (dot / (na * nb)).clamp(0.0, 1.0)
    }
}

impl Database {
    fn semantic_pending_rows(&self, limit: u32) -> Result<Vec<(String, String)>> {
        let mut query = self.connection.prepare(
            "SELECT a.id,a.absolute_path FROM assets a JOIN libraries l ON l.id=a.library_id
             WHERE a.availability='present' AND l.enabled=1
               AND NOT EXISTS(SELECT 1 FROM image_semantic_features f WHERE f.asset_id=a.id AND f.model=?1 AND f.version=?2)
             ORDER BY l.root_path,a.relative_path LIMIT ?3",
        )?;
        let rows = query
            .query_map(params![MODEL, VERSION, limit.min(256)], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn semantic_status(&self) -> Result<Value> {
        let indexed: u32 = self.connection.query_row(
            "SELECT count(*) FROM image_semantic_features WHERE model=?1 AND version=?2",
            params![MODEL, VERSION],
            |row| row.get(0),
        )?;
        let pending = self.semantic_pending_rows(u32::MAX)?.len();
        Ok(json!({
            "available": available(), "model": MODEL, "version": VERSION, "dim": DIM,
            "indexed": indexed, "pending": pending,
            "python": python().map(|p| p.display().to_string()),
            "checkpoint": model().map(|p| p.display().to_string()),
            "script": script().map(|p| p.display().to_string()),
            "sidecar": sidecar_status(),
        }))
    }

    /// Index at most one small batch. Repeated calls are incremental.
    pub fn semantic_index(&self, budget: u32) -> Result<Value> {
        if !available() {
            return Err(
                "Recherche sémantique indisponible : venv, checkpoint ou script local absent"
                    .into(),
            );
        }
        let rows = self.semantic_pending_rows(budget.clamp(1, BATCH))?;
        if rows.is_empty() {
            return Ok(json!({"embedded": 0, "failed": 0, "remaining": 0, "errors": []}));
        }
        let paths: Vec<String> = rows.iter().map(|(_, path)| path.clone()).collect();
        let report = sidecar_embed(&paths)?;
        let vectors = report["vectors"].as_object().cloned().unwrap_or_default();
        let errors = report["errors"].as_object().cloned().unwrap_or_default();
        let tx = self.connection.unchecked_transaction()?;
        let mut embedded = 0u32;
        for (id, path) in &rows {
            let Some(values) = vectors.get(path).and_then(Value::as_array) else {
                continue;
            };
            let vector: Vec<f32> = values
                .iter()
                .filter_map(Value::as_f64)
                .map(|v| v as f32)
                .collect();
            if vector.len() != DIM || vector.iter().any(|v| !v.is_finite()) {
                continue;
            }
            tx.execute(
                "INSERT INTO image_semantic_features(asset_id,model,version,dim,vector)
                 VALUES(?1,?2,?3,?4,?5)
                 ON CONFLICT(asset_id) DO UPDATE SET model=excluded.model,version=excluded.version,dim=excluded.dim,vector=excluded.vector,created_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
                params![id, MODEL, VERSION, DIM as u32, encode(&vector)],
            )?;
            embedded += 1;
        }
        tx.commit()?;
        let remaining = self.semantic_pending_rows(u32::MAX)?.len();
        Ok(json!({
            "embedded": embedded,
            "failed": errors.len(),
            "remaining": remaining,
            "errors": errors,
            "timings": report["timings"],
            "sidecar": sidecar_status(),
        }))
    }

    pub fn semantic_invalidate(&self) -> Result<u64> {
        Ok(self
            .connection
            .execute("DELETE FROM image_semantic_features", [])? as u64)
    }

    pub fn semantic_search(&self, asset_id: &str, limit: u32) -> Result<Value> {
        let stored: Option<Vec<u8>> = self.connection
            .query_row(
                "SELECT vector FROM image_semantic_features WHERE asset_id=?1 AND model=?2 AND version=?3",
                params![asset_id, MODEL, VERSION],
                |row| row.get(0),
            )
            .optional()?;
        let Some(query) = stored.as_deref().and_then(decode) else {
            return Err("Image sans embedding sémantique : lancez l'indexation CLIP".into());
        };
        let mut statement = self.connection.prepare(
            "SELECT f.vector,a.id,a.relative_path FROM image_semantic_features f
             JOIN assets a ON a.id=f.asset_id JOIN libraries l ON l.id=a.library_id
             WHERE f.model=?1 AND f.version=?2 AND a.availability='present'
               AND l.enabled=1 AND a.id!=?3",
        )?;
        let rows = statement
            .query_map(params![MODEL, VERSION, asset_id], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut scored: Vec<(f32, String, String)> = rows
            .into_iter()
            .filter_map(|(bytes, id, path)| {
                decode(&bytes).map(|vector| (cosine(&query, &vector), id, path))
            })
            .collect();
        scored.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
        });
        let matches: Vec<Value> = scored
            .into_iter()
            .take(limit.clamp(1, 96) as usize)
            .map(|(score, id, path)| json!({"id": id, "path": path, "score": score}))
            .collect();
        Ok(json!({
            "model": MODEL, "version": VERSION, "dim": DIM,
            "query": asset_id, "matches": matches,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vector_roundtrip_and_cosine_are_exact() {
        let v = vec![0.1f32; DIM];
        assert_eq!(decode(&encode(&v)).unwrap(), v);
        assert_eq!(decode(&[0u8; 4]), None);
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-5);
        let w: Vec<f32> = v.iter().map(|x| -x).collect();
        assert_eq!(cosine(&v, &w), 0.0);
    }
}
