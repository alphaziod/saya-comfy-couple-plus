//! Generation recipes: exact, portable capture of one generation's parameters.
//! A recipe is a document, never a silent replacement: application only returns
//! explicit edits that must pass the existing mapping and preparation flow.
use crate::{
    contracts::Seed, gallery::digest, storage::VERSION, workflow::api_graph, Database, Result,
};
use rusqlite::params;
use serde::Deserialize;
use serde_json::{json, Value};

pub const RECIPE_FORMAT: &str = "saya-recipe";
pub const RECIPE_VERSION: u32 = 1;
const SECTIONS: [&str; 6] = ["QUALITY", "P1", "P2", "ACTION", "BG", "NEG"];
const MAX_CONTENT_BYTES: usize = 8_000_000;
const REPRODUCTION: &str = "Cette recette décrit les paramètres d'une génération passée. Elle ne garantit aucune reproduction pixel-identique : versions de modèles non vérifiées, état du moteur et du bruit non capturés. L'application ne remplace jamais un workflow ; les valeurs passent par la préparation explicite existante.";
const IGNORED_LORA_MANAGER_KEYS: [&str; 2] = ["text", "__lm_autocomplete_meta_text"];
const NAMED_PARAMETERS: [&str; 5] = ["cfg", "steps", "sampler_name", "scheduler", "denoise"];

fn scalar_kind(value: &Value) -> &'static str {
    if value.is_string() {
        "string"
    } else if value.is_boolean() {
        "boolean"
    } else if value.is_i64() || value.is_u64() {
        "integer"
    } else {
        "number"
    }
}

fn edit_string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

/// Structural view of a graph: classes and links kept, scalar values masked.
/// Two graphs with this same view share their topology.
fn structure_view(graph: &Value) -> Value {
    let mut nodes = serde_json::Map::new();
    for (id, node) in graph.as_object().into_iter().flatten() {
        let mut inputs = serde_json::Map::new();
        for (input, value) in node["inputs"].as_object().into_iter().flatten() {
            let masked = if value.as_array().is_some_and(|link| link.len() == 2) {
                json!(value)
            } else {
                json!("*")
            };
            inputs.insert(input.clone(), masked);
        }
        nodes.insert(
            id.clone(),
            json!({"class":node["class_type"],"inputs":inputs}),
        );
    }
    Value::Object(nodes)
}

fn finite_number(value: &Value) -> Option<f64> {
    value.as_f64().filter(|number| number.is_finite())
}

fn content_prompts(prompts: &serde_json::Map<String, Value>) -> Value {
    Value::Object(prompts.clone())
}

fn error(message: impl std::fmt::Display) -> Box<dyn std::error::Error + Send + Sync> {
    message.to_string().into()
}

/// Locates a unique scalar field for a captured coordinate, then records either
/// an explicit edit or a missing entry. Requires the target mapping to be enabled.
#[allow(clippy::too_many_arguments)]
fn require_field(
    fields: &[crate::workflow::Field],
    node: &str,
    input: &str,
    value: &Value,
    label: &str,
    missing: &mut Vec<String>,
    edits: &mut Vec<Value>,
    applied: &mut std::collections::BTreeMap<String, usize>,
    unchanged: &mut usize,
) {
    let located = match fields
        .iter()
        .find(|f| f.node_id == node && f.input_name == input)
    {
        Some(field) => Some(field),
        None => {
            let same_input: Vec<&crate::workflow::Field> =
                fields.iter().filter(|f| f.input_name == input).collect();
            match same_input.len() {
                0 => None,
                1 => Some(same_input[0]),
                more => {
                    missing.push(format!(
                        "{label} : entrée {input} ambiguë ({more} champs portent ce nom dans le workflow cible)"
                    ));
                    return;
                }
            }
        }
    };
    match located {
        None => missing.push(format!(
            "{label} : entrée {node}.{input} introuvable dans le workflow cible"
        )),
        Some(field) if !field.enabled => {
            // The mapping is only required to change a value; an equal unmapped value needs nothing.
            if field.value == edit_string(value) {
                *unchanged += 1;
            } else {
                missing.push(format!(
                    "{label} : l'entrée {}.{} existe mais n'est pas mappée ; activez-la dans Workflows",
                    field.node_id, field.input_name
                ));
            }
        }
        Some(field) => {
            if field.value != edit_string(value) {
                edits.push(json!({"nodeId":field.node_id,"inputName":field.input_name,"value":edit_string(value),"section":field.section}));
                *applied.entry(label.to_owned()).or_default() += 1;
            } else {
                *unchanged += 1;
            }
        }
    }
}

