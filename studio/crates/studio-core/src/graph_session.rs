//! Interactive editing session over the headless graph editor core.
//!
//! The editor state lives here, in Rust, never in JavaScript: parameter values
//! travel as text and are typed by the engine schema, so u64 seeds stay exact.
//! Four graphs are kept apart: the stored original (never modified), the
//! displayed/edited document (`EditorGraph`), and the executed API graph
//! produced only by `export`, which is validated against the engine schema and
//! saved as a *new* workflow by the caller.
use crate::graph_editor::{
    Command, Connection, EditorGraph, EditorGroup, EditorNode, InputPort, LinkEnd, LinkEndInput,
    OutputPort, Point,
};
use crate::{workflow::validate_graph, Result};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NodeMove {
    pub node: String,
    pub x: f64,
    pub y: f64,
}

/// One user operation. Operations made of several core commands (multi-node
/// moves, layout, grouping) undo and redo as a single step.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum EditOp {
    SetBypass { node: String, bypass: bool },
    Move {
        moves: Vec<NodeMove>,
    },
    #[serde(rename_all = "camelCase")]
    SetParam {
        node: String,
        input: String,
        value: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Connect {
        from_node: String,
        output: usize,
        to_node: String,
        input: String,
    },
    Disconnect {
        connection: String,
    },
    Remove {
        nodes: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    Add {
        class_type: String,
        x: f64,
        y: f64,
    },
    Group {
        title: String,
        nodes: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    StyleNode {
        node: String,
        title: String,
        color: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    StyleGroup {
        group: String,
        title: String,
        color: Option<String>,
    },
    Ungroup {
        group: String,
    },
    MoveGroup {
        group: String,
        dx: f64,
        dy: f64,
    },
    ResizeGroup {
        group: String,
        bounds: crate::graph_editor::GroupBounds,
    },
    Layout,
    Undo,
    Redo,
}

struct ScopeFrame {
    graph: EditorGraph,
    ui_source: crate::comfy_ui::UiSource,
    original_api: Option<Value>,
    original_ids: HashMap<String, String>,
    workflow: String,
    undo_batches: Vec<usize>,
    redo_batches: Vec<usize>,
    parent_source: Value,
    definition_id: String,
    definition_name: String,
    definition: Value,
    boundary_links: Vec<Value>,
    boundary_inputs: BTreeMap<(String, usize), Value>,
}

fn raw_id(value: &Value) -> Option<String> {
    value
        .as_i64()
        .map(|n| n.to_string())
        .or_else(|| value.as_str().map(str::to_owned))
}

fn boundary_link(link: &Value) -> bool {
    let origin = raw_id(&link["origin_id"]);
    let target = raw_id(&link["target_id"]);
    origin.as_deref() == Some("-10") || target.as_deref() == Some("-20")
}
fn ui_link_part(link: &Value, key: &str, index: usize) -> Option<u64> {
    link.get(key).and_then(|v| v.as_u64()).or_else(|| {
        link.as_array()
            .and_then(|a| a.get(index))
            .and_then(Value::as_u64)
    })
}

fn ui_link_id(link: &Value) -> Option<u64> {
    ui_link_part(link, "id", 0)
}

fn ui_link_origin(link: &Value) -> Option<u64> {
    ui_link_part(link, "origin_id", 1)
}

fn ui_link_origin_slot(link: &Value) -> Option<usize> {
    ui_link_part(link, "origin_slot", 2).map(|v| v as usize)
}

fn ui_link_target(link: &Value) -> Option<u64> {
    ui_link_part(link, "target_id", 3)
}

fn ui_link_target_slot(link: &Value) -> Option<usize> {
    ui_link_part(link, "target_slot", 4).map(|v| v as usize)
}

fn ui_link_type(link: &Value) -> String {
    link.get("type")
        .and_then(Value::as_str)
        .or_else(|| {
            link.as_array()
                .and_then(|a| a.get(5))
                .and_then(Value::as_str)
        })
        .unwrap_or("*")
        .to_owned()
}

fn new_raw_node(node: &EditorNode, ui_id: u64, info: &Value) -> Result<Value> {
    let schema = info
        .get(&node.class_type)
        .ok_or_else(|| format!("Classe absente de object_info : {}", node.class_type))?;
    let order = crate::comfy_ui::widget_order(schema)?;
    let mut values = Vec::new();
    for (_name, declaration) in &order {
        let default = declaration[1].get("default").cloned().unwrap_or_else(|| {
            match declaration[0].as_str() {
                Some("INT") => json!(0),
                Some("FLOAT") => json!(0.0),
                Some("BOOLEAN") => json!(false),
                Some("STRING") => json!(""),
                Some("COMBO") => declaration[0]
                    .as_array()
                    .and_then(|values| values.first())
                    .cloned()
                    .unwrap_or_else(|| json!("")),
                _ => json!(""),
            }
        });
        values.push(default);
        if declaration[1]["control_after_generate"] == true {
            values.push(json!("fixed"));
        }
    }
    let inputs: Vec<Value> = node
        .inputs
        .iter()
        .map(|p| json!({"name": p.name, "type": p.port_type, "link": Value::Null}))
        .collect();
    let outputs: Vec<Value> = node
        .outputs
        .iter()
        .map(|p| json!({"name": p.name, "type": p.port_type, "links": []}))
        .collect();
    let position = node
        .position
        .map(|p| json!([p.x, p.y]))
        .unwrap_or_else(|| json!([0.0, 0.0]));
    Ok(json!({
        "id": ui_id,
        "type": node.class_type,
        "pos": position,
        "size": [230, 120],
        "flags": {},
        "order": 0,
        "mode": 0,
        "inputs": inputs,
        "outputs": outputs,
        "properties": {},
        "widgets_values": values,
        "widgets_values_named": {},
    }))
}

fn group_bounds_for(
    group: &EditorGroup,
    nodes: &[EditorNode],
) -> Option<crate::graph_editor::GroupBounds> {
    if let Some(bounds) = group.bounds {
        return Some(bounds);
    }
    let mut x0 = f64::INFINITY;
    let mut y0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    for id in &group.node_ids {
        let node = nodes.iter().find(|n| &n.id == id)?;
        let p = node.position?;
        let s = node.size.unwrap_or(crate::graph_editor::Size {
            width: 230.0,
            height: 120.0,
        });
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x + s.width);
        y1 = y1.max(p.y + s.height);
    }
    if !x0.is_finite() || !y0.is_finite() {
        return None;
    }
    Some(crate::graph_editor::GroupBounds {
        x: x0 - 28.0,
        y: y0 - 48.0,
        width: (x1 - x0) + 56.0,
        height: (y1 - y0) + 76.0,
    })
}

fn sync_groups(original: &Value, groups: &[EditorGroup], nodes: &[EditorNode]) -> Vec<Value> {
    let originals = original["groups"].as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for group in groups {
        let Some(bounds) = group_bounds_for(group, nodes) else {
            continue;
        };
        let mut raw = group
            .id
            .strip_prefix("group-")
            .and_then(|index| index.parse::<usize>().ok())
            .and_then(|index| originals.get(index))
            .cloned()
            .unwrap_or_else(|| json!({"title": group.title, "bounding": []}));
        raw["title"] = json!(group.title);
        raw["bounding"] = json!([bounds.x, bounds.y, bounds.width, bounds.height]);
        out.push(raw);
    }
    out
}

fn sync_widgets(raw: &mut Value, node: &EditorNode, info: &Value) -> Result<()> {
    let schema = info
        .get(&node.class_type)
        .ok_or_else(|| format!("Classe absente de object_info : {}", node.class_type))?;
    let order = crate::comfy_ui::widget_order(schema)?;
    let mut updates: Vec<(String, usize, Value)> = Vec::new();
    {
        let values = raw["widgets_values"]
            .as_array_mut()
            .ok_or("widgets_values absent")?;
        let mut index = 0usize;
        for (name, declaration) in &order {
            if index >= values.len() {
                values.push(Value::Null);
            }
            if let Some(value) = node.params.get(name) {
                values[index] = value.clone();
                updates.push((name.clone(), index, value.clone()));
            }
            index += 1;
            if declaration[1]["control_after_generate"] == true {
                if index >= values.len() {
                    values.push(json!("fixed"));
                }
                index += 1;
            }
        }
    }
    if let Some(named) = raw
        .get_mut("widgets_values_named")
        .and_then(Value::as_object_mut)
    {
        for (name, _index, value) in updates {
            named.insert(name, value);
        }
    }
    Ok(())
}

pub struct GraphSession {
    graph: EditorGraph,
    ui_source: Option<crate::comfy_ui::UiSource>,
    original_api: Option<Value>,
    pub workflow: String,
    /// Editor node id → API id of the stored original (kept on export).
    original_ids: HashMap<String, String>,
    info: Value,
    undo_batches: Vec<usize>,
    redo_batches: Vec<usize>,
    scope: Vec<ScopeFrame>,
}

fn ports_from_schema(class: &str, info: &Value) -> Result<(Vec<InputPort>, Vec<OutputPort>)> {
    let schema = info
        .get(class)
        .ok_or_else(|| format!("Classe absente de ComfyUI : {class}"))?;
    let mut inputs = Vec::new();
    for section in ["required", "optional"] {
        for (key, declaration) in schema
            .pointer(&format!("/input/{section}"))
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let kind = declaration.get(0);
            let port_type = match kind {
                Some(Value::String(text)) => text.clone(),
                Some(Value::Array(_)) => "COMBO".into(),
                _ => "*".into(),
            };
            inputs.push(InputPort {
                name: key.clone(),
                port_type,
                optional: section == "optional",
            });
        }
    }
    let types = schema["output"].as_array().ok_or("Sorties inconnues")?;
    let names = schema["output_name"].as_array();
    let outputs = types
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            let port_type = kind.as_str().unwrap_or("*").to_owned();
            let name = names
                .and_then(|n| n.get(index))
                .and_then(Value::as_str)
                .unwrap_or(&port_type)
                .to_owned();
            OutputPort { name, port_type }
        })
        .collect();
    Ok((inputs, outputs))
}

