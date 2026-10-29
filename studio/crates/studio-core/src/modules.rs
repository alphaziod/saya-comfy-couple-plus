//! Library of reusable ComfyUI modules.
//!
//! A module is a part of an API workflow saved as a reusable
//! building block: the selected subgraph is copied AS IS (internal nodes and links
//! intact), the input/output ports are DERIVED from the links that
//! cross the selection boundary, and the exposed parameters are the
//! primitive values of the kept nodes. An ambiguous cut is refused with
//! an explicit diagnostic. Connecting a module into a pipeline validates a
//! type contract and never submits anything to the engine.

use crate::workflow::lora_manager;
use crate::{gallery::digest, workflow::validate_graph, Database, Result};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Manifest format version; any change goes through a new number.
pub const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleInputPort {
    /// Stable port name, derived from the boundary: `{node}.{input}`.
    pub name: String,
    /// Exact ComfyUI type declared by object_info (IMAGE, LATENT, MODEL, CLIP,
    /// VAE, CONDITIONING… or the `*` wildcard). Any unknown type stays literal.
    pub kind: String,
    /// Target node of the module that will receive this port's connection.
    pub node: String,
    /// Input name of the target node.
    pub input: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleOutputPort {
    /// Stable port name, derived from the boundary: `{node}.{output index}`.
    pub name: String,
    pub kind: String,
    /// Source node of the module producing this output.
    pub node: String,
    /// Output index of the source node.
    pub output_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleParameter {
    pub node: String,
    pub input: String,
    /// string | boolean | seed | integer | number (same convention as the workflow fields).
    pub kind: String,
    /// Value frozen at extraction time, exact text form.
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleDependencies {
    /// ComfyUI classes used by the module; a class missing from
    /// object_info stays listed and its use is flagged with a warning.
    pub classes: Vec<String>,
    pub checkpoints: Vec<String>,
    pub loras: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleManifest {
    pub manifest_version: u32,
    pub name: String,
    pub description: String,
    pub inputs: Vec<ModuleInputPort>,
    pub outputs: Vec<ModuleOutputPort>,
    pub parameters: Vec<ModuleParameter>,
    pub dependencies: ModuleDependencies,
}

/// Extracted subgraph, ready to be versioned. The document is the exact
/// canonical copy of the selected nodes: no rewriting.
#[derive(Debug, Clone)]
pub struct ModuleDraft {
    pub inputs: Vec<ModuleInputPort>,
    pub outputs: Vec<ModuleOutputPort>,
    pub parameters: Vec<ModuleParameter>,
    pub dependencies: ModuleDependencies,
    pub document: Value,
}

impl ModuleDraft {
    pub fn manifest(&self, name: &str, description: &str) -> ModuleManifest {
        ModuleManifest {
            manifest_version: MANIFEST_VERSION,
            name: name.to_owned(),
            description: description.to_owned(),
            inputs: self.inputs.clone(),
            outputs: self.outputs.clone(),
            parameters: self.parameters.clone(),
            dependencies: self.dependencies.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleBinding {
    /// Name of the module port targeted by this connection.
    pub port: String,
    /// Input: pipeline node providing the source. Output: target node.
    pub node: String,
    /// Output only: input name of the target node.
    #[serde(default)]
    pub input: Option<String>,
    /// Input only: output index of the pipeline's source node.
    #[serde(default)]
    pub output_index: Option<u32>,
}

/// Complete connection contract of an instance: each input port must
/// receive exactly one pipeline source; optional output ports
/// feed free inputs; overrides rewrite exposed
/// parameters. Validated by type, never executed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModuleConnection {
    #[serde(default)]
    pub inputs: Vec<ModuleBinding>,
    #[serde(default)]
    pub outputs: Vec<ModuleBinding>,
    #[serde(default)]
    pub overrides: Value,
}

fn nodes_of<'a>(graph: &'a Value, label: &str) -> Result<&'a Map<String, Value>> {
    graph
        .as_object()
        .filter(|o| !o.is_empty() && o.len() <= 2000)
        .ok_or_else(|| format!("{label} invalide : objet API de 1 à 2000 nodes attendu").into())
}

fn valid_node(node: &Value) -> bool {
    node.get("class_type").and_then(Value::as_str).is_some()
        && node.get("inputs").is_some_and(Value::is_object)
}

/// Declared type for a class input, strict LoraManager adapter included.
/// Refuses choice inputs (array) or with an unreadable schema: a boundary
/// boundary requires a known bindable type.
fn declared_kind(node_info: &Value, class: &str, input: &str) -> Result<String> {
    let schema = node_info
        .get(class)
        .ok_or_else(|| format!("Type inconnu : classe {class} absente de object_info"))?;
    let adapter = lora_manager::optional_field(class, schema, input);
    let field = schema
        .pointer("/input/required")
        .and_then(|r| r.get(input))
        .or_else(|| schema.pointer("/input/optional").and_then(|o| o.get(input)))
        .or(adapter.as_ref())
        .ok_or_else(|| {
            format!("Type inconnu : entrée {class}.{input} non déclarée par object_info")
        })?;
    let kind = field
        .get(0)
        .ok_or_else(|| format!("Type inconnu : schéma illisible pour {class}.{input}"))?;
    kind.as_str().map(str::to_owned).ok_or_else(|| {
        format!("Coupure ambiguë : entrée {class}.{input} à choix, non liable").into()
    })
}

fn output_kind(node_info: &Value, class: &str, index: u64) -> Result<String> {
    let outputs = node_info
        .get(class)
        .and_then(|s| s.get("output"))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Type inconnu : sorties de {class} absentes de object_info"))?;
    outputs
        .get(index as usize)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            format!("Type inconnu : sortie {class}[{index}] inexistante ou non textuelle").into()
        })
}

/// Numeric API ids sort numerically, then other ids lexically.
fn node_order(a: Option<&str>, b: Option<&str>) -> std::cmp::Ordering {
    let (a, b) = (a.unwrap_or_default(), b.unwrap_or_default());
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        (Ok(_), Err(_)) => std::cmp::Ordering::Less,
        (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
        _ => a.cmp(b),
    }
}

/// Same rule as validate_graph: exact equality or the `*` wildcard.
fn compatible(expected: &str, provided: &str) -> bool {
    expected == "*" || provided == "*" || expected.split(',').any(|k| k.trim() == provided)
}

fn parameter_kind(class: &str, input: &str, value: &Value) -> Option<(&'static str, String)> {
    if class == "Lora Loader (LoraManager)"
        && ["text", "__lm_autocomplete_meta_text"].contains(&input)
    {
        return None; // Autocomplete strings ignored by the installed loader.
    }
    let kind = if value.is_string() {
        "string"
    } else if value.is_boolean() {
        "boolean"
    } else if input == "seed" || input == "noise_seed" {
        "seed"
    } else if value.is_i64() || value.is_u64() {
        "integer"
    } else if value.is_f64() {
        "number"
    } else {
        return None;
    };
    let text = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    Some((kind, text))
}

/// Feed map of the full graph: (target, input) -> (source, index).
/// Any broken link or input fed twice is refused with a diagnostic.
fn feeds(
    nodes: &Map<String, Value>,
) -> Result<std::collections::BTreeMap<(String, String), (String, u64)>> {
    let mut feeds = std::collections::BTreeMap::new();
    for (id, node) in nodes {
        for (key, value) in node["inputs"].as_object().into_iter().flatten() {
            let Some(link) = value.as_array() else {
                continue;
            };
            let (2, Some(source), Some(output)) = (link.len(), link[0].as_str(), link[1].as_u64())
            else {
                return Err(format!(
                    "Lien invalide ou valeur composite non prise en charge : {id}.{key}"
                )
                .into());
            };
            if !nodes.contains_key(source) {
                return Err(
                    format!("Lien brisé : {id}.{key} pointe vers le node absent {source}").into(),
                );
            }
            if feeds
                .insert((id.clone(), key.clone()), (source.to_owned(), output))
                .is_some()
            {
                return Err(format!("Entrée déjà alimentée en interne : {id}.{key}").into());
            }
        }
    }
    Ok(feeds)
}

/// Extracts a module from an API graph: selection copied as is,
/// ports derived from boundary links, ambiguous cuts refused.
pub fn extract_module(graph: &Value, node_info: &Value, selection: &[&str]) -> Result<ModuleDraft> {
    let nodes = nodes_of(graph, "Graphe source")?;
    if selection.is_empty() || selection.len() > 2000 {
        return Err("Sélection de module vide ou supérieure à 2000 nodes".into());
    }
    let mut selected: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for id in selection {
        if !selected.insert(id.to_string()) {
            return Err(format!("Node sélectionné deux fois : {id}").into());
        }
        let node = nodes
            .get(*id)
            .ok_or_else(|| format!("Node sélectionné absent du workflow : {id}"))?;
        if !valid_node(node) {
            return Err(format!("Node API invalide : {id}").into());
        }
    }
    let feeds = feeds(nodes)?;
    let consumers_of = |feeds: &std::collections::BTreeMap<(String, String), (String, u64)>| {
        let mut consumers: std::collections::BTreeMap<(String, u64), Vec<String>> =
            Default::default();
        for ((target, _), (source, index)) in feeds {
            consumers
                .entry((source.clone(), *index))
                .or_default()
                .push(target.clone());
        }
        consumers
    };
    let consumers = consumers_of(&feeds);

    let mut inputs = Vec::new();
    for id in &selected {
        for ((target, input_name), (source, _index)) in feeds.iter().filter(|((t, _), _)| t == id) {
            if selected.contains(source.as_str()) {
                continue; // Internal link: kept as is in the document.
            }
            let class = nodes[target]["class_type"].as_str().unwrap_or_default();
            let kind = declared_kind(node_info, class, input_name)?;
            inputs.push(ModuleInputPort {
                name: format!("{target}.{input_name}"),
                kind,
                node: target.clone(),
                input: input_name.clone(),
            });
        }
    }

    let mut outputs = Vec::new();
    for id in &selected {
        let class = nodes[id]["class_type"].as_str().unwrap_or_default();
        for ((source, index), targets) in consumers.iter().filter(|((s, _), _)| s == id) {
            if targets.iter().all(|t| selected.contains(t.as_str())) {
                continue; // Consumed only internally.
            }
            let kind = output_kind(node_info, class, *index)?;
            outputs.push(ModuleOutputPort {
                name: format!("{source}.{index}"),
                kind,
                node: source.clone(),
                output_index: *index as u32,
            });
        }
    }

    // Convex boundary: no external node may consume an output of the
    // module and then feed (directly or not) the selection; such a
    // loop through the cut would make the rearrangement ambiguous.
    let mut successors: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
    for ((target, _), (source, _)) in feeds.iter() {
        successors
            .entry(source.as_str())
            .or_default()
            .push(target.as_str());
    }
    let external: Vec<(String, String)> = outputs
        .iter()
        .flat_map(|port| {
            consumers
                .get(&(port.node.clone(), u64::from(port.output_index)))
                .into_iter()
                .flatten()
                .cloned()
                .map(|consumer| (port.name.clone(), consumer))
                .collect::<Vec<_>>()
        })
        .filter(|(_, consumer)| !selected.contains(consumer.as_str()))
        .collect();
    for (port, consumer) in &external {
        let mut seen = std::collections::BTreeSet::new();
        let mut queue = vec![consumer.as_str()];
        while let Some(current) = queue.pop() {
            if !seen.insert(current) {
                continue;
            }
            if selected.contains(current) {
                return Err(format!(
                    "Coupure ambiguë : frontière non convexe — le node externe alimenté par le port {port} reconduit vers le node sélectionné {current}"
                )
                .into());
            }
            queue.extend(successors.get(current).into_iter().flatten().copied());
        }
    }

    let mut document = Map::new();
    for id in &selected {
        document.insert(id.clone(), nodes[id].clone());
    }
    let document = Value::Object(document);
    let mut classes = std::collections::BTreeSet::new();
    let mut parameters = Vec::new();
    for (id, node) in document
        .as_object()
        .expect("document built as an object above")
    {
        let class = node["class_type"].as_str().unwrap_or_default();
        classes.insert(class.to_owned());
        for (key, value) in node["inputs"].as_object().into_iter().flatten() {
            if value.as_array().is_some() {
                continue; // Link, not a parameter.
            }
            if let Some((kind, text)) = parameter_kind(class, key, value) {
                parameters.push(ModuleParameter {
                    node: id.clone(),
                    input: key.clone(),
                    kind: kind.into(),
                    value: text,
                });
            }
        }
    }
    let (checkpoints, loras) = crate::gallery::resources(&document);
    Ok(ModuleDraft {
        inputs,
        outputs,
        parameters,
        dependencies: ModuleDependencies {
            classes: classes.into_iter().collect(),
            checkpoints,
            loras,
        },
        document,
    })
}

fn override_value(kind: &str, value: &Value) -> Result<Value> {
    match kind {
        "string" if value.is_string() => Ok(value.clone()),
        "boolean" if value.is_boolean() => Ok(value.clone()),
        "seed" => {
            let text = value
                .as_str()
                .ok_or("Paramètre seed : valeur textuelle attendue")?;
            crate::contracts::Seed::parse(text)?;
            Ok(json!(text.parse::<u64>()?))
        }
        "integer" if value.is_i64() || value.is_u64() => Ok(value.clone()),
        "number" if value.is_number() => Ok(value.clone()),
        _ => Err(format!("Valeur incompatible avec le paramètre de type {kind}").into()),
    }
}

/// Connection contract: instantiates the module in a copy of the pipeline with
/// type checking on every binding. No execution, no write.
pub fn instantiate_module(
    manifest: &ModuleManifest,
    document: &Value,
    instance: &str,
    pipeline: &Value,
    connection: &ModuleConnection,
    node_info: &Value,
) -> Result<Value> {
    let ModuleConnection {
        inputs: input_bindings,
        outputs: output_bindings,
        overrides,
    } = connection;
    if instance.is_empty()
        || instance.len() > 64
        || !instance
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        || instance.contains("::")
    {
        return Err(
            "Identifiant d'instance invalide (1 à 64 caractères alphanumériques, - _ .)".into(),
        );
    }
    let pipeline_nodes = nodes_of(pipeline, "Pipeline")?;
    for (id, node) in pipeline_nodes {
        if !valid_node(node) {
            return Err(format!("Node API invalide dans le pipeline : {id}").into());
        }
    }
    let document_nodes = nodes_of(document, "Document de module")?;
    for (id, node) in document_nodes {
        if !valid_node(node) {
            return Err(format!("Node API invalide dans le module : {id}").into());
        }
    }
    let mut renamed = Map::new();
    let mut mapping = std::collections::BTreeMap::new();
    for id in document_nodes.keys() {
        let new_id = format!("{instance}::{id}");
        if pipeline_nodes.contains_key(&new_id) {
            return Err(format!("Collision d'identifiant avec le pipeline : {new_id}").into());
        }
        mapping.insert(id.clone(), new_id.clone());
        renamed.insert(new_id, document[id].clone());
    }
    let port_inputs: std::collections::BTreeMap<(String, String), &ModuleInputPort> = manifest
        .inputs
        .iter()
        .map(|p| ((p.node.clone(), p.input.clone()), p))
        .collect();

    // Input bindings: each manifest port must receive exactly
    // one pipeline source of compatible type.
    let mut bound_inputs: std::collections::BTreeMap<&str, &ModuleBinding> = Default::default();
    for binding in input_bindings {
        let Some(port) = manifest.inputs.iter().find(|p| p.name == binding.port) else {
            return Err(format!("Port d'entrée inconnu : {}", binding.port).into());
        };
        if binding.input.is_some() {
            return Err(format!(
                "La liaison d'entrée {} ne prend pas d'entrée cible",
                binding.port
            )
            .into());
        }
        if bound_inputs.insert(port.name.as_str(), binding).is_some() {
            return Err(format!("Port d'entrée connecté plusieurs fois : {}", binding.port).into());
        }
    }
    let mut connections = Vec::new();
    for port in &manifest.inputs {
        let binding = *bound_inputs
            .get(port.name.as_str())
            .ok_or_else(|| format!("Port d'entrée non connecté : {}", port.name))?;
        let source = pipeline_nodes
            .get(&binding.node)
            .ok_or_else(|| format!("Node source absent du pipeline : {}", binding.node))?;
        let index = binding
            .output_index
            .ok_or_else(|| format!("Index de sortie requis pour le port {}", port.name))?;
        let source_class = source["class_type"]
            .as_str()
            .ok_or("Classe source absente")?;
        let provided = output_kind(node_info, source_class, u64::from(index))?;
        if !compatible(&port.kind, &provided) {
            return Err(format!(
                "Types de lien incompatibles : port {} attend {}, reçu {provided}",
                port.name, port.kind
            )
            .into());
        }
        connections.push(json!({"port":port.name,"direction":"in","kind":port.kind,"source":binding.node,"sourceOutput":index}));
    }

    // Output bindings: the module feeds free pipeline inputs
    // of compatible type; an input already fed by a link is refused.
    let mut wired_outputs: std::collections::BTreeMap<(String, String), &ModuleBinding> =
        Default::default();
    for binding in output_bindings {
        let Some(port) = manifest.outputs.iter().find(|p| p.name == binding.port) else {
            return Err(format!("Port de sortie inconnu : {}", binding.port).into());
        };
        let Some(input) = binding.input.as_deref().filter(|i| !i.is_empty()) else {
            return Err(format!("Entrée cible requise pour le port {}", port.name).into());
        };
        if binding.output_index.is_some() {
            return Err(format!(
                "La liaison de sortie {} ne prend pas d'index source",
                port.name
            )
            .into());
        }
        if wired_outputs
            .insert((binding.node.clone(), input.to_owned()), binding)
            .is_some()
        {
            return Err(format!(
                "Entrée cible connectée deux fois : {}.{}",
                binding.node, input
            )
            .into());
        }
    }
    for binding in output_bindings {
        let port = &manifest
            .outputs
            .iter()
            .find(|p| p.name == binding.port)
            .ok_or("Port de sortie inconnu")?;
        let input = binding.input.as_deref().unwrap_or_default();
        let target = pipeline_nodes
            .get(&binding.node)
            .ok_or_else(|| format!("Node cible absent du pipeline : {}", binding.node))?;
        let target_class = target["class_type"]
            .as_str()
            .ok_or("Classe cible absente")?;
        let expected = declared_kind(node_info, target_class, input)?;
        if !compatible(&expected, &port.kind) {
            return Err(format!(
                "Types de lien incompatibles : {}.{} attend {expected}, le port {} fournit {}",
                binding.node, input, port.name, port.kind
            )
            .into());
        }
        if target["inputs"].get(input).is_some_and(Value::is_array) {
            return Err(format!(
                "Entrée déjà alimentée : {}.{} reçoit déjà un lien",
                binding.node, input
            )
            .into());
        }
        connections.push(json!({"port":port.name,"direction":"out","kind":port.kind,"target":binding.node,"targetInput":input}));
    }

    // Exposed parameters: controlled rewrite on the instance, never on the
    // registered module. A parameter whose input is fed internally
    // is refused (double feed).
    let override_list = overrides
        .as_array()
        .ok_or("Overrides de paramètres : tableau attendu")?;
    let mut applied = Vec::new();
    for entry in override_list {
        let node = entry
            .get("node")
            .and_then(Value::as_str)
            .ok_or("Override sans node")?;
        let input = entry
            .get("input")
            .and_then(Value::as_str)
            .ok_or("Override sans entrée")?;
        let value = entry.get("value").ok_or("Override sans valeur")?;
        let parameter = manifest
            .parameters
            .iter()
            .find(|p| p.node == *node && p.input == *input)
            .ok_or_else(|| format!("Paramètre non exposé par ce module : {node}.{input}"))?;
        if document_nodes[&parameter.node]["inputs"]
            .get(&parameter.input)
            .is_some_and(Value::is_array)
        {
            return Err(format!(
                "Paramètre déjà alimenté en interne : {node}.{}",
                parameter.input
            )
            .into());
        }
        applied.push((
            mapping[node].clone(),
            parameter.input.clone(),
            override_value(&parameter.kind, value)?,
        ));
    }
    for (node, input, value) in applied {
        renamed[&node]["inputs"][&input] = value;
    }

    // Internal links are rewritten to the instance ids; boundary
    // links receive their pipeline source bound above.
    let mut merged = pipeline.clone();
    for (old_id, new_id) in &mapping {
        let mut node = renamed[new_id].clone();
        for (key, value) in node["inputs"].as_object_mut().into_iter().flatten() {
            let Some(link) = value.as_array_mut() else {
                continue;
            };
            let source = link[0].as_str().unwrap_or_default().to_owned();
            if mapping.contains_key(&source) {
                link[0] = json!(mapping[&source]);
            } else {
                let port = port_inputs
                    .get(&(old_id.clone(), key.clone()))
                    .ok_or_else(|| {
                        format!(
                            "Port d'entrée introuvable pour le lien de frontière : {old_id}.{key}"
                        )
                    })?;
                let binding = bound_inputs[port.name.as_str()];
                link[0] = json!(binding.node);
                link[1] = json!(binding.output_index.ok_or("Index de sortie requis")?);
            }
        }
        merged
            .as_object_mut()
            .expect("merged is a clone of the pipeline object validated by nodes_of")
            .insert(new_id.clone(), node);
    }
    for binding in output_bindings {
        let port = manifest
            .outputs
            .iter()
            .find(|p| p.name == binding.port)
            .ok_or("Port de sortie inconnu")?;
        let source_id = &mapping[&port.node];
        merged
            .as_object_mut()
            .expect("merged is a clone of the pipeline object validated by nodes_of")
            [binding.node.as_str()]["inputs"][binding.input.as_deref().unwrap_or_default()] =
            json!([source_id, port.output_index]);
    }

    // Warnings: missing custom node → dependency already listed, use
    // flagged. With no missing class, the merged graph must still pass the
    // engine validation (structure, types, cycles) if it contains an output node.
    let mut warnings = Vec::new();
    let mut known = true;
    let mut has_output = false;
    for node in merged
        .as_object()
        .expect("merged is a clone of the pipeline object validated by nodes_of")
        .values()
    {
        let class = node["class_type"].as_str().unwrap_or_default();
        if node_info.get(class).is_none() {
            known = false;
            if !warnings.iter().any(|w: &String| w.contains(class)) {
                warnings.push(format!(
                    "Custom node absent de object_info : {class} (dépendance listée, utilisation à vérifier)"
                ));
            }
        } else if node_info[class]
            .get("output_node")
            .is_some_and(|v| v.as_bool() == Some(true))
        {
            has_output = true;
        }
    }
    if !known {
        warnings
            .push("Validation moteur complète ignorée : classes absentes de object_info.".into());
    } else if has_output {
        validate_graph(&merged, node_info)?;
    } else {
        warnings.push("Aucun node de sortie exécutable dans le pipeline connecté : contrat validé par types seulement.".into());
    }
    Ok(json!({"instance":instance,"connections":connections,"warnings":warnings,"graph":merged}))
}

impl Database {
    /// Records a new version. An absent `module` creates a module;
    /// otherwise an immutable version is added and becomes the current version.
    pub fn save_workflow_module(
        &self,
        module: Option<&str>,
        name: &str,
        description: &str,
        source_workflow_id: Option<&str>,
        source_sha256: &str,
        draft: &ModuleDraft,
    ) -> Result<(String, u32)> {
        if name.trim().is_empty() || name.len() > 300 {
            return Err("Nom de module invalide".into());
        }
        if description.len() > 4000 {
            return Err("Description de module trop longue".into());
        }
        let manifest = draft.manifest(name, description);
        let manifest_json = serde_json::to_string(&manifest)?;
        let document_json = serde_json::to_string(&draft.document)?;
        let tx = self.connection.unchecked_transaction()?;
        let id = match module {
            Some(existing) => {
                let known: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM workflow_modules WHERE id=?)",
                    [existing],
                    |r| r.get(0),
                )?;
                if !known {
                    return Err("Module inconnu".into());
                }
                existing.to_owned()
            }
            None => uuid::Uuid::new_v4().to_string(),
        };
        let version: u32 = tx.query_row(
            "SELECT COALESCE(MAX(version),0)+1 FROM workflow_module_versions WHERE module_id=?",
            [&id],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO workflow_modules(id,name,description,current_version) VALUES(?1,?2,?3,1) ON CONFLICT(id) DO NOTHING",
            params![id, name, description],
        )?;
        tx.execute(
            "INSERT INTO workflow_module_versions(module_id,version,manifest_json,document_json,source_workflow_id,source_sha256) VALUES(?1,?2,?3,?4,?5,?6)",
            params![id, version, manifest_json, document_json, source_workflow_id, source_sha256],
        )?;
        tx.execute(
            "UPDATE workflow_modules SET current_version=? WHERE id=?",
            params![version, id],
        )?;
        tx.commit()?;
        Ok((id, version))
    }

    /// Extracts a module from a saved API workflow, against object_info.
    pub fn save_module_from_workflow(
        &self,
        workflow_id: &str,
        module: Option<&str>,
        name: &str,
        description: &str,
        selection: &[String],
        node_info: &Value,
    ) -> Result<(String, u32)> {
        let text: String = self
            .connection
            .query_row(
                "SELECT document_json FROM workflow_revisions WHERE id=?",
                [workflow_id],
                |r| r.get(0),
            )
            .map_err(|_| "Workflow inconnu")?;
        let graph: Value = serde_json::from_str(&text)?;
        let draft = extract_module(
            &graph,
            node_info,
            &selection.iter().map(String::as_str).collect::<Vec<_>>(),
        )?;
        let sha = digest(text.as_bytes());
        self.save_workflow_module(module, name, description, Some(workflow_id), &sha, &draft)
    }

    fn load_manifest(&self, id: &str, version: u32) -> Result<ModuleManifest> {
        let text: String = self
            .connection
            .query_row(
                "SELECT manifest_json FROM workflow_module_versions WHERE module_id=? AND version=?",
                params![id, version],
                |r| r.get(0),
            )
            .map_err(|_| format!("Version inconnue : {id} v{version}"))?;
        let manifest: ModuleManifest = serde_json::from_str(&text)?;
        if manifest.manifest_version > MANIFEST_VERSION {
            return Err("Manifeste de module plus récent que cette application".into());
        }
        Ok(manifest)
    }

    /// Library: modules with their current version and its ports.
    pub fn workflow_modules(&self) -> Result<Vec<Value>> {
        let mut q = self.connection.prepare(
            "SELECT m.id,m.name,m.description,m.current_version,
             (SELECT count(*) FROM workflow_module_versions v WHERE v.module_id=m.id)
             FROM workflow_modules m ORDER BY m.rowid",
        )?;
        let rows: Vec<(String, String, String, u32, u32)> = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, u32>(3)?,
                    r.get::<_, u32>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut result = Vec::new();
        for (id, name, description, current, count) in rows {
            let manifest = self.load_manifest(&id, current)?;
            result.push(json!({
                "id":id,"name":name,"description":description,
                "currentVersion":current,"versionCount":count,
                "inputs":manifest.inputs,"outputs":manifest.outputs,
                "dependencies":manifest.dependencies,
            }));
        }
        Ok(result)
    }

    /// Version detail: manifest, kept nodes, versions, warnings.
    pub fn module_detail(
        &self,
        id: &str,
        version: Option<u32>,
        node_info: Option<&Value>,
    ) -> Result<Value> {
        let current: u32 = self
            .connection
            .query_row(
                "SELECT current_version FROM workflow_modules WHERE id=?",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| format!("Module inconnu ou illisible : {e}"))?;
        let resolved = version.unwrap_or(current);
        let (created, source_workflow, source_sha): (String, Option<String>, String) = self
            .connection
            .query_row(
                "SELECT v.created_at,v.source_workflow_id,v.source_sha256
                 FROM workflow_module_versions v WHERE v.module_id=?1 AND v.version=?2",
                rusqlite::params![id, resolved],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|e| format!("Version inconnue : {id} v{resolved} : {e}"))?;
        let manifest = self.load_manifest(id, resolved)?;
        let document: String = self
            .connection
            .query_row(
                "SELECT document_json FROM workflow_module_versions WHERE module_id=? AND version=?",
                params![id, resolved],
                |r| r.get(0),
            )
            .map_err(|_| format!("Version inconnue : {id} v{resolved}"))?;
        let document: Value = serde_json::from_str(&document)?;
        let nodes: Vec<Value> = document
            .as_object()
            .ok_or("Document de module invalide")?
            .iter()
            .map(|(id, node)| json!({"id":id,"classType":node["class_type"]}))
            .collect();
        let mut versions = Vec::new();
        let mut q = self.connection.prepare(
            "SELECT version,created_at FROM workflow_module_versions WHERE module_id=? ORDER BY version",
        )?;
        let rows: Vec<(u32, String)> = q
            .query_map([id], |r| Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        versions.extend(
            rows.into_iter()
                .map(|(v, at)| json!({"version":v,"createdAt":at})),
        );
        let mut warnings = Vec::new();
        if let Some(info) = node_info {
            for class in &manifest.dependencies.classes {
                if info.get(class).is_none() {
                    warnings.push(format!(
                        "Custom node absent de object_info : {class} (dépendance listée, utilisation à vérifier)"
                    ));
                }
            }
        }
        Ok(json!({
            "id":id,"version":resolved,"isCurrent":resolved==current,
            "manifest":manifest,"nodes":nodes,"nodeCount":nodes.len(),
            "document":document,
            "versions":versions,"createdAt":created,
            "sourceWorkflowId":source_workflow,"sourceSha256":source_sha,
            "warnings":warnings,
        }))
    }

    /// Exact comparison of two versions: nodes, parameters, ports.
    pub fn compare_module_versions(&self, id: &str, a: u32, b: u32) -> Result<Value> {
        let load = |version: u32| -> Result<(ModuleManifest, Value)> {
            let manifest = self.load_manifest(id, version)?;
            let document: String = self
                .connection
                .query_row(
                    "SELECT document_json FROM workflow_module_versions WHERE module_id=? AND version=?",
                    params![id, version],
                    |r| r.get(0),
                )
                .map_err(|_| format!("Version inconnue : {id} v{version}"))?;
            Ok((manifest, serde_json::from_str(&document)?))
        };
        let (manifest_a, document_a) = load(a)?;
        let (manifest_b, document_b) = load(b)?;
        let nodes = |document: &Value| document.as_object().cloned().unwrap_or_default();
        let (nodes_a, nodes_b) = (nodes(&document_a), nodes(&document_b));
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut changed = Vec::new();
        for (id, node) in &nodes_b {
            match nodes_a.get(id) {
                None => added.push(json!({"id":id,"classType":node["class_type"]})),
                Some(before) if before != node => {
                    let inputs_a = before["inputs"].as_object().cloned().unwrap_or_default();
                    let inputs_b = node["inputs"].as_object().cloned().unwrap_or_default();
                    let input_added: Vec<String> = inputs_b
                        .keys()
                        .filter(|k| !inputs_a.contains_key(*k))
                        .cloned()
                        .collect();
                    let input_removed: Vec<String> = inputs_a
                        .keys()
                        .filter(|k| !inputs_b.contains_key(*k))
                        .cloned()
                        .collect();
                    let input_changed: Vec<Value> = inputs_b
                        .iter()
                        .filter(|(k, v)| inputs_a.get(*k).is_some_and(|before| before != *v))
                        .map(|(k, v)| json!({"input":k,"before":inputs_a[k],"after":v}))
                        .collect();
                    changed.push(json!({
                        "id":id,
                        "classChanged":before["class_type"]!=node["class_type"],
                        "inputs":{"added":input_added,"removed":input_removed,"changed":input_changed},
                    }));
                }
                Some(_) => {}
            }
        }
        for (id, node) in &nodes_a {
            if !nodes_b.contains_key(id) {
                removed.push(json!({"id":id,"classType":node["class_type"]}));
            }
        }
        fn find<'a>(list: &'a [Value], name: &str) -> Option<&'a Value> {
            list.iter().find(|p| p["name"].as_str() == Some(name))
        }
        let diff_ports = |a: &Value, b: &Value, label: &str| {
            let empty = Vec::new();
            let (list_a, list_b) = (
                a.as_array().unwrap_or(&empty),
                b.as_array().unwrap_or(&empty),
            );
            let added: Vec<&Value> = list_b
                .iter()
                .filter(|p| find(list_a, p["name"].as_str().unwrap_or_default()).is_none())
                .collect();
            let removed: Vec<&Value> = list_a
                .iter()
                .filter(|p| find(list_b, p["name"].as_str().unwrap_or_default()).is_none())
                .collect();
            let changed: Vec<Value> = list_b
                .iter()
                .filter_map(|p| {
                    let before = find(list_a, p["name"].as_str().unwrap_or_default())?;
                    (before != p).then(|| json!({label:{"before":before,"after":p}}))
                })
                .collect();
            json!({"added":added,"removed":removed,"changed":changed})
        };
        let parameters = |manifest: &ModuleManifest| {
            manifest
                .parameters
                .iter()
                .map(|p| ((p.node.clone(), p.input.clone()), p.clone()))
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        let (params_a, params_b) = (parameters(&manifest_a), parameters(&manifest_b));
        let mut params_added = Vec::new();
        let mut params_removed = Vec::new();
        let mut params_changed = Vec::new();
        for (key, after) in &params_b {
            match params_a.get(key) {
                None => params_added.push(json!({"node":key.0,"input":key.1,"after":after})),
                Some(before) if before != after => params_changed
                    .push(json!({"node":key.0,"input":key.1,"before":before,"after":after})),
                Some(_) => {}
            }
        }
        for (key, before) in &params_a {
            if !params_b.contains_key(key) {
                params_removed.push(json!({"node":key.0,"input":key.1,"before":before}));
            }
        }
        Ok(json!({
            "module":id,"a":a,"b":b,
            "nodes":{"added":added,"removed":removed,"changed":changed},
            "ports":{"inputs":diff_ports(&serde_json::to_value(&manifest_a.inputs)?,&serde_json::to_value(&manifest_b.inputs)?,"input"),
                     "outputs":diff_ports(&serde_json::to_value(&manifest_a.outputs)?,&serde_json::to_value(&manifest_b.outputs)?,"output")},
            "parameters":{"added":params_added,"removed":params_removed,"changed":params_changed},
        }))
    }

    /// Rollback: the current reference points to an earlier version;
    /// all versions stay intact and readable.
    pub fn rollback_module(&self, id: &str, version: u32) -> Result<()> {
        let known: bool = self
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM workflow_module_versions WHERE module_id=? AND version=?)",
                params![id, version],
                |r| r.get(0),
            )
            .map_err(|_| "Module inconnu")?;
        if !known {
            return Err(format!("Version inconnue : {id} v{version}").into());
        }
        self.connection.execute(
            "UPDATE workflow_modules SET current_version=? WHERE id=?",
            params![version, id],
        )?;
        Ok(())
    }

    /// Connection contract validated by type: returns the merged graph without
    /// execution or write. Requires object_info to check each type.
    pub fn connect_module(
        &self,
        id: &str,
        version: u32,
        instance: &str,
        pipeline: &Value,
        connection: &ModuleConnection,
        node_info: &Value,
    ) -> Result<Value> {
        let manifest = self.load_manifest(id, version)?;
        let document: String = self
            .connection
            .query_row(
                "SELECT document_json FROM workflow_module_versions WHERE module_id=? AND version=?",
                params![id, version],
                |r| r.get(0),
            )
            .map_err(|_| format!("Version inconnue : {id} v{version}"))?;
        let document: Value = serde_json::from_str(&document)?;
        instantiate_module(
            &manifest, &document, instance, pipeline, connection, node_info,
        )
    }

    /// Read-only node listing of an imported workflow (node picker for
    /// extraction): id, class and optional `_meta.title`, numeric ids first.
    pub fn workflow_nodes(&self, workflow: &str) -> Result<Vec<Value>> {
        let text: String = self
            .connection
            .query_row(
                "SELECT document_json FROM workflow_revisions WHERE id=?",
                [workflow],
                |r| r.get(0),
            )
            .map_err(|_| format!("Workflow inconnu : {workflow}"))?;
        let graph: Value = serde_json::from_str(&text)?;
        let mut nodes: Vec<Value> = nodes_of(&graph, "Workflow")?
            .iter()
            .map(|(id, node)| {
                json!({
                    "id": id,
                    "classType": node.get("class_type").cloned().unwrap_or(Value::Null),
                    "title": node.pointer("/_meta/title").cloned().unwrap_or(Value::Null),
                })
            })
            .collect();
        nodes.sort_by(|a, b| node_order(a["id"].as_str(), b["id"].as_str()));
        Ok(nodes)
    }

    /// Typed connection candidates to reuse a module version inside a target
    /// workflow: for each input port, the target nodes whose declared output
    /// type matches; for each output port, the target inputs whose declared
    /// type matches (`linked` marks inputs already fed by a link, which the
    /// connection would replace). Read-only; nothing is merged here.
    pub fn module_candidates(
        &self,
        id: &str,
        version: Option<u32>,
        workflow: &str,
        node_info: &Value,
    ) -> Result<Value> {
        let current: u32 = self
            .connection
            .query_row(
                "SELECT current_version FROM workflow_modules WHERE id=?",
                [id],
                |r| r.get(0),
            )
            .map_err(|_| format!("Module inconnu : {id}"))?;
        let version = version.unwrap_or(current);
        let manifest = self.load_manifest(id, version)?;
        let text: String = self
            .connection
            .query_row(
                "SELECT document_json FROM workflow_revisions WHERE id=?",
                [workflow],
                |r| r.get(0),
            )
            .map_err(|_| format!("Workflow inconnu : {workflow}"))?;
        let graph: Value = serde_json::from_str(&text)?;
        let nodes = nodes_of(&graph, "Workflow")?;
        let mut ids: Vec<&String> = nodes.keys().collect();
        ids.sort_by(|a, b| node_order(Some(a), Some(b)));
        let mut unknown = Vec::new();
        let inputs: Vec<Value> = manifest
            .inputs
            .iter()
            .map(|port| {
                let mut sources = Vec::new();
                for node_id in &ids {
                    let class = nodes[*node_id]["class_type"].as_str().unwrap_or_default();
                    let Some(outputs) = node_info
                        .get(class)
                        .and_then(|s| s.get("output"))
                        .and_then(Value::as_array)
                    else {
                        continue;
                    };
                    for (index, kind) in outputs.iter().enumerate() {
                        if let Some(kind) = kind.as_str().filter(|k| compatible(&port.kind, k)) {
                            sources.push(json!({"node":node_id,"outputIndex":index,"classType":class,"kind":kind,
                                "title":nodes[*node_id].pointer("/_meta/title")}));
                        }
                    }
                }
                json!({"port":port.name,"kind":port.kind,"sources":sources})
            })
            .collect();
        let outputs: Vec<Value> = manifest
            .outputs
            .iter()
            .map(|port| {
                let mut targets = Vec::new();
                for node_id in &ids {
                    let class = nodes[*node_id]["class_type"].as_str().unwrap_or_default();
                    if node_info.get(class).is_none() {
                        continue;
                    }
                    // Every schema-declared input, present or still free; a
                    // linked input cannot be rewired (`connect_module` refuses).
                    let schema = &node_info[class];
                    let mut declared: Vec<&String> = ["/input/required", "/input/optional"]
                        .iter()
                        .filter_map(|p| schema.pointer(p).and_then(Value::as_object))
                        .flat_map(|o| o.keys())
                        .collect();
                    declared.dedup();
                    for input in declared {
                        if let Ok(kind) = declared_kind(node_info, class, input) {
                            if compatible(&kind, &port.kind) {
                                let linked = nodes[*node_id]["inputs"].get(input).is_some_and(Value::is_array);
                                targets.push(json!({"node":node_id,"input":input,"classType":class,"kind":kind,
                                    "linked":linked,"title":nodes[*node_id].pointer("/_meta/title")}));
                            }
                        }
                    }
                }
                json!({"port":port.name,"kind":port.kind,"targets":targets})
            })
            .collect();
        for node_id in &ids {
            let class = nodes[*node_id]["class_type"].as_str().unwrap_or_default();
            if node_info.get(class).is_none() {
                unknown.push(format!("{node_id} ({class})"));
            }
        }
        let missing: Vec<&String> = manifest
            .dependencies
            .classes
            .iter()
            .filter(|class| node_info.get(class.as_str()).is_none())
            .collect();
        Ok(json!({
            "module": id, "version": version, "workflow": workflow,
            "inputs": inputs, "outputs": outputs,
            "missingModuleClasses": missing,
            "unknownWorkflowNodes": unknown,
            "parameters": manifest.parameters,
        }))
    }

    /// Preview of a module instance merged into a stored workflow. The stored
    /// revision is never modified: the merged graph is returned with the
    /// exact list of added nodes and of rewired inputs of existing nodes.
    pub fn module_insert_preview(
        &self,
        id: &str,
        version: Option<u32>,
        workflow: &str,
        instance: &str,
        connection: &ModuleConnection,
        node_info: &Value,
    ) -> Result<Value> {
        let current: u32 = self
            .connection
            .query_row(
                "SELECT current_version FROM workflow_modules WHERE id=?",
                [id],
                |r| r.get(0),
            )
            .map_err(|_| format!("Module inconnu : {id}"))?;
        let version = version.unwrap_or(current);
        let (name, text): (String, String) = self
            .connection
            .query_row(
                "SELECT name,document_json FROM workflow_revisions WHERE id=?",
                [workflow],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| format!("Workflow inconnu : {workflow}"))?;
        let original: Value = serde_json::from_str(&text)?;
        let result =
            self.connect_module(id, version, instance, &original, connection, node_info)?;
        let merged = &result["graph"];
        let before = nodes_of(&original, "Workflow")?;
        let after = nodes_of(merged, "Graphe préparé")?;
        let mut added: Vec<&String> = after.keys().filter(|k| !before.contains_key(*k)).collect();
        added.sort_by(|a, b| node_order(Some(a), Some(b)));
        let mut changed = Vec::new();
        for (node, old) in before {
            let new = after
                .get(node)
                .ok_or("Node d'origine perdu par la composition")?;
            if new["class_type"] != old["class_type"] {
                return Err(format!("Classe du node d'origine {node} modifiée").into());
            }
            for (input, value) in new["inputs"].as_object().into_iter().flatten() {
                if old["inputs"].get(input) != Some(value) {
                    changed.push(json!({"node":node,"input":input,"before":old["inputs"].get(input),"after":value}));
                }
            }
        }
        Ok(json!({
            "module": id, "version": version, "instance": instance,
            "workflow": {"id": workflow, "name": name, "nodeCount": before.len()},
            "addedNodes": added, "changedInputs": changed,
            "connections": result["connections"], "warnings": result["warnings"],
            "graph": merged,
        }))
    }

    /// Name and stored API document of a workflow revision (read-only).
    pub fn workflow_document(&self, workflow: &str) -> Result<(String, String)> {
        self.connection
            .query_row(
                "SELECT name,document_json FROM workflow_revisions WHERE id=?",
                [workflow],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| format!("Workflow inconnu : {workflow}").into())
    }

    /// Id of the most recent workflow revision with this exact name.
    pub fn workflow_document_by_name(&self, name: &str) -> Result<String> {
        self.connection
            .query_row(
                "SELECT id FROM workflow_revisions WHERE name=? ORDER BY rowid DESC LIMIT 1",
                [name],
                |r| r.get(0),
            )
            .map_err(|_| format!("Workflow inconnu : {name}").into())
    }
}