/// Exact recursive diff. Arrays are compared by index; objects by key union.
pub fn diff_values(path: &str, left: &Value, right: &Value, out: &mut Vec<Value>) {
    if left == right {
        return;
    }
    match (left.as_object(), right.as_object()) {
        (Some(a), Some(b)) => {
            let keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
            for key in keys {
                let child = format!("{path}.{key}");
                match (a.get(key), b.get(key)) {
                    (Some(v), None) => {
                        out.push(json!({"path":child,"kind":"removed","left":v,"right":null}))
                    }
                    (None, Some(v)) => {
                        out.push(json!({"path":child,"kind":"added","left":null,"right":v}))
                    }
                    (Some(x), Some(y)) => diff_values(&child, x, y, out),
                    (None, None) => unreachable!(),
                }
            }
        }
        _ => match (left.as_array(), right.as_array()) {
            (Some(a), Some(b)) => {
                for index in 0..a.len().max(b.len()) {
                    let child = format!("{path}[{index}]");
                    match (a.get(index), b.get(index)) {
                        (Some(v), None) => {
                            out.push(json!({"path":child,"kind":"removed","left":v,"right":null}))
                        }
                        (None, Some(v)) => {
                            out.push(json!({"path":child,"kind":"added","left":null,"right":v}))
                        }
                        (Some(x), Some(y)) => diff_values(&child, x, y, out),
                        (None, None) => unreachable!(),
                    }
                }
            }
            _ => out.push(json!({"path":path,"kind":"changed","left":left,"right":right})),
        },
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PortableRecipe {
    format: String,
    version: u32,
    name: String,
    #[serde(default)]
    description: String,
    content: Value,
}

fn valid_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.len() > 300 {
        return Err("Nom de recette invalide".into());
    }
    Ok(())
}

/// (id, name, description, version, content_json, sha, created_at, updated_at)
type RecipeRow = (String, String, String, u32, String, String, String, String);
/// List row: id, name, description, version, sha, updated, workflow name,
/// checkpoint name, character/lora/prompt counts.
type RecipeListRow = (
    String,
    String,
    String,
    u32,
    String,
    String,
    Option<String>,
    Option<String>,
    u32,
    u32,
    u32,
);

impl Database {
    fn recipe_row(&self, id: &str) -> Result<RecipeRow> {
        self.connection
            .query_row(
                "SELECT id,name,description,version,content_json,content_sha256,created_at,updated_at FROM recipes WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
            )
            .map_err(|_| error("Recette introuvable"))
    }

    pub fn recipes(&self) -> Result<Vec<Value>> {
        let mut query = self.connection.prepare(
            "SELECT id,name,description,version,content_sha256,updated_at,
                json_extract(content_json,'$.workflow.workflowName'),
                json_extract(content_json,'$.checkpoints[0].name'),
                json_array_length(content_json,'$.characters'),
                json_array_length(content_json,'$.loras'),
                json_array_length(content_json,'$.prompts')
            FROM recipes ORDER BY rowid DESC",
        )?;
        let rows = query.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, u32>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, u32>(8)?,
                r.get::<_, u32>(9)?,
                r.get::<_, u32>(10)?,
            ))
        })?;
        let rows: Vec<RecipeListRow> = rows.collect::<std::result::Result<_, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(id, name, description, version, sha, updated, workflow, checkpoint, characters, loras, prompts)| {
                json!({"id":id,"name":name,"description":description,"version":version,"contentSha256":sha,"updatedAt":updated,"workflowName":workflow,"checkpoint":checkpoint,"characterCount":characters,"loraCount":loras,"promptCount":prompts})
            })
            .collect())
    }

    pub fn recipe_detail(&self, id: &str) -> Result<Value> {
        let (id, name, description, version, content, sha, created, updated) =
            self.recipe_row(id)?;
        Ok(
            json!({"id":id,"name":name,"description":description,"version":version,"contentSha256":sha,"createdAt":created,"updatedAt":updated,"content":serde_json::from_str::<Value>(&content)?}),
        )
    }

    pub fn recipe_history(&self, id: &str) -> Result<Vec<Value>> {
        self.recipe_row(id)?;
        let mut query = self.connection.prepare("SELECT version,content_json,content_sha256,note,saved_at FROM recipe_versions WHERE recipe_id=?1 ORDER BY version DESC")?;
        let rows = query.query_map([id], |r| {
            Ok((
                r.get::<_, u32>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        let rows: Vec<(u32, String, String, String, String)> =
            rows.collect::<std::result::Result<_, _>>()?;
        rows.into_iter()
            .map(|(version, content, sha, note, saved)| {
                Ok(json!({"version":version,"contentSha256":sha,"note":note,"savedAt":saved,"content":serde_json::from_str::<Value>(&content)?}))
            })
            .collect()
    }

    /// Builds the exact recipe content from a recorded job snapshot.
    pub(crate) fn capture_recipe_content(&self, job: &str) -> Result<Value> {
        let (workflow_id, seed_column, state, snapshot_text): (String, String, String, String) = self
            .connection
            .query_row(
                "SELECT workflow_revision_id,seed,state,request_snapshot_json FROM generation_jobs WHERE id=?1",
                [job],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(|_| error("Job introuvable"))?;
        let snapshot: Value = serde_json::from_str(&snapshot_text)?;
        let graph = snapshot["prompt"]
            .as_object()
            .ok_or("Snapshot sans graphe API")?;
        let (workflow_name, source_hash): (String, String) = self.connection.query_row(
            "SELECT name,source_sha256 FROM workflow_revisions WHERE id=?1",
            [&workflow_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        // Prompt sections come from the mapping recorded for this workflow revision.
        let mut bound: std::collections::BTreeMap<String, (String, String)> = Default::default();
        let mut bindings = self.connection.prepare(
            "SELECT semantic_key,node_id,input_name FROM workflow_bindings WHERE workflow_revision_id=?1",
        )?;
        let rows = bindings.query_map([&workflow_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let rows: Vec<(String, String, String)> = rows.collect::<std::result::Result<_, _>>()?;
        for (key, node, input) in rows {
            bound.insert(key, (node, input));
        }
        let mut prompts = serde_json::Map::new();
        for section in SECTIONS {
            let Some((node, input)) = bound.get(section) else {
                continue;
            };
            // Value indexing (never Map indexing) so an absent node is an explicit error, not a panic.
            let text = &snapshot["prompt"][node]["inputs"][input];
            if !text.is_string() {
                return Err(format!(
                    "Mapping modifié depuis la génération : section {section} absente du graphe snapshot"
                )
                .into());
            }
            prompts.insert(
                section.to_owned(),
                json!({"node":node,"input":input,"text":text.as_str().unwrap()}),
            );
        }
        let prompt_pairs: std::collections::BTreeSet<(&str, &str)> = prompts
            .values()
            .map(|p| (p["node"].as_str().unwrap(), p["input"].as_str().unwrap()))
            .collect();
        let mut checkpoints = Vec::new();
        let mut loras = Vec::new();
        let mut parameters = Vec::new();
        let mut custom = Vec::new();
        for (node_id, node) in graph {
            let class = node["class_type"].as_str().unwrap_or_default();
            let Some(inputs) = node["inputs"].as_object() else {
                continue;
            };
            for (input, value) in inputs {
                if !(value.is_string() || value.is_number() || value.is_boolean()) {
                    continue;
                }
                if class == "Lora Loader (LoraManager)"
                    && IGNORED_LORA_MANAGER_KEYS.contains(&input.as_str())
                {
                    continue;
                }
                let coordinate = (node_id.as_str(), input.as_str());
                if prompt_pairs.contains(&coordinate) {
                    continue;
                }
                match input.as_str() {
                    "ckpt_name" => checkpoints.push(json!({"node":node_id,"name":value})),
                    "lora_name" => {
                        let model = finite_number(&inputs["strength_model"]).unwrap_or(0.0);
                        let clip = finite_number(&inputs["strength_clip"]).unwrap_or(0.0);
                        if !(model == 0.0 && clip == 0.0) {
                            loras.push(json!({"node":node_id,"name":value,"strengthModel":inputs["strength_model"],"strengthClip":inputs["strength_clip"],"composite":false}));
                        }
                    }
                    "loras" => {
                        for entry in value["__value__"].as_array().into_iter().flatten() {
                            if entry["active"].as_bool() == Some(false) {
                                continue;
                            }
                            loras.push(json!({"node":node_id,"name":entry["name"],"strengthModel":entry["strength"],"strengthClip":entry.get("clipStrength").unwrap_or(&Value::Null),"composite":true}));
                        }
                    }
                    "strength_model" | "strength_clip" => {}
                    "seed" | "noise_seed" => {
                        let raw = value.as_u64().ok_or("Seed du snapshot non représentable en u64")?;
                        parameters.push(json!({"node":node_id,"input":input,"kind":"seed","value":Seed::parse(&raw.to_string())?}));
                    }
                    key if NAMED_PARAMETERS.contains(&key) => {
                        parameters.push(json!({"node":node_id,"input":input,"kind":scalar_kind(value),"value":value}));
                    }
                    _ => custom.push(json!({"node":node_id,"input":input,"kind":scalar_kind(value),"value":value})),
                }
            }
        }
        let sort_key = |a: &Value, b: &Value| {
            (
                a["node"].as_str().unwrap_or_default(),
                a["input"].as_str().unwrap_or_default(),
            )
                .cmp(&(
                    b["node"].as_str().unwrap_or_default(),
                    b["input"].as_str().unwrap_or_default(),
                ))
        };
        checkpoints.sort_by(sort_key);
        loras.sort_by(|a, b| {
            (
                a["node"].as_str().unwrap_or_default(),
                a["name"].as_str().unwrap_or_default(),
            )
                .cmp(&(
                    b["node"].as_str().unwrap_or_default(),
                    b["name"].as_str().unwrap_or_default(),
                ))
        });
        parameters.sort_by(sort_key);
        custom.sort_by(sort_key);
        let characters = snapshot["characters"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let changes = snapshot["changes"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let mut modules = vec!["workflow-bindings"];
        if !changes.is_empty() {
            modules.push("explicit-edits");
        }
        if !characters.is_empty() {
            modules.push("characters");
        }
        if characters.iter().any(|c| !c["variant"].is_null()) {
            modules.push("variants");
        }
        if prompts.contains_key("P1") && prompts.contains_key("P2") {
            modules.push("couple");
        }
        if loras.iter().any(|l| l["composite"] == json!(true)) {
            modules.push("lora-manager");
        }
        let snapshot_hash = digest(snapshot_text.as_bytes());
        let mut outputs = Vec::new();
        let mut stored = self.connection.prepare(
            "SELECT asset_id,node_id,output_index,output_kind FROM job_outputs WHERE job_id=?1 ORDER BY asset_id",
        )?;
        for row in stored.query_map([job], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, String>(3)?,
            ))
        })? {
            let (asset, node, index, kind) = row?;
            outputs.push(json!({"assetId":asset,"nodeId":node,"outputIndex":index,"kind":kind}));
        }
        let models: Vec<Value> = checkpoints
            .iter()
            .map(|c| json!({"kind":"checkpoint","node":c["node"],"name":c["name"],"version":{"status":"unverified","sha256":null}}))
            .chain(loras.iter().map(|l| json!({"kind":"lora","node":l["node"],"name":l["name"],"version":{"status":"unverified","sha256":null}})))
            .collect();
        let mut passes = Vec::new();
        let mut pipeline_ref = Value::Null;
        if let Ok((run, step_id, pipeline_id, manifest_text)) = self.connection.query_row(
            "SELECT s.run_id,s.step_id,pr.pipeline_id,pd.manifest_json
             FROM pipeline_run_steps s JOIN pipeline_runs pr ON pr.id=s.run_id
             JOIN pipeline_definitions pd ON pd.id=pr.pipeline_id
             WHERE s.job_id=?1 ORDER BY s.position LIMIT 1",
            [job],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            },
        ) {
            let manifest: Value = serde_json::from_str(&manifest_text)?;
            pipeline_ref = json!({"runId":run,"pipelineId":pipeline_id,"stepId":step_id});
            for step in manifest["steps"].as_array().into_iter().flatten() {
                let workflow = step["workflowId"].as_str().unwrap_or_default();
                let doc: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT document_json FROM workflow_revisions WHERE id=?",
                        [workflow],
                        |r| r.get(0),
                    )
                    .ok();
                let step_state: Option<(String, Option<String>, Option<String>)> = self.connection
                    .query_row("SELECT state,output_kind,output_sha256 FROM pipeline_run_steps WHERE run_id=? AND step_id=?",
                        params![run, step["id"].as_str().unwrap_or_default()], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).ok();
                passes.push(json!({
                    "stepId":step["id"], "workflowId":workflow, "kind":step["kind"],
                    "inputs":step["inputs"], "outputs":step["outputs"], "bindings":step["bindings"],
                    "state":step_state.as_ref().map(|s|s.0.clone()), "outputKind":step_state.as_ref().and_then(|s|s.1.clone()),
                    "outputSha256":step_state.as_ref().and_then(|s|s.2.clone()),
                    "api":doc.as_deref().and_then(|text| serde_json::from_str::<Value>(text).ok()),
                }));
            }
        }
        Ok(json!({
            "format":RECIPE_FORMAT,
            "version":RECIPE_VERSION,
            "workflow":{"workflowId":workflow_id,"workflowName":workflow_name,"sourceHash":source_hash,"snapshotHash":snapshot_hash,"nodeCount":graph.len(),"document":snapshot["prompt"]},
            "checkpoints":checkpoints,
            "loras":loras,
            "prompts":content_prompts(&prompts),
            "parameters":parameters,
            "custom":custom,
            "characters":characters,
            "modules":modules,
            "dependencies":{"capturedWith":format!("{RECIPE_FORMAT}/{RECIPE_VERSION}"),"appVersion":env!("CARGO_PKG_VERSION"),"schemaVersion":VERSION,"workflowSourceHash":source_hash,"snapshotHash":snapshot_hash,"modelVersions":{"status":"unverified"},"models":models,"passes":passes,"pipeline":pipeline_ref,"note":"Seules les empreintes disponibles au moment de la création sont conservées ; les versions des modèles ne sont pas vérifiées."},
            "provenance":{"jobId":job,"jobState":state,"jobSeed":seed_column,"outputs":outputs,
                "restore":{"workflowId":workflow_id,"seed":seed_column,"prompts":content_prompts(&prompts),"parameters":parameters,"passes":passes},"pipeline":pipeline_ref},
            "reproduction":REPRODUCTION
        }))
    }

    pub fn create_recipe_from_job(
        &self,
        job: &str,
        name: &str,
        description: &str,
    ) -> Result<Value> {
        valid_name(name)?;
        if description.len() > 5000 {
            return Err("Description trop longue".into());
        }
        let content = self.capture_recipe_content(job)?;
        self.insert_recipe(
            name,
            description,
            content,
            &format!("créée depuis le job {job}"),
            true,
        )
    }

    fn insert_recipe(
        &self,
        name: &str,
        description: &str,
        content: Value,
        note: &str,
        require_origin: bool,
    ) -> Result<Value> {
        validate_recipe_content(&content)?;
        let serialized = content.to_string();
        let sha = digest(serialized.as_bytes());
        let workflow_id = content["workflow"]["workflowId"]
            .as_str()
            .unwrap_or_default();
        // The anchor exists only when the origin revision is local; portable imports stay unanchored.
        let known: bool = !workflow_id.is_empty()
            && self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM workflow_revisions WHERE id=?1)",
                [workflow_id],
                |r| r.get::<_, bool>(0),
            )?;
        if !known && !workflow_id.is_empty() && require_origin {
            return Err("Workflow d'origine introuvable dans cette base".into());
        }
        let tx = self.connection.unchecked_transaction()?;
        let id = uuid::Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO recipes(id,name,description,workflow_id,version,content_json,content_sha256) VALUES(?1,?2,?3,?4,1,?5,?6)",
            params![id, name, description, if known { Some(workflow_id) } else { None }, serialized, sha],
        )?;
        tx.execute(
            "INSERT INTO recipe_versions(recipe_id,version,content_json,content_sha256,note) VALUES(?1,1,?2,?3,?4)",
            params![id, serialized, sha, note],
        )?;
        tx.commit()?;
        Ok(json!({"id":id,"version":1,"contentSha256":sha}))
    }

    /// Saves an edited copy as a new version; the origin workflow is immutable.
    pub fn save_recipe_edit(
        &self,
        id: &str,
        content: Value,
        note: &str,
        expected_version: u32,
    ) -> Result<Value> {
        if note.len() > 20_000 {
            return Err("Note trop longue".into());
        }
        validate_recipe_content(&content)?;
        let serialized = content.to_string();
        let sha = digest(serialized.as_bytes());
        let workflow_id = content["workflow"]["workflowId"]
            .as_str()
            .unwrap_or_default();
        let tx = self.connection.unchecked_transaction()?;
        let current: u32 = tx
            .query_row("SELECT version FROM recipes WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .map_err(|_| error("Recette introuvable"))?;
        if current != expected_version {
            return Err("Recette modifiée entre-temps ; rechargez la version actuelle".into());
        }
        let anchor: Option<String> =
            tx.query_row("SELECT workflow_id FROM recipes WHERE id=?1", [id], |r| {
                r.get(0)
            })?;
        if anchor.is_some() && anchor.as_deref().unwrap_or_default() != workflow_id {
            return Err("Le workflow d'origine d'une recette est immuable ; dupliquez-la pour changer d'origine".into());
        }
        let version = expected_version + 1;
        let changed = tx.execute(
            "UPDATE recipes SET version=?2,content_json=?3,content_sha256=?4,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1 AND version=?5",
            params![id, version, serialized, sha, expected_version],
        )?;
        if changed != 1 {
            return Err("Recette modifiée entre-temps ; rechargez la version actuelle".into());
        }
        tx.execute(
            "INSERT INTO recipe_versions(recipe_id,version,content_json,content_sha256,note) VALUES(?1,?2,?3,?4,?5)",
            params![id, version, serialized, sha, note],
        )?;
        tx.commit()?;
        Ok(json!({"id":id,"version":version,"contentSha256":sha}))
    }

    pub fn duplicate_recipe(&self, id: &str, name: &str) -> Result<Value> {
        valid_name(name)?;
        let (_, _, description, _, content, _, _, _) = self.recipe_row(id)?;
        let content: Value = serde_json::from_str(&content)?;
        self.insert_recipe(
            name,
            &description,
            content,
            &format!("dupliquée depuis {id}"),
            true,
        )
    }

    pub fn compare_recipes(&self, left_id: &str, right_id: &str) -> Result<Value> {
        let (_, left_name, _, left_version, left, _, _, _) = self.recipe_row(left_id)?;
        let (_, right_name, _, right_version, right, _, _, _) = self.recipe_row(right_id)?;
        let left: Value = serde_json::from_str(&left)?;
        let right: Value = serde_json::from_str(&right)?;
        let mut differences = Vec::new();
        diff_values("$", &left, &right, &mut differences);
        Ok(json!({
            "left":{"id":left_id,"name":left_name,"version":left_version},
            "right":{"id":right_id,"name":right_name,"version":right_version},
            "identical":differences.is_empty(),
            "differences":differences
        }))
    }

    pub fn export_recipe(&self, id: &str) -> Result<Value> {
        let (_, name, description, _, content, _, _, _) = self.recipe_row(id)?;
        Ok(json!({
            "format":RECIPE_FORMAT,
            "version":RECIPE_VERSION,
            "name":name,
            "description":description,
            "content":serde_json::from_str::<Value>(&content)?
        }))
    }

    pub fn import_recipe(&self, text: &str) -> Result<Value> {
        if text.len() > MAX_CONTENT_BYTES * 2 {
            return Err("Recette portable trop volumineuse".into());
        }
        let document: PortableRecipe = serde_json::from_str(text)?;
        if document.format != RECIPE_FORMAT || document.version != RECIPE_VERSION {
            return Err("Format de recette inconnu ; import refusé".into());
        }
        valid_name(&document.name)?;
        self.insert_recipe(
            &document.name,
            &document.description,
            document.content,
            "importée d'un document portable",
            false,
        )
    }

    /// Compatibility check then explicit edits. Refuses with the full missing list;
    /// never writes to the workflow, jobs or graph.
    /// Strict application: refuses with the exact list of problems.
    pub fn apply_recipe(&self, id: &str, workflow_id: &str) -> Result<Value> {
        let mut report = self.recipe_application(id, workflow_id)?;
        let problems: Vec<String> = report["problems"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p.as_str().map(str::to_owned))
            .collect();
        if !problems.is_empty() {
            return Err(format!(
                "Application refusée ; {} élément(s) manquant(s) : {}",
                problems.len(),
                problems.join(" ; ")
            )
            .into());
        }
        for key in ["problems", "ready", "diff", "jobCharacters", "checks"] {
            report.as_object_mut().map(|o| o.remove(key));
        }
        Ok(report)
    }

    /// Application report for the UI: every problem listed (never a partial
    /// application), the explicit edits with their current values, the checks
    /// performed and the job characters recorded by the recipe.
    pub fn recipe_application(&self, id: &str, workflow_id: &str) -> Result<Value> {
        let (_, _, _, _, content_text, _, _, _) = self.recipe_row(id)?;
        let content: Value = serde_json::from_str(&content_text)?;
        validate_recipe_content(&content)?;
        let target = self.workflow(workflow_id)?;
        let (target_text, target_hash): (String, String) = self.connection.query_row(
            "SELECT document_json,source_sha256 FROM workflow_revisions WHERE id=?1",
            [workflow_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let target_graph: Value = serde_json::from_str(&target_text)?;
        let mut missing: Vec<String> = Vec::new();
        let mut edits: Vec<Value> = Vec::new();
        let mut unchanged = 0usize;
        let mut applied = std::collections::BTreeMap::<String, usize>::new();
        // Prompt sections map through the target's recorded section bindings.
        for (section, prompt) in content["prompts"].as_object().into_iter().flatten() {
            let candidates: Vec<_> = target
                .fields
                .iter()
                .filter(|f| f.enabled && f.section.as_deref() == Some(section.as_str()))
                .collect();
            match candidates.len() {
                0 => missing.push(format!(
                    "Section {section} : aucune entrée texte mappée dans le workflow cible"
                )),
                1 => {
                    let field = candidates[0];
                    if field.value != prompt["text"].as_str().unwrap_or_default() {
                        edits.push(json!({"nodeId":field.node_id,"inputName":field.input_name,"value":prompt["text"],"section":field.section}));
                        *applied.entry("prompts".to_owned()).or_default() += 1;
                    } else {
                        unchanged += 1;
                    }
                }
                _ => missing.push(format!(
                    "Section {section} : plusieurs entrées mappées ; cible ambiguë"
                )),
            }
        }
        for parameter in content["parameters"].as_array().into_iter().flatten() {
            let label = if parameter["kind"] == "seed" {
                "seed"
            } else {
                "paramètre"
            };
            require_field(
                &target.fields,
                parameter["node"].as_str().unwrap_or_default(),
                parameter["input"].as_str().unwrap_or_default(),
                &parameter["value"],
                label,
                &mut missing,
                &mut edits,
                &mut applied,
                &mut unchanged,
            );
        }
        for entry in content["custom"].as_array().into_iter().flatten() {
            require_field(
                &target.fields,
                entry["node"].as_str().unwrap_or_default(),
                entry["input"].as_str().unwrap_or_default(),
                &entry["value"],
                "paramètre personnalisé",
                &mut missing,
                &mut edits,
                &mut applied,
                &mut unchanged,
            );
        }
        // Checkpoints and LoRAs are matched by name inside the target graph.
        for checkpoint in content["checkpoints"].as_array().into_iter().flatten() {
            let name = checkpoint["name"].as_str().unwrap_or_default();
            let present = target
                .fields
                .iter()
                .filter(|f| f.input_name == "ckpt_name" && f.value == name)
                .count();
            if present == 0 {
                missing.push(format!("Checkpoint absent du workflow cible : {name}"));
            } else {
                // The value already matches: nothing to write, mapping or not.
                unchanged += present;
            }
        }
        for lora in content["loras"].as_array().into_iter().flatten() {
            let name = lora["name"].as_str().unwrap_or_default();
            if lora["composite"] == json!(true) {
                let known = target_graph
                    .as_object()
                    .into_iter()
                    .flatten()
                    .any(|(_, node)| {
                        node["inputs"]["loras"]["__value__"]
                            .as_array()
                            .is_some_and(|entries| {
                                entries.iter().any(|entry| {
                                    entry["name"] == lora["name"]
                                        && entry["active"].as_bool() != Some(false)
                                        && entry["strength"] == lora["strengthModel"]
                                        && *entry.get("clipStrength").unwrap_or(&Value::Null)
                                            == lora["strengthClip"]
                                })
                            })
                    });
                if !known {
                    missing.push(format!("LoRA composite absente ou de poids différents dans le workflow cible : {name} (ce chemin explicite ne modifie pas les entrées composites)"));
                } else {
                    unchanged += 1;
                }
                continue;
            }
            let nodes: Vec<&String> = target_graph
                .as_object()
                .into_iter()
                .flatten()
                .filter(|(_, node)| node["inputs"]["lora_name"] == lora["name"])
                .map(|(id, _)| id)
                .collect();
            if nodes.is_empty() {
                missing.push(format!("LoRA inconnue du workflow cible : {name}"));
                continue;
            }
            let model = &lora["strengthModel"];
            let clip = &lora["strengthClip"];
            for key in ["strength_model", "strength_clip"] {
                let value = if key == "strength_model" { model } else { clip };
                if value.is_null() {
                    continue;
                }
                let candidates: Vec<&crate::workflow::Field> = target
                    .fields
                    .iter()
                    .filter(|f| nodes.iter().any(|node| **node == f.node_id) && f.input_name == key)
                    .collect();
                match candidates.len() {
                    0 => missing.push(format!(
                        "LoRA {name} : entrée {key} introuvable dans le workflow cible"
                    )),
                    more if more > 1 => missing.push(format!(
                        "LoRA {name} : entrée {key} ambiguë ({} nodes)",
                        more
                    )),
                    _ => {
                        let field = candidates[0];
                        if !field.enabled {
                            if field.value != edit_string(value) {
                                missing.push(format!(
                                    "LoRA {name} : l'entrée {}.{} existe mais n'est pas mappée ; activez-la dans Workflows",
                                    field.node_id, field.input_name
                                ));
                            } else {
                                unchanged += 1;
                            }
                        } else if field.value != edit_string(value) {
                            edits.push(json!({"nodeId":field.node_id,"inputName":field.input_name,"value":edit_string(value),"section":field.section}));
                            *applied.entry("loras".to_owned()).or_default() += 1;
                        } else {
                            unchanged += 1;
                        }
                    }
                }
            }
        }
        for character in content["characters"].as_array().into_iter().flatten() {
            let name = character["name"].as_str().unwrap_or("?");
            let known: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM characters WHERE id=?1 AND archived=0)",
                [character["characterId"].as_str().unwrap_or_default()],
                |r| r.get(0),
            )?;
            if !known {
                missing.push(format!("Fiche personnage absente ou archivée : {name}"));
                continue;
            }
            if let Some(variant) = character["variant"]["id"].as_str() {
                let known: bool = self.connection.query_row(
                    "SELECT EXISTS(SELECT 1 FROM character_variants WHERE id=?1 AND character_id=?2)",
                    params![variant, character["characterId"].as_str().unwrap_or_default()],
                    |r| r.get(0),
                )?;
                if !known {
                    missing.push(format!("Variante absente de la fiche : {name}"));
                }
            }
        }
        let diff: Vec<Value> = edits
            .iter()
            .map(|edit| {
                let before = target
                    .fields
                    .iter()
                    .find(|f| edit["nodeId"] == f.node_id.as_str() && edit["inputName"] == f.input_name.as_str())
                    .map(|f| f.value.clone());
                json!({"nodeId":edit["nodeId"],"inputName":edit["inputName"],"section":edit["section"],"before":before,"after":edit["value"]})
            })
            .collect();
        let job_characters: Vec<Value> = content["characters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| {
                Some(
                    json!({"role":c["role"].as_str()?,"characterId":c["characterId"].as_str()?,
                    "variantId":c["variant"]["id"].as_str()}),
                )
            })
            .collect();
        let checks = json!({
            "checkpoints": content["checkpoints"].as_array().map_or(0, Vec::len),
            "loras": content["loras"].as_array().map_or(0, Vec::len),
            "prompts": content["prompts"].as_object().map_or(0, |o| o.len()),
            "parameters": content["parameters"].as_array().map_or(0, Vec::len) + content["custom"].as_array().map_or(0, Vec::len),
            "characters": job_characters.len(),
        });
        let source_hash = content["workflow"]["sourceHash"]
            .as_str()
            .unwrap_or_default();
        Ok(json!({
            "recipeId":id,
            "workflowId":workflow_id,
            "workflowMatches":source_hash == target_hash,
            "structureIdentical":structure_view(&content["workflow"]["document"]) == structure_view(&target_graph),
            "edits":edits,
            "applied":applied,
            "unchangedCount":unchanged,
            "problems":missing,
            "ready":missing.is_empty(),
            "diff":diff,
            "jobCharacters":job_characters,
            "checks":checks,
            "note":"Aucun remplacement : ces modifications explicites doivent être relues puis préparées via le flux existant.",
            "reproduction":REPRODUCTION
        }))
    }
}

/// Strict validation of recipe content; unknown top-level keys are refused.
pub fn validate_recipe_content(content: &Value) -> Result<()> {
    let object = content.as_object().ok_or("Contenu de recette invalide")?;
    const KEYS: [&str; 13] = [
        "format",
        "version",
        "workflow",
        "checkpoints",
        "loras",
        "prompts",
        "parameters",
        "custom",
        "characters",
        "modules",
        "dependencies",
        "provenance",
        "reproduction",
    ];
    if object.keys().len() != KEYS.len() || KEYS.iter().any(|key| !object.contains_key(*key)) {
        return Err("Clés de recette inconnues ou manquantes".into());
    }
    if content["format"] != json!(RECIPE_FORMAT) || content["version"] != json!(RECIPE_VERSION) {
        return Err("Format de recette inconnu".into());
    }
    if content.to_string().len() > MAX_CONTENT_BYTES {
        return Err("Contenu de recette trop volumineux".into());
    }
    let workflow = &content["workflow"];
    let wf = workflow.as_object().ok_or("Workflow de recette invalide")?;
    const WORKFLOW_KEYS: [&str; 6] = [
        "workflowId",
        "workflowName",
        "sourceHash",
        "snapshotHash",
        "nodeCount",
        "document",
    ];
    if wf.keys().len() != WORKFLOW_KEYS.len()
        || WORKFLOW_KEYS.iter().any(|key| !wf.contains_key(*key))
    {
        return Err("Clés de workflow de recette inconnues ou manquantes".into());
    }
    if !crate::references::valid_hash(workflow["sourceHash"].as_str().unwrap_or_default())
        || !crate::references::valid_hash(workflow["snapshotHash"].as_str().unwrap_or_default())
    {
        return Err("Empreinte de workflow invalide".into());
    }
    if (workflow["nodeCount"].as_u64().unwrap_or(0) == 0)
        || workflow["nodeCount"].as_u64().unwrap_or(0) > 2000
    {
        return Err("Nombre de nodes invalide".into());
    }
    api_graph(workflow["document"].to_string().as_str())?;
    for (name, prompt_value) in content["prompts"].as_object().into_iter().flatten() {
        let name = name.as_str();
        if !SECTIONS.contains(&name) {
            return Err(format!("Section de prompt inconnue : {name}").into());
        }
        let prompt = prompt_value
            .as_object()
            .ok_or("Prompt de recette invalide")?;
        if prompt.keys().len() != 3
            || !prompt.contains_key("node")
            || !prompt.contains_key("input")
            || !prompt.contains_key("text")
        {
            return Err("Clés de prompt inconnues ou manquantes".into());
        }
        let text = prompt["text"].as_str().ok_or("Texte de prompt invalide")?;
        if text.len() > 200_000
            || prompt["node"].as_str().unwrap_or_default().is_empty()
            || prompt["input"].as_str().unwrap_or_default().is_empty()
        {
            return Err("Prompt de recette hors limites".into());
        }
    }
    let scalar_entry = |value: &Value| -> Result<()> {
        let entry = value.as_object().ok_or("Entrée de recette invalide")?;
        if entry.keys().len() != 4
            || !entry.contains_key("node")
            || !entry.contains_key("input")
            || !entry.contains_key("kind")
            || !entry.contains_key("value")
        {
            return Err("Clés d'entrée de recette inconnues ou manquantes".into());
        }
        let kind = value["kind"].as_str().ok_or("Type d'entrée invalide")?;
        if !["string", "seed", "integer", "number", "boolean"].contains(&kind) {
            return Err(format!("Type d'entrée inconnu : {kind}").into());
        }
        let value = &value["value"];
        if kind == "seed" {
            Seed::parse(value.as_str().ok_or("Seed non textuelle")?)?;
        } else if !(value.is_string() || value.is_number() || value.is_boolean()) {
            return Err("Valeur d'entrée invalide".into());
        }
        if value.is_string() && value.as_str().unwrap_or_default().len() > 200_000 {
            return Err("Valeur d'entrée trop longue".into());
        }
        Ok(())
    };
    for group in ["parameters", "custom"] {
        for entry in content[group].as_array().into_iter().flatten() {
            scalar_entry(entry)?;
        }
    }
    for entry in content["checkpoints"].as_array().into_iter().flatten() {
        let checkpoint = entry.as_object().ok_or("Checkpoint de recette invalide")?;
        if checkpoint.keys().len() != 2
            || !checkpoint.contains_key("node")
            || !checkpoint.contains_key("name")
            || entry["name"].as_str().unwrap_or_default().is_empty()
        {
            return Err("Checkpoint de recette invalide".into());
        }
    }
    for entry in content["loras"].as_array().into_iter().flatten() {
        let lora = entry.as_object().ok_or("LoRA de recette invalide")?;
        let name = entry["name"].as_str().unwrap_or_default();
        if lora.keys().len() != 5
            || !lora.contains_key("name")
            || !lora.contains_key("strengthModel")
            || !lora.contains_key("strengthClip")
            || !lora.contains_key("node")
            || !lora.contains_key("composite")
            || name.trim().is_empty()
            || name.len() > 4096
            || !entry["composite"].is_boolean()
            || entry["strengthModel"].is_null()
        {
            return Err("LoRA de recette invalide".into());
        }
    }
    for entry in content["characters"].as_array().into_iter().flatten() {
        if !entry.is_object() || entry["characterId"].as_str().unwrap_or_default().is_empty() {
            return Err("Personnage de recette invalide".into());
        }
    }
    if content["modules"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|m| !m.is_string())
    {
        return Err("Modules de recette invalides".into());
    }
    if !content["dependencies"].is_object() || !content["provenance"].is_object() {
        return Err("Dépendances ou provenance de recette invalides".into());
    }
    let reproduction = content["reproduction"].as_str().unwrap_or_default();
    if reproduction.is_empty() || reproduction.len() > 20_000 {
        return Err("Mention de reproduction absente".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_is_exact_and_ordered() {
        let a = json!({"a":1,"list":[1,2],"s":"x  \n"});
        let b = json!({"a":2,"list":[1,3],"s":"x  \n","new":true});
        let mut out = Vec::new();
        diff_values("$", &a, &b, &mut out);
        let paths: Vec<String> = out
            .iter()
            .map(|d| d["path"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(paths, ["$.a", "$.list[1]", "$.new"]);
        assert_eq!(out[0]["kind"], "changed");
        assert_eq!(out[1]["kind"], "changed");
        assert_eq!(out[2]["kind"], "added");
        let mut same = Vec::new();
        diff_values("$", &a, &a, &mut same);
        assert!(same.is_empty());
    }

    #[test]
    fn content_validation_refuses_unknown_keys_and_bad_seeds() {
        let base = json!({
            "format":RECIPE_FORMAT,"version":1,
            "workflow":{"workflowId":"w","workflowName":"n","sourceHash":"a".repeat(64),"snapshotHash":"b".repeat(64),"nodeCount":1,"document":{"1":{"class_type":"Text","inputs":{"text":"x"}}}},
            "checkpoints":[],"loras":[],"prompts":{},"parameters":[],"custom":[],"characters":[],"modules":[],
            "dependencies":{},"provenance":{},"reproduction":"note"
        });
        validate_recipe_content(&base).unwrap();
        let mut extra = base.clone();
        extra["unexpected"] = json!(1);
        assert!(validate_recipe_content(&extra).is_err());
        let mut seed = base.clone();
        seed["parameters"] = json!([{"node":"1","input":"seed","kind":"seed","value":"01"}]);
        assert!(validate_recipe_content(&seed).is_err());
        seed["parameters"] =
            json!([{"node":"1","input":"seed","kind":"seed","value":"18446744073709551615"}]);
        validate_recipe_content(&seed).unwrap();
        let mut graph = base.clone();
        graph["workflow"]["document"] = json!({"nodes":[]});
        assert!(validate_recipe_content(&graph).is_err());
    }
}
