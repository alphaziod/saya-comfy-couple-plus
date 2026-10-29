//! Action drafts: new engine actions written to an isolated copy of the engine (the
//! engine's own `solo sandbox create`), drawn by the casting from that copy, then
//! promoted one by one into the real engine on the user's YES (`solo action add`, which
//! validates the schema, refuses duplicates and keeps a backup for `solo rollback`).
//!
//! Layout under `<data>/action-drafts/`: `drafts.json` (the list), `engine/` (the copy,
//! `engine/generez` is its engine directory), `tmp/` (action files handed to the CLI).
use crate::Result;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CLI_TIMEOUT: Duration = Duration::from_secs(300);
const ID_MAX: usize = 64;
const ACTIONS: &str = "data/actions";

/// Engine directory of the test copy (what the casting draws from in draft mode).
pub fn sandbox_engine(root: &Path) -> PathBuf {
    root.join("engine").join("generez")
}

fn drafts_file(root: &Path) -> PathBuf {
    root.join("drafts.json")
}

fn read_drafts(root: &Path) -> Result<Vec<Value>> {
    match std::fs::read(drafts_file(root)) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

fn write_drafts(root: &Path, drafts: &[Value]) -> Result<()> {
    std::fs::create_dir_all(root)?;
    let tmp = root.join("drafts.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(drafts)?)?;
    std::fs::rename(tmp, drafts_file(root))?;
    Ok(())
}

/// UTC timestamp `YYYY-MM-DDTHH:MM:SSZ`.
fn now() -> String {
    let clock = std::time::SystemTime::now();
    let secs = clock
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        % 86_400;
    format!(
        "{}T{:02}:{:02}:{:02}Z",
        crate::gallery::capture_day(clock),
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

/// Action families of an engine (`data/actions/<famille>.actions.json`).
pub fn families(engine: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(engine.join(ACTIONS))? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if let Some(family) = name.strip_suffix(".actions.json") {
            names.push(family.to_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// Allowed fields and their types (`data/actions/schema.json` → `champs`).
fn schema_fields(engine: &Path) -> Result<Map<String, Value>> {
    let path = engine.join(ACTIONS).join("schema.json");
    let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("Schéma des actions absent du moteur : {}", path.display()),
        _ => format!("Schéma des actions illisible ({}) : {e}", path.display()),
    })?;
    let schema: Value = serde_json::from_slice(&bytes)?;
    Ok(schema["champs"].as_object().cloned().unwrap_or_default())
}

/// Whether `id` already exists in an engine's action bank (any family file).
fn bank_has(engine: &Path, id: &str) -> Result<bool> {
    for family in families(engine)? {
        let path = engine.join(ACTIONS).join(format!("{family}.actions.json"));
        let payload: Value = serde_json::from_slice(&std::fs::read(path)?)?;
        if payload["actions"].get(id).is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Drafts, action families and fields, for the Casting "Brouillons" panel.
pub fn overview(root: &Path, engine: &Path) -> Result<Value> {
    // No engine (not installed, or SAYA_ENGINE_DIR wrong): the panel says where it looked
    // instead of failing with a raw "No such file or directory".
    if !engine.join(ACTIONS).is_dir() {
        return Ok(json!({
            "drafts": read_drafts(root)?,
            "families": [],
            "fields": {},
            "sandbox": false,
            "engineMissing": engine.display().to_string(),
        }));
    }
    Ok(json!({
        "drafts": read_drafts(root)?,
        "families": families(engine)?,
        // A bank without schema.json still lists drafts; saving then says what is missing.
        "fields": schema_fields(engine).unwrap_or_default(),
        "sandbox": sandbox_engine(root).join("test_solo.py").is_file(),
    }))
}

/// Runs `python3 solo_cli.py <args> --json` in an engine directory and returns the JSON
/// it prints. A non-zero exit or `ok:false` becomes an error carrying the CLI message.
fn solo(engine: &Path, args: &[&str]) -> Result<Value> {
    let script = engine.join("solo_cli.py");
    if !script.is_file() {
        return Err(format!("CLI moteur introuvable : {}", script.display()).into());
    }
    let mut child = Command::new("python3")
        .arg("solo_cli.py")
        .args(args)
        .arg("--json")
        .current_dir(engine)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Lancement du CLI moteur impossible : {e}"))?;
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if started.elapsed() > CLI_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("CLI moteur interrompu : délai dépassé ({CLI_TIMEOUT:?})").into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Option<Value> = serde_json::from_str(stdout.trim()).ok();
    match parsed {
        Some(value) if output.status.success() && value["ok"] != false => Ok(value),
        Some(value) => Err(value["message"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string())
            .into()),
        None => Err(format!(
            "Réponse illisible du CLI moteur (code {}) : {}{}",
            output.status,
            stdout.trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into()),
    }
}

/// Creates the test copy when it does not exist yet (or replaces it with `force`), then
/// replays every pending draft into it.
pub fn ensure_sandbox(root: &Path, engine: &Path, force: bool) -> Result<()> {
    if !force && sandbox_engine(root).join("test_solo.py").is_file() {
        return Ok(());
    }
    let dest = root.join("engine");
    std::fs::create_dir_all(root)?;
    let dest_text = dest.to_str().ok_or("Chemin non UTF-8")?;
    let mut args = vec!["sandbox", "create", "--dest", dest_text];
    if force || dest.exists() {
        args.push("--force");
    }
    solo(engine, &args)?;
    for draft in read_drafts(root)? {
        if draft["state"] == "draft" {
            write_into_sandbox(root, &draft)?;
        }
    }
    Ok(())
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= ID_MAX
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Checks a draft against the real engine's schema; returns the action as it will be
/// written (`valide: true`, required by the casting draw and implied by a YES).
fn checked_action(engine: &Path, id: &str, family: &str, action: &Value) -> Result<Value> {
    if !valid_id(id) {
        return Err("Identifiant d'action invalide : lettres minuscules, chiffres et _ (64 max)".into());
    }
    if !families(engine)?.iter().any(|f| f == family) {
        return Err(format!("Famille d'actions inconnue : {family}").into());
    }
    let mut action = action
        .as_object()
        .cloned()
        .ok_or("L'action doit être un objet JSON")?;
    let fields = schema_fields(engine)?;
    if let Some(unknown) = action.keys().find(|k| !fields.contains_key(*k)) {
        return Err(format!("Champ inconnu du moteur : {unknown}").into());
    }
    if !action
        .get("add")
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
    {
        return Err("Le champ « add » (le texte de l'action) est obligatoire".into());
    }
    if action.get("pas_solo") == Some(&Value::Bool(true)) {
        return Err("Une action « pas_solo » ne peut pas être testée au casting".into());
    }
    action.insert("valide".into(), Value::Bool(true));
    Ok(Value::Object(action))
}

fn action_file(root: &Path, id: &str, action: &Value) -> Result<PathBuf> {
    let dir = root.join("tmp");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{id}.json"));
    std::fs::write(&path, serde_json::to_vec_pretty(action)?)?;
    Ok(path)
}

/// Writes a draft into the test copy: `solo action add` the first time, otherwise the
/// copy's family file is rewritten in place (it is ours) and checked by `action validate`.
fn write_into_sandbox(root: &Path, draft: &Value) -> Result<()> {
    let engine = sandbox_engine(root);
    let id = draft["id"].as_str().ok_or("Brouillon sans id")?;
    let family = draft["famille"].as_str().ok_or("Brouillon sans famille")?;
    let path = engine.join(ACTIONS).join(format!("{family}.actions.json"));
    let mut payload: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    if payload["actions"].get(id).is_some() {
        payload["actions"][id] = draft["action"].clone();
        std::fs::write(&path, serde_json::to_vec_pretty(&payload)?)?;
        solo(&engine, &["action", "validate", id])?;
    } else {
        let file = action_file(root, id, &draft["action"])?;
        let file = file.to_str().ok_or("Chemin non UTF-8")?;
        solo(
            &engine,
            &["action", "add", "--famille", family, "--id", id, "--file", file, "--apply", "--yes"],
        )?;
    }
    Ok(())
}

/// Creates or updates a draft (test copy only; the real engine is never written here).
pub fn save(root: &Path, engine: &Path, id: &str, family: &str, action: &Value) -> Result<Value> {
    let id = id.trim();
    let action = checked_action(engine, id, family, action)?;
    if bank_has(engine, id)? {
        return Err(format!("L'action « {id} » existe déjà dans le pull réel").into());
    }
    let mut drafts = read_drafts(root)?;
    if let Some(old) = drafts.iter().find(|d| d["id"] == id) {
        if old["state"] != "draft" {
            return Err(format!("« {id} » est déjà passé dans le pull réel").into());
        }
        if old["famille"] != family {
            return Err("Changer la famille d'un brouillon : supprime-le puis recrée-le".into());
        }
    }
    ensure_sandbox(root, engine, false)?;
    let draft = json!({"id": id, "famille": family, "action": action, "state": "draft", "updated": now()});
    write_into_sandbox(root, &draft)?;
    drafts.retain(|d| d["id"] != id);
    drafts.push(draft.clone());
    write_drafts(root, &drafts)?;
    Ok(draft)
}

/// Removes a draft from the list and from the test copy (the real engine is untouched).
pub fn delete(root: &Path, id: &str) -> Result<()> {
    let mut drafts = read_drafts(root)?;
    let Some(draft) = drafts.iter().find(|d| d["id"] == id).cloned() else {
        return Err("Brouillon introuvable".into());
    };
    if draft["state"] == "draft" {
        let engine = sandbox_engine(root);
        let family = draft["famille"].as_str().unwrap_or_default();
        let path = engine.join(ACTIONS).join(format!("{family}.actions.json"));
        if path.is_file() {
            let mut payload: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
            if let Some(actions) = payload["actions"].as_object_mut() {
                actions.remove(id);
                std::fs::write(&path, serde_json::to_vec_pretty(&payload)?)?;
            }
            let order_path = engine.join(ACTIONS).join("_ordre.json");
            if let Ok(bytes) = std::fs::read(&order_path) {
                let mut order: Vec<Value> = serde_json::from_slice(&bytes)?;
                order.retain(|v| v != id);
                std::fs::write(&order_path, serde_json::to_vec_pretty(&order)?)?;
            }
        }
    }
    drafts.retain(|d| d["id"] != id);
    write_drafts(root, &drafts)
}

/// The user's YES: adds the draft to the REAL engine with `solo action add` (dry run
/// first, then apply). The engine keeps a backup, so `solo rollback` can undo it.
pub fn promote(root: &Path, engine: &Path, id: &str) -> Result<Value> {
    let mut drafts = read_drafts(root)?;
    let index = drafts
        .iter()
        .position(|d| d["id"] == id)
        .ok_or("Brouillon introuvable")?;
    if drafts[index]["state"] != "draft" {
        return Err(format!("« {id} » est déjà dans le pull réel").into());
    }
    let family = drafts[index]["famille"].as_str().ok_or("Brouillon sans famille")?.to_owned();
    let action = checked_action(engine, id, &family, &drafts[index]["action"])?;
    let file = action_file(root, id, &action)?;
    let file = file.to_str().ok_or("Chemin non UTF-8")?;
    let base = ["action", "add", "--famille", family.as_str(), "--id", id, "--file", file];
    solo(engine, &[&base[..], &["--dry-run"]].concat())?;
    let applied = solo(engine, &[&base[..], &["--apply", "--yes"]].concat())?;
    drafts[index]["state"] = json!("promoted");
    drafts[index]["promotedAt"] = json!(now());
    drafts[index]["backup"] = applied["backup"].clone();
    write_drafts(root, &drafts)?;
    Ok(drafts[index].clone())
}

/// Draft ids accepted by the casting in draft mode (pending drafts only).
pub fn pending_ids(root: &Path) -> Result<Vec<String>> {
    Ok(read_drafts(root)?
        .iter()
        .filter(|d| d["state"] == "draft")
        .filter_map(|d| d["id"].as_str().map(str::to_owned))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal engine: two action families, a schema and the fake `solo_cli.py` of the
    /// native smoke (`sandbox create`, `action add`, `action validate` on its own data).
    fn fake_engine(dir: &Path) -> PathBuf {
        let engine = dir.join("real").join("generez");
        let actions = engine.join(ACTIONS);
        std::fs::create_dir_all(&actions).unwrap();
        std::fs::write(actions.join("schema.json"), r#"{"requis":["add"],"champs":{"add":"str","note":"str","valide":"bool","pas_solo":"bool"}}"#).unwrap();
        std::fs::write(actions.join("debout.actions.json"), r#"{"actions":{"existante":{"add":"standing","valide":true}}}"#).unwrap();
        std::fs::write(actions.join("assise.actions.json"), r#"{"actions":{}}"#).unwrap();
        std::fs::write(actions.join("_ordre.json"), r#"["existante"]"#).unwrap();
        std::fs::write(engine.join("test_solo.py"), "").unwrap();
        std::fs::write(
            engine.join("solo_cli.py"),
            include_str!("../../../scripts/action-drafts-fake-solo.py"),
        )
        .unwrap();
        engine
    }

    fn bank(engine: &Path, family: &str) -> Value {
        serde_json::from_slice(&std::fs::read(engine.join(ACTIONS).join(format!("{family}.actions.json"))).unwrap()).unwrap()
    }

    #[test]
    fn draft_lives_in_the_copy_until_the_yes_promotes_it() {
        let dir = std::env::temp_dir().join(format!("drafts-{}", uuid::Uuid::new_v4()));
        let engine = fake_engine(&dir);
        let root = dir.join("data").join("action-drafts");
        let draft = save(&root, &engine, "bras_leves", "debout", &json!({"add": "arms up"})).unwrap();
        assert_eq!(draft["action"]["valide"], true, "a draft is drawable by the casting");
        let sandbox = sandbox_engine(&root);
        assert_eq!(bank(&sandbox, "debout")["actions"]["bras_leves"]["add"], "arms up");
        assert!(bank(&engine, "debout")["actions"].get("bras_leves").is_none(), "real engine untouched");
        assert_eq!(pending_ids(&root).unwrap(), ["bras_leves"]);

        // Editing rewrites the copy in place, never the real engine.
        save(&root, &engine, "bras_leves", "debout", &json!({"add": "arms raised high"})).unwrap();
        assert_eq!(bank(&sandbox, "debout")["actions"]["bras_leves"]["add"], "arms raised high");

        // Refusals: unknown field, missing text, existing real id, bad id, unknown family.
        assert!(save(&root, &engine, "x1", "debout", &json!({"add": "a", "inconnu": 1})).is_err());
        assert!(save(&root, &engine, "x2", "debout", &json!({"note": "a"})).is_err());
        assert!(save(&root, &engine, "existante", "debout", &json!({"add": "a"})).is_err());
        assert!(save(&root, &engine, "Bad-Id", "debout", &json!({"add": "a"})).is_err());
        assert!(save(&root, &engine, "x3", "inconnue", &json!({"add": "a"})).is_err());

        // YES: into the real bank, once.
        let promoted = promote(&root, &engine, "bras_leves").unwrap();
        assert_eq!(promoted["state"], "promoted");
        assert_eq!(bank(&engine, "debout")["actions"]["bras_leves"]["add"], "arms raised high");
        assert!(promote(&root, &engine, "bras_leves").is_err());
        assert!(pending_ids(&root).unwrap().is_empty());

        // A rejected draft leaves the copy and the list; the real engine never saw it.
        save(&root, &engine, "assise_test", "assise", &json!({"add": "sitting"})).unwrap();
        delete(&root, "assise_test").unwrap();
        assert!(bank(&sandbox, "assise")["actions"].get("assise_test").is_none());
        assert!(bank(&engine, "assise")["actions"].get("assise_test").is_none());
        assert!(read_drafts(&root).unwrap().iter().all(|d| d["id"] != "assise_test"));

        // Refreshing the copy replays the pending drafts.
        save(&root, &engine, "assise_b", "assise", &json!({"add": "sitting b"})).unwrap();
        ensure_sandbox(&root, &engine, true).unwrap();
        assert_eq!(bank(&sandbox, "assise")["actions"]["assise_b"]["add"], "sitting b");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bank_without_schema_lists_and_explains_on_save() {
        let dir = std::env::temp_dir().join(format!("drafts-noschema-{}", uuid::Uuid::new_v4()));
        let engine = fake_engine(&dir);
        std::fs::remove_file(engine.join(ACTIONS).join("schema.json")).unwrap();
        let root = dir.join("data").join("action-drafts");
        let view = overview(&root, &engine).unwrap();
        assert_eq!(view["fields"], json!({}));
        let error = save(&root, &engine, "x1", "debout", &json!({"add": "a"})).unwrap_err().to_string();
        assert!(error.contains("Schéma des actions absent"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_engine_is_reported_not_raised() {
        let dir = std::env::temp_dir().join(format!("drafts-none-{}", uuid::Uuid::new_v4()));
        let view = overview(&dir.join("root"), &dir.join("no-engine")).unwrap();
        assert!(view["engineMissing"].as_str().unwrap().ends_with("no-engine"));
        assert_eq!(view["families"], json!([]));
    }

    #[test]
    fn timestamp_is_utc_iso() {
        let t = now();
        assert_eq!(t.len(), 20);
        assert!(t.ends_with('Z') && t.as_bytes()[10] == b'T');
    }
}