/// Text → JSON value typed by the input port. Integers keep their exact u64/i64.
fn typed_value(port_type: &str, text: &str) -> Result<Value> {
    Ok(match port_type {
        "LORAS" => {
            let value: Value = serde_json::from_str(text)?;
            if !crate::workflow::lora_manager::valid_loras(&value) {
                return Err("Liste LoRA invalide".into());
            }
            value
        }
        "INT" => {
            if let Ok(value) = text.trim().parse::<u64>() {
                json!(value)
            } else {
                json!(text
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| format!("Entier attendu : « {text} »"))?)
            }
        }
        "FLOAT" => {
            let value = text
                .trim()
                .parse::<f64>()
                .map_err(|_| format!("Nombre attendu : « {text} »"))?;
            if !value.is_finite() {
                return Err("Nombre fini attendu".into());
            }
            json!(value)
        }
        "BOOLEAN" => json!(text
            .trim()
            .parse::<bool>()
            .map_err(|_| "true ou false attendu")?),
        _ => json!(text),
    })
}

/// Exact textual form of a parameter, for display (no JS number rounding).
fn exact_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

impl GraphSession {
    /// Opens the stored API graph. Node order of `from_executed` follows the
    /// source object, which lets us remember every original API id.
    pub fn open(workflow: &str, api: &Value, info: &Value, name: &str) -> Result<Self> {
        let graph = EditorGraph::from_executed(api, info, name)?;
        let canonical = crate::workflow::api_graph(&api.to_string())?;
        let keys: Vec<&String> = canonical
            .as_object()
            .ok_or("Graphe invalide")?
            .keys()
            .collect();
        let nodes = &graph.document().nodes;
        if keys.len() != nodes.len() {
            return Err("Correspondance des nodes impossible".into());
        }
        let mut original_ids = HashMap::new();
        for (key, node) in keys.iter().zip(nodes) {
            if canonical[*key]["class_type"].as_str() != Some(node.class_type.as_str()) {
                return Err("Correspondance des nodes incohérente".into());
            }
            original_ids.insert(node.id.clone(), (*key).clone());
        }
        let mut session = Self {
            graph,
            ui_source: None,
            original_api: Some(canonical.clone()),
            workflow: workflow.into(),
            original_ids,
            info: info.clone(),
            undo_batches: Vec::new(),
            redo_batches: Vec::new(),
            scope: Vec::new(),
        };
        // Nodes imported from an API graph carry no position: lay them out once,
        // outside the user's history.
        let moves = session.layout_moves();
        for command in moves {
            session.graph.apply(command)?;
        }
        // Positions are kept; only the history of this initial layout is dropped.
        session.graph = EditorGraph::from_document(session.graph.document().clone())?;
        Ok(session)
    }

