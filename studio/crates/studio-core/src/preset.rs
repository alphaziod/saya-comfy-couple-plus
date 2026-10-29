//! Base preset installed with the package (Saya 2026-10-22): characters validated by GROK BUILD (P1 and
//! anatomy separated, backgrounds, action) and the multi-pass ILUSTCOUPLECLEAN workflow (6 passes).
//!
//! At startup, if the preset version (`preset.json`) is not the one already applied
//! (`presetBase` preference): missing characters are imported (a character already imported from this
//! file, same key, is skipped: nothing is duplicated or overwritten), and the generation space is
//! set up on the workflow if it has none yet (or if it pointed to an old workflow of the
//! preset). The pass workflows (master + 6 passes) are copied into the ComfyUI workflows folder,
//! only if they are missing there: a pass already present (tuned by Saya) is never
//! overwritten. No deletion.

use crate::grok::{parse_characters, Choice};
use crate::{Database, Result};
use rusqlite::params;
use serde_json::{json, Value};
use std::path::Path;

impl Database {
    pub fn seed_preset(&self, dir: &Path) -> Result<Value> {
        let manifest_path = dir.join("preset.json");
        if !manifest_path.is_file() {
            return Ok(json!({"status":"absent"}));
        }
        let manifest: Value = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
        let version = manifest["version"].as_str().unwrap_or_default().to_owned();
        let prefs = self.preferences()?;
        if version.is_empty() || prefs["presetBase"].as_str() == Some(version.as_str()) {
            return Ok(json!({"status":"déjà appliqué","version":version}));
        }
        let mut imported = 0usize;
        if let Some(persos) = manifest["persos"].as_str() {
            let path = dir.join(persos);
            let (entries, _) = parse_characters(&path)?;
            let mut choices = Vec::new();
            for entry in &entries {
                let known: bool = self.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM import_records WHERE source_path=?1 AND source_key=?2)",
                    params![entry.path, entry.key],
                    |r| r.get(0),
                )?;
                if !known {
                    choices.push(Choice {
                        path: entry.path.clone(),
                        sha256: entry.sha256.clone(),
                        key: entry.key.clone(),
                        mode: "create".into(),
                        character: None,
                    });
                }
            }
            if !choices.is_empty() {
                let result = self.grok_import(&choices, true)?;
                imported = result["imported"].as_array().map_or(0, Vec::len);
            }
        }
        if let Some(workflow) = manifest["workflow"].as_str() {
            let source = dir.join(workflow);
            let state = &prefs["generationWorkspace"];
            let current = state["sourcePath"].as_str().unwrap_or_default();
            // Old preset workflow, or the original full workflow (same file name,
            // e.g. ComfyUI/…/ILUSTCOUPLECLEAN.json) replaced by the split passes.
            let full = manifest["passes"]["source"].as_str();
            let from_preset = !current.is_empty()
                && (Path::new(current).starts_with(dir)
                    || full.is_some_and(|f| Path::new(current).file_name() == Some(f.as_ref())));
            if source.is_file()
                && (current.is_empty() || (from_preset && Path::new(current) != source))
            {
                let name = manifest["workflow_nom"]
                    .as_str()
                    .unwrap_or("Preset de base");
                self.set_preference(
                    "generationWorkspace",
                    json!({"sourcePath": source.to_string_lossy(), "name": name}),
                )?;
            }
        }
        let copied = copy_passes(dir, &manifest)?;
        self.set_preference("presetBase", json!(version))?;
        Ok(json!({"status":"appliqué","version":version,"imported":imported,"passes":copied}))
    }
}

