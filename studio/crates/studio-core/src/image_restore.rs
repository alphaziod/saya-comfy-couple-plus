//! Read-only PNG generation metadata. No source image or workflow is modified.
use crate::Result;
use serde_json::{json, Value};
use std::path::Path;

pub(crate) fn text_chunks(info: &png::Info) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for t in &info.uncompressed_latin1_text {
        out.push((t.keyword.clone(), t.text.clone()));
    }
    for z in &info.compressed_latin1_text {
        if let Ok(text) = z.get_text() {
            out.push((z.keyword.clone(), text));
        }
    }
    for t in &info.utf8_text {
        if let Ok(text) = t.get_text() {
            out.push((t.keyword.clone(), text));
        }
    }
    out
}

fn is_connection(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|a| a.len() == 2 && (a[0].is_string() || a[0].is_u64()) && a[1].is_u64())
}

pub fn inspect(path: &Path) -> Result<Value> {
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() {
        return Err("Image absente ou symbolique".into());
    }
    if meta.len() > 64 * 1024 * 1024 {
        return Err("Image trop volumineuse".into());
    }
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path)?));
    let reader = decoder.read_info()?;
    let info = reader.info();
    let width = info.width;
    let height = info.height;
    let chunks = text_chunks(info);
    let mut raw = serde_json::Map::new();
    for (k, v) in &chunks {
        raw.insert(k.clone(), json!(v));
    }
    let workflow = raw
        .get("workflow")
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str::<Value>(s).ok());
    let prompt = raw
        .get("prompt")
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str::<Value>(s).ok());
    let parameters = raw
        .get("parameters")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let graph = prompt.as_ref().or(workflow.as_ref());
    let mut fields = serde_json::Map::new();
    fields.insert("width".into(), json!(width));
    fields.insert("height".into(), json!(height));
    if let Some(graph) = graph.and_then(Value::as_object) {
        let mut seeds = Vec::new();
        let mut checkpoints = Vec::new();
        let mut loras = Vec::new();
        let mut texts = Vec::new();
        for (id, node) in graph {
            let class = node["class_type"]
                .as_str()
                .or_else(|| node["type"].as_str())
                .unwrap_or("");
            let inputs = node["inputs"].as_object();
            if let Some(inputs) = inputs {
                for (k, v) in inputs {
                    if (k == "seed" || k == "noise_seed")
                        && (v.is_u64() || v.as_i64().is_some_and(|n| (-3..=-1).contains(&n)))
                    {
                        seeds.push(json!({"node":id,"input":k,"value":v.to_string()}));
                    }
                    if k == "ckpt_name" {
                        if let Some(s) = v.as_str() {
                            checkpoints.push(json!({"node":id,"value":s}));
                        }
                    }
                    if class.to_lowercase().contains("lora") && (k == "lora_name" || k == "name") {
                        if let Some(s) = v.as_str() {
                            loras.push(json!({"node":id,"value":s}));
                        }
                    }
                }
                if let Some(text) = inputs
                    .get("text")
                    .or_else(|| inputs.get("text_g"))
                    .or_else(|| inputs.get("text_l"))
                    .and_then(Value::as_str)
                {
                    texts.push(json!({"node":id,"class":class,"text":text}));
                }
            }
            if let Some(text) = node["widgets_values"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(Value::as_str)
            {
                texts.push(json!({"node":id,"class":class,"text":text}));
            }
        }
        fields.insert("seeds".into(), json!(seeds));
        fields.insert("checkpoints".into(), json!(checkpoints));
        fields.insert("loras".into(), json!(loras));
        fields.insert("texts".into(), json!(texts));
    }
    let inputs = editable_inputs(prompt.as_ref().unwrap_or(&Value::Null));
    fields.insert("inputs".into(), json!(inputs));
    Ok(json!({
        "path":path.to_string_lossy(), "width":width, "height":height,
        "keys":chunks.iter().map(|(k,_)| k.clone()).collect::<Vec<_>>(),
        "hasPrompt":prompt.is_some(), "hasWorkflow":workflow.is_some(), "hasParameters":parameters.is_some(),
        "prompt":prompt, "workflow":workflow, "parameters":parameters, "fields":fields,
        "warnings": if prompt.is_none() && workflow.is_none() && parameters.is_none() { json!(["Aucune métadonnée de génération exploitable"]) } else { json!([]) }
    }))
}

