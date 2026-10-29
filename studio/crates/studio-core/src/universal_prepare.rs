//! F13 universal preparation profiles: workflow-specific mappings, character
//! selection, BG/action resolution and quick controls. Preparation always uses
//! the existing draft path on a copy of the API graph.
use crate::{
    gallery::digest,
    workflow::{Edit, JobCharacter},
    Database, Result,
};
use rusqlite::params;
use serde_json::{json, Value};

const COMMANDS: [&str; 10] = [
    "P1",
    "P2",
    "ANATOMY_P1",
    "ANATOMY_P2",
    "BG",
    "ACTION",
    "QUICK",
    "QUALITY",
    "ADDITIONAL",
    "NEG",
];

fn signature(graph: &Value) -> Result<Value> {
    let mut nodes = Vec::new();
    for (id, node) in graph.as_object().ok_or("Graphe API invalide")? {
        let mut inputs: Vec<&String> = node["inputs"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, _)| k)
            .collect();
        inputs.sort();
        nodes.push(json!({"id":id,"class":node["class_type"],"inputs":inputs}));
    }
    nodes.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(json!({"nodes":nodes,"sha256":digest(serde_json::to_string(&nodes)?.as_bytes())}))
}

fn valid_profile_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.len() > 200 {
        return Err("Nom de profil invalide".into());
    }
    Ok(())
}

fn pick(candidates: &[Value], selected: Option<&Value>, label: &str) -> Result<Value> {
    if let Some(selected) = selected.filter(|v| !v.is_null()) {
        if candidates.contains(selected) {
            return Ok(selected.clone());
        }
        return Err(format!("{label} sélectionné absent des candidats").into());
    }
    if candidates.is_empty() {
        return Err(format!("Aucun {label} candidat").into());
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as usize)
        .unwrap_or(0);
    Ok(candidates[now % candidates.len()].clone())
}

impl Database {
    /// Last accepted submission, never a preview. Ambiguous independent seeds remain unspecified.
    pub fn last_queued_seed(&self, workflow: &str) -> Result<Value> {
        use rusqlite::OptionalExtension;
        let snapshot:Option<String>=self.connection.query_row(
            "SELECT j.request_snapshot_json FROM job_events e JOIN generation_jobs j ON j.id=e.job_id WHERE j.workflow_revision_id=? AND e.event_type='submitted' ORDER BY e.id DESC LIMIT 1",
            [workflow],|r|r.get(0)).optional()?;
        let Some(snapshot) = snapshot else {
            return Ok(Value::Null);
        };
        let snapshot: Value = serde_json::from_str(&snapshot)?;
        let nodes: Vec<_> = snapshot["prompt"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(_, n)| n)
            .collect();
        let has_master = nodes.iter().any(|n| n["class_type"] == "Seed (rgthree)");
        let mut seeds = std::collections::BTreeSet::new();
        for n in nodes {
            if has_master && n["class_type"] != "Seed (rgthree)" {
                continue;
            }
            for key in ["seed", "noise_seed"] {
                if let Some(v) = n["inputs"][key].as_u64() {
                    seeds.insert(v.to_string());
                }
            }
        }
        let mut seeds = seeds.into_iter();
        Ok(match (seeds.next(), seeds.next()) {
            (Some(seed), None) => json!(seed),
            _ => Value::Null,
        })
    }
}

