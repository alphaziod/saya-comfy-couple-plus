//! Slice U1 — 1-to-1 casting: reading the engine catalogue and filling the pass-1
//! source, then persistence of the YESes through `Database` (never a direct SQLite write
//! nor a write into the generez engine).
//!
//! The engine (`test_solo.py --casting N [--famille X] --graine G`, wrapped by
//! `engine_runtime::casting`) computes the EXACT pass-1 widget values in
//! the `casting` field of each shot (`comfy.casting_passe_un`, faithful port of
//! `fill_workflow`/`tune_loras`). This module copies these values into a copy of the
//! UI source — it never reinterprets the prompt.
//!
//! Node ids = those of the Saya workflow, identical to the engine (`comfy.py` fill_workflow:501):
//! 100309 main (prefixe/decor/decor_libre), 100300 action, 807 P1, 100301 P1 anatomy,
//! 145 negative, 808/100302 empty P2, 810 checkpoint hub (+ 488 in its subgraph
//! definition), 662/663/664 LoRA loaders, 654/655 emptied texts, 100042 format, and
//! `solo` on the `SayaMultiCouple`. Seeds are not written here:
//! `prepare_phase_api` forces them all via master_seed.

use crate::Result;
use rusqlite::OptionalExtension;
use serde_json::{json, Map, Value};
use std::path::Path;

/// Engine families (`registre_ids.py:41`), in registry order.
pub const CASTING_FAMILIES: [&str; 15] = [
    "hum", "bny", "kmn", "slm", "lcs", "cat", "dem", "elf", "cute", "edgy", "hor", "ard", "div",
    "sop", "lra",
];

const MAIN: u64 = 100309;
const ACTION: u64 = 100300;
const P1: u64 = 807;
const P1_ANATOMY: u64 = 100301;
const NEGATIVE: u64 = 145;
const P2: u64 = 808;
const P2_ANATOMY: u64 = 100302;
const HUB_CHECKPOINT: u64 = 810;
const HUB_CHECKPOINT_SUB: u64 = 488;
const LOADERS_LORA: [u64; 3] = [662, 663, 664];
const EMPTIED_TEXTS: [u64; 2] = [654, 655];
const FORMAT: u64 = 100042;

/// Reads the catalogue produced by the engine and validates the expected shape of a casting.
pub fn read_catalogue(chemin: &Path) -> Result<Vec<Value>> {
    let raw = std::fs::read(chemin)?;
    let shots: Vec<Value> = serde_json::from_slice(&raw)
        .map_err(|e| format!("Catalogue casting illisible ({}): {e}", chemin.display()))?;
    if shots.is_empty() {
        return Err(format!("Catalogue casting vide : {}", chemin.display()).into());
    }
    for shot in &shots {
        validate_shot(shot)?;
    }
    Ok(shots)
}

/// Text of a shot field, number accepted (u64 seed of the catalogue → string).
fn text_field(shot: &Value, champ: &str) -> Option<String> {
    match shot.get(champ) {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

/// A casting shot must carry a stem, a name, a known family, a known model,
/// a shot seed and its pass-1 values (`casting`).
pub fn validate_shot(shot: &Value) -> Result<()> {
    if !shot.is_object() {
        return Err("Shot de casting invalide : pas un objet".into());
    }
    for champ in ["stem", "name", "famille", "model", "seed"] {
        let value = text_field(shot, champ).filter(|v| !v.trim().is_empty());
        match value {
            Some(v) if v.len() <= 300 => {}
            Some(_) => return Err(format!("Shot de casting : {champ} trop long").into()),
            None => return Err(format!("Shot de casting sans {champ}").into()),
        }
    }
    let famille = shot["famille"].as_str().unwrap_or_default();
    if !famille
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(format!("Shot de casting : famille inconnue ({famille})").into());
    }
    let model = shot["model"].as_str().unwrap_or_default();
    if model != "noobai" && model != "illustrious" {
        return Err(format!("Shot de casting : modèle inconnu ({model})").into());
    }
    shot.get("casting")
        .and_then(Value::as_object)
        .filter(|pass_one| {
            pass_one.contains_key("prefixe")
                && pass_one.contains_key("p1")
                && pass_one.contains_key("lora_loaders")
        })
        .ok_or("Shot de casting sans valeurs de passe 1 (champ « casting »)")?;
    Ok(())
}

fn node_mut<'a>(root: &'a mut Value, id: u64) -> Option<&'a mut Value> {
    root["nodes"]
        .as_array_mut()?
        .iter_mut()
        .find(|node| node.get("id").and_then(Value::as_u64) == Some(id))
}

fn widget(node: &mut Value, nom: &str, value: Value) -> Result<()> {
    let id = node.get("id").and_then(Value::as_u64).unwrap_or(0);
    let named = node
        .get_mut("widgets_values_named")
        .and_then(Value::as_object_mut)
        .ok_or(format!("Node {id} sans widgets_values_named"))?;
    if !named.contains_key(nom) {
        return Err(format!("Node {id} : widget {nom} absent").into());
    }
    named.insert(nom.to_owned(), value);
    Ok(())
}

/// Copies the pass-1 values of the shot into a COPY of the UI source and returns the
/// filled source. The original source is never modified (read-only).
pub fn fill_source(source: &str, shot: &Value) -> Result<String> {
    let pass_one = shot["casting"].as_object().ok_or("Shot sans passe 1")?;

    let mut root: Value = serde_json::from_str(source)?;
    let main = node_mut(&mut root, MAIN)
        .ok_or("Node 100309 (main) absent : la source n'est pas le workflow Saya")?;
    widget(main, "prefixe", json!(pass_one["prefixe"].clone()))?;
    widget(main, "decor", json!("libre"))?;
    widget(main, "decor_libre", json!(pass_one["decor_libre"].clone()))?;

    for (id, champ) in [
        (ACTION, "action"),
        (P1, "p1"),
        (P1_ANATOMY, "anatomy"),
        (NEGATIVE, "negative"),
        (P2, "p2"),
        (P2_ANATOMY, "p2_anatomy"),
    ] {
        let node = node_mut(&mut root, id).ok_or(format!(
            "Node {id} absent : la source n'est pas le workflow Saya"
        ))?;
        widget(node, "value", json!(pass_one[champ].clone()))?;
    }

    let checkpoint = pass_one
        .get("checkpoint")
        .and_then(Value::as_str)
        .filter(|c| !c.is_empty())
        .ok_or("Passe 1 sans checkpoint")?
        .to_owned();
    let hub_type = {
        let hub = node_mut(&mut root, HUB_CHECKPOINT)
            .ok_or("Node 810 (checkpoint) absent : la source n'est pas le workflow Saya")?;
        widget(hub, "ckpt_name", json!(checkpoint))?;
        hub.get("type").and_then(Value::as_str).map(str::to_owned)
    };
    if let Some(type_hub) = hub_type {
        if let Some(sous_node) = root
            .get_mut("definitions")
            .and_then(|d| d.get_mut("subgraphs"))
            .and_then(Value::as_array_mut)
            .and_then(|subs| {
                subs.iter_mut()
                    .find(|s| s.get("id").and_then(Value::as_str) == Some(type_hub.as_str()))
            })
            .and_then(|d| d.get_mut("nodes").and_then(Value::as_array_mut))
            .and_then(|nodes| {
                nodes
                    .iter_mut()
                    .find(|n| n.get("id").and_then(Value::as_u64) == Some(HUB_CHECKPOINT_SUB))
            })
        {
            widget(sous_node, "ckpt_name", json!(checkpoint))?;
        }
    }

    for id in LOADERS_LORA {
        let Some(node) = node_mut(&mut root, id) else {
            continue; // loader absent from this source: nothing to set on it
        };
        let Some(plan) = pass_one
            .get("lora_loaders")
            .and_then(|l| l.get(id.to_string()))
            .and_then(Value::as_object)
        else {
            continue;
        };
        // silence_all then ensure_lora (comfy.py:296): the existing rows become
        // inactive, the kept rows are enabled with the engine's weights.
        let gardees = plan
            .get("loras")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut rows: Vec<Value> = node
            .get("widgets_values_named")
            .and_then(|n| n.get("loras"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|mut ligne| {
                if let Some(objet) = ligne.as_object_mut() {
                    objet.insert("active".into(), json!(false));
                }
                ligne
            })
            .collect();
        for gardee in &gardees {
            let nom = gardee
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if nom.is_empty() {
                continue;
            }
            let mut found = false;
            for ligne in rows.iter_mut() {
                if ligne.get("name").and_then(Value::as_str) == Some(nom) {
                    if let Some(objet) = ligne.as_object_mut() {
                        objet.insert("active".into(), json!(true));
                        for champ in ["strength", "clipStrength"] {
                            if let Some(value) = gardee.get(champ) {
                                objet.insert(champ.into(), value.clone());
                            }
                        }
                    }
                    found = true;
                    break;
                }
            }
            if !found {
                rows.push(gardee.clone());
            }
        }
        widget(node, "loras", Value::Array(rows))?;
        widget(
            node,
            "text",
            json!(plan.get("text").cloned().unwrap_or(json!(""))),
        )?;
    }

    for id in EMPTIED_TEXTS {
        if let Some(node) = node_mut(&mut root, id) {
            widget(node, "text", json!(""))?;
        }
    }

    if let Some(node) = node_mut(&mut root, FORMAT) {
        widget(node, "dimensions", json!("Portrait - 832x1216"))?;
        widget(
            node,
            "invert",
            json!(pass_one
                .get("format_paysage")
                .and_then(Value::as_bool)
                .unwrap_or(false)),
        )?;
    }

    if let Some(definitions) = root
        .get_mut("definitions")
        .and_then(|d| d.get_mut("subgraphs"))
        .and_then(Value::as_array_mut)
    {
        for definition in definitions.iter_mut() {
            let Some(nodes) = definition.get_mut("nodes").and_then(Value::as_array_mut) else {
                continue;
            };
            for node in nodes.iter_mut() {
                if node.get("type").and_then(Value::as_str) == Some("SayaMultiCouple") {
                    widget(node, "solo", json!(true))?;
                }
            }
        }
    }

    Ok(root.to_string())
}

fn timestamp_now() -> String {
    format!("{:?}", std::time::SystemTime::now())
}

/// Shot seed as u64 text: master seed of pass 1 (prepare_phase_api forces it
/// on all seed/noise_seed inputs), the image stays tied to the catalogue.
pub fn shot_seed(shot: &Value) -> Result<String> {
    let seed = text_field(shot, "seed").filter(|v| !v.trim().is_empty());
    let Some(seed) = seed else {
        return Err("Shot de casting sans seed".into());
    };
    let _value: u64 = seed
        .parse()
        .map_err(|_| format!("Seed du shot invalide : {seed}"))?;
    Ok(seed)
}

fn validate_run(run: &str) -> Result<()> {
    if run.len() < 8 || !run.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("Run casting invalide".into());
    }
    Ok(())
}

