use crate::{contracts::Seed, gallery::digest, Database, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
pub(crate) mod lora_manager;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub node_id: String,
    pub input_name: String,
    pub class_type: String,
    pub value_type: String,
    pub value: String,
    pub enabled: bool,
    pub section: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    #[serde(default)]
    pub section: Option<String>,
    pub node_id: String,
    pub input_name: String,
    pub value: String,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobCharacter {
    pub role: String,
    pub character_id: String,
    pub variant_id: Option<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub fields: Vec<Field>,
    pub node_count: usize,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    pub job_id: String,
    pub snapshot_hash: String,
    pub request_json: String,
    pub removed: Vec<String>,
    pub changes: Vec<Value>,
    pub node_count: usize,
}

pub fn api_graph(text: &str) -> Result<Value> {
    if text.len() > 16_000_000 {
        return Err("Workflow supérieur à 16 Mo".into());
    }
    let mut value: Value = serde_json::from_str(text)?;
    if value.get("nodes").is_some() {
        return Err("JSON UI non compatible : exporter ComfyUI en format API. Les subgraphs UI ne sont pas convertis automatiquement.".into());
    }
    if value.get("prompt").is_some_and(Value::is_object) {
        value = value["prompt"].clone()
    }
    let nodes = value
        .as_object()
        .ok_or("Le workflow doit être un objet API")?;
    if nodes.is_empty() || nodes.len() > 2000 {
        return Err("Nombre de nodes invalide".into());
    }
    for (id, node) in nodes {
        if id.is_empty()
            || node.get("class_type").and_then(Value::as_str).is_none()
            || !node.get("inputs").is_some_and(Value::is_object)
        {
            return Err(format!("Node API invalide : {id}").into());
        }
    }
    Ok(value)
}
fn fields(graph: &Value) -> Vec<Field> {
    let mut result = Vec::new();
    for (id, node) in graph.as_object().into_iter().flatten() {
        // Graphs are validated by `api_graph`; a node without class_type is skipped, never a panic.
        let Some(class_type) = node["class_type"].as_str() else {
            continue;
        };
        for (key, value) in node["inputs"].as_object().into_iter().flatten() {
            if (node["class_type"] == "Lora Loader (LoraManager)"
                && ["text", "__lm_autocomplete_meta_text"].contains(&key.as_str()))
                || (node["class_type"] == "Image Comparer (rgthree)" && key == "rgthree_comparer")
            {
                continue;
            }
            if value.is_string() || value.is_number() || value.is_boolean() {
                let kind = if value.is_string() {
                    "string"
                } else if value.is_boolean() {
                    "boolean"
                } else if key == "seed" || key == "noise_seed" {
                    "seed"
                } else if value.is_i64() || value.is_u64() {
                    "integer"
                } else {
                    "number"
                };
                result.push(Field {
                    node_id: id.clone(),
                    input_name: key.clone(),
                    class_type: class_type.into(),
                    value_type: kind.into(),
                    value: value
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| value.to_string()),
                    enabled: false,
                    section: None,
                });
            }
        }
    }
    result
}
fn removed_tokens(old: &str, new: &str) -> Vec<String> {
    let split = |s: &str| -> Vec<String> {
        s.split([',', '\n'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let mut remaining = BTreeMap::<String, usize>::new();
    for word in split(new) {
        *remaining.entry(word).or_default() += 1
    }
    let mut removed = Vec::new();
    for word in split(old) {
        let count = remaining.entry(word.clone()).or_default();
        if *count > 0 {
            *count -= 1
        } else {
            removed.push(word)
        }
    }
    removed
}

impl Database {
    pub fn import_workflow(&self, name: &str, text: &str) -> Result<String> {
        if name.trim().is_empty() || name.len() > 300 {
            return Err("Nom de workflow invalide".into());
        }
        let graph = api_graph(text)?;
        let id = uuid::Uuid::new_v4().to_string();
        self.connection.execute("INSERT INTO workflow_revisions(id,name,source_sha256,format,document_json,adapter_id,adapter_version) VALUES(?1,?2,?3,'comfy_api',?4,'explicit-api',1)",params![id,name,digest(text.as_bytes()),graph.to_string()])?;
        Ok(id)
    }
    /// Per-run copy of a Saya phase graph: kept for job provenance, hidden from the
    /// workflow list and purged by `purge_internal_workflows` once nothing uses it.
    pub(crate) fn import_internal_workflow(&self, name: &str, text: &str) -> Result<String> {
        let id = self.import_workflow(name, text)?;
        self.connection.execute(
            "UPDATE workflow_revisions SET adapter_id='saya-phase' WHERE id=?",
            [&id],
        )?;
        Ok(id)
    }
    /// Deletes per-run Saya phase graphs that no job, recipe, A/B campaign or profile
    /// references any more, keeping the `keep` most recent ones.
    pub fn purge_internal_workflows(&self, keep: u32) -> Result<usize> {
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let unused = "SELECT w.id FROM workflow_revisions w WHERE w.adapter_id='saya-phase'
            AND w.id NOT IN (SELECT id FROM workflow_revisions WHERE adapter_id='saya-phase' ORDER BY rowid DESC LIMIT ?1)
            AND NOT EXISTS(SELECT 1 FROM generation_jobs j WHERE j.workflow_revision_id=w.id)
            AND NOT EXISTS(SELECT 1 FROM recipes r WHERE r.workflow_id=w.id)
            AND NOT EXISTS(SELECT 1 FROM ab_campaigns c WHERE c.workflow_revision_id=w.id)
            AND NOT EXISTS(SELECT 1 FROM prepare_profiles p WHERE p.workflow_revision_id=w.id)";
        tx.execute(&format!("DELETE FROM workflow_bindings WHERE workflow_revision_id IN ({unused})"), [keep])?;
        let removed = tx.execute(&format!("DELETE FROM workflow_revisions WHERE id IN ({unused})"), [keep])?;
        tx.commit()?;
        Ok(removed)
    }
    /// Only created at explicit submission, retained solely for job provenance.
    pub fn prepare_temporary_image(
        &self,
        graph: &Value,
        info: &Value,
        ui: Option<&Value>,
    ) -> Result<Prepared> {
        validate_graph(graph, info)?;
        let tx = self.connection.unchecked_transaction()?;
        let id = self.import_workflow("Image temporaire", &graph.to_string())?;
        self.connection.execute(
            "UPDATE workflow_revisions SET adapter_id='temporary-image' WHERE id=?",
            [&id],
        )?;
        let prepared = self.prepare_job_from_graph(
            &id,
            graph,
            info,
            json!({"temporaryImage":true,"uiWorkflow":ui}),
        )?;
        tx.commit()?;
        Ok(prepared)
    }
    pub fn workflows(&self) -> Result<Vec<Value>> {
        let mut q = self.connection.prepare("SELECT w.id,w.name,
            EXISTS(SELECT 1 FROM workflow_bindings b WHERE b.workflow_revision_id=w.id AND b.semantic_key='P1'),
            EXISTS(SELECT 1 FROM workflow_bindings b WHERE b.workflow_revision_id=w.id AND b.semantic_key='P2')
            FROM workflow_revisions w WHERE w.adapter_id NOT IN ('temporary-image','saya-phase') ORDER BY w.rowid DESC")?;
        let rows = q
            .query_map([], |r| {
                let profile = match (r.get::<_, bool>(2)?, r.get::<_, bool>(3)?) {
                    (true, true) => "couple",
                    (true, false) => "solo",
                    _ => "unconfigured",
                };
                Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"promptProfile":profile}))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
    pub fn workflow(&self, id: &str) -> Result<Workflow> {
        let (name, text): (String, String) = self.connection.query_row(
            "SELECT name,document_json FROM workflow_revisions WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let graph: Value = serde_json::from_str(&text)?;
        let mut fields = fields(&graph);
        for f in &mut fields {
            f.enabled=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM workflow_bindings WHERE workflow_revision_id=?1 AND node_id=?2 AND input_name=?3)",params![id,f.node_id,f.input_name],|r|r.get(0))?;
            if f.enabled {
                let key:String=self.connection.query_row("SELECT semantic_key FROM workflow_bindings WHERE workflow_revision_id=?1 AND node_id=?2 AND input_name=?3",params![id,f.node_id,f.input_name],|r|r.get(0))?;
                if ["QUALITY", "P1", "P2", "ACTION", "BG", "NEG"].contains(&key.as_str()) {
                    f.section = Some(key);
                }
            }
        }
        Ok(Workflow {
            id: id.into(),
            name,
            fields,
            node_count: graph.as_object().ok_or("Graphe invalide")?.len(),
        })
    }
    pub fn save_bindings(&mut self, id: &str, selected: &[Edit]) -> Result<()> {
        let available = self.workflow(id)?.fields;
        let tx = self.connection.transaction()?;
        tx.execute(
            "DELETE FROM workflow_bindings WHERE workflow_revision_id=?",
            [id],
        )?;
        let mut targets = std::collections::HashSet::new();
        for selection in selected {
            if !targets.insert((&selection.node_id, &selection.input_name)) {
                return Err("Deux sections ne peuvent pas viser la même entrée".into());
            }
            let f = available
                .iter()
                .find(|f| f.node_id == selection.node_id && f.input_name == selection.input_name)
                .ok_or("Champ inconnu ou lié")?;
            let key = if let Some(section) = &selection.section {
                if f.value_type != "string"
                    || !["QUALITY", "P1", "P2", "ACTION", "BG", "NEG"].contains(&section.as_str())
                {
                    return Err("Section de prompt incompatible avec ce champ".into());
                }
                section.clone()
            } else {
                format!("{}:{}", f.node_id, f.input_name)
            };
            tx.execute("INSERT INTO workflow_bindings(workflow_revision_id,semantic_key,node_id,expected_class,input_name,value_type) VALUES(?1,?2,?3,?4,?5,?6)",params![id,key,f.node_id,f.class_type,f.input_name,f.value_type])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn prepare_job(&self, id: &str, edits: &[Edit], node_info: &Value) -> Result<Prepared> {
        self.prepare_job_with_characters(id, edits, node_info, &[])
    }
    pub fn prepare_job_with_characters(
        &self,
        id: &str,
        edits: &[Edit],
        node_info: &Value,
        people: &[JobCharacter],
    ) -> Result<Prepared> {
        self.prepare_job_with_characters_extra(id, edits, node_info, people, &[])
    }

    /// Same preparation with explicit graph patches reserved for adapter
    /// inputs such as LoraManager `text`. Patches are reported as changes and
    /// validated structurally; no field is rewritten silently.
    pub fn prepare_job_with_characters_extra(
        &self,
        id: &str,
        edits: &[Edit],
        node_info: &Value,
        people: &[JobCharacter],
        patches: &[Value],
    ) -> Result<Prepared> {
        // IMMEDIATE: this transaction reads (characters, workflow) before inserting the job.
        // A deferred one fails at once with "database is locked" when another connection
        // (job sync, casting tick) commits in between; IMMEDIATE takes the write lock first
        // and waits through busy_timeout instead.
        let tx = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let characters = self.snapshot_characters(id, people)?;
        let original: String = self.connection.query_row(
            "SELECT document_json FROM workflow_revisions WHERE id=?",
            [id],
            |r| r.get(0),
        )?;
        let mut graph: Value = serde_json::from_str(&original)?;
        let workflow = self.workflow(id)?;
        let mut removed = Vec::new();
        let mut changes = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for edit in edits {
            if !seen.insert((&edit.node_id, &edit.input_name)) {
                return Err("Champ modifié plusieurs fois".into());
            }
            let f = workflow
                .fields
                .iter()
                .find(|f| f.enabled && f.node_id == edit.node_id && f.input_name == edit.input_name)
                .ok_or("Champ non autorisé par le mapping")?;
            if edit.value.len() > 200_000 {
                return Err("Champ trop long".into());
            }
            // JSON's initial numeric spelling does not determine a ComfyUI widget's type.
            // A null schema is the explicit offline-preview mode: exact values are
            // still applied, but no engine schema validation is claimed.
            let declared = if node_info.is_null() {
                None
            } else {
                let schema = &node_info[&f.class_type]["input"];
                schema["required"]
                    .get(&f.input_name)
                    .or_else(|| schema["optional"].get(&f.input_name))
            };
            let value_type = if f.value_type == "integer"
                && declared.and_then(|v| v[0].as_str()) == Some("FLOAT")
            {
                "number"
            } else {
                f.value_type.as_str()
            };
            let value = match value_type {
                "string" => {
                    removed.extend(
                        removed_tokens(&f.value, &edit.value)
                            .into_iter()
                            .map(|s| format!("{} / {} : {}", f.node_id, f.input_name, s)),
                    );
                    json!(edit.value)
                }
                "seed" => {
                    Seed::parse(&edit.value)?;
                    json!(edit.value.parse::<u64>()?)
                }
                "integer" => serde_json::from_str::<Value>(&edit.value)
                    .ok()
                    .filter(|v| v.is_i64() || v.is_u64())
                    .ok_or("Entier invalide")?,
                "number" => serde_json::from_str::<Value>(&edit.value)
                    .ok()
                    .filter(Value::is_number)
                    .ok_or("Nombre invalide")?,
                "boolean" => json!(edit.value.parse::<bool>()?),
                _ => return Err("Type non pris en charge".into()),
            };
            if f.value != edit.value {
                changes.push(json!({"node":f.node_id,"input":f.input_name,"before":f.value,"after":edit.value}));
            }
            graph[&edit.node_id]["inputs"][&edit.input_name] = value;
        }
        for patch in patches {
            let node = patch["node"].as_str().ok_or("Patch sans node")?;
            let input = patch["input"].as_str().ok_or("Patch sans entrée")?;
            let value = patch.get("value").ok_or("Patch sans valeur")?;
            if edits
                .iter()
                .any(|e| e.node_id == node && e.input_name == input)
            {
                return Err("Patch LoRA en conflit avec une édition".into());
            }
            let before = graph[node]["inputs"][input].clone();
            graph[node]["inputs"][input] = value.clone();
            changes.push(json!({"node":node,"input":input,"before":before,"after":value}));
        }
        if !node_info.is_null() {
            validate_graph(&graph, node_info)?;
        }
        let seed_requested = fields(&graph)
            .into_iter()
            .find(|f| f.value_type == "seed")
            .map(|f| f.value)
            .unwrap_or_else(|| "0".into());
        // The SQLite job column is an unsigned u64; rgthree sentinels live only in the prompt.
        // Keep the requested sentinel in the snapshot for auditability.
        let seed = if seed_requested.starts_with('-') {
            "0".to_owned()
        } else {
            seed_requested.clone()
        };
        let job = uuid::Uuid::new_v4().to_string();
        let (checkpoints, loras) = crate::gallery::resources(&graph);
        let snapshot = json!({"workflowId":id,"sourceHash":digest(original.as_bytes()),"resources":{"checkpoints":checkpoints,"loras":loras},"prompt":graph,"clientId":self.comfy_client_id()?,"removed":removed,"changes":changes,"characters":characters});
        let serialized = snapshot.to_string();
        self.connection.execute("INSERT INTO generation_jobs(id,workflow_revision_id,seed,request_snapshot_json,state,endpoint_url) VALUES(?1,?2,?3,?4,'draft','http://127.0.0.1:8188')",params![job,id,seed,serialized])?;
        for person in &characters {
            tx.execute("INSERT INTO job_characters(job_id,role,character_id,variant_id,snapshot_json) VALUES(?1,?2,?3,?4,?5)",params![job,person["role"].as_str(),person["characterId"].as_str(),person["variant"]["id"].as_str(),person.to_string()])?;
        }
        tx.commit()?;
        Ok(Prepared {
            job_id: job,
            snapshot_hash: digest(serialized.as_bytes()),
            request_json: serialized,
            removed,
            changes,
            node_count: workflow.node_count,
        })
    }
}
impl Database {
    /// Draft job from a graph built by the orchestrator (stored workflow plus
    /// pass inputs). Same table, snapshot shape, validation and later submit
    /// path as user preparations; `extra` records the orchestration context.
    pub fn prepare_job_from_graph(
        &self,
        id: &str,
        graph: &Value,
        node_info: &Value,
        extra: Value,
    ) -> Result<Prepared> {
        let original: String = self.connection.query_row(
            "SELECT document_json FROM workflow_revisions WHERE id=?",
            [id],
            |r| r.get(0),
        )?;
        validate_graph(graph, node_info)?;
        let seed = fields(graph)
            .into_iter()
            .find(|f| f.value_type == "seed")
            .map(|f| f.value)
            .unwrap_or_else(|| "0".into());
        let job = uuid::Uuid::new_v4().to_string();
        let (checkpoints, loras) = crate::gallery::resources(graph);
        let snapshot = json!({"workflowId":id,"sourceHash":digest(original.as_bytes()),"resources":{"checkpoints":checkpoints,"loras":loras},"prompt":graph,"clientId":self.comfy_client_id()?,"removed":[],"changes":[],"characters":[],"orchestration":extra});
        let serialized = snapshot.to_string();
        self.connection.execute("INSERT INTO generation_jobs(id,workflow_revision_id,seed,request_snapshot_json,state,endpoint_url) VALUES(?1,?2,?3,?4,'draft','http://127.0.0.1:8188')",params![job,id,seed,serialized])?;
        Ok(Prepared {
            job_id: job,
            snapshot_hash: digest(serialized.as_bytes()),
            request_json: serialized,
            removed: Vec::new(),
            changes: Vec::new(),
            node_count: graph.as_object().map_or(0, |o| o.len()),
        })
    }
}
/// Input declaration according to the object_info schema, adapter contracts
/// including LoraManager. Same resolution as `validate_graph`.
pub(crate) fn schema_input(class: &str, schema: &Value, key: &str) -> Option<Value> {
    let required = schema.pointer("/input/required").and_then(Value::as_object);
    let optional = schema.pointer("/input/optional").and_then(Value::as_object);
    required
        .and_then(|r| r.get(key))
        .or_else(|| optional.and_then(|o| o.get(key)))
        .cloned()
        .or_else(|| lora_manager::optional_field(class, schema, key))
}

pub fn validate_graph(graph: &Value, info: &Value) -> Result<()> {
    let nodes = graph
        .as_object()
        .filter(|v| !v.is_empty() && v.len() <= 2000)
        .ok_or("Graphe invalide")?;
    let mut dependencies = BTreeMap::<String, Vec<String>>::new();
    let mut output = false;
    // ComfyUI's graphToPrompt emits active but unreachable nodes, including
    // dangling Saya phase nodes with unsatisfied inputs. The executor prunes
    // them; validation must ignore them too, while still checking every node
    // reachable from a real output.
    let mut output_ids: Vec<String> = Vec::new();
    for (id, node) in nodes {
        if node["class_type"]
            .as_str()
            .and_then(|c| info.get(c))
            .is_some_and(|s| s["output_node"] == true)
        {
            output_ids.push(id.clone());
        }
    }
    let reachable: Option<std::collections::BTreeSet<String>> = if output_ids.is_empty() {
        None
    } else {
        let mut seen = std::collections::BTreeSet::new();
        let mut stack = output_ids;
        while let Some(id) = stack.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            let Some(node) = nodes.get(&id) else { continue };
            for value in node["inputs"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(_, v)| v)
            {
                if let Some(link) = value
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(Value::as_str)
                {
                    if nodes.contains_key(link) {
                        stack.push(link.to_owned());
                    }
                }
            }
        }
        Some(seen)
    };
    for (id, node) in nodes {
        let class = node["class_type"].as_str().ok_or("Classe absente")?;
        if reachable.as_ref().is_some_and(|r| !r.contains(id)) {
            continue;
        }
        let schema = info
            .get(class)
            .ok_or_else(|| format!("Classe absente de ComfyUI : {class}"))?;
        output |= schema["output_node"] == true;
        let inputs = node["inputs"].as_object().ok_or("Inputs absents")?;
        let required = schema.pointer("/input/required").and_then(Value::as_object);
        let optional = schema.pointer("/input/optional").and_then(Value::as_object);
        for key in required.into_iter().flat_map(|r| r.keys()) {
            if !inputs.contains_key(key) {
                return Err(format!("Entrée requise absente : {id}.{key}").into());
            }
        }
        let mut edges = Vec::new();
        for (key, value) in inputs {
            // CustomCombo accepts frontend-defined optionN/index inputs
            // (`accept_all_inputs=True` in its backend schema).
            if (class == "CustomCombo" && (key == "index" || key.starts_with("option")))
                || class == "Power Puter (rgthree)"
            {
                continue;
            }
            if (class == "Lora Loader (LoraManager)" && key == "__lm_autocomplete_meta_text")
                || (class == "Image Comparer (rgthree)" && key == "rgthree_comparer")
            {
                continue;
            }
            let adapter_field = lora_manager::optional_field(class, schema, key);
            let field = required
                .and_then(|r| r.get(key))
                .or_else(|| optional.and_then(|r| r.get(key)))
                .or(adapter_field.as_ref())
                .ok_or_else(|| format!("Entrée non déclarée par ComfyUI : {id}.{key}"))?;
            let kind = field
                .get(0)
                .ok_or_else(|| format!("Schéma inconnu : {id}.{key}"))?;
            if let Some(valid) = lora_manager::literal(class, schema, key, value) {
                if !valid {
                    return Err(format!(
                        "Widget LoraManager invalide ou non pris en charge : {id}.{key}"
                    )
                    .into());
                }
                continue;
            }
            if let Some(link) = value.as_array() {
                let (2, Some(source_id), Some(index)) =
                    (link.len(), link[0].as_str(), link[1].as_u64())
                else {
                    return Err(format!(
                        "Lien invalide ou valeur composite non prise en charge : {id}.{key}"
                    )
                    .into());
                };
                let source = nodes.get(source_id).ok_or("Lien vers un node absent")?;
                let source_class = source["class_type"]
                    .as_str()
                    .ok_or("Classe source absente")?;
                let outputs = info[source_class]["output"]
                    .as_array()
                    .ok_or("Sorties inconnues")?;
                if index >= outputs.len() as u64 {
                    return Err("Index de sortie invalide".into());
                }
                let provided = outputs[index as usize]
                    .as_str()
                    .ok_or("Type de sortie inconnu")?;
                if let Some(expected) = kind.as_str() {
                    // ComfyUI v3 match-type outputs are resolved dynamically by the
                    // engine. They are not a concrete STRING/IMAGE claim; accept the
                    // link but never rewrite the target's declared type.
                    let dynamic = expected.starts_with("COMFY_MATCHTYPE_")
                        || provided.starts_with("COMFY_MATCHTYPE_");
                    if !dynamic
                        && expected != "*"
                        && provided != "*"
                        && !expected.split(',').any(|v| v.trim() == provided)
                    {
                        return Err(format!("Types de lien incompatibles : {id}.{key} attend {expected}, reçu {provided}").into());
                    }
                }
                edges.push(source_id.to_owned());
            } else if kind.as_str() == Some("COMBO")
                || kind.get(0).and_then(Value::as_str) == Some("COMBO")
            {
                // ComfyUI 0.34 combo schema: ["COMBO", {"options":[...]}].
                // Dynamic custom combos may expose an empty options list; in
                // that case the node's own companion inputs define validity and
                // the string is accepted as-is, never silently rewritten.
                let options = kind
                    .get(1)
                    .and_then(|v| v.get("options"))
                    .and_then(Value::as_array);
                if let Some(options) = options.filter(|o| !o.is_empty()) {
                    if !options.contains(value) {
                        return Err(format!("Choix non disponible : {id}.{key}").into());
                    }
                } else if !value.is_string() {
                    return Err(format!("Valeur COMBO dynamique non textuelle : {id}.{key}").into());
                }
            } else if let Some(choices) = kind.as_array() {
                if !choices.contains(value) {
                    return Err(format!("Choix non disponible : {id}.{key}").into());
                }
            } else {
                let valid = match kind.as_str() {
                    Some("*") => value.is_string() || value.is_boolean() || value.is_number(),
                    Some("STRING") => value.is_string(),
                    Some("BOOLEAN") => value.is_boolean(),
                    Some("INT") => value.is_i64() || value.is_u64(),
                    Some("FLOAT") => value.is_number(),
                    _ => false,
                };
                if !valid {
                    return Err(format!("Valeur ou type non pris en charge : {id}.{key}").into());
                }
                if let Some(number) = value.as_f64() {
                    if field
                        .get(1)
                        .and_then(|v| v["min"].as_f64())
                        .is_some_and(|min| number < min)
                        || field
                            .get(1)
                            .and_then(|v| v["max"].as_f64())
                            .is_some_and(|max| number > max)
                    {
                        return Err(format!("Valeur hors limites : {id}.{key}").into());
                    }
                }
            }
        }
        dependencies.insert(id.clone(), edges);
    }
    if info
        .as_object()
        .is_some_and(|o| o.values().any(|v| v.get("output_node").is_some()))
        && !output
    {
        return Err("Aucun node de sortie exécutable".into());
    }
    let mut resolved = std::collections::BTreeSet::new();
    while resolved.len() < dependencies.len() {
        let before = resolved.len();
        for (id, edges) in &dependencies {
            if edges.iter().all(|e| resolved.contains(e)) {
                resolved.insert(id.clone());
            }
        }
        if resolved.len() == before {
            return Err("Cycle détecté dans le workflow".into());
        }
    }
    Ok(())
}

impl Database {
    /// Universal mapping: upsert one semantic command without deleting the
    /// other mappings. The target must be an existing primitive input.
    pub fn save_semantic_binding(
        &self,
        id: &str,
        key: &str,
        node: &str,
        input: &str,
    ) -> Result<Value> {
        if key.is_empty()
            || key.len() > 80
            || !key
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err("Clé de mapping invalide".into());
        }
        let fields = self.workflow(id)?.fields;
        let field = fields
            .iter()
            .find(|f| f.node_id == node && f.input_name == input)
            .ok_or("Champ cible inconnu")?;
        let bound: Option<String> = self
            .connection
            .query_row(
                "SELECT semantic_key FROM workflow_bindings WHERE workflow_revision_id=? AND node_id=? AND input_name=? AND semantic_key<>?",
                params![id, node, input, key],
                |r| r.get(0),
            )
            .ok();
        if let Some(other) = bound {
            return Err(format!("Entrée déjà liée à la commande {other}").into());
        }
        self.connection.execute(
            "INSERT INTO workflow_bindings(workflow_revision_id,semantic_key,node_id,expected_class,input_name,value_type)
             VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(workflow_revision_id,semantic_key) DO UPDATE SET node_id=excluded.node_id,
             expected_class=excluded.expected_class,input_name=excluded.input_name,value_type=excluded.value_type",
            params![id,key,field.node_id,field.class_type,field.input_name,field.value_type],
        )?;
        Ok(
            json!({"workflowId":id,"command":key,"nodeId":field.node_id,"inputName":field.input_name,"classType":field.class_type,"valueType":field.value_type}),
        )
    }

    pub fn prepare_bindings(&self, id: &str) -> Result<Vec<Value>> {
        let mut q = self.connection.prepare(
            "SELECT semantic_key,node_id,expected_class,input_name,value_type FROM workflow_bindings WHERE workflow_revision_id=? ORDER BY semantic_key",
        )?;
        let rows = q.query_map([id], |r| {
            Ok(json!({"command":r.get::<_,String>(0)?,"nodeId":r.get::<_,String>(1)?,
                "classType":r.get::<_,String>(2)?,"inputName":r.get::<_,String>(3)?,"valueType":r.get::<_,String>(4)?}))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

#[cfg(test)]
mod internal_workflow_tests {
    use crate::Database;
    use serde_json::json;

    #[test]
    fn per_run_phase_graphs_are_hidden_and_purged_when_unused() {
        let db = Database::in_memory().unwrap();
        let graph = json!({"1":{"class_type":"Fixture","inputs":{"text":"sfw"}}}).to_string();
        let user = db.import_workflow("Mine", &graph).unwrap();
        let ids: Vec<String> = (0..25)
            .map(|_| db.import_internal_workflow("Saya Couple · phase 1", &graph).unwrap())
            .collect();
        // The oldest one is still the provenance of a job.
        db.connection
            .execute(
                "INSERT INTO generation_jobs(id,workflow_revision_id,seed,request_snapshot_json) VALUES('job',?1,'1','{}')",
                [&ids[0]],
            )
            .unwrap();
        let listed: Vec<String> = db.workflows().unwrap().iter().map(|w| w["id"].as_str().unwrap().to_owned()).collect();
        assert_eq!(listed, vec![user.clone()]);
        assert_eq!(db.purge_internal_workflows(20).unwrap(), 4);
        let left = |id: &str| {
            db.connection
                .query_row("SELECT count(*) FROM workflow_revisions WHERE id=?", [id], |r| r.get::<_, i64>(0))
                .unwrap()
        };
        assert_eq!(left(&ids[0]), 1, "job provenance kept");
        assert!(ids[1..5].iter().all(|id| left(id) == 0));
        assert!(ids[5..].iter().all(|id| left(id) == 1), "20 most recent kept");
        assert_eq!(left(&user), 1);
        assert_eq!(db.purge_internal_workflows(20).unwrap(), 0);
    }
}