impl Database {
    pub fn prepare_profiles(&self) -> Result<Vec<Value>> {
        let mut q = self.connection.prepare(
            "SELECT p.id,p.name,p.workflow_revision_id,p.mode,p.config_json,p.signature_json,p.updated_at,w.name
             FROM prepare_profiles p JOIN workflow_revisions w ON w.id=p.workflow_revision_id
             ORDER BY p.updated_at DESC,p.id",
        )?;
        type ProfileRow = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        );
        let rows: Vec<ProfileRow> = q
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        rows.into_iter().map(|(id,name,workflow,mode,config,sig,updated,workflow_name)| Ok(json!({
            "id":id,"name":name,"workflowId":workflow,"mode":mode,"config":serde_json::from_str::<Value>(&config)?,
            "signature":serde_json::from_str::<Value>(&sig)?,"updatedAt":updated,"workflowName":workflow_name
        }))).collect()
    }

    pub fn prepare_profile(&self, id: &str) -> Result<Value> {
        let (name, workflow, mode, config, stored): (String, String, String, String, String) = self
            .connection
            .query_row(
                "SELECT name,workflow_revision_id,mode,config_json,signature_json FROM prepare_profiles WHERE id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .map_err(|_| "Profil de préparation inconnu")?;
        let (_, document) = self.workflow_document(&workflow)?;
        let current = signature(&serde_json::from_str(&document)?)?;
        Ok(
            json!({"id":id,"name":name,"workflowId":workflow,"mode":mode,
            "config":serde_json::from_str::<Value>(&config)?,"signature":serde_json::from_str::<Value>(&stored)?,
            "currentSignature":current,"changed":current["sha256"] != serde_json::from_str::<Value>(&stored)?["sha256"]}),
        )
    }

    pub fn save_prepare_profile(
        &self,
        id: Option<&str>,
        name: &str,
        workflow: &str,
        mode: &str,
        config: &Value,
    ) -> Result<Value> {
        valid_profile_name(name)?;
        if !["solo", "duo"].contains(&mode) {
            return Err("Mode de préparation invalide".into());
        }
        if config.to_string().len() > 2_000_000 {
            return Err("Configuration de préparation trop volumineuse".into());
        }
        let (_, text) = self.workflow_document(workflow)?;
        let graph: Value = serde_json::from_str(&text)?;
        let sig = signature(&graph)?;
        let id = id
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.connection.execute(
            "INSERT INTO prepare_profiles(id,name,workflow_revision_id,mode,config_json,signature_json)
             VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name,workflow_revision_id=excluded.workflow_revision_id,
             mode=excluded.mode,config_json=excluded.config_json,signature_json=excluded.signature_json,
             updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![id, name, workflow, mode, config.to_string(), sig.to_string()],
        )?;
        self.prepare_profile(&id)
    }

    pub fn delete_prepare_profile(&self, id: &str) -> Result<()> {
        self.connection
            .execute("DELETE FROM prepare_profiles WHERE id=?", [id])?;
        Ok(())
    }

    /// Resolve the profile for preview. It returns the exact edits and the
    /// resolved random BG/action; nothing is submitted and no workflow changes.
    pub fn prepare_universal(&self, id: &str, info: &Value) -> Result<Value> {
        let profile = self.prepare_profile(id)?;
        if profile["changed"] == true {
            return Err(
                "Workflow modifié depuis le profil : remapper ou confirmer les changements".into(),
            );
        }
        let workflow = profile["workflowId"].as_str().ok_or("Workflow absent")?;
        let config = &profile["config"];
        let mode = profile["mode"].as_str().unwrap_or("solo");
        let mut people = Vec::new();
        let mut resolution = serde_json::Map::new();
        for role in ["P1", "P2"] {
            if role == "P2" && mode == "solo" {
                continue;
            }
            let key = role.to_ascii_lowercase();
            let Some(character) = config[&key]["characterId"]
                .as_str()
                .filter(|s| !s.is_empty())
            else {
                if role == "P2" {
                    continue;
                }
                return Err("Choisir un personnage P1".into());
            };
            let mut sheet = self.character_sheet(character)?;
            for (command, field) in [
                (role, "identityPrompt"),
                (&format!("ANATOMY_{role}"), "anatomyPrompt"),
            ] {
                if let Some(value) = config["promptOverrides"][command].as_str() {
                    sheet[field] = json!(value);
                }
            }
            people.push(JobCharacter {
                role: role.into(),
                character_id: character.into(),
                variant_id: config[&key]["variantId"].as_str().map(str::to_owned),
            });
            resolution.insert(format!("{key}Name"), sheet["name"].clone());
            resolution.insert(format!("{key}Prompt"), sheet["identityPrompt"].clone());
            resolution.insert(format!("{key}Anatomy"), sheet["anatomyPrompt"].clone());
        }
        let bg = &config["bg"];
        let mut candidates: Vec<Value> = Vec::new();
        match bg["source"].as_str().unwrap_or("fixed") {
            "fixed" => candidates.push(bg["text"].clone()),
            "random" => candidates.extend(bg["candidates"].as_array().cloned().unwrap_or_default()),
            "character" | "combined" => {
                let roles: &[&str] = if bg["source"] == "combined" {
                    &["p1", "p2"]
                } else {
                    &["p1"]
                };
                for role in roles {
                    let Some(character) = config[role]["characterId"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                    else {
                        continue;
                    };
                    let sheet = self.character_sheet(character)?;
                    for item in sheet["backgrounds"].as_array().into_iter().flatten() {
                        let variant = item["variantId"].as_str().unwrap_or_default();
                        let enabled = bg["enabledByVariant"].as_array().is_none_or(|list| {
                            list.is_empty() || list.iter().any(|v| v == variant)
                        });
                        if enabled {
                            candidates.push(item["bg"].clone());
                        }
                    }
                }
            }
            other => return Err(format!("Source BG inconnue : {other}").into()),
        }
        candidates.retain(|v| v.as_str().is_some_and(|s| !s.trim().is_empty()));
        let selected_bg = pick(&candidates, bg.get("selected"), "BG")?;
        resolution.insert("bg".into(), selected_bg.clone());
        let action = &config["action"];
        let mut actions: Vec<Value> = vec![];
        match action["source"].as_str().unwrap_or("fixed") {
            "fixed" => {
                if let Some(text) = action["text"].as_str() {
                    actions.push(json!(text));
                }
            }
            "random" | "custom" => {
                for item in action["candidates"].as_array().into_iter().flatten() {
                    let allowed = item["modes"]
                        .as_array()
                        .is_none_or(|m| m.iter().any(|v| v == mode));
                    if allowed {
                        actions.push(item["text"].clone());
                    }
                }
            }
            other => return Err(format!("Source action inconnue : {other}").into()),
        }
        let selected_action = pick(&actions, action.get("selected"), "action")?;
        resolution.insert("action".into(), selected_action.clone());
        let mut edits: Vec<Edit> = Vec::new();
        let mut add =
            |section: Option<&str>, node: &str, input: &str, value: &Value| -> Result<()> {
                if !value.is_string() {
                    return Err("Valeur de préparation non textuelle".into());
                }
                if edits
                    .iter()
                    .any(|e| e.node_id == node && e.input_name == input)
                {
                    return Err(
                        format!("Deux commandes visent la même entrée {node}.{input}").into(),
                    );
                }
                edits.push(Edit {
                    section: section.map(str::to_owned),
                    node_id: node.into(),
                    input_name: input.into(),
                    value: value.as_str().unwrap_or_default().into(),
                });
                Ok(())
            };
        let mappings = config["mappings"].as_object().ok_or("Mappings absents")?;
        let target = |command: &str| -> Result<(String, String)> {
            let entry = mappings
                .get(command)
                .ok_or_else(|| format!("Mapping {command} absent"))?;
            Ok((
                entry["nodeId"].as_str().ok_or("nodeId absent")?.into(),
                entry["inputName"]
                    .as_str()
                    .ok_or("inputName absent")?
                    .into(),
            ))
        };
        for (command, role) in [("P1", "p1"), ("P2", "p2")] {
            if role == "p2" && mode == "solo" {
                continue;
            }
            let Some(character) = config[role]["characterId"]
                .as_str()
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            let sheet = self.character_sheet(character)?;
            let (node, input) = target(command)?;
            add(
                Some(command),
                &node,
                &input,
                config["promptOverrides"]
                    .get(command)
                    .unwrap_or(&sheet["identityPrompt"]),
            )?;
            let anatomy = format!("ANATOMY_{command}");
            if let Some((node, input)) = mappings.get(&anatomy).map(|e| {
                (
                    e["nodeId"].as_str().unwrap_or_default(),
                    e["inputName"].as_str().unwrap_or_default(),
                )
            }) {
                add(
                    Some(&anatomy),
                    node,
                    input,
                    config["promptOverrides"]
                        .get(&anatomy)
                        .unwrap_or(&sheet["anatomyPrompt"]),
                )?;
            }
        }
        let (node, input) = target("BG")?;
        add(Some("BG"), &node, &input, &selected_bg)?;
        let (node, input) = target("ACTION")?;
        add(Some("ACTION"), &node, &input, &selected_action)?;
        for (command, key) in [("QUALITY", "quality"), ("ADDITIONAL", "additional")] {
            let Some(text) = config[key].as_str().filter(|s| !s.trim().is_empty()) else {
                continue;
            };
            let (node, input) = target(command)?;
            add(Some(command), &node, &input, &json!(text))?;
        }
        if let Some(text) = config["negative"].as_str() {
            if mappings.contains_key("NEG") {
                let (node, input) = target("NEG")?;
                add(Some("NEG"), &node, &input, &json!(text))?;
            } else if !text.is_empty() {
                return Err("Mapping NEG absent".into());
            }
        }
        for quick in config["quick"].as_array().into_iter().flatten() {
            if quick["visible"] == false {
                continue;
            }
            let Some(value) = quick["value"].as_str().filter(|s| !s.is_empty()) else {
                continue;
            };
            let node = quick["nodeId"].as_str().ok_or("quick.nodeId absent")?;
            let input = quick["inputName"]
                .as_str()
                .ok_or("quick.inputName absent")?;
            add(quick["section"].as_str(), node, input, &json!(value))?;
        }
        let mut patches = Vec::new();
        let mut master_seed = Value::Null;
        if let Some(text) = config["masterSeed"]
            .as_str()
            .filter(|t| !t.trim().is_empty())
        {
            let sentinel = matches!(text, "-1" | "-2" | "-3");
            let seed_value = if sentinel {
                json!(text.parse::<i64>().map_err(|_| "Master seed invalide")?)
            } else {
                json!(text
                    .parse::<u64>()
                    .map_err(|_| "Master seed u64 invalide")?)
            };
            master_seed = json!(text);
            let (_, document) = self.workflow_document(workflow)?;
            let graph: Value = serde_json::from_str(&document)?;
            let mut seed_nodes: Vec<&String> = graph
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(_, n)| {
                    n["class_type"]
                        .as_str()
                        .is_some_and(|c| c.starts_with("Seed ("))
                })
                .map(|(id, _)| id)
                .collect();
            // Master seed follows the workflow's real Seed node when present.
            // Direct KSampler seeds are only patched when no Seed node exists.
            let all_seed_inputs: Vec<(&String, &String)> = graph
                .as_object()
                .into_iter()
                .flatten()
                .flat_map(|(id, n)| {
                    n["inputs"]
                        .as_object()
                        .into_iter()
                        .flatten()
                        .filter(|(input, value)| {
                            (input.as_str() == "seed" || input.as_str() == "noise_seed")
                                && value.is_number()
                        })
                        .map(move |(input, _)| (id, input))
                })
                .collect();
            if seed_nodes.is_empty() {
                if sentinel {
                    return Err("Seed aléatoire -1 : workflow sans node Seed (rgthree)".into());
                }
                for (node, input) in all_seed_inputs {
                    patches.push(json!({"node":node,"input":input,"value":seed_value}));
                }
            } else {
                seed_nodes.sort();
                for node in seed_nodes {
                    patches.push(json!({"node":node,"input":"seed","value":seed_value}));
                }
            }
        }
        let commands: Vec<&str> = mappings.keys().map(String::as_str).collect();
        if commands.iter().any(|c| !COMMANDS.contains(c)) {
            return Err("Commande de mapping inconnue".into());
        }
        let mut lora_edits = Vec::new();
        if config["lora"]["selections"]
            .as_array()
            .is_some_and(|s| !s.is_empty())
        {
            let lora = self.preview_lora(workflow, &config["lora"])?;
            for edit in lora["edits"].as_array().into_iter().flatten() {
                patches
                    .push(json!({"node":edit["node"],"input":edit["input"],"value":edit["after"]}));
                lora_edits.push(edit.clone());
            }
        }
        if !lora_edits.is_empty() {
            resolution.insert("loraEdits".into(), json!(lora_edits));
        }
        if !master_seed.is_null() {
            resolution.insert("masterSeed".into(), master_seed);
        }
        let prepared =
            self.prepare_job_with_characters_extra(workflow, &edits, info, &people, &patches)?;
        Ok(
            json!({"prepared":prepared,"resolution":resolution,"edits":edits,"loraEdits":lora_edits,"mode":mode,"profile":profile}),
        )
    }
}