impl crate::Database {
    /// Character targeting (slice V1, 2026-10-25): the characters the engine can
    /// cast, read from the engine's `negatifs_figes.json` (READ-ONLY — this registry
    /// is never written by the app; an OC validated by a casting YES enters it through the
    /// engine). Each row carries the id understood by `--persos` (`neg_id`), its name, its
    /// slug, its model, its family and the joined app sheet if one exists.
    /// Sheet join: first the IMPORT KEY (`characters:<neg_id>` set by the
    /// base preset that imports the validated ones at startup — insensitive to sheet
    /// renames), then as a fallback the EXACT NAME (registry names are unique and the
    /// sheets come from these exports). `ficheId: null` = castable character without a sheet:
    /// the window shows a tile without a portrait, always selectable.
    pub fn available_characters(&self, engine: &Path) -> Result<Vec<Value>> {
        let chemin = engine.join("negatifs_figes.json");
        let unreadable = |e: String| {
            format!(
                "Registre des persos validés illisible ({}): {e}",
                chemin.display()
            )
        };
        let raw = std::fs::read(&chemin).map_err(|e| unreadable(e.to_string()))?;
        let value: Value = serde_json::from_slice(&raw).map_err(|e| unreadable(e.to_string()))?;
        let rows = value["valides"]
            .as_array()
            .ok_or_else(|| unreadable("sans liste « valides »".into()))?;
        let mut sheet_by_key = self.connection.prepare(
            "SELECT c.id FROM import_records ir JOIN characters c ON c.id=ir.character_id \
             WHERE ir.source_key=?1 AND c.archived=0 ORDER BY ir.imported_at DESC, c.id LIMIT 1",
        )?;
        let mut sheet_by_name = self.connection.prepare(
            "SELECT id FROM characters WHERE name=?1 AND archived=0 ORDER BY revision DESC, id LIMIT 1",
        )?;
        let mut out = Vec::new();
        for ligne in rows {
            let Some(neg_id) = ligne.get("neg_id").and_then(Value::as_str) else {
                continue;
            };
            let Some(nom) = ligne.get("nom").and_then(Value::as_str).map(str::to_owned) else {
                continue;
            };
            if nom.trim().is_empty() {
                continue; // no name: no label nor reliable fallback join
            }
            let by_key: Option<String> = sheet_by_key
                .query_row(
                    rusqlite::params![format!("characters:{neg_id}")],
                    |rankée| rankée.get(0),
                )
                .optional()?;
            let sheet = match by_key {
                Some(id) => Some(id),
                None => sheet_by_name
                    .query_row(rusqlite::params![nom], |rankée| rankée.get(0))
                    .optional()?,
            };
            out.push(json!({
                "negId": neg_id,
                "nom": nom,
                "slug": ligne.get("slug").and_then(Value::as_str).unwrap_or_default(),
                "model": ligne.get("model").and_then(Value::as_str).unwrap_or_default(),
                "famille": ligne.get("famille").and_then(Value::as_str).unwrap_or_default(),
                "source": ligne.get("source").and_then(Value::as_str).unwrap_or_default(),
                "ficheId": sheet,
            }));
        }
        if out.is_empty() {
            return Err(unreadable("aucun perso validé lisible".into()).into());
        }
        Ok(out)
    }

    /// Persists a casting YES (Saya keeps the OC). Written by the Database struct,
    /// single row per (run, stem): re-validating the same shot updates the row.
    pub fn casting_validate(
        &self,
        run: &str,
        graine: &str,
        catalogue: &str,
        shot: &Value,
    ) -> Result<Value> {
        validate_run(run)?;
        validate_shot(shot)?;
        let stem = text_field(shot, "stem").unwrap_or_default();
        let p1 = shot["casting"]["p1"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if p1.trim().is_empty() {
            return Err("Casting : shot sans p1".into());
        }
        if graine.trim().is_empty() {
            return Err("Casting : graine absente".into());
        }
        let pass_one = shot["casting"]
            .as_object()
            .cloned()
            .unwrap_or_else(Map::new);
        let pass_text = |champ: &str| {
            pass_one
                .get(champ)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let id = uuid::Uuid::new_v4().to_string();
        self.connection.execute(
            "INSERT INTO casting_choices(id,run,stem,name,famille,model,graine,seed,p1,anatomy,pose,bg,prefixe,catalogue,decided_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
             ON CONFLICT(run,stem) DO UPDATE SET
               decided_at=excluded.decided_at, p1=excluded.p1, anatomy=excluded.anatomy,
               pose=excluded.pose, bg=excluded.bg, prefixe=excluded.prefixe, catalogue=excluded.catalogue",
            rusqlite::params![
                id,
                run,
                stem,
                text_field(shot, "name").unwrap_or_default(),
                text_field(shot, "famille").unwrap_or_default(),
                text_field(shot, "model").unwrap_or_default(),
                graine,
                text_field(shot, "seed").unwrap_or_default(),
                p1,
                pass_text("anatomy"),
                pass_text("action"),
                pass_text("decor_libre"),
                pass_text("prefixe"),
                catalogue,
                timestamp_now(),
            ],
        )?;
        let id: String = self.connection.query_row(
            "SELECT id FROM casting_choices WHERE run=?1 AND stem=?2",
            rusqlite::params![run, stem],
            |rankée| rankée.get(0),
        )?;
        Ok(json!({"id": id, "stem": stem, "persiste": true}))
    }

    /// The persisted casting YESes, from most recent to oldest.
    /// Picture of each action and each character in the casting pickers: the most recent
    /// casting trial that carried it (a kept trial wins over a newer one; its final image
    /// wins over pass 1). The engine has no image of its own for an action.
    /// Returns `{"actions": {action_name: asset}, "persos": {shot id: asset}}`.
    pub fn casting_previews(&self) -> Result<Value> {
        let mut runs = self.saya_runs()?;
        // Newest first, then kept first: the first image seen for a key wins.
        runs.sort_by(|a, b| b["createdAt"].as_str().cmp(&a["createdAt"].as_str()));
        runs.sort_by_key(|run| run["casting"]["published"] != true);
        let (mut actions, mut persos) = (Map::new(), Map::new());
        for run in &runs {
            let shot = &run["casting"]["shot"];
            let Some(asset) = ["6", "1"]
                .iter()
                .find_map(|phase| run["phases"][phase]["asset"].as_str())
            else {
                continue;
            };
            let present: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE id=? AND availability='present')",
                [asset],
                |r| r.get(0),
            )?;
            if !present {
                continue;
            }
            if let Some(name) = shot["action_name"].as_str().filter(|n| !n.is_empty()) {
                actions.entry(name.to_owned()).or_insert(json!(asset));
            }
            if let Some(id) = shot["id"].as_str().filter(|n| !n.is_empty()) {
                persos.entry(id.to_owned()).or_insert(json!(asset));
            }
        }
        Ok(json!({"actions": actions, "persos": persos}))
    }

