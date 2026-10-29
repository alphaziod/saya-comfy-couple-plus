// Non-destructive image editor: loading the original (read-only) and
// export of the retouched canvas to a NEW file. The blur algorithm lives
// on the interface side; Rust only reads the source and writes the destination.

use crate::{Database, Result};
use base64::Engine;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::io::Cursor;
use std::path::Path;

const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_DIMENSION: u32 = 16384;

fn mime(format: image::ImageFormat) -> Option<&'static str> {
    match format {
        image::ImageFormat::Png => Some("image/png"),
        image::ImageFormat::Jpeg => Some("image/jpeg"),
        image::ImageFormat::WebP => Some("image/webp"),
        _ => None,
    }
}

// Returns the full original as a data URL, without re-encoding or modifying the file.
pub fn load_original(path: &Path) -> Result<Value> {
    let metadata =
        std::fs::metadata(path).map_err(|_| format!("Fichier introuvable : {}", path.display()))?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err("Image supérieure à 128 Mio".into());
    }
    let reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let format = reader
        .format()
        .ok_or("Format de fichier non reconnu comme une image")?;
    let mime = mime(format)
        .ok_or("Format non pris en charge par l'éditeur (PNG, JPEG ou WebP attendus)")?;
    let (width, height) = reader.into_dimensions()?;
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(format!("Dimensions non prises en charge : {width}×{height}").into());
    }
    let bytes = std::fs::read(path)?;
    Ok(json!({
        "path": path,
        "width": width,
        "height": height,
        "mime": mime,
        "data": format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)),
    }))
}

// Writes the exported canvas (PNG or JPEG) to a file that does not exist yet.
// Refuses: source not found, existing destination, destination == source,
// unreadable content. The original is never opened for writing.
pub fn save_export(
    source: &Path,
    destination: &Path,
    format: &str,
    image_base64: &str,
) -> Result<String> {
    let source = source
        .canonicalize()
        .map_err(|_| format!("Fichier source introuvable : {}", source.display()))?;
    if destination.exists() {
        return Err(format!(
            "La destination existe déjà, export refusé : {}",
            destination.display()
        )
        .into());
    }
    let parent = destination
        .parent()
        .ok_or("Chemin de destination invalide")?;
    if !parent.is_dir() {
        return Err(format!("Dossier de destination absent : {}", parent.display()).into());
    }
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.starts_with('.'))
        .ok_or("Nom de fichier de destination invalide")?;
    let destination = parent.canonicalize()?.join(name);
    if destination == source {
        return Err("La destination est identique au fichier source : export refusé.".into());
    }
    let expected = match format {
        "png" => image::ImageFormat::Png,
        "jpeg" => image::ImageFormat::Jpeg,
        other => return Err(format!("Format d'export inconnu : {other}").into()),
    };
    let bytes = base64::engine::general_purpose::STANDARD.decode(image_base64)?;
    let mut reader = image::ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
    reader.limits({
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_DIMENSION);
        limits.max_image_height = Some(MAX_DIMENSION);
        limits.max_alloc = Some(512 * 1024 * 1024);
        limits
    });
    if reader.format() != Some(expected) {
        return Err("Le canevas reçu ne correspond pas au format d'export demandé.".into());
    }
    let decoded = reader.decode()?;
    if decoded.width() == 0 || decoded.height() == 0 {
        return Err("Canevas exporté vide.".into());
    }
    // create_new guarantees no overwrite, even against a race.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)?;
    std::io::Write::write_all(&mut file, &bytes)?;
    Ok(destination.display().to_string())
}

/// Validated stroke document persistence. The source image is never touched:
/// this table stores only the non-destructive stroke list.
pub fn validate_blur_document(value: &Value) -> Result<()> {
    if value["version"] != 1 {
        return Err("Version de document de flou non prise en charge".into());
    }
    let strokes = value["strokes"]
        .as_array()
        .ok_or("Liste de tracés absente")?;
    if strokes.len() > 10_000 {
        return Err("Trop de tracés de flou".into());
    }
    for stroke in strokes {
        let id = stroke["id"].as_str().ok_or("Identifiant de tracé absent")?;
        if id.is_empty() || id.len() > 120 {
            return Err("Identifiant de tracé invalide".into());
        }
        let number = |key: &str| {
            stroke[key]
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("{key} de tracé invalide"))
        };
        let size = number("size")?;
        let intensity = number("intensity")?;
        let hardness = number("hardness")?;
        if !(1.0..=2048.0).contains(&size)
            || !(0.0..=1.0).contains(&intensity)
            || !(0.0..=1.0).contains(&hardness)
        {
            return Err("Paramètres de tracé hors bornes".into());
        }
        let points = stroke["points"]
            .as_array()
            .ok_or("Points de tracé absents")?;
        if points.is_empty() || points.len() > 100_000 {
            return Err("Nombre de points invalide".into());
        }
        for point in points {
            for key in ["x", "y"] {
                if !point[key].as_f64().is_some_and(|v| v.is_finite()) {
                    return Err("Point de tracé invalide".into());
                }
            }
        }
    }
    Ok(())
}