impl Database {
    pub fn export_prepare_profile(&self, id: &str) -> Result<Value> {
        let profile = self.prepare_profile(id)?;
        Ok(
            json!({"format":"saya-prepare-profile","version":1,"name":profile["name"],
            "mode":profile["mode"],"config":profile["config"],"sourceSignature":profile["signature"]}),
        )
    }

    pub fn import_prepare_profile(&self, text: &str, workflow: &str) -> Result<Value> {
        let value: Value = serde_json::from_str(text)?;
        if value["format"] != "saya-prepare-profile" || value["version"] != 1 {
            return Err("Profil de préparation inconnu".into());
        }
        self.save_prepare_profile(
            None,
            value["name"].as_str().ok_or("Nom de profil absent")?,
            workflow,
            value["mode"].as_str().ok_or("Mode absent")?,
            value.get("config").ok_or("Configuration absente")?,
        )
    }

    /// Compares stored mappings/signature with a target workflow. Suggestions
    /// are only proposals; no remapping is applied here.
    pub fn audit_prepare_profile(&self, id: &str, workflow: &str) -> Result<Value> {
        let profile = self.prepare_profile(id)?;
        let (_, text) = self.workflow_document(workflow)?;
        let graph: Value = serde_json::from_str(&text)?;
        let current = signature(&graph)?;
        let before = profile["signature"]["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let after = current["nodes"].as_array().cloned().unwrap_or_default();
        let key = |n: &Value| n["id"].as_str().unwrap_or_default().to_owned();
        let old_ids: std::collections::BTreeSet<String> = before.iter().map(key).collect();
        let new_ids: std::collections::BTreeSet<String> = after.iter().map(key).collect();
        let added: Vec<String> = new_ids.difference(&old_ids).cloned().collect();
        let removed: Vec<String> = old_ids.difference(&new_ids).cloned().collect();
        let fields = self.workflow(workflow)?.fields;
        let mut mappings = serde_json::Map::new();
        for (command, entry) in profile["config"]["mappings"]
            .as_object()
            .into_iter()
            .flatten()
        {
            let node = entry["nodeId"].as_str().unwrap_or_default();
            let input = entry["inputName"].as_str().unwrap_or_default();
            let exact = fields
                .iter()
                .find(|f| f.node_id == node && f.input_name == input);
            let status = if exact.is_some() { "valid" } else { "missing" };
            let suggestions: Vec<Value> = if status == "valid" {
                vec![]
            } else {
                fields.iter().filter(|f| f.input_name == input || f.class_type == entry["classType"].as_str().unwrap_or(""))
                    .take(5).map(|f| json!({"nodeId":f.node_id,"inputName":f.input_name,"classType":f.class_type,"valueType":f.value_type,"requiresConfirmation":true})).collect()
            };
            mappings.insert(
                command.clone(),
                json!({"status":status,"suggestions":suggestions}),
            );
        }
        Ok(
            json!({"profile":id,"workflow":workflow,"changed":profile["signature"]["sha256"] != current["sha256"],
            "addedNodes":added,"removedNodes":removed,"mappings":mappings}),
        )
    }

    pub fn remap_prepare_profile(
        &self,
        id: &str,
        workflow: &str,
        mappings: &Value,
        confirmed: bool,
    ) -> Result<Value> {
        if !confirmed {
            return Err("Confirmation explicite requise pour remapper".into());
        }
        let profile = self.prepare_profile(id)?;
        let fields = self.workflow(workflow)?.fields;
        for (command, entry) in mappings.as_object().into_iter().flatten() {
            let node = entry["nodeId"].as_str().ok_or("nodeId absent")?;
            let input = entry["inputName"].as_str().ok_or("inputName absent")?;
            if !fields
                .iter()
                .any(|f| f.node_id == node && f.input_name == input)
            {
                return Err(format!("Cible inconnue pour {command} : {node}.{input}").into());
            }
        }
        let mut config = profile["config"].clone();
        config["mappings"] = mappings.clone();
        self.save_prepare_profile(
            Some(id),
            profile["name"].as_str().unwrap_or("Profil"),
            workflow,
            profile["mode"].as_str().unwrap_or("solo"),
            &config,
        )
    }
}

impl Database {
    /// Nodes that can receive a LoRA selection, read from the workflow document.
    pub fn lora_loaders(&self, workflow: &str) -> Result<Vec<Value>> {
        let (_, text) = self.workflow_document(workflow)?;
        let graph: Value = serde_json::from_str(&text)?;
        let mut loaders = Vec::new();
        for (id, node) in graph.as_object().into_iter().flatten() {
            let class = node["class_type"].as_str().unwrap_or_default();
            let lower = class.to_ascii_lowercase();
            if !lower.contains("lora") || !lower.contains("loader") && !lower.contains("manager") {
                continue;
            }
            let mode = if class == "Lora Loader (LoraManager)" {
                "manager"
            } else {
                "simple"
            };
            let inputs: Vec<&String> = node["inputs"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(k, _)| k)
                .collect();
            loaders.push(json!({"nodeId":id,"classType":class,"mode":mode,"inputs":inputs}));
        }
        loaders.sort_by(|a, b| a["nodeId"].as_str().cmp(&b["nodeId"].as_str()));
        Ok(loaders)
    }