    pub fn casting_choices(&self) -> Result<Vec<Value>> {
        let mut declaration = self.connection.prepare(
            "SELECT id,run,stem,name,famille,model,graine,seed,p1,anatomy,pose,bg,prefixe,catalogue,decided_at
             FROM casting_choices ORDER BY decided_at DESC",
        )?;
        let rows = declaration.query_map([], |rankée| {
            Ok(json!({
                "id": rankée.get::<_, String>(0)?,
                "run": rankée.get::<_, String>(1)?,
                "stem": rankée.get::<_, String>(2)?,
                "name": rankée.get::<_, String>(3)?,
                "famille": rankée.get::<_, String>(4)?,
                "model": rankée.get::<_, String>(5)?,
                "graine": rankée.get::<_, String>(6)?,
                "seed": rankée.get::<_, String>(7)?,
                "p1": rankée.get::<_, String>(8)?,
                "anatomy": rankée.get::<_, String>(9)?,
                "pose": rankée.get::<_, String>(10)?,
                "bg": rankée.get::<_, String>(11)?,
                "prefixe": rankée.get::<_, String>(12)?,
                "catalogue": rankée.get::<_, String>(13)?,
                "decidedAt": rankée.get::<_, String>(14)?,
            }))
        })?;
        let mut out = Vec::new();
        for ligne in rows {
            out.push(ligne?);
        }
        Ok(out)
    }

    /// ComfyUI prompt of each requested job (casting_history progress): the WS monitor
    /// (`live_transport::LiveState`) indexes by prompt_id, the history by job id.
    pub fn prompt_ids_for(
        &self,
        jobs: &[String],
    ) -> Result<std::collections::BTreeMap<String, String>> {
        let mut out = std::collections::BTreeMap::new();
        for groupe in jobs.chunks(64) {
            let mut request = self.connection.prepare(&format!(
                "SELECT id,comfy_prompt_id FROM generation_jobs
                 WHERE comfy_prompt_id IS NOT NULL AND id IN ({})",
                vec!["?"; groupe.len()].join(",")
            ))?;
            let rows = request
                .query_map(rusqlite::params_from_iter(groupe.iter()), |rankée| {
                    Ok((rankée.get::<_, String>(0)?, rankée.get::<_, String>(1)?))
                })?;
            for ligne in rows {
                let (job, prompt) = ligne?;
                out.insert(job, prompt);
            }
        }
        Ok(out)
    }

    /// STOP of a casting trial (Saya's feedback 3, 2026-10-26): interrupts the current
    /// ComfyUI computation (`/interrupt` endpoint qualified by `control_job`) and cancels the
    /// pending trials (removed from the queue, marked `cancelled` in generation_jobs).
    /// The run state becomes `stopped` IN THE PERSISTED MANIFEST (survives a
    /// restart) and `autoAdvance` drops: no pass will ever be submitted again.
    /// Each control is reported as is — an engine refusal is an
    /// `envoye:false` row, never an invented success.
    pub fn casting_stop(
        &mut self,
        run: &str,
        client: &crate::comfy::Comfy,
        results: &Path,
    ) -> Result<Value> {
        if let Err(error) = self.sync_jobs(client, results) {
            eprintln!("casting: casting_stop: sync_jobs failed for run {run}: {error}");
        }
        let result = self.casting_stop_synced(run, client)?;
        if let Ok(value) = self.saya_status(run, client, results) {
            return Ok(
                json!({"run":run,"state":value["state"],"controles":result["controles"],"manifeste":value}),
            );
        }
        Ok(result)
    }

    /// Batch callers synchronize once, not twice for every trial in the batch.
    pub fn casting_stop_synced(
        &mut self,
        run: &str,
        client: &crate::comfy::Comfy,
    ) -> Result<Value> {
        validate_run(run)?;
        let mut value = crate::saya::read_run(self, run)?;
        if value["state"] == "completed" {
            return Err("Run casting déjà terminé : rien à arrêter".into());
        }
        let jobs = self.jobs_by_ids(&crate::saya::phase_jobs(&value))?;
        let mut controls = Vec::new();
        for phase in 1..=6 {
            let Some(job) = value["phases"][phase.to_string()]["job"].as_str() else {
                continue;
            };
            let Some(active) = jobs.iter().find(|j| j["id"] == job) else {
                continue;
            };
            let etat = active["state"].as_str().unwrap_or("");
            let action = match etat {
                "queued" | "submission_unknown" => Some("cancel_queue"),
                "running" => Some("interrupt"),
                _ => None,
            };
            let Some(action) = action else {
                continue;
            };
            match self.control_job(job, action, true, client) {
                Ok(response) => controls.push(json!({
                    "job": job, "phase": phase, "action": action, "envoye": true,
                    "reponse": response,
                })),
                Err(erreur) => controls.push(json!({
                    "job": job, "phase": phase, "action": action, "envoye": false,
                    "erreur": erreur.to_string(),
                })),
            }
        }
        if value["phases"]["1"]["state"] == "waiting" {
            value["phases"]["1"]["state"] = json!("cancelled");
        }
        value["state"] = json!("stopped");
        value["autoAdvance"] = json!(false);
        value["stoppedAt"] = json!(timestamp_now());
        value["stopPending"] = json!(controls.iter().any(|c| c["envoye"] == false));
        crate::saya::write_run(self, run, &value)?;
        Ok(json!({"run": run, "state": value["state"], "controles": controls}))
    }

    /// Local cancellation remains possible without contacting ComfyUI.
    pub fn casting_stop_offline(&mut self, run: &str) -> Result<Value> {
        validate_run(run)?;
        let mut value = crate::saya::read_run(self, run)?;
        let mut pending = false;
        for phase in 1..=6 {
            let key = phase.to_string();
            let state = value["phases"][&key]["state"].as_str().unwrap_or("");
            if matches!(state, "waiting" | "draft") {
                if let Some(job) = value["phases"][&key]["job"].as_str() {
                    self.connection.execute(
                        "UPDATE generation_jobs SET state='cancelled' WHERE id=? AND state='draft'",
                        [job],
                    )?;
                }
                value["phases"][&key]["state"] = json!("cancelled");
            } else if matches!(
                state,
                "submitted" | "submitting" | "queued" | "running" | "submission_unknown"
            ) {
                pending = true;
            }
        }
        value["state"] = json!("stopped");
        value["autoAdvance"] = json!(false);
        value["stopPending"] = json!(pending);
        value["stoppedAt"] = json!(timestamp_now());
        crate::saya::write_run(self, run, &value)?;
        Ok(json!({"run":run,"state":"stopped","stopPending":pending,"manifeste":value}))
    }

    /// PAUSE of a casting trial (Saya's feedback 3, 2026-10-26) — AT THE NEXT BOUNDARY:
    /// the running ComfyUI computation is NOT interrupted (ComfyUI has no sampler pause;
    /// `/interrupt` is a cancellation and is NEVER sent here), but no
    /// NEW trial is queued anymore: the manifest's `paused` state blocks
    /// `next_ready_phase` (automatic advance) and `casting_decide` (pass 2+ launch).
    /// Persisted: survives a restart.
    pub fn casting_pause(&mut self, run: &str) -> Result<Value> {
        validate_run(run)?;
        let mut value = crate::saya::read_run(self, run)?;
        match value["state"].as_str() {
            Some("paused") => return Ok(value), // already paused: idempotent
            Some("running") => {}
            autre => {
                return Err(format!(
                    "Run casting ni en cours ni en pause (état « {} ») : pause impossible",
                    autre.unwrap_or("?")
                )
                .into())
            }
        }
        value["state"] = json!("paused");
        value["pausedAt"] = json!(timestamp_now());
        crate::saya::write_run(self, run, &value)?;
        Ok(value)
    }

