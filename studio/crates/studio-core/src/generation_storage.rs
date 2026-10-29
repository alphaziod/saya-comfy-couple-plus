//! Planned user-visible generation layout: /image/<character>/<timestamp>/<image>.
//! Pure path builder: no directory or file is created here.
use crate::Result;
use std::path::{Path, PathBuf};

fn safe_component(value: &str, what: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > 100
        || trimmed
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':' | '\0'))
        || trimmed == "."
        || trimmed == ".."
    {
        return Err(format!("{what} invalide").into());
    }
    Ok(trimmed.to_owned())
}

pub fn generation_destination(
    root: &Path,
    character: &str,
    timestamp: &str,
    filename: &str,
) -> Result<PathBuf> {
    if !root.is_absolute() {
        return Err("Racine /image absolue requise".into());
    }
    let character = safe_component(character, "Nom de personnage")?;
    let timestamp = safe_component(timestamp, "Horodatage")?;
    let filename = safe_component(filename, "Nom d'image")?;
    if !filename.to_ascii_lowercase().ends_with(".png") {
        return Err("Image de sortie PNG attendue".into());
    }
    Ok(root.join(character).join(timestamp).join(filename))
}

/// Atomically publish a new final image without overwriting. Collisions get a
/// deterministic numeric suffix; the original is never replaced.
pub fn publish_final(
    root: &Path,
    character: &str,
    timestamp: &str,
    bytes: &[u8],
) -> Result<PathBuf> {
    if bytes.is_empty() {
        return Err("Image finale vide".into());
    }
    if !root.is_absolute() {
        return Err("Racine Images absolue requise".into());
    }
    let character = safe_component(character, "Personnage")?;
    let timestamp = safe_component(timestamp, "Horodatage")?;
    let dir = root.join("Saya Studio").join(&character).join(&timestamp);
    std::fs::create_dir_all(&dir)?;
    for n in 0..1000 {
        let name = if n == 0 {
            "image.png".to_owned()
        } else {
            format!("image-{n}.png")
        };
        let path = dir.join(name);
        if path.exists() {
            continue;
        }
        let temp = dir.join(format!(".{}.{}.tmp", uuid::Uuid::new_v4(), n));
        std::fs::write(&temp, bytes)?;
        if std::fs::rename(&temp, &path).is_err() {
            let _ = std::fs::remove_file(&temp);
            if path.exists() {
                continue;
            }
            return Err("Publication finale impossible".into());
        }
        return Ok(path);
    }
    Err("Trop de collisions d'image finale".into())
}