    pub fn open_file(text: &str, info: &Value, name: &str) -> Result<Self> {
        if text.len() > 16_000_000 {
            return Err("Workflow supérieur à 16 Mo".into());
        }
        let source: Value = serde_json::from_str(text)?;
        if source.get("nodes").is_none() {
            return Self::open("", &source, info, name);
        }
        let (source, graph) = crate::comfy_ui::UiSource::open(source, info, name)?;
        let original_ids = graph
            .document()
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.id.clone()))
            .collect();
        Ok(Self {
            graph,
            ui_source: Some(source),
            original_api: None,
            workflow: String::new(),
            original_ids,
            info: info.clone(),
            undo_batches: vec![],
            redo_batches: vec![],
            scope: Vec::new(),
        })
    }
    pub fn is_scoped(&self) -> bool {
        !self.scope.is_empty()
    }

    /// Enters the definition of an instantiated subgraph. Only internal links
    /// are shown; the `-10/-20` boundary links are kept
    /// for re-injection on exit.
    pub fn enter_subgraph(&mut self, container_id: &str) -> Result<Value> {
        self.enter_subgraph_state(container_id)?;
        Ok(self.view())
    }

    fn enter_subgraph_state(&mut self, container_id: &str) -> Result<()> {
        let parent_source = self
            .ui_source
            .as_ref()
            .ok_or("Entrée dans un subgraph réservée aux documents UI")?
            .original
            .clone();
        let (instance_id, definition_id) = {
            let document = self.graph.document();
            let container = document
                .subgraphs
                .iter()
                .find(|c| c.id == container_id)
                .ok_or("Conteneur subgraph inconnu")?;
            let instance = container
                .node_ids
                .first()
                .ok_or("Conteneur subgraph vide")?;
            let node = document
                .nodes
                .iter()
                .find(|n| &n.id == instance)
                .ok_or("Instance subgraph absente")?;
            (instance.clone(), node.class_type.clone())
        };
        let _ = instance_id;
        let definition = parent_source
            .pointer("/definitions/subgraphs")
            .and_then(Value::as_array)
            .and_then(|defs| {
                defs.iter()
                    .find(|d| d["id"].as_str() == Some(definition_id.as_str()))
            })
            .cloned()
            .ok_or("Définition de subgraph introuvable")?;
        let definition_name = definition["name"]
            .as_str()
            .unwrap_or(&definition_id)
            .to_owned();
        let all_links = definition["links"].as_array().cloned().unwrap_or_default();
        let boundary_links: Vec<Value> = all_links
            .iter()
            .filter(|l| boundary_link(l))
            .cloned()
            .collect();
        let boundary_ids: BTreeSet<String> = boundary_links
            .iter()
            .filter_map(|link| raw_id(&link["id"]))
            .collect();
        let internal_links: Vec<Value> = all_links
            .iter()
            .filter(|l| !boundary_link(l))
            .cloned()
            .collect();
        let mut synthetic = definition.clone();
        synthetic["version"] = json!(0.4);
        synthetic["definitions"] = parent_source
            .get("definitions")
            .cloned()
            .unwrap_or_else(|| json!({"subgraphs": []}));
        let mut boundary_inputs = BTreeMap::new();
        if let Some(nodes) = synthetic["nodes"].as_array_mut() {
            for node in nodes {
                let node_id = raw_id(&node["id"]).ok_or("Node subgraph sans identifiant")?;
                if let Some(inputs) = node["inputs"].as_array_mut() {
                    for (index, input) in inputs.iter_mut().enumerate() {
                        let Some(link) = input.get("link").and_then(raw_id) else {
                            continue;
                        };
                        if boundary_ids.contains(&link) {
                            boundary_inputs.insert((node_id.clone(), index), input["link"].clone());
                            input["link"] = Value::Null;
                        }
                    }
                }
            }
        }
        synthetic["links"] = json!(internal_links);
        let (inner_ui, inner_graph) =
            crate::comfy_ui::UiSource::open(synthetic, &self.info, &definition_name)?;
        let inner_ids = inner_graph
            .document()
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.id.clone()))
            .collect();
        let frame = ScopeFrame {
            graph: std::mem::replace(&mut self.graph, inner_graph),
            ui_source: self
                .ui_source
                .take()
                .ok_or("Document UI absent pendant l'entrée")?,
            original_api: self.original_api.take(),
            original_ids: std::mem::take(&mut self.original_ids),
            workflow: std::mem::take(&mut self.workflow),
            undo_batches: std::mem::take(&mut self.undo_batches),
            redo_batches: std::mem::take(&mut self.redo_batches),
            parent_source,
            definition_id,
            definition_name,
            definition: definition.clone(),
            boundary_links,
            boundary_inputs,
        };
        self.ui_source = Some(inner_ui);
        self.original_ids = inner_ids;
        self.scope.push(frame);
        Ok(())
    }

    fn build_definition(&self, frame: &ScopeFrame) -> Result<Value> {
        if let Some(source) = &self.ui_source {
            if !self.ui_structure_changed(source) {
                // A widget edit must not reconstruct untouched boundary links or UI nodes.
                let edited = source.edited(&self.graph)?;
                let mut definition = frame.definition.clone();
                for node in definition["nodes"]
                    .as_array_mut()
                    .ok_or("Nodes internes absents")?
                {
                    if let Some(updated) = edited["nodes"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|n| n["id"] == node["id"])
                    {
                        for key in ["widgets_values", "widgets_values_named", "pos"] {
                            if let Some(value) = updated.get(key) {
                                node[key] = value.clone();
                            }
                        }
                    }
                }
                return Ok(definition);
            }
        }
        let document = self.graph.document();
        for (node_id, _) in frame.boundary_inputs.keys() {
            if !document.nodes.iter().any(|n| n.id == *node_id) {
                return Err(format!(
                    "Suppression refusée : node frontière {node_id} encore relié à l'extérieur"
                )
                .into());
            }
        }
        for link in &frame.boundary_links {
            if raw_id(&link["target_id"]).as_deref() == Some("-20") {
                if let Some(origin) = link.get("origin_id").and_then(Value::as_u64) {
                    if !document.nodes.iter().any(|n| n.id == origin.to_string()) {
                        return Err(
                            "Suppression refusée : un node alimente la sortie du subgraph".into(),
                        );
                    }
                }
            }
        }
        let original_nodes = frame.definition["nodes"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut next_node = original_nodes
            .iter()
            .filter_map(|n| n["id"].as_u64())
            .max()
            .unwrap_or(0)
            + 1;
        let mut ui_ids = BTreeMap::new();
        for node in &document.nodes {
            let existing = original_nodes
                .iter()
                .find(|n| raw_id(&n["id"]).as_deref() == Some(node.id.as_str()))
                .and_then(|n| n["id"].as_u64());
            let id = existing.unwrap_or_else(|| {
                let id = next_node;
                next_node += 1;
                id
            });
            ui_ids.insert(node.id.clone(), id);
        }
        let mut raw_nodes = Vec::new();
        for node in &document.nodes {
            let ui_id = ui_ids[&node.id];
            let mut raw = match original_nodes
                .iter()
                .find(|n| raw_id(&n["id"]).as_deref() == Some(node.id.as_str()))
            {
                Some(original) => original.clone(),
                None => new_raw_node(node, ui_id, &self.info)?,
            };
            raw["id"] = json!(ui_id);
            if let Some(position) = node.position {
                raw["pos"] = json!([position.x, position.y]);
            }
            // Subgraph instances have no object_info schema:
            // their promoted widgets stay carried by the source.
            if self.info.get(&node.class_type).is_some() {
                sync_widgets(&mut raw, node, &self.info)?;
            }
            raw_nodes.push((node.id.clone(), raw));
        }

        let original_links = frame.definition["links"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let boundary_ids: BTreeSet<u64> =
            frame.boundary_links.iter().filter_map(ui_link_id).collect();
        let internal_existing: Vec<&Value> = original_links
            .iter()
            .filter(|l| {
                !boundary_ids.contains(&ui_link_id(l).unwrap_or(0))
                    && ui_link_origin(l).is_some()
                    && ui_link_target(l).is_some()
            })
            .collect();
        let mut next_link = original_links
            .iter()
            .filter_map(ui_link_id)
            .max()
            .unwrap_or(0)
            + 1;
        let mut internal_links = Vec::new();
        let mut targets: BTreeMap<(String, String), u64> = BTreeMap::new();
        let mut sources: BTreeMap<(String, usize), Vec<u64>> = BTreeMap::new();
        for connection in &document.connections {
            let from_id = ui_ids[&connection.from.node];
            let to_id = ui_ids[&connection.to.node];
            let source = document
                .nodes
                .iter()
                .find(|n| n.id == connection.from.node)
                .ok_or("Node source absent")?;
            let target = document
                .nodes
                .iter()
                .find(|n| n.id == connection.to.node)
                .ok_or("Node cible absent")?;
            let target_slot = target
                .inputs
                .iter()
                .position(|p| p.name == connection.to.input)
                .ok_or("Entrée cible absente")?;
            let reuse = internal_existing
                .iter()
                .find(|l| {
                    ui_link_origin(l) == Some(from_id)
                        && ui_link_origin_slot(l) == Some(connection.from.output)
                        && ui_link_target(l) == Some(to_id)
                        && ui_link_target_slot(l) == Some(target_slot)
                })
                .copied();
            let id = match reuse.and_then(ui_link_id) {
                Some(id) => id,
                None => {
                    let id = next_link;
                    next_link += 1;
                    id
                }
            };
            let kind = source
                .outputs
                .get(connection.from.output)
                .map(|p| p.port_type.clone())
                .filter(|t| !t.is_empty())
                .or_else(|| reuse.map(ui_link_type))
                .unwrap_or_else(|| "*".into());
            internal_links.push(json!({
                "id": id,
                "origin_id": from_id,
                "origin_slot": connection.from.output,
                "target_id": to_id,
                "target_slot": target_slot,
                "type": kind,
            }));
            targets.insert(
                (connection.to.node.clone(), connection.to.input.clone()),
                id,
            );
            sources
                .entry((connection.from.node.clone(), connection.from.output))
                .or_default()
                .push(id);
        }
        for (document_id, raw) in &mut raw_nodes {
            for input in raw["inputs"].as_array_mut().into_iter().flatten() {
                input["link"] = Value::Null;
            }
            for output in raw["outputs"].as_array_mut().into_iter().flatten() {
                output["links"] = json!([]);
            }
            for ((node_id, input), link) in &targets {
                if node_id != document_id {
                    continue;
                }
                let slot = raw["inputs"]
                    .as_array_mut()
                    .and_then(|inputs| inputs.iter_mut().find(|p| p["name"] == json!(input)))
                    .ok_or("Entrée de node absente")?;
                slot["link"] = json!(link);
            }
            for ((node_id, output), links) in &sources {
                if node_id != document_id {
                    continue;
                }
                let shot = raw["outputs"]
                    .as_array_mut()
                    .and_then(|outputs| outputs.get_mut(*output))
                    .ok_or("Sortie de node absente")?;
                shot["links"] = json!(links);
            }
            for ((node_id, index), link) in &frame.boundary_inputs {
                if node_id != document_id {
                    continue;
                }
                let input = raw["inputs"]
                    .get_mut(*index)
                    .ok_or("Entrée frontière absente")?;
                input["link"] = link.clone();
            }
        }
        for link in &frame.boundary_links {
            if raw_id(&link["target_id"]).as_deref() != Some("-20") {
                continue;
            }
            let Some(origin) = link.get("origin_id").and_then(Value::as_u64) else {
                continue;
            };
            let Some(slot) = link.get("origin_slot").and_then(Value::as_u64) else {
                continue;
            };
            let Some(id) = ui_link_id(link) else { continue };
            let raw = raw_nodes
                .iter_mut()
                .find(|(_, raw)| raw["id"].as_u64() == Some(origin))
                .map(|(_, raw)| raw)
                .ok_or("Node de sortie frontière absent")?;
            let output = raw["outputs"]
                .as_array_mut()
                .and_then(|outputs| outputs.get_mut(slot as usize))
                .ok_or("Sortie frontière hors limites")?;
            let links = output["links"]
                .as_array_mut()
                .ok_or("Liens de sortie absents")?;
            links.push(json!(id));
        }
        let mut definition = frame.definition.clone();
        definition["nodes"] = Value::Array(raw_nodes.into_iter().map(|(_, raw)| raw).collect());
        definition["groups"] = Value::Array(sync_groups(
            &frame.definition,
            &document.groups,
            &document.nodes,
        ));
        let mut links = frame.boundary_links.clone();
        links.extend(internal_links);
        definition["links"] = Value::Array(links);
        Ok(definition)
    }

    /// Leaves the definition. `commit` re-injects internal nodes/links into the
    /// UI source, restoring the boundary links exactly; internal
    /// structural modifications stay refused.
    pub fn exit_subgraph(&mut self, commit: bool) -> Result<Value> {
        self.exit_subgraph_state(commit)?;
        Ok(self.view())
    }

    fn exit_subgraph_state(&mut self, commit: bool) -> Result<()> {
        let frame = self.scope.last().ok_or("Aucun subgraph ouvert")?;
        let mut new_ui = None;
        if commit {
            let updated = self.build_definition(frame)?;
            let mut parent_source = frame.parent_source.clone();
            // Nested definitions modified in the current scope
            // bubble up with the source before replacing the target definition.
            if let Some(definitions) = self
                .ui_source
                .as_ref()
                .and_then(|source| source.original.get("definitions"))
                .cloned()
            {
                parent_source["definitions"] = definitions;
            }
            let definition = parent_source
                .pointer_mut("/definitions/subgraphs")
                .and_then(Value::as_array_mut)
                .and_then(|defs| {
                    defs.iter_mut()
                        .find(|d| d["id"].as_str() == Some(frame.definition_id.as_str()))
                })
                .ok_or("Définition de subgraph introuvable à la sortie")?;
            *definition = updated;
            let (ui, _) =
                crate::comfy_ui::UiSource::open(parent_source, &self.info, &frame.definition_name)?;
            new_ui = Some(ui);
        }
        let frame = self.scope.pop().ok_or("Aucun subgraph ouvert")?;
        self.graph = frame.graph;
        self.original_api = frame.original_api;
        self.original_ids = frame.original_ids;
        self.workflow = frame.workflow;
        self.undo_batches = frame.undo_batches;
        self.redo_batches = frame.redo_batches;
        self.ui_source = Some(match new_ui {
            Some(ui) => ui,
            None => frame.ui_source,
        });
        Ok(())
    }

    /// Serializable snapshot of a session: edited document and preserved
    /// original source, never rewritten.
    pub fn snapshot(&self) -> Result<Value> {
        if self.is_scoped() {
            return Err("Sortir du subgraph avant d'enregistrer la session".into());
        }
        let document = self.graph.serialize()?;
        let source_format = if self.ui_source.is_some() {
            "comfy_ui"
        } else {
            "comfy_api"
        };
        let ui_source = match &self.ui_source {
            Some(source) => Some(serde_json::to_string(source.original())?),
            None => None,
        };
        let original_api = self
            .original_api
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let original_ids = serde_json::to_string(&self.original_ids)?;
        Ok(json!({
            "name": self.graph.document().name,
            "workflow": self.workflow,
            "sourceFormat": source_format,
            "document": document,
            "uiSource": ui_source,
            "originalApi": original_api,
            "originalIds": original_ids,
        }))
    }

    /// Rebuilds a persisted session. The UI source is reopened with the
    /// current schema; the edited document remains the truth.
    pub fn restore(
        workflow: &str,
        document: &str,
        ui_source: Option<&str>,
        original_api: Option<&str>,
        original_ids: &str,
        info: Value,
    ) -> Result<Self> {
        let graph = if ui_source.is_some() {
            EditorGraph::deserialize_ui(document)?
        } else {
            EditorGraph::deserialize(document)?
        };
        let name = graph.document().name.clone();
        let ui_source = match ui_source {
            Some(text) => {
                let value: Value = serde_json::from_str(text)?;
                Some(crate::comfy_ui::UiSource::open(value, &info, &name)?.0)
            }
            None => None,
        };
        let original_api = match original_api {
            Some(text) => Some(serde_json::from_str(text)?),
            None => None,
        };
        if ui_source.is_some() && original_api.is_some() {
            return Err("Session d'édition incohérente : source UI et API simultanées".into());
        }
        let original_ids: HashMap<String, String> = serde_json::from_str(original_ids)?;
        Ok(Self {
            graph,
            ui_source,
            original_api,
            workflow: workflow.to_owned(),
            original_ids,
            info,
            undo_batches: Vec::new(),
            redo_batches: Vec::new(),
            scope: Vec::new(),
        })
    }

    /// Copies a selection as a portable fragment: complete nodes and
    /// internal connections only. No document modification.
    pub fn copy_fragment(&self, nodes: &[String]) -> Result<Value> {
        if nodes.is_empty() {
            return Err("Sélection vide".into());
        }
        let unique: BTreeSet<&str> = nodes.iter().map(String::as_str).collect();
        if unique.len() != nodes.len() || unique.len() > 256 {
            return Err("Sélection invalide ou supérieure à 256 nodes".into());
        }
        let document = self.graph.document();
        let mut selected = Vec::new();
        for id in &unique {
            let node = document
                .nodes
                .iter()
                .find(|n| n.id == *id)
                .ok_or_else(|| format!("Node inconnu : {id}"))?;
            selected.push(node.clone());
        }
        let connections: Vec<Connection> = document
            .connections
            .iter()
            .filter(|c| {
                unique.contains(c.from.node.as_str()) && unique.contains(c.to.node.as_str())
            })
            .cloned()
            .collect();
        Ok(json!({
            "format": 1,
            "nodes": selected,
            "connections": connections,
        }))
    }

    /// Pastes a copied fragment, in a single undo/redo step. Identifiers,
    /// ports and links are regenerated; parameters are kept.
    ///
    /// Preserved UI documents keep their structure untouchable: structural
    /// pasting is refused there until a dedicated scope model is
    /// available.
    pub fn paste_fragment(&mut self, fragment: &Value, offset: Point) -> Result<Value> {
        if !offset.x.is_finite()
            || !offset.y.is_finite()
            || offset.x.abs() > 1_000_000.0
            || offset.y.abs() > 1_000_000.0
        {
            return Err("Décalage de collage invalide".into());
        }
        if fragment.get("format").and_then(Value::as_u64) != Some(1) {
            return Err("Fragment de collage inconnu".into());
        }
        let nodes = fragment
            .get("nodes")
            .and_then(Value::as_array)
            .ok_or("Fragment sans nodes")?;
        if nodes.is_empty() || nodes.len() > 256 {
            return Err("Fragment vide ou supérieur à 256 nodes".into());
        }
        let connections = fragment
            .get("connections")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if connections.len() > 1024 {
            return Err("Trop de connexions dans le fragment".into());
        }
        let mut ids = BTreeMap::new();
        let mut order = Vec::new();
        let mut commands = Vec::new();
        for value in nodes {
            let original: EditorNode = serde_json::from_value(value.clone())
                .map_err(|e| format!("Node de fragment invalide : {e}"))?;
            if original.class_type.is_empty() {
                return Err("Classe absente dans le fragment".into());
            }
            if ids.contains_key(&original.id) {
                return Err(format!("Node dupliqué dans le fragment : {}", original.id).into());
            }
            let (inputs, outputs) = ports_from_schema(&original.class_type, &self.info)?;
            let id = self.graph.new_id();
            ids.insert(original.id.clone(), id.clone());
            order.push(id.clone());
            let position = original
                .position
                .map(|p| Point {
                    x: p.x + offset.x,
                    y: p.y + offset.y,
                })
                .or(Some(offset));
            commands.push(Command::AddNode {
                node: EditorNode {
                    id,
                    class_type: original.class_type,
                    title: original.title,
                    color: original.color,
                    position,
                    size: original.size,
                    inputs,
                    outputs,
                    params: original.params,
                },
            });
        }
        for value in connections {
            let connection: Connection = serde_json::from_value(value)
                .map_err(|e| format!("Connexion de fragment invalide : {e}"))?;
            let from = ids
                .get(&connection.from.node)
                .ok_or("Connexion vers un node hors fragment")?;
            let to = ids
                .get(&connection.to.node)
                .ok_or("Connexion vers un node hors fragment")?;
            commands.push(Command::Connect {
                connection: Connection {
                    id: self.graph.new_id(),
                    from: LinkEnd {
                        node: from.clone(),
                        output: connection.from.output,
                    },
                    to: LinkEndInput {
                        node: to.clone(),
                        input: connection.to.input,
                    },
                },
            });
        }
        self.batch(commands)?;
        Ok(json!({"nodes": order, "view": self.view()}))
    }

    fn ui_structure_changed(&self, source: &crate::comfy_ui::UiSource) -> bool {
        let document = self.graph.document();
        let original = source.original();
        let original_nodes: BTreeSet<String> = original["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| raw_id(&n["id"]))
            .collect();
        let current_nodes: BTreeSet<String> = document.nodes.iter().map(|n| n.id.clone()).collect();
        if original_nodes != current_nodes {
            return true;
        }
        let link_key =
            |from: u64, slot: usize, to: u64, target: usize| format!("{from}:{slot}:{to}:{target}");
        let original_links: BTreeSet<String> = original["links"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|link| {
                Some(link_key(
                    ui_link_origin(link)?,
                    ui_link_origin_slot(link)?,
                    ui_link_target(link)?,
                    ui_link_target_slot(link)?,
                ))
            })
            .collect();
        let current_links: BTreeSet<String> = document
            .connections
            .iter()
            .filter_map(|connection| {
                let from = raw_id(&json!(connection.from.node))?;
                let to = raw_id(&json!(connection.to.node))?;
                let target = document
                    .nodes
                    .iter()
                    .find(|n| n.id == connection.to.node)?
                    .inputs
                    .iter()
                    .position(|p| p.name == connection.to.input)?;
                Some(link_key(
                    from.parse().ok()?,
                    connection.from.output,
                    to.parse().ok()?,
                    target,
                ))
            })
            .collect();
        if original_links != current_links {
            return true;
        }
        let original_groups = original["groups"].as_array().cloned().unwrap_or_default();
        if original_groups.len() != document.groups.len() {
            return true;
        }
        for (index, group) in document.groups.iter().enumerate() {
            let Some(expected) = original_groups.get(index) else {
                return true;
            };
            let bounds = match group_bounds_for(group, &document.nodes) {
                Some(bounds) => bounds,
                None => return true,
            };
            if expected["title"].as_str() != Some(group.title.as_str())
                || expected["bounding"].as_array().is_none_or(|b| {
                    b.len() != 4
                        || b[0].as_f64() != Some(bounds.x)
                        || b[1].as_f64() != Some(bounds.y)
                        || b[2].as_f64() != Some(bounds.width)
                        || b[3].as_f64() != Some(bounds.height)
                })
            {
                return true;
            }
        }
        false
    }

    fn serialize_ui_root(&self, source: &crate::comfy_ui::UiSource) -> Result<Value> {
        let document = self.graph.document();
        let original = source.original();
        let original_nodes = original["nodes"].as_array().cloned().unwrap_or_default();
        let mut next_node = original_nodes
            .iter()
            .filter_map(|n| n["id"].as_u64())
            .max()
            .unwrap_or(0)
            + 1;
        let mut ui_ids = BTreeMap::new();
        for node in &document.nodes {
            let existing = original_nodes
                .iter()
                .find(|n| raw_id(&n["id"]).as_deref() == Some(node.id.as_str()))
                .and_then(|n| n["id"].as_u64());
            let id = existing.unwrap_or_else(|| {
                let id = next_node;
                next_node += 1;
                id
            });
            ui_ids.insert(node.id.clone(), id);
        }
        let mut raw_nodes = Vec::new();
        for node in &document.nodes {
            let ui_id = ui_ids[&node.id];
            let mut raw = match original_nodes
                .iter()
                .find(|n| raw_id(&n["id"]).as_deref() == Some(node.id.as_str()))
            {
                Some(existing) => existing.clone(),
                None => new_raw_node(node, ui_id, &self.info)?,
            };
            raw["id"] = json!(ui_id);
            if let Some(position) = node.position {
                raw["pos"] = json!([position.x, position.y]);
            }
            if self.info.get(&node.class_type).is_some() {
                sync_widgets(&mut raw, node, &self.info)?;
            }
            raw_nodes.push((node.id.clone(), raw));
        }
        let original_links = original["links"].as_array().cloned().unwrap_or_default();
        let mut next_link = original_links
            .iter()
            .filter_map(ui_link_id)
            .max()
            .unwrap_or(0)
            + 1;
        let mut links = Vec::new();
        let mut targets: BTreeMap<(String, String), u64> = BTreeMap::new();
        let mut sources: BTreeMap<(String, usize), Vec<u64>> = BTreeMap::new();
        for connection in &document.connections {
            let from_id = ui_ids[&connection.from.node];
            let to_id = ui_ids[&connection.to.node];
            let source_node = document
                .nodes
                .iter()
                .find(|n| n.id == connection.from.node)
                .ok_or("Node source absent")?;
            let target_node = document
                .nodes
                .iter()
                .find(|n| n.id == connection.to.node)
                .ok_or("Node cible absent")?;
            let target_slot = target_node
                .inputs
                .iter()
                .position(|p| p.name == connection.to.input)
                .ok_or("Entrée cible absente")?;
            let reuse = original_links
                .iter()
                .find(|link| {
                    ui_link_origin(link) == Some(from_id)
                        && ui_link_origin_slot(link) == Some(connection.from.output)
                        && ui_link_target(link) == Some(to_id)
                        && ui_link_target_slot(link) == Some(target_slot)
                })
                .cloned();
            let id = reuse.as_ref().and_then(ui_link_id).unwrap_or_else(|| {
                let id = next_link;
                next_link += 1;
                id
            });
            let kind = source_node
                .outputs
                .get(connection.from.output)
                .map(|p| p.port_type.clone())
                .filter(|t| !t.is_empty())
                .or_else(|| reuse.as_ref().map(ui_link_type))
                .unwrap_or_else(|| "*".into());
            links.push(json!([
                id,
                from_id,
                connection.from.output,
                to_id,
                target_slot,
                kind
            ]));
            targets.insert(
                (connection.to.node.clone(), connection.to.input.clone()),
                id,
            );
            sources
                .entry((connection.from.node.clone(), connection.from.output))
                .or_default()
                .push(id);
        }
        for (document_id, raw) in &mut raw_nodes {
            for input in raw["inputs"].as_array_mut().into_iter().flatten() {
                input["link"] = Value::Null;
            }
            for output in raw["outputs"].as_array_mut().into_iter().flatten() {
                output["links"] = json!([]);
            }
            for ((node_id, input), link) in &targets {
                if node_id != document_id {
                    continue;
                }
                let slot = raw["inputs"]
                    .as_array_mut()
                    .and_then(|inputs| inputs.iter_mut().find(|p| p["name"] == json!(input)))
                    .ok_or("Entrée de node absente")?;
                slot["link"] = json!(link);
            }
            for ((node_id, output), source_links) in &sources {
                if node_id != document_id {
                    continue;
                }
                let shot = raw["outputs"]
                    .as_array_mut()
                    .and_then(|outputs| outputs.get_mut(*output))
                    .ok_or("Sortie de node absente")?;
                shot["links"] = json!(source_links);
            }
        }
        let mut result = original.clone();
        result["nodes"] = Value::Array(raw_nodes.into_iter().map(|(_, raw)| raw).collect());
        result["links"] = Value::Array(links);
        result["groups"] = Value::Array(sync_groups(original, &document.groups, &document.nodes));
        crate::comfy_ui::UiSource::open(result.clone(), &self.info, &document.name)?;
        Ok(result)
    }

    pub fn export_ui(&self) -> Result<Value> {
        if self.is_scoped() {
            return Err("Sortir du subgraph avant export".into());
        }
        if let Some(source) = &self.ui_source {
            if self.ui_structure_changed(source) {
                self.serialize_ui_root(source)
            } else {
                source.edited(&self.graph)
            }
        } else {
            crate::comfy_ui::from_api(
                &self.export()?,
                self.graph.document(),
                &self.original_ids,
                &self.info,
            )
        }
    }

    fn node(&self, id: &str) -> Result<&EditorNode> {
        self.graph
            .document()
            .nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or_else(|| format!("Node inconnu : {id}").into())
    }

    /// Layered layout: column = longest path from a source, row = order.
    fn layout_moves(&self) -> Vec<Command> {
        let document = self.graph.document();
        let mut depth: BTreeMap<&str, usize> =
            document.nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
        for _ in 0..document.nodes.len() {
            let mut changed = false;
            for c in &document.connections {
                let from = depth.get(c.from.node.as_str()).copied().unwrap_or(0);
                let to = depth.entry(c.to.node.as_str()).or_insert(0);
                if *to < from + 1 {
                    *to = from + 1;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut rows: BTreeMap<usize, usize> = BTreeMap::new();
        document
            .nodes
            .iter()
            .map(|node| {
                let column = depth[node.id.as_str()];
                let row = rows.entry(column).or_insert(0);
                let position = Point {
                    x: column as f64 * 340.0,
                    y: *row as f64 * 200.0,
                };
                *row += 1;
                Command::MoveNode {
                    node: node.id.clone(),
                    position: Some(position),
                }
            })
            .collect()
    }

    fn batch(&mut self, commands: Vec<Command>) -> Result<()> {
        let mut applied = 0;
        for command in commands {
            if let Err(error) = self.graph.apply(command) {
                for _ in 0..applied {
                    self.graph.undo()?;
                }
                return Err(error);
            }
            applied += 1;
        }
        if applied > 0 {
            self.undo_batches.push(applied);
            self.redo_batches.clear();
        }
        Ok(())
    }

    pub fn apply(&mut self, op: EditOp) -> Result<()> {
        if let EditOp::SetBypass { node, bypass } = &op {
            // ComfyUI mode 4 = bypass: the node stays in the workflow, its outputs
            // are wired to its inputs of the same type at API export. Subgraph
            // nodes cannot be targeted: their ids are not those of the root.
            if self.is_scoped() {
                return Err("Bypass disponible seulement pour les nodes du workflow principal".into());
            }
            let source=self.ui_source.as_mut().ok_or("Bypass : ouvrir le workflow UI pour conserver ses connexions")?;
            let raw=source.original["nodes"].as_array_mut().and_then(|nodes|nodes.iter_mut().find(|n|n["id"].as_str().map(str::to_owned).unwrap_or_else(||n["id"].to_string())==*node)).ok_or("Node absent du workflow")?;
            raw["mode"]=json!(if *bypass {4}else{0});
            // Saya's split into phases re-reads this mode for tagged nodes.
            if !raw["properties"].is_object(){raw["properties"]=json!({});}
            raw["properties"]["saya_original_mode"]=raw["mode"].clone();
            // Mark read by the passes of a Saya preset (saya_preset::overlay_widgets).
            if *bypass {raw["properties"]["saya_bypass"]=json!(true);} else if let Some(p)=raw["properties"].as_object_mut(){p.remove("saya_bypass");}
            self.undo_batches.clear();self.redo_batches.clear();
            return Ok(());
        }
        if self.is_scoped() {
        } else if let Some(source) = &self.ui_source {
            match &op {
                EditOp::Move { .. }
                | EditOp::Layout
                | EditOp::Undo
                | EditOp::Redo
                | EditOp::Add { .. }
                | EditOp::Remove { .. }
                | EditOp::Connect { .. }
                | EditOp::Disconnect { .. } => {}
                EditOp::SetParam { node, input, value }
                    if value.is_some() && source.can_edit_widget(node, input) => {}
                EditOp::Group { .. }
                | EditOp::Ungroup { .. }
                | EditOp::MoveGroup { .. }
                | EditOp::ResizeGroup { .. }
                | EditOp::StyleNode { .. }
                | EditOp::StyleGroup { .. } => {}
                _ => return Err("Document UI conservé : paramètre non reconnu".into()),
            }
        }
        match op {
            EditOp::SetBypass { .. } => unreachable!(),
            EditOp::Undo => {
                let size = self.undo_batches.pop().ok_or("Rien à annuler")?;
                for _ in 0..size {
                    self.graph.undo()?;
                }
                self.redo_batches.push(size);
                Ok(())
            }
            EditOp::Redo => {
                let size = self.redo_batches.pop().ok_or("Rien à rétablir")?;
                for _ in 0..size {
                    self.graph.redo()?;
                }
                self.undo_batches.push(size);
                Ok(())
            }
            EditOp::Move { moves } => {
                let commands = moves
                    .into_iter()
                    .map(|m| Command::MoveNode {
                        node: m.node,
                        position: Some(Point { x: m.x, y: m.y }),
                    })
                    .collect();
                self.batch(commands)
            }
            EditOp::SetParam { node, input, value } => {
                let port = self
                    .node(&node)?
                    .inputs
                    .iter()
                    .find(|p| p.name == input)
                    .ok_or_else(|| format!("Entrée inconnue : {input}"))?
                    .port_type
                    .clone();
                let value = match value {
                    None => Value::Null,
                    Some(text) => typed_value(&port, &text)?,
                };
                self.batch(vec![Command::SetParam { node, input, value }])
            }
            EditOp::Connect {
                from_node,
                output,
                to_node,
                input,
            } => {
                let id = self.graph.new_id();
                self.batch(vec![Command::Connect {
                    connection: Connection {
                        id,
                        from: LinkEnd {
                            node: from_node,
                            output,
                        },
                        to: LinkEndInput {
                            node: to_node,
                            input,
                        },
                    },
                }])
            }
            EditOp::Disconnect { connection } => {
                self.batch(vec![Command::Disconnect { connection }])
            }
            EditOp::Remove { nodes } => self.batch(
                nodes
                    .into_iter()
                    .map(|node| Command::RemoveNode { node })
                    .collect(),
            ),
            EditOp::Add { class_type, x, y } => {
                let (inputs, outputs) = ports_from_schema(&class_type, &self.info)?;
                let node = EditorNode {
                    id: self.graph.new_id(),
                    class_type,
                    title: String::new(),
                    color: None,
                    position: Some(Point { x, y }),
                    size: None,
                    inputs,
                    outputs,
                    params: BTreeMap::new(),
                };
                self.batch(vec![Command::AddNode { node }])
            }
            EditOp::Group { title, nodes } => {
                let group = EditorGroup {
                    id: self.graph.new_id(),
                    title,
                    node_ids: nodes,
                    bounds: None,
                    color: None,
                };
                self.batch(vec![Command::AddGroup { group }])
            }
            EditOp::StyleNode { node, title, color } => {
                self.batch(vec![Command::SetNodeStyle { node, title, color }])
            }
            EditOp::StyleGroup {
                group,
                title,
                color,
            } => self.batch(vec![Command::SetGroupStyle {
                group,
                title,
                color,
            }]),
            EditOp::Ungroup { group } => self.batch(vec![Command::DissolveGroup { group }]),
            EditOp::MoveGroup { group, dx, dy } => self.batch(vec![Command::MoveGroup {
                group,
                delta: Point { x: dx, y: dy },
            }]),
            EditOp::ResizeGroup { group, bounds } => {
                self.batch(vec![Command::ResizeGroup { group, bounds }])
            }
            EditOp::Layout => {
                let moves = self.layout_moves();
                self.batch(moves)
            }
        }
    }

    /// Displayed graph for the UI, with exact parameter text and status.
    /// Inspect or edit one exposed field without leaving the shared session scoped.
    pub fn scoped_widget(&mut self, path: &[String], edit: Option<EditOp>) -> Result<Value> {
        if self.is_scoped() || path.is_empty() || path.len() > 8 {
            return Err("Chemin de subgraph invalide".into());
        }
        if edit
            .as_ref()
            .is_some_and(|op| !matches!(op, EditOp::SetParam { .. } | EditOp::SetBypass { .. }))
        {
            return Err("Seuls les widgets sont éditables depuis Génération".into());
        }
        let commit = edit.is_some();
        let result = (|| {
            for container in path {
                self.enter_subgraph_state(container)?;
            }
            if let Some(op) = edit {
                self.apply(op)?;
            }
            Ok(self.view())
        })();
        while self.is_scoped() {
            if let Err(error) = self.exit_subgraph_state(commit && result.is_ok()) {
                while self.is_scoped() {
                    self.exit_subgraph_state(false)?;
                }
                return Err(error);
            }
        }
        result
    }

    pub fn view(&self) -> Value {
        let document = self.graph.document();
        let mut params = Map::new();
        for node in &document.nodes {
            let texts: Map<String, Value> = node
                .params
                .iter()
                .map(|(k, v)| (k.clone(), json!(exact_text(v))))
                .collect();
            params.insert(node.id.clone(), Value::Object(texts));
        }
        let missing: Vec<Value> = self
            .graph
            .missing_required_inputs()
            .into_iter()
            .map(|(node, input)| json!({"node": node, "input": input}))
            .collect();
        let export = if self.is_scoped() {
            Err("Sortir du subgraph avant export".into())
        } else {
            self.export()
        };
        json!({
            "scope": self.scope.last().map(|f| json!({"id": f.definition_id, "name": f.definition_name})),
            "workflow": self.workflow,
            "sourceFormat": if self.ui_source.is_some() {"comfy_ui"} else {"comfy_api"},
            "sourceDetails": self.ui_source.as_ref().map(|u|u.original.to_string()),
            "diagnostics": self.ui_source.as_ref().map(|u|u.diagnostics.clone()).unwrap_or_default(),
            "document": document,
            "paramText": params,
            "originalIds": self.original_ids,
            "canUndo": !self.undo_batches.is_empty(),
            "canRedo": !self.redo_batches.is_empty(),
            "missing": missing,
            "exportable": export.is_ok(),
            "exportError": export.err().map(|e| e.to_string()),
        })
    }

    /// Executed API graph: original API ids kept, new nodes numbered after the
    /// largest numeric id, links rewritten, validated against the engine schema.
    /// Execution path for both normal UI workflows (including subgraphs) and API imports.
    pub fn prepare_execution(&self, info: &Value) -> Result<(Value, Option<Value>)> {
        let ui = self.export_ui().ok();
        let api = if self.ui_source.is_some() {
            let source = self.export_ui()?;
            let prepared = crate::workflow_ui_api::prepare_api(&source.to_string(), Some(info))?;
            if prepared["report"]["complete"] != true {
                return Err(
                    format!("Conversion du workflow incomplète : {}", prepared["report"]).into(),
                );
            }
            prepared["api"].clone()
        } else {
            self.export()?
        };
        validate_graph(&api, info)?;
        Ok((api, ui))
    }

    pub fn export(&self) -> Result<Value> {
        if self.is_scoped() {
            return Err("Sortir du subgraph avant export API".into());
        }
        if let Some(source) = &self.ui_source {
            if self.ui_structure_changed(source) {
                if self.graph.document().subgraphs.is_empty() {
                    let graph = self.graph.to_executed()?;
                    validate_graph(&graph, &self.info)?;
                    return Ok(graph);
                }
                return Err(
                    "Structure UI modifiée : exporter en API via la préparation qualifiée".into(),
                );
            }
            return source.api(&self.graph, &self.info);
        }
        let executed = self.graph.to_executed()?;
        let nodes = executed.as_object().ok_or("Graphe exporté invalide")?;
        let mut next = self
            .original_ids
            .values()
            .filter_map(|id| id.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        let mut rename: HashMap<String, String> = HashMap::new();
        let mut ids: Vec<&String> = nodes.keys().collect();
        ids.sort();
        for id in ids {
            let target = match self.original_ids.get(id) {
                Some(original) => original.clone(),
                None => {
                    let fresh = next.to_string();
                    next += 1;
                    fresh
                }
            };
            rename.insert(id.clone(), target);
        }
        let mut out = Map::new();
        for (id, node) in nodes {
            let mut node = node.clone();
            for value in node["inputs"]
                .as_object_mut()
                .into_iter()
                .flat_map(|o| o.values_mut())
            {
                if let Some(link) = value.as_array_mut() {
                    if let Some(source) = link
                        .first()
                        .and_then(Value::as_str)
                        .and_then(|s| rename.get(s))
                    {
                        link[0] = json!(source);
                    }
                }
            }
            if let Some(original) = self
                .original_api
                .as_ref()
                .and_then(|a| a.get(&rename[id]))
                .and_then(Value::as_object)
            {
                for (key, value) in original {
                    if key != "inputs" && key != "class_type" {
                        node[key] = value.clone();
                    }
                }
            }
            out.insert(rename[id].clone(), node);
        }
        let graph = Value::Object(out);
        validate_graph(&graph, &self.info)?;
        Ok(graph)
    }

    /// Extracts a module from a selection of editor nodes: the
    /// displayed document is projected to API, session ids are renamed
    /// to their original ids (fresh numbers after the largest for
    /// nodes added in the session), then the module ports are derived
    /// from the crossing links by `modules::extract_module`. Exact values
    /// (u64, composite widgets) and original fields (_meta,
    /// extra) are preserved. Refused in subgraph view: a container's
    /// boundaries are opaque there, no partial extraction is invented;
    /// also refused when subgraph containers exist (API projection
    /// impossible without qualified flattening, see `prepare_api`).
    pub fn extract_module_draft(
        &self,
        selection: &[String],
    ) -> Result<crate::modules::ModuleDraft> {
        if self.is_scoped() {
            return Err("Sortir du subgraph avant l'extraction d'un module".into());
        }
        let (api, rename) = self.projection_with_original_ids()?;
        // The selection arrives in editor ids; the projection is in original ids.
        let renamed: Vec<String> = selection
            .iter()
            .map(|id| {
                rename
                    .get(id.as_str())
                    .cloned()
                    .ok_or_else(|| format!("Node sélectionné inconnu de la session : {id}").into())
            })
            .collect::<Result<_>>()?;
        crate::modules::extract_module(
            &api,
            &self.info,
            &renamed.iter().map(String::as_str).collect::<Vec<_>>(),
        )
    }

    /// API projection of the displayed document with original ids: same
    /// renaming as `export` (session ids → original ids, fresh numbers
    /// after the largest for added nodes), without validating the
    /// required inputs — a module is extracted from a workflow in progress, even
    /// incomplete outside the selected group. Also returns the renaming
    /// (editor id → projected id) to translate the selection.
    fn projection_with_original_ids(&self) -> Result<(Value, HashMap<String, String>)> {
        let projected = self.graph.to_executed()?;
        let objects = projected.as_object().ok_or("Projection API invalide")?;
        let mut next = self
            .original_ids
            .values()
            .filter_map(|id| id.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        let mut rename: HashMap<&str, String> = HashMap::new();
        for id in objects.keys() {
            let target = match self.original_ids.get(id) {
                Some(original) => original.clone(),
                None => {
                    let fresh = next.to_string();
                    next += 1;
                    fresh
                }
            };
            rename.insert(id.as_str(), target);
        }
        let mut out = Map::new();
        for (id, node) in objects {
            let mut node = node.clone();
            for value in node["inputs"]
                .as_object_mut()
                .into_iter()
                .flat_map(|o| o.values_mut())
            {
                if let Some(link) = value.as_array_mut() {
                    if let Some(source) = link
                        .first()
                        .and_then(Value::as_str)
                        .and_then(|s| rename.get(s))
                    {
                        link[0] = json!(source);
                    }
                }
            }
            if let Some(original) = self
                .original_api
                .as_ref()
                .and_then(|a| a.get(rename[id.as_str()].as_str()))
                .and_then(Value::as_object)
            {
                for (key, value) in original {
                    if key != "inputs" && key != "class_type" {
                        node[key] = value.clone();
                    }
                }
            } else if let Some(definition) =
                self.graph.document().nodes.iter().find(|n| &n.id == id)
            {
                // Node added in the session: the displayed title becomes _meta.
                if !definition.title.is_empty() {
                    node["_meta"] = json!({"title": definition.title});
                }
            }
            out.insert(rename[id.as_str()].clone(), node);
        }
        let mapping = rename.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
        Ok((Value::Object(out), mapping))
    }

    /// Fingerprint of the projected API graph (original ids): exact provenance of the
    /// module extracted from the editor — what was copied, independent of the
    /// ids de session.
    pub fn extract_module_fingerprint(&self) -> Result<String> {
        let (api, _) = self.projection_with_original_ids()?;
        Ok(crate::gallery::digest(api.to_string().as_bytes()))
    }
}