    /// RESUME of a paused casting trial: the queue of remaining trials restarts WITHOUT
    /// restarting anything — the pause cancelled nothing, the recorded jobs are never
    /// resubmitted (saya.rs), and `saya_status` restarts the automatic advance of an
    /// already decided run. If ComfyUI is unreachable, the resume STAYS in effect (persisted
    /// state) and the synchronization error is reported honestly.
    pub fn casting_resume(
        &mut self,
        run: &str,
        client: &crate::comfy::Comfy,
        results: &Path,
    ) -> Result<Value> {
        validate_run(run)?;
        let mut value = crate::saya::read_run(self, run)?;
        if value["state"] != "paused" {
            return Err("Run casting pas en pause : rien à reprendre".into());
        }
        value["state"] = json!("running");
        if let Some(objet) = value.as_object_mut() {
            objet.remove("pausedAt");
        }
        crate::saya::write_run(self, run, &value)?;
        match self.saya_status(run, client, results) {
            Ok(manifest) => {
                Ok(json!({"run": run, "state": manifest["state"].clone(), "manifeste": manifest}))
            }
            Err(erreur) => Ok(json!({
                "run": run, "state": value["state"],
                "manifeste": value,
                "syncErreur": erreur.to_string(),
            })),
        }
    }

    /// Explicit batch replay: preserve completed outputs and clone only terminal attempts.
    /// Unknown submissions must be reconciled before any new job can be created.
    pub fn casting_resume_batch(&mut self, run: &str) -> Result<Value> {
        validate_run(run)?;
        let current = crate::saya::read_run(self, run)?;
        let catalogue = current["casting"]["catalogue"]
            .as_str()
            .ok_or("Catalogue absent")?;
        let rows: Vec<_> = self
            .casting_history()?
            .into_iter()
            .filter(|r| r["catalogue"].as_str() == Some(catalogue))
            .collect();
        if rows.iter().any(|r| {
            r["runState"] == "stopped"
                && r["phases"].as_object().is_some_and(|p| {
                    p.values().any(|v| {
                        matches!(
                            v["state"].as_str(),
                            Some(
                                "submitted"
                                    | "submitting"
                                    | "queued"
                                    | "running"
                                    | "submission_unknown"
                            )
                        )
                    })
                })
        }) {
            return Err("Confirmer l’arrêt distant du lot avant de le reprendre".into());
        }
        let mut staged = Vec::new();
        for row in &rows {
            let retry = matches!(
                row["state"].as_str(),
                Some("cancelled" | "failed" | "interrupted")
            );
            if row["runState"] != "stopped" && row["runState"] != "paused" && !retry {
                continue;
            }
            let id = row["run"].as_str().ok_or("Run absent")?;
            let mut value = crate::saya::read_run(self, id)?;
            value["phases"] = row["phases"].clone();
            if retry {
                let old = row["phases"]["1"]["job"].as_str().ok_or("Job absent")?;
                let snapshot: String = self.connection.query_row(
                    "SELECT request_snapshot_json FROM generation_jobs WHERE id=?",
                    [old],
                    |r| r.get(0),
                )?;
                let new = uuid::Uuid::new_v4().to_string();
                let tx = self.connection.unchecked_transaction()?;
                tx.execute("INSERT INTO generation_jobs(id,workflow_revision_id,parent_job_id,state,seed,request_snapshot_json,endpoint_url) SELECT ?1,workflow_revision_id,id,'draft',seed,request_snapshot_json,endpoint_url FROM generation_jobs WHERE id=?2", rusqlite::params![new,old])?;
                tx.execute("INSERT INTO job_characters(job_id,role,character_id,variant_id,snapshot_json) SELECT ?1,role,character_id,variant_id,snapshot_json FROM job_characters WHERE job_id=?2", rusqlite::params![new,old])?;
                tx.commit()?;
                let previous = value["phases"]["1"].clone();
                match value["previousPhase1"].as_array_mut() {
                    Some(list) => list.push(previous),
                    None => value["previousPhase1"] = json!([previous]),
                }
                value["phases"]["1"] = json!({"job":new,"state":"waiting","snapshotHash":crate::gallery::digest(snapshot.as_bytes())});
                value["autoAdvance"] = json!(false);
                value["stopPending"] = json!(false);
                value["casting"]["deferred"] = json!(true);
                value["casting"]["prepared"] = json!(true);
            }
            value["state"] = json!("paused");
            crate::saya::write_run(self, id, &value)?;
            staged.push((id.to_owned(), value));
        }
        // Resume only once every replacement is persisted. No network request here.
        let mut resumed = Vec::new();
        for (id, mut value) in staged {
            value["state"] = json!("running");
            let object = value.as_object_mut().ok_or("Manifeste de run invalide")?;
            object.remove("pausedAt");
            object.remove("autoError");
            crate::saya::write_run(self, &id, &value)?;
            resumed.push(json!({"run":id,"state":"running"}));
        }
        Ok(json!({"runs":resumed}))
    }

    /// Reusable SAVE of a casting result as an app character sheet
    /// (Saya's feedback 4, 2026-10-26). Reuses the existing mechanisms: the sheet is
    /// created/attached by the import key `casting-run:<run>` + stem (the same trial
    /// re-saved ATTACHES the same sheet, never a duplicate — `import_character`), and
    /// the run's pass-1 asset becomes its reference (`associate`, role `reference`).
    /// The identity is the character only: P1 + anatomy (and the model). The
    /// trial's action is not saved; its background becomes a « Décor du casting »
    /// variant (Saya's feedback, 2026-10-29). This function NEVER writes the run's
    /// manifest: saving cannot launch passes 2-6.
    pub fn casting_save_character(&self, run: &str, stem: &str, nom: &str) -> Result<Value> {
        validate_run(run)?;
        let stem = stem.trim();
        if stem.is_empty() || stem.len() > 300 {
            return Err("Stem de casting invalide".into());
        }
        let nom = nom.trim();
        if nom.is_empty() || nom.len() > 300 {
            return Err("Nom du personnage invalide".into());
        }
        let manifest = crate::saya::read_run(self, run)?;
        let shot = &manifest["casting"]["shot"];
        if shot["stem"].as_str() != Some(stem) {
            return Err(format!(
                "Ce run casting ne porte pas le stem « {stem} » : sauvegarde refusée"
            )
            .into());
        }
        let asset = manifest["phases"]["1"]["asset"]
            .as_str()
            .filter(|a| !a.is_empty())
            .ok_or(
                "Image de passe 1 absente du run : synchroniser l'essai avant d'enregistrer le perso",
            )?;
        let pass_one = &shot["casting"];
        let texte = |champ: &str| pass_one[champ].as_str().unwrap_or_default().to_owned();
        // Anatomy tags left in P1 move to the anatomy, as in the GROK import.
        let (p1, anatomy) = crate::grok::split_anatomy(&texte("p1"), &texte("anatomy"));
        // Clothed (bulge) or caged trial: the prompt carries no penis; the sheet gets the
        // character's real anatomy back, with a note saying so.
        let (anatomy, anatomy_note) = complete_anatomy(&p1, &anatomy);
        let decor = texte("decor_libre");
        let sheet = json!({
            "name": nom,
            "identity_prompt": p1,
            "anatomy_prompt": anatomy,
            "p1": p1,
            "anatomy": anatomy,
            "anatomy_note": anatomy_note,
            "checkpoint": pass_one["checkpoint"],
            "famille": shot["famille"],
            "model": shot["model"],
            "graine": manifest["casting"]["graine"],
            "seed": shot["seed"],
            "castingRun": run,
            "castingStem": stem,
        });
        // Import key: (source_path, source_key) = (casting-run:<run>, stem). The same
        // pair ALWAYS returns the same sheet (characters.rs import_character), the name
        // requested afterwards does not retroactively rename the existing sheet.
        let id = self.import_character(
            &format!("casting-run:{run}"),
            &crate::gallery::digest(format!("{run}:{stem}").as_bytes()),
            stem,
            &sheet,
        )?;
        self.associate(&id, asset, true)?;
        if !decor.trim().is_empty() && self.variants(&id)?.is_empty() {
            self.save_variant(
                &id,
                None,
                "Décor du casting",
                json!({"P1": crate::grok::join_prompt(&p1, &anatomy), "BG": decor}),
                json!({}),
            )?;
        }
        Ok(json!({"characterId": id, "asset": asset, "stem": stem, "run": run}))
    }
}