/// Only explicitly selected value inputs on matching nodes; never copy links or guess roles.
pub fn selected_patches(
    source: &Value,
    target: &Value,
    selection: &[(String, String)],
) -> Result<Vec<Value>> {
    if selection.is_empty() || selection.len() > 1000 {
        return Err("Sélection de champs vide ou trop grande".into());
    }
    let mut patches = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (node, input) in selection {
        if !seen.insert((node, input)) {
            return Err("Champ sélectionné deux fois".into());
        }
        let value = source[node]["inputs"]
            .get(input)
            .ok_or("Champ absent des métadonnées")?;
        let old = target[node]["inputs"]
            .get(input)
            .ok_or("Champ absent du workflow cible")?;
        if source[node]["class_type"] != target[node]["class_type"]
            || is_connection(value)
            || is_connection(old)
        {
            return Err(format!("Cible incompatible : {node}.{input}").into());
        }
        if value.is_string() != old.is_string()
            || value.is_number() != old.is_number()
            || value.is_boolean() != old.is_boolean()
            || value.is_object() != old.is_object()
            || value.is_array() != old.is_array()
        {
            return Err(format!("Type incompatible : {node}.{input}").into());
        }
        patches.push(json!({"node":node,"input":input,"value":value}));
    }
    Ok(patches)
}

/// Literal fields only; graph links and exact integer values stay in Rust.
pub fn editable_inputs(graph: &Value) -> Vec<Value> {
    let mut inputs = Vec::new();
    if let Some(graph) = graph.as_object() {
        for (node, item) in graph {
            for (input, value) in item["inputs"].as_object().into_iter().flatten() {
                if is_connection(value) {
                    continue;
                } // Links are preserved by workflow import, never treated as values.
                inputs.push(json!({"node":node,"input":input,"class":item["class_type"],"title":item["_meta"]["title"],"value":value.as_str().map(str::to_owned).unwrap_or_else(||value.to_string()),"valueType":if value.is_string(){"string"}else{"json"}}));
            }
        }
    }
    inputs
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemporaryEdit {
    pub node: String,
    pub input: String,
    pub value: String,
}

pub fn edited_temporary_graph(original: &Value, edits: &[TemporaryEdit]) -> Result<Value> {
    if edits.len() > 10000 {
        return Err("Trop de champs modifiés".into());
    }
    let mut graph = original.clone();
    let mut seen = std::collections::HashSet::new();
    for edit in edits {
        if !seen.insert((&edit.node, &edit.input)) {
            return Err("Champ répété".into());
        }
        let value = graph
            .get_mut(&edit.node)
            .and_then(|n| n.get_mut("inputs"))
            .and_then(|i| i.get_mut(&edit.input))
            .ok_or("Champ absent du PNG")?;
        if is_connection(value) {
            return Err("Une connexion ne peut pas être remplacée".into());
        }
        let replacement = if value.is_string() {
            json!(edit.value)
        } else {
            serde_json::from_str(&edit.value)?
        };
        if is_connection(&replacement) {
            return Err("Une valeur ne peut pas devenir une connexion".into());
        }
        *value = replacement;
    }
    Ok(graph)
}

/// Activate the imported working copy, including nested subgraph definitions.
/// Widget values, links and the original file remain untouched.
pub fn activate_workflow(workflow: &mut Value) {
    if let Some(nodes) = workflow.get_mut("nodes").and_then(Value::as_array_mut) {
        for node in nodes {
            node["mode"] = json!(0);
            if let Some(original) = node.pointer_mut("/properties/saya_original_mode") {
                *original = json!(0);
            }
        }
    }
    if let Some(phases) = workflow
        .pointer_mut("/extra/saya_preset/phases")
        .and_then(Value::as_array_mut)
    {
        for phase in phases {
            activate_workflow(phase);
        }
    }
    if let Some(definitions) = workflow
        .pointer_mut("/definitions/subgraphs")
        .and_then(Value::as_array_mut)
    {
        for definition in definitions {
            activate_workflow(definition);
        }
    }
}

pub fn is_saya_workflow(workflow: &Value) -> bool {
    workflow.pointer("/extra/saya_preset").is_some()
        || workflow
            .get("nodes")
            .and_then(Value::as_array)
            .is_some_and(|nodes| {
                nodes.iter().any(|node| {
                    node.pointer("/properties/saya_phase")
                        .and_then(Value::as_u64)
                        .is_some_and(|phase| phase > 1)
                })
            })
        || workflow
            .pointer("/definitions/subgraphs")
            .and_then(Value::as_array)
            .is_some_and(|defs| defs.iter().any(is_saya_workflow))
}
