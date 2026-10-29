use crate::{gallery::digest, Database, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SceneEntry {
    pub path: String,
    pub sha256: String,
    pub key: String,
    pub name: String,
    pub variants: Vec<Value>,
    pub warnings: Vec<String>,
    pub raw: Value,
    pub duplicate: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneChoice {
    pub path: String,
    pub sha256: String,
    pub key: String,
}

fn blocks(row: &Value) -> Value {
    let mut b = json!({});
    for (role, keys) in [
        ("QUALITY", vec!["quality"]),
        ("P1", vec!["p1", "identity_prompt"]),
        ("P1 ANATOMY", vec!["anatomy", "anatomy_prompt", "anatomie"]),
        ("P2", vec!["p2"]),
        ("BG", vec!["bg", "scene_bg"]),
        ("ACTION", vec!["pose"]),
        ("NEG", vec!["negative"]),
    ] {
        if let Some(v) = keys.iter().find_map(|key| row[*key].as_str()) {
            b[role] = json!(v);
        }
    }
    b
}
pub fn parse_scenes(path: &Path) -> Result<(Vec<SceneEntry>, Vec<String>)> {
    let path = path.canonicalize()?;
    let mut bytes = Vec::new();
    std::fs::File::open(&path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Fichier supérieur à 16 Mio".into());
    }
    let sha = digest(&bytes);
    let mut groups: Vec<(String, String, Vec<Value>, Value)> = Vec::new();
    let mut warnings = Vec::new();
    match path.extension().and_then(|s| s.to_str()) {
        Some("py") => {
            let mut child = Command::new("python3")
                .args([
                    "-I",
                    "-S",
                    "-c",
                    include_str!("../../../scripts/grok_literals.py"),
                ])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| format!("Analyse statique Python indisponible : {e}"))?;
            child
                .stdin
                .take()
                .ok_or("stdin Python absent")?
                .write_all(&bytes)?;
            let result = child.wait_with_output()?;
            if !result.status.success() {
                return Err(format!(
                    "Analyse AST refusée : {}",
                    String::from_utf8_lossy(&result.stderr)
                )
                .into());
            }
            let data: Value = serde_json::from_slice(&result.stdout)?;
            warnings.extend(
                data["warnings"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_owned),
            );
            if let Some(rows) = data["tables"]["POLE_PERSO"].as_object() {
                for (id, row) in rows {
                    let mut variants = Vec::new();
                    for key in ["scene", "intime"] {
                        if let Some(bg) = row[key].as_str() {
                            variants.push(json!({"name":key,"blocks":{"BG":bg},"characterSourceId":id,"raw":row}));
                        }
                    }
                    if variants.is_empty() {
                        warnings.push(format!(
                            "POLE_PERSO:{id} : alias ou forme dynamique non résolu"
                        ));
                    } else {
                        groups.push((
                            format!("POLE_PERSO:{id}"),
                            format!("{id} · POLE_PERSO"),
                            variants,
                            row.clone(),
                        ));
                    }
                }
            }
            let keys: BTreeSet<_> = ["POLE_STAGES", "POLE_INTIMES"]
                .iter()
                .filter_map(|k| data["tables"][*k].as_object())
                .flat_map(|m| m.keys())
                .collect();
            for key in keys {
                let mut variants = Vec::new();
                for (table, label) in [("POLE_STAGES", "scene"), ("POLE_INTIMES", "intime")] {
                    if let Some(bg) = data["tables"][table][key].as_str() {
                        variants.push(json!({"name":label,"blocks":{"BG":bg},"raw":bg}));
                    }
                }
                if !variants.is_empty() {
                    groups.push((format!("POLE_THEME:{key}"),format!("{key} · décor pole"),variants,json!({"POLE_STAGES":data["tables"]["POLE_STAGES"][key],"POLE_INTIMES":data["tables"]["POLE_INTIMES"][key]})));
                }
            }
        }
        Some("json") => {
            let value: Value = serde_json::from_slice(&bytes)?;
            let rows = value
                .as_array()
                .or_else(|| value["characters"].as_array())
                .or_else(|| value["persos"].as_array())
                .or_else(|| value["valides"].as_array())
                .cloned()
                .unwrap_or_else(|| vec![value]);
            for (index, row) in rows.iter().enumerate() {
                let base = blocks(row);
                let id = row["id"]
                    .as_str()
                    .or_else(|| row["slug"].as_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("ligne-{}", index + 1));
                let name = row["name"]
                    .as_str()
                    .or_else(|| row["nom"].as_str())
                    .unwrap_or(&id)
                    .to_owned();
                let mut variants = Vec::new();
                if base.get("BG").is_some() || base.get("ACTION").is_some() {
                    variants.push(json!({"name":"Principale","blocks":base,"characterSourceId":row.get("bank_id").or_else(||row.get("id")),"raw":row}));
                }
                if let Some(options) = row["bg_options"].as_array() {
                    for (i, bg) in options.iter().enumerate() {
                        if let Some(content) = bg["scene_bg"].as_str() {
                            let mut b = base.clone();
                            b["BG"] = json!(content);
                            variants.push(json!({"name":bg["bg_name"].as_str().map(str::to_owned).unwrap_or_else(||format!("Décor {}",i+1)),"blocks":b,"characterSourceId":row["id"],"raw":bg}));
                        }
                    }
                }
                if let Some(options) = row["decors"].as_array() {
                    for (i, bg) in options.iter().enumerate() {
                        if bg.is_string() {
                            let mut b = base.clone();
                            b["BG"] = bg.clone();
                            variants.push(json!({"name":format!("Décor {}",i+1),"blocks":b,"characterSourceId":row["id"],"raw":bg}));
                        }
                    }
                }
                if variants.is_empty() {
                    warnings.push(format!("{id} : aucun BG/ACTION exporté ; ignoré"));
                } else {
                    groups.push((format!("json:{id}"), name, variants, row.clone()));
                }
            }
        }
        _ => {
            return Err("Format de scène non reconnu : JSON exporté ou tables Python POLE_*".into())
        }
    }
    let mut entries = Vec::new();
    let mut keys = BTreeSet::new();
    for (key, name, variants, raw) in groups {
        if !keys.insert(key.clone()) {
            return Err(format!("Clé de scène dupliquée : {key}").into());
        }
        let mut notes=vec!["P1/P2, modèles, LoRA et mappings doivent être choisis explicitement dans le compositeur. Données brutes conservées.".into()];
        if variants.iter().any(|v| {
            v["blocks"].as_object().is_some_and(|b| {
                b.values()
                    .any(|v| v.as_str().is_some_and(|s| s.contains("{accent}")))
            })
        }) {
            notes.push(
                "Variable {accent} non résolue : saisissez le texte final avant préparation."
                    .into(),
            );
        }
        entries.push(SceneEntry {
            path: path.to_string_lossy().into(),
            sha256: sha.clone(),
            key,
            name,
            variants,
            warnings: notes,
            raw,
            duplicate: false,
        });
    }
    if entries.len() > 10000 {
        return Err("Maximum 10000 scènes par fichier".into());
    }
    Ok((entries, warnings))
}
impl Database {
    pub fn grok_scene_preview(&self, paths: &[String]) -> Result<Value> {
        if paths.is_empty() || paths.len() > 64 {
            return Err("Choisissez 1 à 64 fichiers".into());
        }
        let mut entries = Vec::new();
        let mut warnings = Vec::new();
        let mut bytes = 0u64;
        for path in paths {
            bytes = bytes.saturating_add(std::fs::metadata(path)?.len());
            if bytes > 64 * 1024 * 1024 {
                return Err("Lot supérieur à 64 Mio".into());
            }
            match parse_scenes(Path::new(path)) {
                Ok((rows, notes)) => {
                    warnings.extend(notes);
                    for mut row in rows {
                        row.duplicate=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM grok_scenes WHERE source_path=?1 AND source_sha256=?2 AND source_key=?3)",params![row.path,row.sha256,row.key],|r|r.get(0))?;
                        entries.push(row);
                    }
                }
                Err(e) => warnings.push(format!("{path} : {e}")),
            }
        }
        Ok(json!({"entries":entries,"warnings":warnings}))
    }
    pub fn grok_scene_import(
        &self,
        choices: &[SceneChoice],
        confirmed: bool,
    ) -> Result<Vec<String>> {
        if !confirmed || choices.is_empty() || choices.len() > 10000 {
            return Err("Confirmation et sélection requises".into());
        }
        let paths: BTreeSet<_> = choices.iter().map(|c| &c.path).collect();
        if paths.len() > 64 {
            return Err("Maximum 64 fichiers".into());
        }
        let mut files = BTreeMap::new();
        let mut bytes = 0u64;
        for path in paths {
            bytes = bytes.saturating_add(std::fs::metadata(path)?.len());
            if bytes > 64 * 1024 * 1024 {
                return Err("Lot supérieur à 64 Mio".into());
            }
            files.insert(path, parse_scenes(Path::new(path))?.0);
        }
        let tx = self.connection.unchecked_transaction()?;
        let mut ids = Vec::new();
        for choice in choices {
            let entry = files[&choice.path]
                .iter()
                .find(|e| e.key == choice.key && e.sha256 == choice.sha256)
                .ok_or("Source modifiée depuis l’aperçu")?;
            let old:Option<String>=tx.query_row("SELECT id FROM grok_scenes WHERE source_path=?1 AND source_sha256=?2 AND source_key=?3",params![entry.path,entry.sha256,entry.key],|r|r.get(0)).optional()?;
            let id = if let Some(old) = old {
                old
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                let raw = serde_json::to_string(entry)?;
                if raw.len() > 2_000_000 {
                    return Err("Scène supérieure à 2 Mo".into());
                }
                tx.execute("INSERT INTO grok_scenes(id,source_path,source_sha256,source_key,name,document_json) VALUES(?1,?2,?3,?4,?5,?6)",params![id,entry.path,entry.sha256,entry.key,entry.name,raw])?;
                id
            };
            ids.push(id);
        }
        tx.commit()?;
        Ok(ids)
    }
    pub fn grok_scenes(&self) -> Result<Value> {
        let mut q=self.connection.prepare("SELECT id,name,source_path,source_key,source_sha256,imported_at FROM grok_scenes ORDER BY name,imported_at DESC,id")?;
        let rows=q.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"path":r.get::<_,String>(2)?,"key":r.get::<_,String>(3)?,"sha256":r.get::<_,String>(4)?,"importedAt":r.get::<_,String>(5)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(json!(rows))
    }
    pub fn grok_scene(&self, id: &str) -> Result<Value> {
        let text: String = self.connection.query_row(
            "SELECT document_json FROM grok_scenes WHERE id=?",
            [id],
            |r| r.get(0),
        )?;
        Ok(serde_json::from_str(&text)?)
    }
}