/// Anatomy of a character saved from a casting trial whose prompt had no penis (bulge
/// under clothing, chastity cage): rebuilt like the GROK engine (`valider.perso_sauve` +
/// `anatomie.anatomy_for`): size read from the bulge (big = medium, none = small), pubic
/// hair from the hair colour, cute_penis for tiny/small, erect. Returns the note to keep.
fn complete_anatomy(p1: &str, anatomy: &str) -> (String, Option<String>) {
    let lower = anatomy.to_lowercase();
    if lower.contains("penis") {
        return (anatomy.to_owned(), None);
    }
    let size = ["tiny", "small", "medium", "big"]
        .into_iter()
        .find(|s| lower.contains(s))
        .map(|s| if s == "big" { "medium" } else { s })
        .unwrap_or("small");
    let colour = hair_colour(p1);
    let pubic = if colour.is_empty() { "pubic hair".to_owned() } else { format!("{colour} pubic hair") };
    let (head, tail) = match size {
        "tiny" => ("(tiny penis:1.25), (very small penis:1.15), cute_penis", "(tiny testicles:1.2), small scrotum"),
        "small" => ("(small penis:1.25), cute_penis", "(small testicles:1.2), small scrotum"),
        _ => ("(medium penis:1.25)", "(medium testicles:1.2)"),
    };
    let rebuilt = format!(
        "{head}, erect penis, erection, (slightly veiny penis:0.9), well-shaped penis, smooth glans, penis matching skin tone, (scrotum matching skin tone:1.15), {tail}, {pubic}"
    );
    let source = if lower.contains("cage") { "cage" } else if lower.contains("bulge") { "renflement sous la tenue" } else { "sans pénis" };
    let note = format!("Anatomie rebâtie : l'essai n'en montrait pas ({source}). Taille {size}, à vérifier.");
    (rebuilt, Some(note))
}

/// Hair colour key of a P1 (engine `hair_key`, plus the shades it does not know).
fn hair_colour(p1: &str) -> &'static str {
    const BASE: [&str; 14] = ["black", "brown", "blonde", "red", "pink", "blue", "aqua", "green", "purple", "white", "grey", "gray", "silver", "orange"];
    const EXTRA: [(&str, &str); 10] = [("chestnut", "brown"), ("honey", "blonde"), ("platinum", "white"), ("lavender", "purple"), ("violet", "purple"), ("magenta", "pink"), ("copper", "red"), ("auburn", "red"), ("apricot", "orange"), ("peach", "orange")];
    let hairy = ["hair", "bob", "curls", "ponytail", "twintails", "braid", "waves", "bun", "locks"];
    for tag in p1.split(',').map(|t| t.trim().to_lowercase()) {
        let words: Vec<&str> = tag.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
        if !words.iter().any(|w| hairy.contains(w)) || tag.contains("pubic") || tag.contains("eye") {
            continue;
        }
        if let Some((_, colour)) = EXTRA.iter().find(|(w, _)| tag.contains(w)) {
            return colour;
        }
        if let Some(word) = BASE.iter().find(|b| words.contains(b)) {
            return match *word { "gray" | "silver" => "grey", w => w };
        }
    }
    ""
}

#[cfg(test)]
mod controle_tests {
    use super::*;
    use crate::{comfy::Comfy, Database};
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::time::Duration;