/// Copies `passes.dossier` from the preset to `<ComfyUI workflows>/<passes.comfy>` without overwriting anything.
/// ComfyUI folder: `SAYA_COMFY_WORKFLOWS`, otherwise `~/AI/ComfyUI/user/default/workflows` if it exists.
fn copy_passes(dir: &Path, manifest: &Value) -> Result<usize> {
    let (Some(folder), Some(sub)) = (
        manifest["passes"]["dossier"].as_str(),
        manifest["passes"]["comfy"].as_str(),
    ) else {
        return Ok(0);
    };
    let workflows = match std::env::var_os("SAYA_COMFY_WORKFLOWS") {
        Some(p) => std::path::PathBuf::from(p),
        None => match crate::paths::comfyui_dir() {
            Ok(dir) => dir.join("user/default/workflows"),
            Err(_) => return Ok(0),
        },
    };
    if !workflows.is_dir() {
        return Ok(0);
    }
    let target = workflows.join(sub);
    std::fs::create_dir_all(&target)?;
    let mut copied = 0;
    for entry in std::fs::read_dir(dir.join(folder))? {
        let path = entry?.path();
        let Some(name) = path.file_name() else {
            continue;
        };
        let dest = target.join(name);
        if path.is_file() && !dest.exists() {
            std::fs::copy(&path, &dest)?;
            copied += 1;
        }
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use crate::Database;
    use serde_json::json;

    #[test]
    fn imports_once_and_prepares_workspace() {
        let dir = std::env::temp_dir().join(format!("saya-preset-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("persos.json"), json!({"characters":[
            {"id":"CH--9099","name":"Maren","identity_prompt":"1girl, solo, elf, long hair","anatomy_prompt":"small wings, long tail","pose":"solo, sitting","bg_options":[{"bg_id":"a","bg_name":"Décor 1","scene_bg":"indoors, (bedroom:1.2)"}]}
        ]}).to_string()).unwrap();
        std::fs::write(dir.join("wf.json"), "{}").unwrap();
        std::fs::write(dir.join("preset.json"), json!({"version":"v1","persos":"persos.json","workflow":"wf.json","workflow_nom":"Base"}).to_string()).unwrap();
        let db = Database::in_memory().unwrap();
        let first = db.seed_preset(&dir).unwrap();
        assert_eq!(first["imported"], 1);
        assert_eq!(
            db.preferences().unwrap()["generationWorkspace"]["name"],
            "Base"
        );
        assert_eq!(db.seed_preset(&dir).unwrap()["status"], "déjà appliqué");
        // New version: the already imported character is not duplicated.
        std::fs::write(
            dir.join("preset.json"),
            json!({"version":"v2","persos":"persos.json"}).to_string(),
        )
        .unwrap();
        assert_eq!(db.seed_preset(&dir).unwrap()["imported"], 0);
    }

    #[test]
    fn copies_passes_without_overwriting_and_moves_preset_workspace() {
        let dir = std::env::temp_dir().join(format!("saya-preset-{}", uuid::Uuid::new_v4()));
        let comfy = dir.join("comfy");
        std::fs::create_dir_all(dir.join("passes")).unwrap();
        std::fs::create_dir_all(comfy.join("P")).unwrap();
        std::fs::write(dir.join("passes/a.json"), "neuf").unwrap();
        std::fs::write(dir.join("passes/b.json"), "neuf").unwrap();
        std::fs::write(comfy.join("P/b.json"), "réglé par Saya").unwrap();
        std::fs::write(dir.join("old.json"), "{}").unwrap();
        std::fs::write(dir.join("passes/01.json"), "{}").unwrap();
        std::fs::write(dir.join("preset.json"), json!({"version":"v3","workflow":"passes/01.json","passes":{"dossier":"passes","comfy":"P","source":"FULL.json"}}).to_string()).unwrap();
        std::env::set_var("SAYA_COMFY_WORKFLOWS", &comfy);
        let db = Database::in_memory().unwrap();
        db.set_preference(
            "generationWorkspace",
            json!({"sourcePath": "/ailleurs/ComfyUI/FULL.json", "session": "x"}),
        )
        .unwrap();
        let r = db.seed_preset(&dir).unwrap();
        std::env::remove_var("SAYA_COMFY_WORKFLOWS");
        assert_eq!(r["passes"], 2);
        assert_eq!(
            std::fs::read_to_string(comfy.join("P/b.json")).unwrap(),
            "réglé par Saya"
        );
        assert_eq!(
            std::fs::read_to_string(comfy.join("P/a.json")).unwrap(),
            "neuf"
        );
        assert_eq!(
            db.preferences().unwrap()["generationWorkspace"]["sourcePath"],
            dir.join("passes/01.json").to_string_lossy().as_ref()
        );
    }
}