impl Database {
    pub fn blur_document(&self, asset: &str) -> Result<Value> {
        let exists: i64 =
            self.connection
                .query_row("SELECT count(*) FROM assets WHERE id=?", [asset], |r| {
                    r.get(0)
                })?;
        if exists == 0 {
            return Err("Asset inconnu".into());
        }
        let row: Option<(String, String, String)> = self
            .connection
            .query_row(
                "SELECT document_json,document_sha256,updated_at FROM image_blur_documents WHERE asset_id=?",
                [asset],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        Ok(match row {
            None => json!({"document":null,"sha256":null,"updatedAt":null}),
            Some((text, sha, updated)) => json!({
                "document": serde_json::from_str::<Value>(&text)?,
                "sha256": sha,
                "updatedAt": updated,
            }),
        })
    }

    pub fn save_blur_document(&self, asset: &str, document: &Value) -> Result<Value> {
        validate_blur_document(document)?;
        let exists: i64 =
            self.connection
                .query_row("SELECT count(*) FROM assets WHERE id=?", [asset], |r| {
                    r.get(0)
                })?;
        if exists == 0 {
            return Err("Asset inconnu".into());
        }
        let text = serde_json::to_string(document)?;
        let sha = crate::gallery::digest(text.as_bytes());
        self.connection.execute(
            "INSERT INTO image_blur_documents(asset_id,document_json,document_sha256,updated_at)
             VALUES(?1,?2,?3,strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             ON CONFLICT(asset_id) DO UPDATE SET document_json=excluded.document_json,
             document_sha256=excluded.document_sha256,updated_at=excluded.updated_at",
            params![asset, text, sha],
        )?;
        Ok(json!({"sha256":sha,"strokes":document["strokes"].as_array().map_or(0,Vec::len)}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn temp_directory(name: &str) -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("studio-editor-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(width, height)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    #[test]
    fn load_original_returns_full_image_and_refuses_unknown_formats() {
        let directory = temp_directory("load");
        let path = directory.join("photo.png");
        std::fs::write(&path, png_bytes(3, 2)).unwrap();
        let loaded = load_original(&path).unwrap();
        assert_eq!(loaded["width"], 3);
        assert_eq!(loaded["height"], 2);
        assert_eq!(loaded["mime"], "image/png");
        assert!(loaded["data"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        let text = directory.join("text.png");
        std::fs::write(&text, "ceci n'est pas une image").unwrap();
        assert!(load_original(&text).is_err());
        assert!(load_original(&directory.join("absent.png")).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn export_writes_new_file_validates_content_and_keeps_source_intact() {
        let directory = temp_directory("export");
        let source = directory.join("source.png");
        let original = png_bytes(4, 3);
        std::fs::write(&source, &original).unwrap();
        let destination = directory.join("source retouche.png");
        let base64 = base64::engine::general_purpose::STANDARD.encode(png_bytes(4, 3));
        let saved = save_export(&source, &destination, "png", &base64).unwrap();
        assert_eq!(saved, destination.display().to_string());
        let written = std::fs::read(&destination).unwrap();
        let decoded = image::ImageReader::new(Cursor::new(&written))
            .with_guessed_format()
            .unwrap()
            .decode()
            .unwrap();
        assert_eq!((decoded.width(), decoded.height()), (4, 3));
        assert_eq!(std::fs::read(&source).unwrap(), original);
        // JPEG content announced as PNG: refused, nothing is written.
        let mut jpeg = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut jpeg, image::ImageFormat::Jpeg)
            .unwrap();
        let jpeg = base64::engine::general_purpose::STANDARD.encode(jpeg.into_inner());
        assert!(save_export(&source, &directory.join("wrong.png"), "png", &jpeg).is_err());
        assert!(!directory.join("wrong.png").exists());
        // Unreadable data: refused.
        assert!(save_export(
            &source,
            &directory.join("junk.png"),
            "png",
            "pas-du-base64!"
        )
        .is_err());
        assert!(save_export(&source, &directory.join("junk.png"), "png", "aGVsbG8=").is_err());
        assert!(!directory.join("junk.png").exists());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn export_refuses_existing_destination_and_destination_equal_to_source() {
        let directory = temp_directory("guards");
        let source = directory.join("source.png");
        std::fs::write(&source, png_bytes(2, 2)).unwrap();
        let base64 = base64::engine::general_purpose::STANDARD.encode(png_bytes(2, 2));
        assert!(save_export(&source, &source, "png", &base64).is_err());
        // Same file through a non-canonical path relative to the parent folder.
        assert!(save_export(
            &source,
            &directory.join(".").join("source.png"),
            "png",
            &base64
        )
        .is_err());
        let existing = directory.join("existant.png");
        std::fs::write(&existing, b"deja la").unwrap();
        assert!(save_export(&source, &existing, "png", &base64).is_err());
        assert_eq!(std::fs::read(&existing).unwrap(), b"deja la");
        assert!(save_export(
            &source,
            &directory.join("absent").join("out.png"),
            "png",
            &base64
        )
        .is_err());
        assert!(save_export(
            &directory.join("absent.png"),
            &directory.join("out.png"),
            "png",
            &base64
        )
        .is_err());
        assert!(save_export(&source, &directory.join("out.bmp"), "bmp", &base64).is_err());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