    fn base() -> (std::path::PathBuf, Database) {
        let dir = std::env::temp_dir().join(format!("casting-controle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
        (dir, db)
    }

    #[test]
    fn pickers_show_the_latest_trial_and_prefer_kept_ones() {
        let (dir, db) = base();
        db.connection
            .execute_batch(
                "INSERT INTO libraries(id,label,root_path,platform) VALUES('lib','S','/s','linux');
                 INSERT INTO assets(id,library_id,relative_path,availability) VALUES
                   ('old','lib','a.png','present'),('new','lib','b.png','present'),
                   ('kept','lib','c.png','present'),('gone','lib','d.png','missing');",
            )
            .unwrap();
        let run = |id: &str, at: &str, action: &str, perso: &str, asset: &str, kept: bool| {
            crate::saya::write_run(&db, id, &json!({"id": id, "createdAt": at,
                "phases": {"1": {"asset": asset}},
                "casting": {"published": kept, "shot": {"action_name": action, "id": perso}}}))
            .unwrap();
        };
        run("00000000-0000-4000-8000-000000000001", "2026-10-01T00:00:00Z", "bras_leves", "CH--0001", "old", false);
        run("00000000-0000-4000-8000-000000000002", "2026-10-02T00:00:00Z", "bras_leves", "CH--0002", "new", false);
        run("00000000-0000-4000-8000-000000000003", "2026-09-01T00:00:00Z", "assise", "CH--0002", "kept", true);
        run("00000000-0000-4000-8000-000000000004", "2026-10-03T00:00:00Z", "assise", "CH--0003", "gone", false);
        let p = db.casting_previews().unwrap();
        assert_eq!(p["actions"]["bras_leves"], "new", "latest trial");
        assert_eq!(p["actions"]["assise"], "kept", "kept wins, missing file skipped");
        assert_eq!(p["persos"]["CH--0002"], "kept");
        assert_eq!(p["persos"]["CH--0001"], "old");
        assert!(p["persos"].get("CH--0003").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    fn shot() -> Value {
        json!({
            "stem": "bny-00001-stub-oc-noobai",
            "name": "Stub OC",
            "famille": "bny",
            "model": "noobai",
            "seed": "248919022623436",
            "casting": {
                "prefixe": "embedding:lazypos, embedding:lazyneg",
                "decor_libre": "night club, neon signs",
                "action": "hands clasped behind head",
                "p1": "1futanari, solo, futanari",
                "anatomy": "(tiny penis:1.25), cute_penis",
                "negative": "embedding:lazyneg",
                "checkpoint": "noobai.safetensors",
                "lora_loaders": {}
            }
        })
    }

    fn manifest(id: &str, etat: &str, phase_un: Value) -> Value {
        json!({
            "id": id, "state": etat, "masterSeed": "248919022623436",
            "createdAt": "2026-10-26T10:00:00Z", "autoAdvance": false,
            "isolatedRoot": "casting-tests/x", "sourcePath": "passe1.json",
            "phases": {"1": phase_un},
            "casting": {"graine": "7", "catalogue": "catalogue.json", "decision": "", "shot": shot()}
        })
    }

    #[test]
    fn offline_stop_cancels_local_work_and_keeps_remote_stop_retryable() {
        let (_dir, mut db) = base();
        let local = "offline-local-01";
        let remote = "offline-remote-01";
        crate::saya::write_run(
            &db,
            local,
            &manifest(local, "running", json!({"state":"waiting"})),
        )
        .unwrap();
        crate::saya::write_run(
            &db,
            remote,
            &manifest(
                remote,
                "running",
                json!({"state":"running","job":"remote-job"}),
            ),
        )
        .unwrap();
        let stopped = db.casting_stop_offline(local).unwrap();
        assert_eq!(stopped["manifeste"]["phases"]["1"]["state"], "cancelled");
        assert_eq!(stopped["stopPending"], false);
        let stopped = db.casting_stop_offline(remote).unwrap();
        assert_eq!(stopped["state"], "stopped");
        assert_eq!(stopped["stopPending"], true);
        assert_eq!(stopped["manifeste"]["phases"]["1"]["state"], "running");
        assert_eq!(
            db.casting_batch_runs(remote).unwrap(),
            vec![remote.to_owned()]
        );
        assert_eq!(
            db.casting_stop_offline(remote).unwrap()["stopPending"],
            true
        );
    }

    /// Complete HTTP request received on a connection (status line + JSON body).
    fn request(stream: &mut TcpStream) -> (String, Value) {
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut chunk = [0; 4096];
        loop {
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(fin) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..fin]).into_owned();
                let size = header
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|v| v.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                if bytes.len() >= fin + 4 + size {
                    return (
                        header.lines().next().unwrap().into(),
                        if size == 0 {
                            Value::Null
                        } else {
                            serde_json::from_slice(&bytes[fin + 4..fin + 4 + size]).unwrap()
                        },
                    );
                }
            }
        }
    }

    fn respond(stream: &mut TcpStream, value: Value) {
        let body = value.to_string();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    }

    /// Inserts a real job (workflow + draft) then forces its state and its prompt.
    fn forced_job(db: &Database, dir: &std::path::Path, id_job: &str, etat: &str, prompt: &str) {
        let workflow = db
            .import_workflow(
                "synthetic",
                r#"{"n":{"class_type":"Text","inputs":{"text":"exact"}}}"#,
            )
            .unwrap();
        let schema = json!({"Text":{"input":{"required":{"text":["STRING"]}},"output":["STRING"]}});
        let draft = db.prepare_job(&workflow, &[], &schema).unwrap();
        rusqlite::Connection::open(dir.join("studio.sqlite3"))
            .unwrap()
            .execute(
                "UPDATE generation_jobs SET id=?2,state=?3,comfy_prompt_id=?4 WHERE id=?1",
                rusqlite::params![draft.job_id, id_job, etat, prompt],
            )
            .unwrap();
    }

    #[test]
    fn resume_cancelled_batch_preserves_snapshots_and_completed_jobs() {
        let (dir, mut db) = base();
        for (id, state) in [
            ("retry-run-01", "cancelled"),
            ("done-run-01", "succeeded"),
            ("failed-run-01", "failed"),
        ] {
            forced_job(&db, &dir, id, state, "");
            crate::saya::write_run(
                &db,
                id,
                &manifest(
                    id,
                    if state == "failed" {
                        "running"
                    } else {
                        "stopped"
                    },
                    json!({"job":id,"state":"submitted"}),
                ),
            )
            .unwrap();
        }
        let before: String = db
            .connection
            .query_row(
                "SELECT request_snapshot_json FROM generation_jobs WHERE id='retry-run-01'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            db.casting_history()
                .unwrap()
                .iter()
                .find(|r| r["run"] == "done-run-01")
                .unwrap()["state"],
            "succeeded"
        );
        let result = db.casting_resume_batch("retry-run-01").unwrap();
        assert_eq!(result["runs"].as_array().unwrap().len(), 3);
        let resumed = crate::saya::read_run(&db, "retry-run-01").unwrap();
        let new = resumed["phases"]["1"]["job"].as_str().unwrap();
        assert_ne!(new, "retry-run-01");
        let (snapshot, parent, state): (String, String, String) = db
            .connection
            .query_row(
                "SELECT request_snapshot_json,parent_job_id,state FROM generation_jobs WHERE id=?",
                [new],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(snapshot, before);
        assert_eq!(parent, "retry-run-01");
        assert_eq!(state, "draft");
        assert_eq!(resumed["phases"]["1"]["state"], "waiting");
        assert_eq!(resumed["masterSeed"], "248919022623436");
        let done = crate::saya::read_run(&db, "done-run-01").unwrap();
        assert_eq!(done["phases"]["1"]["job"], "done-run-01");
        assert_eq!(done["phases"]["1"]["state"], "succeeded");
        assert_eq!(done["state"], "running");
        assert!(db.casting_resume_batch("retry-run-01").unwrap()["runs"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn resume_refuses_unknown_submission_without_creating_jobs() {
        let (dir, mut db) = base();
        forced_job(&db, &dir, "unknown-1", "submission_unknown", "");
        crate::saya::write_run(
            &db,
            "unknown-1",
            &manifest(
                "unknown-1",
                "stopped",
                json!({"job":"unknown-1","state":"cancelled"}),
            ),
        )
        .unwrap();
        assert!(db
            .casting_resume_batch("unknown-1")
            .unwrap_err()
            .to_string()
            .contains("arrêt distant"));
        assert_eq!(db.jobs().unwrap().len(), 1);
    }

    #[test]
    fn casting_history_reconciles_jobs_beyond_latest_hundred() {
        let (dir, db) = base();
        forced_job(&db, &dir, "old-job", "succeeded", "");
        crate::saya::write_run(
            &db,
            "old-run-01",
            &manifest(
                "old-run-01",
                "stopped",
                json!({"job":"old-job","state":"submitted"}),
            ),
        )
        .unwrap();
        for n in 0..101 {
            db.connection.execute("INSERT INTO generation_jobs(id,workflow_revision_id,state,seed,request_snapshot_json) SELECT ?1,workflow_revision_id,'draft',seed,request_snapshot_json FROM generation_jobs WHERE id='old-job'",[format!("new-{n}")]).unwrap();
        }
        assert_eq!(db.jobs().unwrap().len(), 100);
        assert_eq!(db.casting_history().unwrap()[0]["state"], "succeeded");
    }

    #[test]
    fn run_status_syncs_phase_jobs_older_than_the_latest_hundred() {
        let (dir, mut db) = base();
        forced_job(&db, &dir, "old-phase-job", "succeeded", "");
        crate::saya::write_run(
            &db,
            "old-sync-run-01",
            &manifest(
                "old-sync-run-01",
                "running",
                json!({"job":"old-phase-job","state":"submitted"}),
            ),
        )
        .unwrap();
        for n in 0..101 {
            db.connection.execute("INSERT INTO generation_jobs(id,workflow_revision_id,state,seed,request_snapshot_json) SELECT ?1,workflow_revision_id,'draft',seed,request_snapshot_json FROM generation_jobs WHERE id='old-phase-job'",[format!("newer-{n}")]).unwrap();
        }
        db.saya_status_synced("old-sync-run-01", &Comfy::loopback(1).unwrap(), &dir)
            .unwrap();
        assert_eq!(
            crate::saya::read_run(&db, "old-sync-run-01").unwrap()["phases"]["1"]["state"],
            "succeeded"
        );
    }

    #[test]
    fn pause_blocks_decide_and_resume_restarts_without_comfy() {
        let (dir, mut db) = base();
        let id = "pauseresume-1";
        crate::saya::write_run(
            &db,
            id,
            &manifest(id, "running", json!({"job": "job-1", "state": "running"})),
        )
        .unwrap();
        // PAUSE: persisted state, no ComfyUI request (never an interrupt).
        let value = db.casting_pause(id).unwrap();
        assert_eq!(value["state"], "paused");
        drop(db);
        let mut db = Database::open(&dir.join("studio.sqlite3")).unwrap();
        assert_eq!(crate::saya::read_run(&db, id).unwrap()["state"], "paused");
        // Deciding on a paused run = refusal (it would launch pass 2). Pass 1 is
        // successful: the refusal really comes from the pause, not from a pending pass.
        crate::saya::write_run(
            &db,
            id,
            &manifest(id, "paused", json!({"job": "job-1", "state": "succeeded"})),
        )
        .unwrap();
        let erreur = db
            .casting_decide(id, true, &Comfy::loopback(1).unwrap(), &dir)
            .unwrap_err()
            .to_string();
        assert!(erreur.contains("reprendre"), "{erreur}");
        // RESUME: the state goes back to running (no active job in the database → no HTTP call).
        let resumed_run = db
            .casting_resume(id, &Comfy::loopback(1).unwrap(), &dir)
            .unwrap();
        assert_eq!(resumed_run["state"], "running");
        assert!(resumed_run.get("syncErreur").is_none());
        assert_eq!(crate::saya::read_run(&db, id).unwrap()["state"], "running");
        // Resume outside a pause and pause of a stopped run: honest refusals.
        assert!(db
            .casting_resume(id, &Comfy::loopback(1).unwrap(), &dir)
            .unwrap_err()
            .to_string()
            .contains("pas en pause"));
        crate::saya::write_run(
            &db,
            id,
            &manifest(id, "stopped", json!({"job": "job-1", "state": "cancelled"})),
        )
        .unwrap();
        assert!(db
            .casting_pause(id)
            .unwrap_err()
            .to_string()
            .contains("pause impossible"));
    }

    #[test]
    fn stop_interrupts_current_computation_and_persists() {
        let (dir, mut db) = base();
        let id = "stoprunning-1";
        let prompt = uuid::Uuid::new_v4().to_string();
        forced_job(&db, &dir, "job-1", "running", &prompt);
        crate::saya::write_run(
            &db,
            id,
            &manifest(id, "running", json!({"job": "job-1", "state": "running"})),
        )
        .unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = Comfy::loopback(listener.local_addr().unwrap().port()).unwrap();
        // /history/<prompt> answers ONE object indexed by prompt_id (comfy.rs history.get).
        let interrompu = json!({prompt.clone(): {"status": {"status_str": "error", "completed": false,
            "messages": [["execution_interrupted"]]}, "outputs": [], "prompt": []}});
        let server = std::thread::spawn(move || {
            let mut expected_values = vec![
                (
                    "GET /queue HTTP/1.1",
                    json!({"queue_running": [[0, prompt.clone(), {}, {}]], "queue_pending": []}),
                ),
                ("GET /history/", json!({})),
                (
                    "GET /queue HTTP/1.1",
                    json!({"queue_running": [[0, prompt.clone(), {}, {}]], "queue_pending": []}),
                ),
                (
                    "GET /system_stats HTTP/1.1",
                    json!({"system": {"comfyui_version": "0.34.0"}}),
                ),
                ("POST /interrupt HTTP/1.1", json!({})),
                (
                    "GET /queue HTTP/1.1",
                    json!({"queue_running": [], "queue_pending": []}),
                ),
                ("GET /history/", interrompu),
            ];
            for (requete_attendue, response) in expected_values.drain(..) {
                let (mut stream, _) = listener.accept().unwrap();
                let (recue, _) = request(&mut stream);
                assert!(
                    recue.starts_with(&requete_attendue),
                    "{recue} ≁ {requete_attendue}"
                );
                respond(&mut stream, response);
            }
        });
        let value = db.casting_stop(id, &client, &dir.join("results")).unwrap();
        server.join().unwrap();
        // The interruption was sent and reported, the run is durably stopped.
        assert_eq!(value["controles"][0]["action"], "interrupt");
        assert_eq!(value["controles"][0]["envoye"], json!(true));
        assert_eq!(value["state"], "stopped");
        assert_eq!(value["manifeste"]["phases"]["1"]["state"], "interrupted");
        drop(db);
        let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
        assert_eq!(crate::saya::read_run(&db, id).unwrap()["state"], "stopped");
        assert_eq!(db.jobs().unwrap()[0]["state"], "interrupted");
    }

    #[test]
    fn stop_cancels_pending_trials_marked_cancelled() {
        let (dir, mut db) = base();
        let id = "stopqueued-1";
        let prompt = uuid::Uuid::new_v4().to_string();
        forced_job(&db, &dir, "job-1", "queued", &prompt);
        crate::saya::write_run(
            &db,
            id,
            &manifest(id, "running", json!({"job": "job-1", "state": "queued"})),
        )
        .unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = Comfy::loopback(listener.local_addr().unwrap().port()).unwrap();
        let server = std::thread::spawn(move || {
            let expected_values = vec![
                (
                    "GET /queue HTTP/1.1",
                    json!({"queue_running": [], "queue_pending": [[0, prompt.clone(), {}, {}]]}),
                ),
                ("GET /history/", json!({})),
                (
                    "GET /queue HTTP/1.1",
                    json!({"queue_running": [], "queue_pending": [[0, prompt.clone(), {}, {}]]}),
                ),
                ("POST /queue HTTP/1.1", json!({})),
                (
                    "GET /queue HTTP/1.1",
                    json!({"queue_running": [], "queue_pending": []}),
                ),
                ("GET /history/", json!({})),
            ];
            for (requete_attendue, response) in expected_values {
                let (mut stream, _) = listener.accept().unwrap();
                let (recue, body) = request(&mut stream);
                assert!(
                    recue.starts_with(&requete_attendue),
                    "{recue} ≁ {requete_attendue}"
                );
                if requete_attendue == "POST /queue HTTP/1.1" {
                    assert_eq!(
                        body,
                        json!({"delete": [prompt]}),
                        "annulation ciblée du prompt"
                    );
                }
                respond(&mut stream, response);
            }
        });
        let value = db.casting_stop(id, &client, &dir.join("results")).unwrap();
        server.join().unwrap();
        assert_eq!(value["controles"][0]["action"], "cancel_queue");
        assert_eq!(value["controles"][0]["envoye"], json!(true));
        assert_eq!(value["state"], "stopped");
        drop(db);
        let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
        assert_eq!(db.jobs().unwrap()[0]["state"], "cancelled");
        assert_eq!(crate::saya::read_run(&db, id).unwrap()["state"], "stopped");
    }

    /// Regression (U1 smoke of 2026-10-26, "Source Saya absente"): `casting_tick`
    /// wrote manifests rebuilt from `saya_runs`, which strips `source`
    /// for the UI response. The following casting decision (`saya_continue`) read a
    /// manifest without a source and the chaining of the 6 passes died. The written
    /// manifest must start again from a complete re-read: `source` survives the tick.
    #[test]
    fn casting_tick_persists_the_complete_manifest_including_source() {
        let (dir, mut db) = base();
        let id = "ticksource-1";
        let workflow = db
            .import_workflow(
                "synthetic",
                r#"{"n":{"class_type":"Text","inputs":{"text":"exact"}}}"#,
            )
            .unwrap();
        let schema = json!({"Text":{"input":{"required":{"text":["STRING"]}},"output":["STRING"]}});
        let draft = db.prepare_job(&workflow, &[], &schema).unwrap();
        let source = json!({"nodes": [], "extra": {"saya_preset": true}}).to_string();
        crate::saya::write_run(
            &db,
            id,
            &json!({
                "id": id, "state": "running", "masterSeed": "7",
                "createdAt": "2026-10-26T10:00:00Z",
                "source": source, "sourceSha256": "sha",
                "isolatedRoot": "casting-tests/x",
                "phases": {"1": {"job": draft.job_id, "state": "waiting",
                                 "snapshotHash": draft.snapshot_hash}},
                "casting": {"graine": "7", "catalogue": "catalogue.json",
                            "decision": "", "deferred": true, "prepared": true, "index": 0,
                            "shot": shot()}
            }),
        )
        .unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let client = Comfy::loopback(listener.local_addr().unwrap().port()).unwrap();
        let prompt_id = uuid::Uuid::new_v4().to_string();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let (ligne, _) = request(&mut stream);
            assert!(ligne.starts_with("POST /prompt"), "{ligne}");
            respond(&mut stream, json!({"prompt_id": prompt_id}));
        });
        db.casting_tick(&client, &dir).unwrap();
        server.join().unwrap();
        let manifest = crate::saya::read_run(&db, id).unwrap();
        assert_eq!(
            manifest["source"],
            json!(source),
            "source intacte après tick"
        );
        assert_eq!(manifest["phases"]["1"]["state"], "submitted");
        assert_eq!(manifest["casting"]["deferred"], true);
    }

    #[test]
    fn stop_refuses_an_already_finished_run() {
        let (dir, mut db) = base();
        let id = "stopdone-01";
        crate::saya::write_run(
            &db,
            id,
            &manifest(
                id,
                "completed",
                json!({"job": "job-1", "state": "succeeded"}),
            ),
        )
        .unwrap();
        let erreur = db
            .casting_stop(id, &Comfy::loopback(1).unwrap(), &dir)
            .unwrap_err()
            .to_string();
        assert!(erreur.contains("déjà terminé"), "{erreur}");
    }

    #[test]
    fn save_character_creates_sheet_with_p1_anatomy_and_image() {
        let (dir, db) = base();
        let id = "enregistr-01";
        crate::saya::write_run(
            &db,
            id,
            &manifest(
                id,
                "running",
                json!({"job": "job-1", "state": "succeeded", "asset": "img-1"}),
            ),
        )
        .unwrap();
        {
            let sql = rusqlite::Connection::open(dir.join("studio.sqlite3")).unwrap();
            sql.execute_batch(
                "INSERT INTO libraries(id,label,root_path,platform) VALUES('lib','Synthetic','/synthetic','linux');
                 INSERT INTO assets(id,library_id,relative_path,availability) VALUES('img-1','lib','passe1.png','present');",
            )
            .unwrap();
        }
        // Inconsistent stem: refused (identity protected).
        assert!(db
            .casting_save_character(id, "autre-stem", "Nom")
            .unwrap_err()
            .to_string()
            .contains("ne porte pas"));
        let value = db
            .casting_save_character(id, "bny-00001-stub-oc-noobai", "Saya Cast")
            .unwrap();
        let sheet_id = value["characterId"].as_str().unwrap().to_owned();
        // Without a pass-1 image: honest refusal (no mute sheet).
        let (_dir2, db2) = base();
        let id2 = "enregistr-02";
        crate::saya::write_run(
            &db2,
            id2,
            &manifest(
                id2,
                "running",
                json!({"job": "job-1", "state": "succeeded"}),
            ),
        )
        .unwrap();
        assert!(db2
            .casting_save_character(id2, "bny-00001-stub-oc-noobai", "X")
            .unwrap_err()
            .to_string()
            .contains("synchroniser"));
        // The sheet exists, carries p1/anatomy and its reference image.
        let sheets = db.characters().unwrap();
        assert_eq!(sheets.len(), 1);
        assert_eq!(sheets[0].id, sheet_id);
        assert_eq!(sheets[0].name, "Saya Cast");
        assert_eq!(sheets[0].identity["p1"], json!("1futanari, solo, futanari"));
        assert_eq!(
            sheets[0].identity["anatomy"],
            json!("(tiny penis:1.25), cute_penis")
        );
        let sheet = db.character_sheet(&sheet_id).unwrap();
        assert_eq!(sheet["identityPrompt"], sheets[0].identity["p1"]);
        assert_eq!(sheet["anatomyPrompt"], sheets[0].identity["anatomy"]);
        assert_eq!(sheets[0].identity["castingRun"], json!(id));
        // The trial's action is not part of the character; its background is a variant.
        assert!(sheets[0].identity.get("action").is_none());
        assert!(sheets[0].identity.get("casting").is_none());
        let variants = db.variants(&sheet_id).unwrap();
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0]["name"], "Décor du casting");
        assert_eq!(variants[0]["promptBlocks"]["BG"], json!("night club, neon signs"));
        assert!(variants[0]["promptBlocks"].get("ACTION").is_none());
        assert_eq!(sheets[0].image_ids, vec!["img-1".to_owned()]);
        // Re-saving of the SAME trial: the SAME sheet is attached (never a duplicate).
        let still = db
            .casting_save_character(id, "bny-00001-stub-oc-noobai", "Saya Cast")
            .unwrap();
        assert_eq!(still["characterId"], json!(sheet_id));
        assert_eq!(db.characters().unwrap().len(), 1);
        assert_eq!(db.variants(&sheet_id).unwrap().len(), 1, "no duplicate variant");
    }

    #[test]
    fn clothed_or_caged_trial_saves_the_real_anatomy() {
        let p1 = "1futanari, solo, (long lavender hair:1.2), nude";
        let (a, note) = complete_anatomy(p1, "(bulge:1.25), (medium bulge:0.9)");
        assert!(a.starts_with("(medium penis:1.25), erect penis"), "{a}");
        assert!(a.ends_with("purple pubic hair") && a.contains("(medium testicles:1.2)"), "{a}");
        assert!(note.unwrap().contains("renflement"));
        let (a, note) = complete_anatomy("1futanari, short honey-blonde bob", "flat chastity cage, testicles");
        assert!(a.starts_with("(small penis:1.25), cute_penis") && a.ends_with("blonde pubic hair"), "{a}");
        assert!(note.unwrap().contains("cage"));
        let kept = "(tiny penis:1.25), flaccid penis, scrotum";
        assert_eq!(complete_anatomy(p1, kept), (kept.to_owned(), None));
    }

    #[test]
    fn prompt_ids_for_links_jobs_to_prompts() {
        let (dir, db) = base();
        let prompt = uuid::Uuid::new_v4().to_string();
        forced_job(&db, &dir, "job-1", "running", &prompt);
        let carte = db
            .prompt_ids_for(&["job-1".into(), "job-absent".into()])
            .unwrap();
        assert_eq!(
            carte.get("job-1").map(String::as_str),
            Some(prompt.as_str())
        );
        assert!(carte.get("job-absent").is_none());
    }

    /// Synthetic casting batch: neutral images in `results/`, jobs linked through
    /// job_outputs, saya-runs manifests. Covers gallery (final vs pass 1),
    /// validation without duplicates and batch deletion without breaking the rest.
    #[test]
    fn lot_publication_representative_and_deletion_keep_only_valid_data() {
        let (dir, mut db) = base();
        let results = dir.join("results");
        std::fs::create_dir_all(&results).unwrap();
        for (name, c) in [("a1", 10u8), ("a6", 20), ("b1", 30), ("c1", 40), ("d1", 50)] {
            image::RgbImage::from_pixel(8, 8, image::Rgb([c, c, c]))
                .save(results.join(format!("{name}.png")))
                .unwrap();
        }
        db.index_library(&results).unwrap();
        let asset = |db: &Database, name: &str| -> String {
            db.connection
                .query_row(
                    "SELECT id FROM assets WHERE relative_path=?",
                    [format!("{name}.png")],
                    |r| r.get(0),
                )
                .unwrap()
        };
        let mut ids = std::collections::HashMap::new();
        for name in ["a1", "a6", "b1", "c1", "d1"] {
            let job = format!("job-{name}");
            forced_job(&db, &dir, &job, "succeeded", "");
            let id = asset(&db, name);
            db.connection.execute("INSERT INTO job_outputs(job_id,asset_id,node_id,output_index,output_kind) VALUES(?,?,'9',0,'saved')", [&job, &id]).unwrap();
            ids.insert(name, id);
        }
        let batch_l = dir.join("casting/u1-l/catalogue.json");
        let batch_m = dir.join("casting/u1-m/catalogue.json");
        for batch in [&batch_l, &batch_m] {
            std::fs::create_dir_all(batch.parent().unwrap()).unwrap();
            std::fs::write(batch, "{}").unwrap();
        }
        let phase = |name: &str| json!({"job": format!("job-{name}"), "state": "succeeded", "asset": ids[name]});
        let run = |id: &str, batch: &std::path::Path, phases: Value| {
            let mut m = manifest(id, "completed", Value::Null);
            m["phases"] = phases;
            m["casting"]["catalogue"] = json!(batch.to_str().unwrap());
            crate::saya::write_run(&db, id, &m).unwrap();
        };
        run(
            "run-a-complet",
            &batch_l,
            json!({"1": phase("a1"), "6": phase("a6")}),
        );
        run("run-b-passe-1", &batch_l, json!({"1": phase("b1")}));
        run("run-d-ecarte1", &batch_l, json!({"1": phase("d1")}));
        run("run-e-partage", &batch_l, json!({"1": phase("c1")}));
        run("run-c-autre-l", &batch_m, json!({"1": phase("c1")}));
        let shown = |db: &Database| -> Vec<String> {
            let mut v: Vec<String> = db
                .gallery(&Default::default())
                .unwrap()
                .items
                .into_iter()
                .map(|a| a.id)
                .collect();
            v.sort();
            v
        };
        assert!(shown(&db).is_empty(), "casting non validé : hors galerie");
        // Multiple validation: final for the full workflow, pass 1 otherwise.
        assert_eq!(
            db.casting_publish(&["run-a-complet".into(), "run-b-passe-1".into()])
                .unwrap()["published"],
            2
        );
        let mut expected = vec![ids["a6"].clone(), ids["b1"].clone()];
        expected.sort();
        assert_eq!(shown(&db), expected);
        assert_eq!(
            db.casting_publish(&["run-a-complet".into()]).unwrap()["published"],
            0,
            "deuxième validation : aucun doublon"
        );
        assert_eq!(shown(&db), expected);
        // Deletion of batch L: validated ones kept, discarded trial erased, shared file kept.
        let report = db.casting_delete_batch("run-b-passe-1").unwrap();
        assert_eq!(
            (
                report["removedRuns"].as_u64(),
                report["archivedRuns"].as_u64(),
                report["removedFiles"].as_u64()
            ),
            (Some(2), Some(2), Some(1))
        );
        assert!(!results.join("d1.png").exists());
        for kept in ["a1", "a6", "b1", "c1"] {
            assert!(results.join(format!("{kept}.png")).exists(), "{kept}");
        }
        let (removed, missing): (i64, String) = db
            .connection
            .query_row(
                "SELECT catalog_removed,availability FROM assets WHERE id=?",
                [&ids["d1"]],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (removed, missing.as_str()),
            (1, "missing"),
            "ligne gardée pour la provenance du job, hors catalogue"
        );
        assert_eq!(shown(&db), expected);
        let history: Vec<Value> = db.casting_history().unwrap();
        assert_eq!(
            history
                .iter()
                .map(|r| r["run"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["run-c-autre-l"]
        );
        assert!(
            batch_l.parent().unwrap().exists(),
            "catalogue encore cité par les essais validés"
        );
        // Batch M: nothing kept anymore, the batch's file and folder disappear.
        db.casting_delete_batch("run-c-autre-l").unwrap();
        assert!(!results.join("c1.png").exists());
        assert!(!batch_m.parent().unwrap().exists());
        assert!(db.casting_history().unwrap().is_empty());
        assert_eq!(shown(&db), expected);
        // Lot actif : refus.
        let mut active = manifest(
            "run-actif-01",
            "running",
            json!({"job": "job-x", "state": "running"}),
        );
        active["casting"]["catalogue"] = json!(batch_l.to_str().unwrap());
        crate::saya::write_run(&db, "run-actif-01", &active).unwrap();
        assert!(db.casting_delete_batch("run-actif-01").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
