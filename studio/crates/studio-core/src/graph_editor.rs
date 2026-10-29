//! Headless foundations of a future visual workflow editor.
//!
//! Explicit separation: the DISPLAYED graph (editable, with positions, sizes,
//! groups and subgraph containers) is distinct from the EXECUTED graph (ComfyUI API
//! format validated by the `workflow` engine). The `from_executed` and
//! `to_executed` adapters convert both ways for a tested subset
//! (nodes, links, parameters); positions are ignored by the executed graph and
//! subgraph containers stay opaque (no arbitrary conversion).
//!
//! Invariants guaranteed after each command:
//! - unique stable identifiers (uuid v4, generated once, never recycled);
//! - no dangling reference: each connection targets an existing node/port;
//! - no orphan connection: deleting a node removes its links;
//! - port type compatibility, no duplicate per input, no
//!   self-connection, no cycle;
//! - bounded undo/redo (`MAX_HISTORY`), discrete and reversible commands.
//!
//! Execution completeness (required inputs filled, output node
//! present, ComfyUI schema) remains the job of `workflow::validate_graph` on
//! the result of `to_executed`: the editor allows intermediate states
//! as long as they stay structurally consistent.

use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// Version of the editor's JSON serialization format.
pub const FORMAT_VERSION: u32 = 1;
pub const MAX_NODES: usize = 2000;
pub const MAX_CONNECTIONS: usize = 4000;
/// Maximum depth of the edit history (undo).
pub const MAX_HISTORY: usize = 200;
const MAX_TEXT: usize = 200_000;
const MAX_NAME: usize = 300;
const MAX_DOCUMENT: usize = 16_000_000;

