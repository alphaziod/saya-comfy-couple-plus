//! App settings kept in the user's config folder (`paths::config_dir()`), next to the draw
//! engine and the workflows: `parametres/preferences.json` mirrors the lasting settings so the
//! folder alone is enough to set up the app on another machine. The database stays the source
//! of truth; on start, settings missing from the database are read back from the file.
//! Off until the application calls `enable()` at start (unit and integration tests never
//! touch the real folder), and never written while a test sandbox is active unless
//! `SAYA_CONFIG_DIR` points elsewhere.
use crate::{Database, Result};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Called once by the application at start.
pub fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}

/// Lasting settings worth carrying to another machine (not drafts, selections or journals).
pub const MIRRORED: [&str; 13] = [
    "language",
    "fileNamingLanguage",
    "libraryRoot",
    "captureRoot",
    "generationRoot",
    "loraRoot",
    "loraFavorites",
    "loraStacks",
    "modelFolderPaths",
    "promptEngineRoot",
    "presetBase",
    "comfyOutputDir",
    "galleryView",
];

fn settings_file(config: &Path) -> PathBuf {
    config.join("parametres").join("preferences.json")
}

/// The config folder to write in, or `None` inside a test sandbox without an explicit one.
fn writable_config() -> Option<PathBuf> {
    if !ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    let explicit = std::env::var_os("SAYA_CONFIG_DIR").is_some_and(|v| !v.is_empty());
    if crate::gallery::test_sandbox().is_some() && !explicit {
        return None;
    }
    Some(crate::paths::config_dir())
}

impl Database {
    /// Writes the mirrored settings to `<config>/parametres/preferences.json` (atomic).
    pub fn export_settings_to(&self, config: &Path) -> Result<()> {
        let all = self.preferences()?;
        let picked: Map<String, Value> = MIRRORED
            .iter()
            .filter_map(|k| all.get(*k).map(|v| ((*k).to_owned(), v.clone())))
            .collect();
        let file = settings_file(config);
        std::fs::create_dir_all(file.parent().ok_or("Chemin de réglages invalide")?)?;
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&Value::Object(picked))?)?;
        std::fs::rename(tmp, file)?;
        Ok(())
    }

    /// Reads back the mirrored settings the database does not have yet; returns how many.
    /// Each value goes through `set_preference`, so invalid ones are refused, not imported.
    pub fn import_missing_settings_from(&self, config: &Path) -> Result<usize> {
        let file = settings_file(config);
        if !file.is_file() {
            return Ok(0);
        }
        let saved: Map<String, Value> = serde_json::from_slice(&std::fs::read(&file)?)?;
        let current = self.preferences()?;
        let mut imported = 0;
        for key in MIRRORED {
            if let Some(value) = saved.get(key) {
                if current.get(key).is_none() && self.set_preference(key, value.clone()).is_ok() {
                    imported += 1;
                }
            }
        }
        Ok(imported)
    }

    /// Mirror after a settings change (silently skipped in test sandboxes).
    pub(crate) fn mirror_settings(&self, key: &str) {
        if !MIRRORED.contains(&key) {
            return;
        }
        if let Some(config) = writable_config() {
            if let Err(error) = self.export_settings_to(&config) {
                eprintln!("Réglages non recopiés dans {} : {error}", config.display());
            }
        }
    }

    /// At start: settings missing from the database come back from the config folder.
    pub fn import_missing_settings(&self) -> Result<usize> {
        match writable_config() {
            Some(config) => self.import_missing_settings_from(&config),
            None => Ok(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lasting_settings_go_to_the_folder_and_come_back() {
        let dir = std::env::temp_dir().join(format!("user-config-{}", uuid::Uuid::new_v4()));
        let config = dir.join("config");
        std::fs::create_dir_all(&dir).unwrap();
        let db = Database::open(&dir.join("a.sqlite3")).unwrap();
        db.set_preference("language", json!("en")).unwrap();
        db.set_preference("loraFavorites", json!(["x.safetensors"])).unwrap();
        db.set_preference("gallerySelection", json!(["not mirrored"])).unwrap();
        db.export_settings_to(&config).unwrap();
        let saved: Value =
            serde_json::from_slice(&std::fs::read(settings_file(&config)).unwrap()).unwrap();
        assert_eq!(saved["language"], "en");
        assert!(saved.get("gallerySelection").is_none(), "drafts and selections stay local");

        // A fresh database takes the missing settings back; an existing value is kept.
        let other = Database::open(&dir.join("b.sqlite3")).unwrap();
        other.set_preference("language", json!("ja")).unwrap();
        assert_eq!(other.import_missing_settings_from(&config).unwrap(), 1);
        let prefs = other.preferences().unwrap();
        assert_eq!(prefs["language"], "ja");
        assert_eq!(prefs["loraFavorites"], json!(["x.safetensors"]));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
