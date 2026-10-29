//! Safe download commit primitives. Network/provider clients are deliberately
//! absent in this tranche: a caller must supply an already staged file, an
//! explicit confirmation, a destination and (when known) an expected digest.
//! Files are never overwritten and `.pt/.pth` are treated as opaque bytes.
use crate::{gallery::digest, Result};
use serde_json::{json, Value};
use std::path::Path;

pub fn commit_download(
    staging: &Path,
    destination: &Path,
    expected_sha256: Option<&str>,
    expected_bytes: Option<u64>,
    confirmed: bool,
) -> Result<Value> {
    if !confirmed {
        return Err("Confirmation explicite requise avant installation".into());
    }
    let meta = std::fs::symlink_metadata(staging)?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err("Fichier staging absent ou symbolique".into());
    }
    if meta.len() > 8 * 1024 * 1024 * 1024 {
        return Err("Téléchargement trop volumineux".into());
    }
    if expected_bytes.is_some_and(|size| size != meta.len()) {
        return Err("Taille téléchargée différente de la taille annoncée".into());
    }
    let bytes = std::fs::read(staging)?;
    let sha = digest(&bytes);
    if expected_sha256.is_some_and(|expected| expected != sha) {
        return Err("Empreinte SHA-256 du téléchargement invalide".into());
    }
    if destination.exists() {
        return Err("Destination existante : aucun écrasement silencieux".into());
    }
    let parent = destination
        .parent()
        .ok_or("Destination sans dossier parent")?;
    if !parent.is_dir() {
        return Err("Dossier de destination absent".into());
    }
    crate::fsutil::write_atomic(destination, &bytes)?;
    Ok(json!({"destination":destination.to_string_lossy(),"sha256":sha,"bytes":bytes.len()}))
}
