//! Read-only LoRA inventory. The caller supplies an explicit directory; no
//! ComfyUI path is hardcoded, no file is modified and nothing is downloaded.
use crate::Result;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const MAX_ENTRIES: usize = 5_000;
const MAX_DEPTH: usize = 6;

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<()> {
    if depth > MAX_DEPTH || out.len() >= MAX_ENTRIES {
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if out.len() >= MAX_ENTRIES {
            break;
        }
        let meta = entry.metadata()?;
        if meta.file_type().is_symlink() {
            continue;
        }
        let path = entry.path();
        if meta.is_dir() {
            walk(&path, depth + 1, out)?;
        } else if meta.is_file()
            && path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                matches!(
                    e.to_ascii_lowercase().as_str(),
                    "safetensors" | "pt" | "pth"
                )
            })
        {
            out.push(path);
        }
    }
    Ok(())
}

pub fn inventory(directory: &str) -> Result<Value> {
    let root = Path::new(directory);
    if !root.is_dir() {
        return Err("Dossier LoRA invalide".into());
    }
    let root = root.canonicalize()?;
    let mut paths = Vec::new();
    walk(&root, 0, &mut paths)?;
    let mut items = Vec::new();
    for path in &paths {
        let meta = std::fs::metadata(path)?;
        let relative = path.strip_prefix(&root).unwrap_or(path);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("LoRA");
        let preview = path.with_extension("png");
        let sidecar = path.with_extension("json");
        let owned_sidecar = path.with_extension("metadata.json");
        let metadata =
            if owned_sidecar.is_file() && std::fs::metadata(&owned_sidecar)?.len() <= 262_144 {
                std::fs::read(&owned_sidecar)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            } else if sidecar.is_file() && std::fs::metadata(&sidecar)?.len() <= 262_144 {
                std::fs::read(&sidecar)
                    .ok()
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            } else {
                None
            };
        items.push(json!({
            "name": stem, "relativePath": relative.to_string_lossy(), "extension": path.extension().and_then(|e| e.to_str()).unwrap_or(""),
            "bytes": meta.len(), "preview": preview.is_file().then(|| preview.to_string_lossy().to_string()),
            "metadata": metadata,
        }));
    }
    items.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(
        json!({"root":root.to_string_lossy(),"count":items.len(),"truncated":items.len()>=MAX_ENTRIES,"items":items}),
    )
}

/// Small preview image as a data URL. Read-only, bounded, and restricted to
/// regular PNG files; no directory is scanned or written.
pub fn cover(path: &str) -> Result<String> {
    use base64::Engine;
    let path = Path::new(path);
    if !path.is_absolute()
        || !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("png"))
    {
        return Err("Aperçu LoRA PNG absolu requis".into());
    }
    let meta = std::fs::metadata(path)?;
    if !meta.is_file() || meta.len() > 8 * 1024 * 1024 {
        return Err("Aperçu LoRA invalide ou trop lourd".into());
    }
    let bytes = std::fs::read(path)?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

/// Persist Saya-owned LoRA metadata next to the model without touching the
/// model file or a pre-existing `.json` sidecar. Atomic replace of our own
/// `.metadata.json` only; a previous Saya metadata file is kept as `.bak`.
pub fn save_metadata(path: &str, metadata: &Value) -> Result<Value> {
    let path = Path::new(path);
    if !path.is_absolute()
        || !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("safetensors"))
    {
        return Err("Fichier LoRA .safetensors absolu requis".into());
    }
    if !metadata.is_object() || metadata.to_string().len() > 64 * 1024 {
        return Err("Métadonnées LoRA invalides".into());
    }
    let sidecar = path.with_extension("metadata.json");
    if sidecar.is_file() {
        std::fs::copy(&sidecar, sidecar.with_extension("metadata.json.bak"))?;
    }
    crate::fsutil::write_atomic(&sidecar, &serde_json::to_vec_pretty(metadata)?)?;
    Ok(json!({"path":sidecar.to_string_lossy()}))
}
