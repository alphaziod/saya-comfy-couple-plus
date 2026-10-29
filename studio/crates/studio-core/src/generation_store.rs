//! Saya-owned generation store: one directory per character/timestamp, one
//! manifest and verified pass files. ComfyUI remains the executor; the app keeps
//! the authoritative pass state needed to resume without re-running a pass.
use crate::{gallery::digest, Result};
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};

const FORMAT: &str = "saya-generation";
const VERSION: u32 = 1;

fn safe(value: &str, what: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 120
        || value
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '\0'))
        || value == "."
        || value == ".."
    {
        return Err(format!("{what} invalide").into());
    }
    Ok(value.to_owned())
}

pub fn create(root: &Path, character: &str, timestamp: &str) -> Result<(PathBuf, Value)> {
    if !root.is_absolute() {
        return Err("Racine de générations absolue requise".into());
    }
    let character = safe(character, "Personnage")?;
    let timestamp = safe(timestamp, "Horodatage")?;
    let dir = root.join(&character).join(&timestamp);
    if dir.exists() {
        return Err("Dossier de génération déjà existant : aucun écrasement".into());
    }
    std::fs::create_dir_all(dir.join("passes"))?;
    let manifest = json!({
        "format": FORMAT, "version": VERSION, "character": character, "timestamp": timestamp,
        "state": "running", "passes": [],
    });
    std::fs::write(dir.join("manifest.json"), manifest.to_string())?;
    Ok((dir, manifest))
}

pub fn record_pass(run: &Path, pass: &str, kind: &str, bytes: &[u8], meta: Value) -> Result<Value> {
    let pass = safe(pass, "Passe")?;
    if !matches!(kind, "image" | "latent") {
        return Err("Type de passe inconnu".into());
    }
    let dir = run.join("passes").join(&pass);
    if dir.exists() {
        return Err("Passe déjà enregistrée : pas de réécriture".into());
    }
    std::fs::create_dir_all(&dir)?;
    let ext = if kind == "image" {
        "png"
    } else {
        "safetensors"
    };
    let name = format!("state.{ext}");
    let sha = digest(bytes);
    crate::fsutil::write_atomic(&dir.join(&name), bytes)?;
    let entry = json!({"pass":pass,"kind":kind,"file":format!("passes/{pass}/{name}"),"sha256":sha,"bytes":bytes.len(),"meta":meta});
    let manifest_path = run.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
    if manifest["format"] != FORMAT || manifest["version"] != VERSION {
        return Err("Manifeste de génération inconnu".into());
    }
    manifest["passes"]
        .as_array_mut()
        .ok_or("Manifeste invalide")?
        .push(entry.clone());
    crate::fsutil::write_atomic(&manifest_path, manifest.to_string().as_bytes())?;
    Ok(entry)
}

pub fn load_pass(run: &Path, pass: &str, expected_sha: &str) -> Result<Vec<u8>> {
    let pass = safe(pass, "Passe")?;
    let manifest: Value = serde_json::from_slice(&std::fs::read(run.join("manifest.json"))?)?;
    let entry = manifest["passes"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["pass"] == pass)
        .ok_or("Passe absente du manifeste")?;
    if entry["sha256"] != expected_sha {
        return Err("Empreinte de passe incohérente".into());
    }
    let file = entry["file"].as_str().ok_or("Fichier de passe absent")?;
    let relative = Path::new(file);
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("Chemin de passe non sûr".into());
    }
    let bytes = std::fs::read(run.join(relative))?;
    if digest(&bytes) != expected_sha {
        return Err("État de passe corrompu".into());
    }
    Ok(bytes)
}

pub fn update_metadata(run: &Path, metadata: &Value) -> Result<()> {
    let path = run.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    manifest["metadata"] = metadata.clone();
    crate::fsutil::write_atomic(&path, manifest.to_string().as_bytes())?;
    Ok(())
}

pub fn status(run: &Path) -> Result<Value> {
    let manifest: Value = serde_json::from_slice(&std::fs::read(run.join("manifest.json"))?)?;
    Ok(
        json!({"format":manifest["format"],"character":manifest["character"],"timestamp":manifest["timestamp"],
        "passes":manifest["passes"].as_array().map_or(0,Vec::len)}),
    )
}