pub type NodeId = String;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputPort {
    pub name: String,
    #[serde(rename = "type")]
    pub port_type: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputPort {
    pub name: String,
    #[serde(rename = "type")]
    pub port_type: String,
}

/// Exposed input or output of a subgraph container, kept as is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExposedPort {
    pub name: String,
    #[serde(rename = "type")]
    pub port_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorNode {
    pub id: NodeId,
    pub class_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub position: Option<Point>,
    #[serde(default)]
    pub size: Option<Size>,
    #[serde(default)]
    pub inputs: Vec<InputPort>,
    #[serde(default)]
    pub outputs: Vec<OutputPort>,
    #[serde(default)]
    pub params: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinkEnd {
    pub node: NodeId,
    pub output: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinkEndInput {
    pub node: NodeId,
    pub input: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Connection {
    pub id: String,
    pub from: LinkEnd,
    pub to: LinkEndInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GroupBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorGroup {
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub node_ids: Vec<NodeId>,
    #[serde(default)]
    pub bounds: Option<GroupBounds>,
    #[serde(default)]
    pub color: Option<String>,
}

/// Opaque box: the member nodes stay strictly identical, only the existing
/// exposed inputs/outputs are described. No transformation of the
/// content is offered by this module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubgraphContainer {
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub node_ids: Vec<NodeId>,
    #[serde(default)]
    pub inputs: Vec<ExposedPort>,
    #[serde(default)]
    pub outputs: Vec<ExposedPort>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditorDocument {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<EditorNode>,
    #[serde(default)]
    pub connections: Vec<Connection>,
    #[serde(default)]
    pub groups: Vec<EditorGroup>,
    #[serde(default)]
    pub subgraphs: Vec<SubgraphContainer>,
}

/// Discrete editor commands. Each command is atomic, validated
/// then reversible; identifiers are supplied by `EditorGraph::new_id`.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    AddNode {
        node: EditorNode,
    },
    RemoveNode {
        node: NodeId,
    },
    MoveNode {
        node: NodeId,
        position: Option<Point>,
    },
    ResizeNode {
        node: NodeId,
        size: Option<Size>,
    },
    /// `Value::Null` removes the parameter; otherwise a literal value (scalar or
    /// composite widget preserved as is, never a link array).
    SetParam {
        node: NodeId,
        input: String,
        value: Value,
    },
    Connect {
        connection: Connection,
    },
    Disconnect {
        connection: String,
    },
    AddGroup {
        group: EditorGroup,
    },
    DissolveGroup {
        group: String,
    },
    AssignGroup {
        group: String,
        node: NodeId,
    },
    UnassignGroup {
        node: NodeId,
    },
    MoveGroup {
        group: String,
        delta: Point,
    },
    ResizeGroup {
        group: String,
        bounds: GroupBounds,
    },
    SetNodeStyle {
        node: NodeId,
        title: String,
        color: Option<String>,
    },
    SetGroupStyle {
        group: String,
        title: String,
        color: Option<String>,
    },
    AddSubgraph {
        container: SubgraphContainer,
    },
    RemoveSubgraph {
        container: String,
    },
    MoveSubgraph {
        container: String,
        delta: Point,
    },
}

#[derive(Debug, Clone)]
struct HistoryEntry {
    forward: Command,
    inverse: Vec<Command>,
}

/// Edit graph (displayed) + history. The document is the source of
/// truth; the history is not serialized.
#[derive(Debug, Clone)]
pub struct EditorGraph {
    document: EditorDocument,
    display_cycles: bool,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

fn types_compatible(expected: &str, provided: &str) -> bool {
    expected == "*" || provided == "*" || expected.split(',').any(|t| t.trim() == provided)
}

fn valid_color(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn valid_title(title: &str) -> bool {
    title.len() <= MAX_NAME
}

fn finite_point(point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

fn finite_size(size: Size) -> bool {
    size.width.is_finite() && size.height.is_finite() && size.width > 0.0 && size.height > 0.0
}

fn valid_param_value(value: &Value) -> bool {
    if value.is_null() || value.is_array() {
        return false;
    }
    value.as_str().is_none_or(|s| s.len() <= MAX_TEXT)
}

impl EditorNode {
    fn ports_valid(&self) -> Result<()> {
        let mut input_names = BTreeSet::new();
        for port in &self.inputs {
            if port.name.is_empty() || port.port_type.is_empty() {
                return Err("Port sans nom ou sans type".into());
            }
            if !input_names.insert(port.name.as_str()) {
                return Err(format!("Port d'entrée dupliqué : {}", port.name).into());
            }
        }
        let mut output_names = BTreeSet::new();
        for port in &self.outputs {
            if port.name.is_empty() || port.port_type.is_empty() {
                return Err("Port sans nom ou sans type".into());
            }
            if !output_names.insert(port.name.as_str()) {
                return Err(format!("Port de sortie dupliqué : {}", port.name).into());
            }
        }
        Ok(())
    }

    fn params_valid(&self) -> Result<()> {
        for (key, value) in &self.params {
            if !self.inputs.iter().any(|p| &p.name == key) {
                return Err(format!("Paramètre sans port déclaré : {key}").into());
            }
            if !valid_param_value(value) {
                return Err(format!("Valeur de paramètre interdite : {key}").into());
            }
        }
        Ok(())
    }

    pub fn input_port(&self, name: &str) -> Option<&InputPort> {
        self.inputs.iter().find(|p| p.name == name)
    }

    pub fn output_port(&self, index: usize) -> Option<&OutputPort> {
        self.outputs.get(index)
    }
}

impl EditorGraph {
    /// New empty document, identifier generated once.
    pub fn new(name: &str) -> Result<Self> {
        Self::from_document(EditorDocument {
            format_version: FORMAT_VERSION,
            id: new_id(),
            name: name.to_owned(),
            nodes: Vec::new(),
            connections: Vec::new(),
            groups: Vec::new(),
            subgraphs: Vec::new(),
        })
    }

    pub fn new_id(&self) -> String {
        new_id()
    }

    pub fn document(&self) -> &EditorDocument {
        &self.document
    }

    /// Resumption of a document (after deserialization or construction):
    /// all the structural consistency is re-verified.
    pub fn from_document(document: EditorDocument) -> Result<Self> {
        Self::from_document_mode(document, false)
    }
    pub(crate) fn from_ui_document(document: EditorDocument) -> Result<Self> {
        Self::from_document_mode(document, true)
    }
    fn from_document_mode(document: EditorDocument, display_cycles: bool) -> Result<Self> {
        let graph = Self {
            document,
            display_cycles,
            undo: Vec::new(),
            redo: Vec::new(),
        };
        graph.validate()?;
        Ok(graph)
    }

    pub fn serialize(&self) -> Result<String> {
        serde_json::to_string(&self.document)
            .map_err(|e| format!("Sérialisation impossible : {e}").into())
    }

    /// Strict deserialization of the v1 format: version enforced, unknown
    /// fields refused, size bounded, consistency re-verified.
    pub fn deserialize(text: &str) -> Result<Self> {
        Self::deserialize_mode(text, false)
    }

    pub(crate) fn deserialize_ui(text: &str) -> Result<Self> {
        Self::deserialize_mode(text, true)
    }

    fn deserialize_mode(text: &str, display_cycles: bool) -> Result<Self> {
        if text.len() > MAX_DOCUMENT {
            return Err("Document supérieur à 16 Mo".into());
        }
        let document: EditorDocument =
            serde_json::from_str(text).map_err(|e| format!("Document éditeur invalide : {e}"))?;
        if document.format_version != FORMAT_VERSION {
            return Err(format!(
                "Format d'éditeur non pris en charge : {} (attendu {FORMAT_VERSION})",
                document.format_version
            )
            .into());
        }
        Self::from_document_mode(document, display_cycles)
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Applies an edit command after validation. The history keeps
    /// the exact inverse; redo is lost as soon as a new command is
    /// applied. Commands targeting a node of a subgraph container are
    /// refused (opaque box), except moving the whole box.
    pub fn apply(&mut self, command: Command) -> Result<()> {
        let inverse = self.apply_one(&command)?;
        self.undo.push(HistoryEntry {
            forward: command,
            inverse,
        });
        if self.undo.len() > MAX_HISTORY {
            self.undo.remove(0);
        }
        self.redo.clear();
        Ok(())
    }

    pub fn undo(&mut self) -> Result<bool> {
        let Some(entry) = self.undo.pop() else {
            return Ok(false);
        };
        for command in &entry.inverse {
            self.restore(command)?;
        }
        self.redo.push(entry);
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool> {
        let Some(entry) = self.redo.pop() else {
            return Ok(false);
        };
        self.apply_one(&entry.forward)?;
        self.undo.push(entry);
        if self.undo.len() > MAX_HISTORY {
            self.undo.remove(0);
        }
        Ok(true)
    }

    /// Applies the inverse of a command during an undo. The inverses
    /// restore an already validated earlier state: for the sole move
    /// of a member node of a container (moved box), the opacity
    /// check is bypassed because the exact earlier position must be
    /// restored without touching the structure.
    fn restore(&mut self, command: &Command) -> Result<()> {
        if matches!(
            command,
            Command::Connect { .. } | Command::Disconnect { .. }
        ) {
            self.apply_one_opts(command, true)?;
            return Ok(());
        }
        if let Command::MoveNode { node, position } = command {
            if self.frozen().contains(node.as_str()) {
                if let Some(point) = position {
                    if !finite_point(*point) {
                        return Err("Position invalide".into());
                    }
                }
                self.document
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == *node)
                    .ok_or_else(|| format!("Node inconnu : {node}"))?
                    .position = *position;
                return Ok(());
            }
        }
        self.apply_one(command)?;
        Ok(())
    }

    fn node(&self, id: &str) -> Result<&EditorNode> {
        self.document
            .nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or_else(|| format!("Node inconnu : {id}").into())
    }

    fn frozen(&self) -> BTreeSet<&str> {
        self.document
            .subgraphs
            .iter()
            .flat_map(|c| c.node_ids.iter())
            .map(String::as_str)
            .collect()
    }

    fn assert_editable(&self, id: &str) -> Result<()> {
        if self.frozen().contains(id) {
            return Err(format!("Node dans un conteneur subgraph, non modifiable : {id}").into());
        }
        Ok(())
    }

    fn group_of(&self, node: &str) -> Option<&EditorGroup> {
        self.document
            .groups
            .iter()
            .find(|g| g.node_ids.iter().any(|n| n == node))
    }

    fn container_of(&self, node: &str) -> Option<&SubgraphContainer> {
        self.document
            .subgraphs
            .iter()
            .find(|c| c.node_ids.iter().any(|n| n == node))
    }

    fn id_taken(&self, id: &str) -> bool {
        self.document.nodes.iter().any(|n| n.id == id)
            || self.document.connections.iter().any(|c| c.id == id)
            || self.document.groups.iter().any(|g| g.id == id)
            || self.document.subgraphs.iter().any(|s| s.id == id)
    }

    /// Can the origin reach the target by following the connections?
    fn reaches(&self, start: &str, target: &str) -> bool {
        let mut seen = BTreeSet::new();
        let mut queue = vec![start];
        while let Some(current) = queue.pop() {
            if !seen.insert(current) {
                continue;
            }
            if current == target {
                return true;
            }
            for connection in &self.document.connections {
                if connection.from.node == current {
                    queue.push(connection.to.node.as_str());
                }
            }
        }
        false
    }

    /// Complete structural validation of the displayed document. Does not depend on the
    /// ComfyUI schema: the port types come from the document itself.
    pub fn validate(&self) -> Result<()> {
        if self.document.name.trim().is_empty() || self.document.name.len() > MAX_NAME {
            return Err("Nom de document invalide".into());
        }
        if self.document.id.is_empty() {
            return Err("Identifiant de document absent".into());
        }
        if self.document.nodes.len() > MAX_NODES {
            return Err("Nombre de nodes supérieur à la limite".into());
        }
        if self.document.connections.len() > MAX_CONNECTIONS {
            return Err("Nombre de connexions supérieur à la limite".into());
        }
        let mut ids = BTreeSet::new();
        for node in &self.document.nodes {
            if node.id.is_empty() || !ids.insert(node.id.as_str()) {
                return Err(format!("Identifiant de node absent ou dupliqué : {}", node.id).into());
            }
            if node.class_type.is_empty() {
                return Err(format!("Classe absente : {}", node.id).into());
            }
            if node.position.is_some_and(|p| !finite_point(p))
                || node.size.is_some_and(|s| !finite_size(s))
            {
                return Err(format!("Géométrie de node invalide : {}", node.id).into());
            }
            node.ports_valid()?;
            node.params_valid()?;
        }
        let mut connection_ids = BTreeSet::new();
        let mut targets = BTreeSet::new();
        for connection in &self.document.connections {
            if connection.id.is_empty() || !connection_ids.insert(connection.id.as_str()) {
                return Err(format!(
                    "Identifiant de connexion absent ou dupliqué : {}",
                    connection.id
                )
                .into());
            }
            let from = self.node(&connection.from.node)?;
            let to = self.node(&connection.to.node)?;
            if connection.from.node == connection.to.node {
                return Err("Auto-connexion interdite".into());
            }
            let provided = from
                .output_port(connection.from.output)
                .ok_or_else(|| format!("Index de sortie invalide : {}", connection.id))?;
            let expected = to
                .input_port(&connection.to.input)
                .ok_or_else(|| format!("Entrée inconnue : {}.{}", to.id, connection.to.input))?;
            if !types_compatible(&expected.port_type, &provided.port_type) {
                return Err(format!(
                    "Types de lien incompatibles : {}.{} attend {}, reçu {}",
                    to.id, connection.to.input, expected.port_type, provided.port_type
                )
                .into());
            }
            if !targets.insert((connection.to.node.as_str(), connection.to.input.as_str())) {
                return Err(
                    format!("Entrée déjà connectée : {}.{}", to.id, connection.to.input).into(),
                );
            }
        }
        if !self.display_cycles && self.reaches_from_targets() {
            return Err("Cycle détecté dans le graphe d'édition".into());
        }
        let mut group_of_node = BTreeMap::new();
        for group in &self.document.groups {
            if group.id.is_empty() || !ids.insert(group.id.as_str()) {
                return Err(
                    format!("Identifiant de groupe absent ou dupliqué : {}", group.id).into(),
                );
            }
            if !valid_title(&group.title) || group.node_ids.is_empty() {
                return Err("Groupe invalide (titre trop long ou aucun node)".into());
            }
            if let Some(b) = group.bounds {
                if !b.x.is_finite()
                    || !b.y.is_finite()
                    || !b.width.is_finite()
                    || !b.height.is_finite()
                    || b.width <= 0.0
                    || b.height <= 0.0
                {
                    return Err("Géométrie de groupe invalide".into());
                }
            }
            for node in &group.node_ids {
                self.node(node)?;
                if group_of_node
                    .insert(node.as_str(), group.id.as_str())
                    .is_some()
                {
                    return Err(format!("Node dans deux groupes : {node}").into());
                }
            }
        }
        for container in &self.document.subgraphs {
            if container.id.is_empty() || !ids.insert(container.id.as_str()) {
                return Err(format!(
                    "Identifiant de conteneur absent ou dupliqué : {}",
                    container.id
                )
                .into());
            }
            if !valid_title(&container.title) || container.node_ids.is_empty() {
                return Err("Conteneur subgraph invalide".into());
            }
            let mut exposed = BTreeSet::new();
            for port in container.inputs.iter().chain(&container.outputs) {
                if port.name.is_empty() || port.port_type.is_empty() {
                    return Err("Port exposé sans nom ou sans type".into());
                }
                if !exposed.insert(port.name.as_str()) {
                    return Err(format!("Port exposé dupliqué : {}", port.name).into());
                }
            }
            for node in &container.node_ids {
                self.node(node)?;
                if group_of_node.contains_key(node.as_str()) {
                    return Err(format!("Node dans un groupe et un conteneur : {node}").into());
                }
            }
        }
        Ok(())
    }

    /// Global cycle detection (connections validated by `apply` already
    /// prevent it; this protects imported documents).
    fn reaches_from_targets(&self) -> bool {
        for connection in &self.document.connections {
            let source = connection.from.node.as_str();
            if self.reaches(&connection.to.node, source) {
                return true;
            }
        }
        false
    }

    fn apply_one(&mut self, command: &Command) -> Result<Vec<Command>> {
        self.apply_one_opts(command, false)
    }

    /// `restore` bypasses the opacity check for the Connect/Disconnect
    /// inverses: the undo of an accepted command must restore
    /// the exact earlier state, even when the removed link touched a
    /// frozen member through its external neighbour (G1 regression).
    fn apply_one_opts(&mut self, command: &Command, restore: bool) -> Result<Vec<Command>> {
        let inverse = self.apply_checked_opts(command, restore)?;
        self.validate()?;
        Ok(inverse)
    }

    fn apply_checked_opts(&mut self, command: &Command, restore: bool) -> Result<Vec<Command>> {
        match command {
            Command::AddNode { node } => {
                if self.document.nodes.len() >= MAX_NODES {
                    return Err("Nombre de nodes supérieur à la limite".into());
                }
                if node.id.is_empty() || self.id_taken(&node.id) {
                    return Err("Identifiant de node absent ou déjà pris".into());
                }
                if node.class_type.is_empty() {
                    return Err("Classe absente".into());
                }
                if node.position.is_some_and(|p| !finite_point(p))
                    || node.size.is_some_and(|s| !finite_size(s))
                {
                    return Err("Géométrie de node invalide".into());
                }
                node.ports_valid()?;
                node.params_valid()?;
                self.document.nodes.push(node.clone());
                Ok(vec![Command::RemoveNode {
                    node: node.id.clone(),
                }])
            }
            Command::RemoveNode { node } => {
                self.assert_editable(node)?;
                let index = self
                    .document
                    .nodes
                    .iter()
                    .position(|n| n.id == *node)
                    .ok_or_else(|| format!("Node inconnu : {node}"))?;
                let removed = self.document.nodes.remove(index);
                let mut inverse = vec![Command::AddNode { node: removed }];
                for connection in self
                    .document
                    .connections
                    .iter()
                    .filter(|c| c.from.node == *node || c.to.node == *node)
                    .cloned()
                    .collect::<Vec<_>>()
                {
                    self.document.connections.retain(|c| c.id != connection.id);
                    inverse.push(Command::Connect { connection });
                }
                if let Some((group_id, group_title)) = self
                    .document
                    .groups
                    .iter()
                    .find(|g| g.node_ids.iter().any(|n| n == node))
                    .map(|g| (g.id.clone(), g.title.clone()))
                {
                    let group = self
                        .document
                        .groups
                        .iter_mut()
                        .find(|g| g.id == group_id)
                        .unwrap();
                    group.node_ids.retain(|n| n != node);
                    if group.node_ids.is_empty() {
                        // An empty group disappears; the inverse recreates it with
                        // its single member.
                        self.document.groups.retain(|g| g.id != group_id);
                        inverse.push(Command::AddGroup {
                            group: EditorGroup {
                                id: group_id,
                                title: group_title,
                                node_ids: vec![node.clone()],
                                bounds: None,
                                color: None,
                            },
                        });
                    } else {
                        inverse.push(Command::AssignGroup {
                            group: group_id,
                            node: node.clone(),
                        });
                    }
                }
                Ok(inverse)
            }
            Command::MoveNode { node, position } => {
                self.assert_editable(node)?;
                let node_ref = self.node(node)?.clone();
                if let Some(point) = position {
                    if !finite_point(*point) {
                        return Err("Position invalide".into());
                    }
                }
                let previous = node_ref.position;
                self.document
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == *node)
                    .unwrap()
                    .position = *position;
                Ok(vec![Command::MoveNode {
                    node: node.clone(),
                    position: previous,
                }])
            }
            Command::ResizeNode { node, size } => {
                self.assert_editable(node)?;
                if let Some(size) = size {
                    if !finite_size(*size) {
                        return Err("Taille invalide".into());
                    }
                }
                let previous = self.node(node)?.size;
                self.document
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == *node)
                    .unwrap()
                    .size = *size;
                Ok(vec![Command::ResizeNode {
                    node: node.clone(),
                    size: previous,
                }])
            }
            Command::SetParam { node, input, value } => {
                // UI containers expose their own widgets; their inner nodes stay opaque.
                let exposed_widget = self.display_cycles
                    && self
                        .document
                        .subgraphs
                        .iter()
                        .any(|s| s.node_ids.len() == 1 && s.node_ids[0] == *node)
                    && self.node(node)?.params.contains_key(input);
                if !exposed_widget {
                    self.assert_editable(node)?;
                }
                if self.node(node)?.input_port(input).is_none() {
                    return Err(format!("Entrée inconnue : {node}.{input}").into());
                }
                let previous = self
                    .node(node)?
                    .params
                    .get(input)
                    .cloned()
                    .unwrap_or(Value::Null);
                let entry = self
                    .document
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == *node)
                    .unwrap();
                if value.is_null() {
                    entry.params.remove(input);
                } else {
                    if !valid_param_value(value) {
                        return Err("Valeur de paramètre interdite".into());
                    }
                    entry.params.insert(input.clone(), value.clone());
                }
                Ok(vec![Command::SetParam {
                    node: node.clone(),
                    input: input.clone(),
                    value: previous,
                }])
            }
            Command::Connect { connection } => {
                if connection.id.is_empty() || self.id_taken(&connection.id) {
                    return Err("Identifiant de connexion absent ou déjà pris".into());
                }
                if !restore {
                    self.assert_editable(&connection.from.node)?;
                    self.assert_editable(&connection.to.node)?;
                }
                if connection.from.node == connection.to.node {
                    return Err("Auto-connexion interdite".into());
                }
                let from = self.node(&connection.from.node)?.clone();
                let to = self.node(&connection.to.node)?.clone();
                let provided = from
                    .output_port(connection.from.output)
                    .ok_or("Index de sortie invalide")?;
                let expected = to.input_port(&connection.to.input).ok_or_else(|| {
                    format!("Entrée inconnue : {}.{}", to.id, connection.to.input)
                })?;
                if !types_compatible(&expected.port_type, &provided.port_type) {
                    return Err(format!(
                        "Types de lien incompatibles : attend {}, reçu {}",
                        expected.port_type, provided.port_type
                    )
                    .into());
                }
                if self
                    .document
                    .connections
                    .iter()
                    .any(|c| c.to.node == connection.to.node && c.to.input == connection.to.input)
                {
                    return Err(format!(
                        "Entrée déjà connectée : {}.{}",
                        to.id, connection.to.input
                    )
                    .into());
                }
                if self.reaches(&connection.to.node, &connection.from.node) {
                    return Err("Cycle détecté dans le graphe d'édition".into());
                }
                if self.document.connections.len() >= MAX_CONNECTIONS {
                    return Err("Nombre de connexions supérieur à la limite".into());
                }
                self.document.connections.push(connection.clone());
                Ok(vec![Command::Disconnect {
                    connection: connection.id.clone(),
                }])
            }
            Command::Disconnect { connection } => {
                let removed = self
                    .document
                    .connections
                    .iter()
                    .find(|c| c.id == *connection)
                    .cloned()
                    .ok_or_else(|| format!("Connexion inconnue : {connection}"))?;
                if !restore {
                    self.assert_editable(&removed.from.node)?;
                    self.assert_editable(&removed.to.node)?;
                }
                self.document.connections.retain(|c| c.id != *connection);
                Ok(vec![Command::Connect {
                    connection: removed,
                }])
            }
            Command::SetNodeStyle { node, title, color } => {
                if !valid_title(title) || color.as_deref().is_some_and(|c| !valid_color(c)) {
                    return Err("Style de node invalide".into());
                }
                let old = self.node(node)?.clone();
                let target = self
                    .document
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == *node)
                    .ok_or("Node absent")?;
                target.title = title.clone();
                target.color = color.clone();
                Ok(vec![Command::SetNodeStyle {
                    node: node.clone(),
                    title: old.title,
                    color: old.color,
                }])
            }
            Command::SetGroupStyle {
                group,
                title,
                color,
            } => {
                if !valid_title(title) || color.as_deref().is_some_and(|c| !valid_color(c)) {
                    return Err("Style de groupe invalide".into());
                }
                let old = self
                    .document
                    .groups
                    .iter()
                    .find(|g| g.id == *group)
                    .cloned()
                    .ok_or("Groupe absent")?;
                let target = self
                    .document
                    .groups
                    .iter_mut()
                    .find(|g| g.id == *group)
                    .ok_or("Groupe absent")?;
                target.title = title.clone();
                target.color = color.clone();
                Ok(vec![Command::SetGroupStyle {
                    group: group.clone(),
                    title: old.title,
                    color: old.color,
                }])
            }
            Command::AddGroup { group } => {
                if group.id.is_empty() || self.id_taken(&group.id) {
                    return Err("Identifiant de groupe absent ou déjà pris".into());
                }
                if !valid_title(&group.title)
                    || group.color.as_deref().is_some_and(|c| !valid_color(c))
                    || group.node_ids.is_empty()
                {
                    return Err("Groupe invalide (titre trop long ou aucun node)".into());
                }
                let mut group = group.clone();
                if group.bounds.is_none() {
                    group.bounds = self.compute_group_bounds(&group.node_ids);
                }
                if group.bounds.is_some_and(|b| {
                    !b.x.is_finite()
                        || !b.y.is_finite()
                        || !b.width.is_finite()
                        || !b.height.is_finite()
                        || b.width <= 0.0
                        || b.height <= 0.0
                }) {
                    return Err("Géométrie de groupe invalide".into());
                }
                for node in &group.node_ids {
                    self.node(node)?;
                    if self.group_of(node).is_some() || self.container_of(node).is_some() {
                        return Err(format!(
                            "Node déjà pris par un groupe ou un conteneur : {node}"
                        )
                        .into());
                    }
                }
                self.document.groups.push(group.clone());
                Ok(vec![Command::DissolveGroup {
                    group: group.id.clone(),
                }])
            }
            Command::DissolveGroup { group } => {
                let index = self
                    .document
                    .groups
                    .iter()
                    .position(|g| g.id == *group)
                    .ok_or_else(|| format!("Groupe inconnu : {group}"))?;
                let removed = self.document.groups.remove(index);
                Ok(vec![Command::AddGroup { group: removed }])
            }
            Command::AssignGroup { group, node } => {
                self.assert_editable(node)?;
                self.node(node)?;
                if self.group_of(node).is_some() || self.container_of(node).is_some() {
                    return Err(
                        format!("Node déjà pris par un groupe ou un conteneur : {node}").into(),
                    );
                }
                let target = self
                    .document
                    .groups
                    .iter_mut()
                    .find(|g| g.id == *group)
                    .ok_or_else(|| format!("Groupe inconnu : {group}"))?;
                target.node_ids.push(node.clone());
                Ok(vec![Command::UnassignGroup { node: node.clone() }])
            }
            Command::UnassignGroup { node } => {
                self.assert_editable(node)?;
                self.node(node)?;
                let Some((group_id, group_title)) = self
                    .document
                    .groups
                    .iter()
                    .find(|g| g.node_ids.iter().any(|n| n == node))
                    .map(|g| (g.id.clone(), g.title.clone()))
                else {
                    return Err(format!("Node hors groupe : {node}").into());
                };
                let group = self
                    .document
                    .groups
                    .iter_mut()
                    .find(|g| g.id == group_id)
                    .unwrap();
                group.node_ids.retain(|n| n != node);
                if group.node_ids.is_empty() {
                    self.document.groups.retain(|g| g.id != group_id);
                    Ok(vec![Command::AddGroup {
                        group: EditorGroup {
                            id: group_id,
                            title: group_title,
                            node_ids: vec![node.clone()],
                            bounds: None,
                            color: None,
                        },
                    }])
                } else {
                    Ok(vec![Command::AssignGroup {
                        group: group_id,
                        node: node.clone(),
                    }])
                }
            }
            Command::MoveGroup { group, delta } => {
                if !finite_point(*delta) {
                    return Err("Déplacement invalide".into());
                }
                let (members, previous) = {
                    let found = self
                        .document
                        .groups
                        .iter()
                        .find(|g| g.id == *group)
                        .ok_or_else(|| format!("Groupe inconnu : {group}"))?;
                    (found.node_ids.clone(), found.bounds)
                };
                let mut inverse = self.shift_nodes(&members, *delta);
                if let Some(previous) = previous {
                    let shifted = GroupBounds {
                        x: previous.x + delta.x,
                        y: previous.y + delta.y,
                        width: previous.width,
                        height: previous.height,
                    };
                    if let Some(found) = self.document.groups.iter_mut().find(|g| g.id == *group) {
                        found.bounds = Some(shifted);
                    }
                    inverse.push(Command::ResizeGroup {
                        group: group.clone(),
                        bounds: previous,
                    });
                }
                Ok(inverse)
            }
            Command::ResizeGroup { group, bounds } => {
                if !bounds.x.is_finite()
                    || !bounds.y.is_finite()
                    || !bounds.width.is_finite()
                    || !bounds.height.is_finite()
                    || bounds.width <= 0.0
                    || bounds.height <= 0.0
                {
                    return Err("Géométrie de groupe invalide".into());
                }
                let found = self
                    .document
                    .groups
                    .iter_mut()
                    .find(|g| g.id == *group)
                    .ok_or_else(|| format!("Groupe inconnu : {group}"))?;
                let previous = found.bounds.unwrap_or(GroupBounds {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                });
                found.bounds = Some(*bounds);
                Ok(vec![Command::ResizeGroup {
                    group: group.clone(),
                    bounds: previous,
                }])
            }
            Command::AddSubgraph { container } => {
                if container.id.is_empty() || self.id_taken(&container.id) {
                    return Err("Identifiant de conteneur absent ou déjà pris".into());
                }
                if !valid_title(&container.title) || container.node_ids.is_empty() {
                    return Err("Conteneur subgraph invalide".into());
                }
                let mut exposed = BTreeSet::new();
                for port in container.inputs.iter().chain(&container.outputs) {
                    if port.name.is_empty() || port.port_type.is_empty() {
                        return Err("Port exposé sans nom ou sans type".into());
                    }
                    if !exposed.insert(port.name.as_str()) {
                        return Err(format!("Port exposé dupliqué : {}", port.name).into());
                    }
                }
                for node in &container.node_ids {
                    self.node(node)?;
                    if self.group_of(node).is_some() || self.container_of(node).is_some() {
                        return Err(format!(
                            "Node déjà pris par un groupe ou un conteneur : {node}"
                        )
                        .into());
                    }
                }
                self.document.subgraphs.push(container.clone());
                Ok(vec![Command::RemoveSubgraph {
                    container: container.id.clone(),
                }])
            }
            Command::RemoveSubgraph { container } => {
                let index = self
                    .document
                    .subgraphs
                    .iter()
                    .position(|c| c.id == *container)
                    .ok_or_else(|| format!("Conteneur inconnu : {container}"))?;
                let removed = self.document.subgraphs.remove(index);
                Ok(vec![Command::AddSubgraph { container: removed }])
            }
            Command::MoveSubgraph { container, delta } => {
                if !finite_point(*delta) {
                    return Err("Déplacement invalide".into());
                }
                let members = self
                    .document
                    .subgraphs
                    .iter()
                    .find(|c| c.id == *container)
                    .ok_or_else(|| format!("Conteneur inconnu : {container}"))?
                    .node_ids
                    .clone();
                Ok(self.shift_nodes(&members, *delta))
            }
        }
    }

    /// Moves the members of a group or container. Nodes without a
    /// position are considered at the origin; the exact inverse is captured.
    fn compute_group_bounds(&self, members: &[NodeId]) -> Option<GroupBounds> {
        let mut x0 = f64::INFINITY;
        let mut y0 = f64::INFINITY;
        let mut x1 = f64::NEG_INFINITY;
        let mut y1 = f64::NEG_INFINITY;
        for member in members {
            let node = self.document.nodes.iter().find(|n| &n.id == member)?;
            let position = node.position?;
            let size = node.size.unwrap_or(Size {
                width: 230.0,
                height: 120.0,
            });
            x0 = x0.min(position.x);
            y0 = y0.min(position.y);
            x1 = x1.max(position.x + size.width);
            y1 = y1.max(position.y + size.height);
        }
        if !x0.is_finite() || !y0.is_finite() {
            return None;
        }
        Some(GroupBounds {
            x: x0 - 28.0,
            y: y0 - 48.0,
            width: (x1 - x0) + 56.0,
            height: (y1 - y0) + 76.0,
        })
    }

    fn shift_nodes(&mut self, members: &[NodeId], delta: Point) -> Vec<Command> {
        let mut inverse = Vec::new();
        for member in members {
            let node = self
                .document
                .nodes
                .iter_mut()
                .find(|n| &n.id == member)
                .unwrap();
            let previous = node.position;
            let base = previous.unwrap_or(Point { x: 0.0, y: 0.0 });
            node.position = Some(Point {
                x: base.x + delta.x,
                y: base.y + delta.y,
            });
            inverse.push(Command::MoveNode {
                node: member.clone(),
                position: previous,
            });
        }
        inverse
    }

    /// Required (non-optional) inputs without a connection or parameter:
    /// intermediate state allowed during editing, flagged to the interface.
    pub fn missing_required_inputs(&self) -> Vec<(NodeId, String)> {
        let connected: BTreeSet<(&str, &str)> = self
            .document
            .connections
            .iter()
            .map(|c| (c.to.node.as_str(), c.to.input.as_str()))
            .collect();
        self.document
            .nodes
            .iter()
            .flat_map(|node| {
                node.inputs.iter().filter_map(|port| {
                    let filled = connected.contains(&(node.id.as_str(), port.name.as_str()))
                        || node.params.contains_key(&port.name);
                    (!port.optional && !filled).then(|| (node.id.clone(), port.name.clone()))
                })
            })
            .collect()
    }

    /// Displayed graph → executed graph conversion (API format). Positions,
    /// sizes, groups and dormant parameters are ignored. Refused if any
    /// subgraph containers are present: no arbitrary conversion.
    pub fn to_executed(&self) -> Result<Value> {
        if self.reaches_from_targets() {
            return Err("Cycle détecté dans le graphe exécutable".into());
        }
        if !self.document.subgraphs.is_empty() {
            return Err(
                "Export API refusé : conteneurs subgraph présents, aucune conversion automatique"
                    .into(),
            );
        }
        if self.document.nodes.is_empty() {
            return Err("Graphe vide".into());
        }
        let mut out = Map::new();
        for node in &self.document.nodes {
            let mut inputs = Map::new();
            for connection in &self.document.connections {
                if connection.to.node == node.id {
                    inputs.insert(
                        connection.to.input.clone(),
                        json!([connection.from.node, connection.from.output]),
                    );
                }
            }
            for (key, value) in &node.params {
                // A parameter under a connected input is dormant (widget value
                // kept): the link remains the executed form.
                inputs.entry(key.clone()).or_insert_with(|| value.clone());
            }
            out.insert(
                node.id.clone(),
                json!({"class_type": node.class_type, "inputs": inputs}),
            );
        }
        Ok(Value::Object(out))
    }

    /// Executed graph (ComfyUI API) → displayed graph conversion. Ports
    /// come from the object_info schema (same contracts as `validate_graph`,
    /// LoraManager fields included); each node receives a new stable
    /// identifier; positions and sizes are absent. The imported document must
    /// be structurally complete: required inputs present, valid links,
    /// no class unknown to the supplied schema.
    pub fn from_executed(api: &Value, info: &Value, name: &str) -> Result<Self> {
        if name.trim().is_empty() || name.len() > MAX_NAME {
            return Err("Nom de document invalide".into());
        }
        let canonical = crate::workflow::api_graph(&api.to_string())?;
        let source = canonical.as_object().ok_or("Graphe invalide")?;
        if source.len() > MAX_NODES {
            return Err("Nombre de nodes supérieur à la limite".into());
        }
        let mut ids = BTreeMap::new();
        for original in source.keys() {
            ids.insert(original.clone(), new_id());
        }
        let mut nodes = Vec::new();
        for (original, node) in source {
            let class = node["class_type"].as_str().unwrap_or_default();
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
                    let kind = declaration
                        .get(0)
                        .ok_or_else(|| format!("Schéma inconnu : {original}.{key}"))?;
                    let port_type = if let Some(text) = kind.as_str() {
                        text.to_owned()
                    } else if kind.is_array() {
                        "COMBO".to_owned()
                    } else {
                        "*".to_owned()
                    };
                    inputs.push(InputPort {
                        name: key.clone(),
                        port_type,
                        optional: section == "optional",
                    });
                }
            }
            let mut outputs = Vec::new();
            let types = schema["output"].as_array().ok_or("Sorties inconnues")?;
            let names = schema["output_name"].as_array();
            for (index, kind) in types.iter().enumerate() {
                let port_type = kind.as_str().ok_or("Type de sortie inconnu")?.to_owned();
                let name = names
                    .and_then(|n| n.get(index))
                    .and_then(Value::as_str)
                    .unwrap_or(&port_type)
                    .to_owned();
                outputs.push(OutputPort { name, port_type });
            }
            let mut params = BTreeMap::new();
            for (key, value) in node["inputs"].as_object().ok_or("Inputs absents")? {
                if !inputs.iter().any(|p| &p.name == key) {
                    // Adapter fields (e.g. LoraManager) absent from the public
                    // schema: the existing explicit contract provides the type.
                    let Some(declaration) = crate::workflow::schema_input(class, schema, key)
                    else {
                        return Err(
                            format!("Entrée non déclarée par ComfyUI : {original}.{key}").into(),
                        );
                    };
                    let kind = declaration.get(0);
                    let port_type = if let Some(text) = kind.and_then(Value::as_str) {
                        text.to_owned()
                    } else if kind.is_some_and(Value::is_array) {
                        "COMBO".to_owned()
                    } else {
                        "*".to_owned()
                    };
                    inputs.push(InputPort {
                        name: key.clone(),
                        port_type,
                        optional: true,
                    });
                }
                if let Some(link) = value.as_array() {
                    if link.len() != 2 || !link[0].is_string() || !link[1].is_u64() {
                        return Err(format!(
                            "Lien invalide ou valeur composite non prise en charge : {original}.{key}"
                        )
                        .into());
                    }
                } else {
                    params.insert(key.clone(), value.clone());
                }
            }
            nodes.push(EditorNode {
                id: ids[original].clone(),
                class_type: class.to_owned(),
                title: String::new(),
                color: None,
                position: None,
                size: None,
                inputs,
                outputs,
                params,
            });
        }
        let mut connections = Vec::new();
        for (original, node) in source {
            for (key, value) in node["inputs"].as_object().into_iter().flatten() {
                let Some(link) = value.as_array() else {
                    continue;
                };
                let (Some(source_id), Some(output)) = (link[0].as_str(), link[1].as_u64()) else {
                    return Err(format!("Lien invalide : {original}.{key}").into());
                };
                let mapped = ids
                    .get(source_id)
                    .ok_or_else(|| format!("Lien vers un node absent : {original}.{key}"))?;
                connections.push(Connection {
                    id: new_id(),
                    from: LinkEnd {
                        node: mapped.clone(),
                        output: output as usize,
                    },
                    to: LinkEndInput {
                        node: ids[original].clone(),
                        input: key.clone(),
                    },
                });
            }
        }
        let document = EditorDocument {
            format_version: FORMAT_VERSION,
            id: new_id(),
            name: name.to_owned(),
            nodes,
            connections,
            groups: Vec::new(),
            subgraphs: Vec::new(),
        };
        let graph = Self::from_document(document)?;
        if let Some((node, port)) = graph.missing_required_inputs().into_iter().next() {
            return Err(format!("Entrée requise absente : {node}.{port}").into());
        }
        Ok(graph)
    }
}