    /// Exact, read-only preview of LoRA edits. It never writes the graph nor
    /// submits; unsupported/ambiguous loaders are refused.
    pub fn preview_lora(&self, workflow: &str, config: &Value) -> Result<Value> {
        let (_, text) = self.workflow_document(workflow)?;
        let mut graph: Value = serde_json::from_str(&text)?;
        let mut edits = Vec::new();
        let selections = config["selections"].as_array().cloned().unwrap_or_default();
        if selections.is_empty() {
            return Err("Aucune LoRA sélectionnée".into());
        }
        for selection in &selections {
            if selection["enabled"] == false {
                continue;
            }
            let node_id = selection["loaderNode"]
                .as_str()
                .ok_or("loaderNode absent")?;
            let name = selection["name"].as_str().ok_or("Nom LoRA absent")?;
            let model = selection["modelWeight"]
                .as_f64()
                .ok_or("Poids modèle absent")?;
            let clip = selection["clipWeight"].as_f64().unwrap_or(model);
            if !model.is_finite()
                || !clip.is_finite()
                || !(-10.0..=10.0).contains(&model)
                || !(-10.0..=10.0).contains(&clip)
            {
                return Err(format!("Poids LoRA hors bornes pour {name}").into());
            }
            if name.contains(['\n', '\r', '<', '>']) || name.len() > 300 {
                return Err("Nom de LoRA invalide".into());
            }
            let node = graph
                .get_mut(node_id)
                .ok_or_else(|| format!("Loader {node_id} absent"))?;
            let class = node["class_type"].as_str().unwrap_or_default().to_owned();
            if class == "Lora Loader (LoraManager)" {
                if selections
                    .iter()
                    .filter(|s| s["loaderNode"] == selection["loaderNode"] && s["enabled"] != false)
                    .count()
                    > 1
                {
                    return Err(format!("Loader manager {node_id} : composition multiple non prise en charge dans cette tranche").into());
                }
                let token = format!("<lora:{name}:{model}:{clip}>");
                let before = node["inputs"]["text"].as_str().unwrap_or("").to_owned();
                if !before.contains("<lora:") {
                    // Preserve non-LoRA text; the explicit preview shows the whole value.
                }
                node["inputs"]["text"] = json!(token);
                edits.push(json!({"node":node_id,"input":"text","classType":class,"before":before,"after":token,"mode":"manager"}));
            } else if class.contains("LoraLoader") || class == "LoraLoader" {
                if selections
                    .iter()
                    .filter(|s| s["loaderNode"] == selection["loaderNode"] && s["enabled"] != false)
                    .count()
                    > 1
                {
                    return Err(format!(
                        "Loader simple {node_id} ne peut recevoir qu'une LoRA à la fois"
                    )
                    .into());
                }
                let before_name = node["inputs"]["lora_name"].clone();
                let before_model = node["inputs"]["strength_model"].clone();
                let before_clip = node["inputs"]["strength_clip"].clone();
                node["inputs"]["lora_name"] = json!(name);
                node["inputs"]["strength_model"] = json!(model);
                node["inputs"]["strength_clip"] = json!(clip);
                edits.push(json!({"node":node_id,"input":"lora_name","classType":class,"before":before_name,"after":name,"mode":"simple"}));
                edits.push(json!({"node":node_id,"input":"strength_model","classType":class,"before":before_model,"after":model,"mode":"simple"}));
                edits.push(json!({"node":node_id,"input":"strength_clip","classType":class,"before":before_clip,"after":clip,"mode":"simple"}));
            } else {
                return Err(format!(
                    "Loader {node_id} ({class}) incompatible : aucun contrat LoRA connu"
                )
                .into());
            }
        }
        Ok(json!({"edits":edits,"previewGraph":graph,"selectionCount":edits.len()}))
    }
}

#[cfg(test)]
mod submission_tests {
    use super::*;
    #[test]
    fn last_seed_uses_submission_order_and_ignores_drafts() {
        let db = Database::in_memory().unwrap();
        let wf = db
            .import_workflow(
                "test",
                &json!({"1":{"class_type":"Seed (rgthree)","inputs":{"seed":9007199254740993u64}}})
                    .to_string(),
            )
            .unwrap();
        for (id, seed) in [("a", 9007199254740993u64), ("b", 42u64)] {
            let snapshot =
                json!({"prompt":{"1":{"class_type":"Seed (rgthree)","inputs":{"seed":seed}}}});
            db.connection.execute("INSERT INTO generation_jobs(id,workflow_revision_id,seed,request_snapshot_json) VALUES(?1,?2,?3,?4)",params![id,wf,seed.to_string(),snapshot.to_string()]).unwrap();
        }
        assert_eq!(db.last_queued_seed(&wf).unwrap(), Value::Null);
        db.job_event("b", "submitted", &json!({})).unwrap();
        assert_eq!(db.last_queued_seed(&wf).unwrap(), json!("42"));
        db.job_event("a", "submitted", &json!({})).unwrap();
        assert_eq!(db.last_queued_seed(&wf).unwrap(), json!("9007199254740993"));
    }
}
