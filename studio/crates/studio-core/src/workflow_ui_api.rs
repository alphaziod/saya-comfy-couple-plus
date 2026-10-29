//! Faithful conversion of a ComfyUI UI workflow (format 0.4/1) to the API prompt.
//!
//! This module reproduces the semantics of the ComfyUI 1.51.10 frontend:
//! `graphToPrompt` (executionUtil.ts), `ExecutableNodeDTO` (subgraph
//! expansion, mute/bypass, per-path execution IDs) and `SubgraphNode`
//! (values promoted by name). No value is invented: an unresolved
//! element is reported and excluded from the partial graph, which stays marked incomplete.
//!
//! Hidden widgets known to the frontend and the installed extensions
//! (`control_after_generate`, rgthree buttons) are never serialized.

use crate::{gallery::digest, Database, Result};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const MODE_NEVER: i64 = 2;
const MODE_BYPASS: i64 = 4;

/// Widgets added by the frontend or extensions with `serialize: false`.
/// They appear in `widgets_values_named` but never in the API prompt.
const HIDDEN_WIDGETS: &[&str] = &[
    "control_after_generate",
    "control_filter_list",
    "🎲 New Fixed Random",
    "🎲 Randomize Each Time",
    "USE_LAST_SEED",
];

/// Purely frontend nodes, with no executable output.
const VIRTUAL_NO_OUTPUT: &[&str] = &["Fast Groups Bypasser (rgthree)"];

#[derive(Clone, Debug)]
struct Link {
    id: u64,
    origin_id: String,
    origin_slot: usize,
    target_id: String,
    target_slot: usize,
}

#[derive(Clone, Debug)]
struct InstanceRef {
    node_id: String,
    parent: usize,
}

struct Scope {
    nodes: BTreeMap<String, Value>,
    order: Vec<String>,
    links: BTreeMap<u64, Link>,
    path: Vec<String>,
    instance: Option<InstanceRef>,
    parent: Option<usize>,
    children: BTreeMap<String, usize>,
}

#[derive(Clone, Debug)]
enum Resolved {
    Link {
        node: String,
        slot: usize,
    },
    Value(Value),
    /// The link leads to a promoted widget without a value: the frontend serializes
    /// `undefined`, which removes the input from the JSON prompt.
    UndefinedWidget,
}

struct Converter {
    scopes: Vec<Scope>,
    definitions: BTreeMap<String, Value>,
    object_info: Option<Value>,
}

fn node_key(v: &Value) -> String {
    match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn as_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .or_else(|| v.as_i64().and_then(|n| u64::try_from(n).ok()))
}

fn as_usize(v: &Value) -> Option<usize> {
    as_u64(v).and_then(|n| usize::try_from(n).ok())
}

fn parse_link(v: &Value) -> Option<Link> {
    if let Some(a) = v.as_array() {
        if a.len() < 5 {
            return None;
        }
        return Some(Link {
            id: as_u64(a.first()?)?,
            origin_id: node_key(a.get(1)?),
            origin_slot: as_usize(a.get(2)?)?,
            target_id: node_key(a.get(3)?),
            target_slot: as_usize(a.get(4)?)?,
        });
    }
    let o = v.as_object()?;
    Some(Link {
        id: as_u64(o.get("id")?)?,
        origin_id: node_key(o.get("origin_id")?),
        origin_slot: as_usize(o.get("origin_slot")?)?,
        target_id: node_key(o.get("target_id")?),
        target_slot: as_usize(o.get("target_slot")?)?,
    })
}

fn mode(node: &Value) -> i64 {
    node.get("mode").and_then(Value::as_i64).unwrap_or(0)
}

fn name_of(value: &Value, fallback: &str) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| fallback.to_owned())
}

fn input_type(input: &Value) -> String {
    name_of(input.get("type").unwrap_or(&Value::Null), "")
}

fn wrap_widget(value: &Value) -> Value {
    if value.is_array() {
        json!({ "__value__": value })
    } else {
        value.clone()
    }
}

fn type_matches(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() || a == "*" || b == "*" || a == b {
        return true;
    }
    a.split(',').any(|t| t == b) || b.split(',').any(|t| t == a)
}

impl Converter {
    fn node(&self, scope: usize, id: &str) -> Option<&Value> {
        self.scopes[scope].nodes.get(id)
    }

    fn link(&self, scope: usize, id: u64) -> Option<&Link> {
        self.scopes[scope].links.get(&id)
    }

    fn exec_id(&self, scope: usize, node: &str) -> String {
        let mut parts = self.scopes[scope].path.clone();
        parts.push(node.to_owned());
        parts.join(":")
    }

    fn named(&self, scope: usize, node: &str, name: &str) -> Option<Value> {
        self.node(scope, node)?
            .get("widgets_values_named")?
            .as_object()?
            .get(name)
            .cloned()
    }

    fn constant(&self, scope: usize, node: &str) -> String {
        let Some(value) = self.node(scope, node) else {
            return String::new();
        };
        if let Some(v) = value
            .get("widgets_values_named")
            .and_then(|n| n.get("Constant"))
        {
            return name_of(v, "");
        }
        value
            .get("widgets_values")
            .and_then(Value::as_array)
            .and_then(|a| a.first())
            .map(|v| name_of(v, ""))
            .unwrap_or_default()
    }

    /// Lexical lookup of the SetNode: current graph then parent graphs.
    fn find_setter(
        &self,
        mut scope: usize,
        name: &str,
    ) -> std::result::Result<(usize, String), String> {
        let mut found: Vec<(usize, String)> = Vec::new();
        loop {
            for id in &self.scopes[scope].order {
                let Some(node) = self.node(scope, id) else {
                    continue;
                };
                if node.get("type").and_then(Value::as_str) == Some("SetNode")
                    && self.constant(scope, id) == name
                {
                    found.push((scope, id.clone()));
                }
            }
            if !found.is_empty() {
                break;
            }
            match self.scopes[scope].parent {
                Some(parent) => scope = parent,
                None => break,
            }
        }
        match found.len() {
            0 => Err(format!(
                "GetNode : aucun SetNode pour « {name} » dans la portée"
            )),
            1 => Ok(found.remove(0)),
            n => Err(format!(
                "GetNode : {n} SetNodes nommés « {name} » dans la même portée"
            )),
        }
    }

    fn bypass_index(node: &Value, slot: usize, target: &str) -> i64 {
        let inputs = node
            .get("inputs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let output_type = node
            .get("outputs")
            .and_then(Value::as_array)
            .and_then(|o| o.get(slot))
            .map(|o| name_of(o.get("type").unwrap_or(&Value::Null), ""))
            .unwrap_or_default();
        if target == "*" || target.is_empty() {
            return if inputs.len() > slot { slot as i64 } else { 0 };
        }
        if let Some(input) = inputs.get(slot) {
            let t = input_type(input);
            if type_matches(&t, &output_type) && type_matches(&t, target) {
                return slot as i64;
            }
        }
        if let Some(index) = inputs.iter().position(|i| input_type(i) == target) {
            return index as i64;
        }
        inputs
            .iter()
            .position(|i| {
                let t = input_type(i);
                type_matches(&t, &output_type) && type_matches(&t, target)
            })
            .map(|i| i as i64)
            .unwrap_or(-1)
    }

    fn resolve_input(
        &self,
        scope: usize,
        node_id: &str,
        slot: usize,
        target_type: &str,
        visited: &mut BTreeSet<String>,
    ) -> std::result::Result<Option<Resolved>, String> {
        let key = format!("{scope}:{node_id}:{slot}:i");
        if !visited.insert(key.clone()) {
            return Err(format!(
                "référence circulaire pendant la résolution de {node_id}[{slot}]"
            ));
        }
        let result = self.resolve_input_inner(scope, node_id, slot, target_type, visited);
        visited.remove(&key);
        result
    }

    fn resolve_input_inner(
        &self,
        scope: usize,
        node_id: &str,
        slot: usize,
        target_type: &str,
        visited: &mut BTreeSet<String>,
    ) -> std::result::Result<Option<Resolved>, String> {
        let node = self
            .node(scope, node_id)
            .ok_or_else(|| format!("node {node_id} absent du graphe"))?;
        let input = node
            .get("inputs")
            .and_then(Value::as_array)
            .and_then(|a| a.get(slot))
            .ok_or_else(|| format!("entrée {slot} absente de {node_id}"))?;
        let link_id = input.get("link").and_then(as_u64);
        if link_id.is_none() {
            if let Some(widget) = input.get("widget") {
                let name = widget
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| input.get("name").and_then(Value::as_str).unwrap_or(""));
                if let Some(value) = self.named(scope, node_id, name) {
                    return Ok(Some(Resolved::Value(value)));
                }
                return Ok(Some(Resolved::UndefinedWidget));
            }
            return Ok(None);
        }
        let link = self
            .link(scope, link_id.unwrap())
            .ok_or_else(|| format!("lien {} absent dans la portée", link_id.unwrap()))?;
        let t = if target_type.is_empty() {
            input_type(input)
        } else {
            target_type.to_owned()
        };
        if self.scopes[scope].instance.is_some() && link.origin_id == "-10" {
            let instance = self.scopes[scope]
                .instance
                .clone()
                .ok_or("portée de subgraph sans instance")?;
            return self.resolve_input(
                instance.parent,
                &instance.node_id,
                link.origin_slot,
                &t,
                visited,
            );
        }
        let origin = self
            .node(scope, &link.origin_id)
            .ok_or_else(|| format!("origine {} absente de la portée", link.origin_id))?;
        let _ = origin;
        self.resolve_output(scope, &link.origin_id, link.origin_slot, &t, visited)
    }

    fn resolve_output(
        &self,
        scope: usize,
        node_id: &str,
        slot: usize,
        target_type: &str,
        visited: &mut BTreeSet<String>,
    ) -> std::result::Result<Option<Resolved>, String> {
        let key = format!("{scope}:{node_id}:{slot}:o");
        if !visited.insert(key.clone()) {
            return Err(format!(
                "référence circulaire pendant la résolution de {node_id}[{slot}]"
            ));
        }
        let result = self.resolve_output_inner(scope, node_id, slot, target_type, visited);
        visited.remove(&key);
        result
    }

    fn resolve_output_inner(
        &self,
        scope: usize,
        node_id: &str,
        slot: usize,
        target_type: &str,
        visited: &mut BTreeSet<String>,
    ) -> std::result::Result<Option<Resolved>, String> {
        let node = self
            .node(scope, node_id)
            .ok_or_else(|| format!("node {node_id} absent du graphe"))?;
        match mode(node) {
            MODE_NEVER => return Ok(None),
            MODE_BYPASS => {
                let index = Self::bypass_index(node, slot, target_type);
                if index < 0 {
                    return Err(format!(
                        "bypass {node_id}[{slot}] : aucune entrée de type « {target_type} »"
                    ));
                }
                return self.resolve_input(scope, node_id, index as usize, target_type, visited);
            }
            _ => {}
        }
        let class = node
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if self.definitions.contains_key(&class) {
            let definition = self
                .definitions
                .get(&class)
                .ok_or("définition de subgraph absente")?;
            let output = definition
                .get("outputs")
                .and_then(Value::as_array)
                .and_then(|o| o.get(slot))
                .ok_or_else(|| format!("sortie {slot} absente du subgraph {class}"))?;
            let ids: BTreeSet<u64> = output
                .get("linkIds")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(as_u64).collect())
                .unwrap_or_default();
            let inner = definition
                .get("links")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(parse_link)
                .find(|l| ids.contains(&l.id) && l.target_id == "-20" && l.target_slot == slot);
            let Some(inner) = inner else {
                return Ok(None);
            };
            let child = *self.scopes[scope]
                .children
                .get(node_id)
                .ok_or_else(|| format!("instance {node_id} non expansée"))?;
            return self.resolve_output(
                child,
                &inner.origin_id,
                inner.origin_slot,
                target_type,
                visited,
            );
        }
        match class.as_str() {
            "Reroute" | "SetNode" => self.resolve_input(scope, node_id, 0, target_type, visited),
            "GetNode" => {
                let name = self.constant(scope, node_id);
                if name.is_empty() {
                    return Ok(None);
                }
                match self.find_setter(scope, &name) {
                    Ok((setter_scope, setter)) => {
                        self.resolve_input(setter_scope, &setter, 0, target_type, visited)
                    }
                    Err(_) => Ok(None),
                }
            }
            _ if VIRTUAL_NO_OUTPUT.contains(&class.as_str()) => Ok(None),
            _ => Ok(Some(Resolved::Link {
                node: self.exec_id(scope, node_id),
                slot,
            })),
        }
    }

    fn widget_values(
        &self,
        scope: usize,
        node: &Value,
    ) -> (Map<String, Value>, usize, Option<String>) {
        let mut out = Map::new();
        let mut hidden = 0usize;
        if let Some(named) = node.get("widgets_values_named").and_then(Value::as_object) {
            for (name, value) in named {
                if HIDDEN_WIDGETS.contains(&name.as_str()) {
                    hidden += 1;
                    continue;
                }
                out.insert(name.clone(), wrap_widget(value));
            }
            return (out, hidden, None);
        }
        let names: Vec<String> = node
            .get("inputs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|i| i.get("widget").is_some())
            .map(|i| name_of(i.get("name").unwrap_or(&Value::Null), ""))
            .collect();
        let values = node
            .get("widgets_values")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        // Without named values, position is only reliable if the counts
        // match exactly (no hidden widget unrepresented).
        if values.len() == names.len() {
            for (name, value) in names.into_iter().zip(values) {
                out.insert(name, wrap_widget(&value));
            }
            return (out, hidden, None);
        }
        // Frontends older than widgets_values_named serialize a positional
        // array that includes hidden control slots. Reconstruct it only from
        // object_info's authoritative widget order; never guess without schema.
        if let Some(info) = self.object_info.as_ref() {
            if let Some(schema) = info.get(node.get("type").and_then(Value::as_str).unwrap_or("")) {
                if let Ok(order) = crate::comfy_ui::widget_order(schema) {
                    let mut idx = 0usize;
                    for (name, declaration) in order {
                        let control = declaration
                            .get(1)
                            .and_then(|d| d.get("control_after_generate"))
                            .and_then(Value::as_bool)
                            == Some(true);
                        if HIDDEN_WIDGETS.contains(&name.as_str()) {
                            if idx < values.len() {
                                idx += 1;
                            }
                            continue;
                        }
                        if idx < values.len() {
                            out.insert(name, wrap_widget(&values[idx]));
                            idx += 1;
                        }
                        if control && idx < values.len() {
                            idx += 1;
                        }
                    }
                    if !out.is_empty() {
                        return (out, hidden, None);
                    }
                }
            }
        }
        let _ = scope;
        (
            out,
            hidden,
            Some(format!(
                "widgets_values sans valeurs nommées : {} valeurs pour {} widgets",
                values.len(),
                names.len()
            )),
        )
    }

    fn required_inputs(&self, class: &str) -> BTreeMap<String, bool> {
        let mut out = BTreeMap::new();
        let Some(info) = self.object_info.as_ref() else {
            return out;
        };
        let Some(required) = info
            .get(class)
            .and_then(|c| c.get("input"))
            .and_then(|i| i.get("required"))
            .and_then(Value::as_object)
        else {
            return out;
        };
        for (name, declaration) in required {
            let force = declaration
                .as_array()
                .and_then(|a| a.get(1))
                .and_then(|o| o.get("forceInput"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            out.insert(name.clone(), force);
        }
        out
    }

    fn known_class(&self, class: &str) -> bool {
        self.object_info
            .as_ref()
            .map(|info| info.get(class).is_some())
            .unwrap_or(true)
    }
}

fn build_scope(
    graph: &Value,
    path: Vec<String>,
    instance: Option<InstanceRef>,
    parent: Option<usize>,
) -> Scope {
    let mut nodes = BTreeMap::new();
    let mut order = Vec::new();
    if let Some(array) = graph.get("nodes").and_then(Value::as_array) {
        for node in array {
            if let Some(id) = node.get("id") {
                let key = node_key(id);
                order.push(key.clone());
                nodes.insert(key, node.clone());
            }
        }
    }
    let mut links = BTreeMap::new();
    if let Some(array) = graph.get("links").and_then(Value::as_array) {
        for link in array {
            if let Some(link) = parse_link(link) {
                links.insert(link.id, link);
            }
        }
    }
    Scope {
        nodes,
        order,
        links,
        path,
        instance,
        parent,
        children: BTreeMap::new(),
    }
}

fn build_children(
    scopes: &mut Vec<Scope>,
    definitions: &BTreeMap<String, Value>,
    scope: usize,
    stack: &mut Vec<String>,
) -> Result<()> {
    let order = scopes[scope].order.clone();
    for node_id in order {
        let class = scopes[scope]
            .nodes
            .get(&node_id)
            .and_then(|n| n.get("type"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let Some(class) = class else {
            continue;
        };
        let Some(definition) = definitions.get(&class).cloned() else {
            continue;
        };
        if stack.iter().any(|s| s == &class) {
            return Err(format!("référence circulaire de subgraphs : {class}").into());
        }
        let mut path = scopes[scope].path.clone();
        path.push(node_id.clone());
        let child = build_scope(
            &definition,
            path,
            Some(InstanceRef {
                node_id: node_id.clone(),
                parent: scope,
            }),
            Some(scope),
        );
        let child_index = scopes.len();
        scopes.push(child);
        scopes[scope].children.insert(node_id, child_index);
        stack.push(class.to_owned());
        build_children(scopes, definitions, child_index, stack)?;
        stack.pop();
    }
    Ok(())
}

struct EmitState {
    api: Map<String, Value>,
    widgets: BTreeMap<String, Map<String, Value>>,
    unresolved: Vec<Value>,
    muted: usize,
    bypassed: usize,
    virtual_skipped: usize,
    hidden_widgets: usize,
    named_nodes: usize,
    promoted_resolved: usize,
    classes_unverified: BTreeSet<String>,
    known_widget_rules: usize,
}

impl EmitState {
    fn unresolved(&mut self, id: &str, class: &str, cause: impl Into<String>) {
        self.unresolved.push(json!({
            "executionId": id,
            "classType": class,
            "cause": cause.into(),
        }));
    }
}

/// Compatibility rules limited to the known frontend widgets, documented
/// in frontend 1.51.10 and the installed extensions.
fn apply_known_widget_rules(class: &str, inputs: &mut Map<String, Value>) -> bool {
    let mut applied = false;
    if class == "CustomCombo" {
        // customWidgets.ts: `index` = 0-based position of the effective choice among
        // the optionN widgets, recomputed at serialization.
        if inputs.contains_key("index") {
            if let Some(choice) = inputs.get("choice").cloned() {
                let choice = name_of(&choice, "");
                let mut options: Vec<(u32, String)> = inputs
                    .iter()
                    .filter_map(|(key, value)| {
                        let suffix = key.strip_prefix("option")?;
                        let index: u32 = suffix.parse().ok()?;
                        Some((index, name_of(value, "")))
                    })
                    .collect();
                options.sort_by_key(|(index, _)| *index);
                let position = options
                    .iter()
                    .position(|(_, value)| value == &choice)
                    .map(|i| i as i64)
                    .unwrap_or(-1);
                inputs.insert("index".to_owned(), json!(position));
                applied = true;
            }
        }
    }
    if class == "Lora Loader (LoraManager)" && inputs.contains_key("__lm_autocomplete_meta_text") {
        // Kept exactly like the official graphToPrompt payload.
        applied = true;
    }
    if class == "Image Comparer (rgthree)" {
        // image_comparer.js serializeValue: each image is copied without `img`.
        if let Some(images) = inputs
            .get_mut("rgthree_comparer")
            .and_then(|v| v.get_mut("images"))
            .and_then(Value::as_array_mut)
        {
            for image in images {
                if let Some(object) = image.as_object_mut() {
                    object.remove("img");
                }
            }
            // Kept exactly like the official graphToPrompt payload; the custom
            // comparer consumes it and validation treats it as an adapter field.
            applied = true;
        }
    }
    applied
}

fn emit_scope(converter: &Converter, scope: usize, is_root: bool, state: &mut EmitState) {
    for node_id in &converter.scopes[scope].order {
        let Some(node) = converter.node(scope, node_id) else {
            continue;
        };
        let class = node
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let node_mode = mode(node);
        if converter.definitions.contains_key(&class) {
            if is_root && matches!(node_mode, MODE_NEVER | MODE_BYPASS) {
                if node_mode == MODE_NEVER {
                    state.muted += 1;
                } else {
                    state.bypassed += 1;
                }
                continue;
            }
            if let Some(&child) = converter.scopes[scope].children.get(node_id) {
                emit_scope(converter, child, false, state);
            }
            continue;
        }
        if matches!(node_mode, MODE_NEVER | MODE_BYPASS) {
            if node_mode == MODE_NEVER {
                state.muted += 1;
            } else {
                state.bypassed += 1;
            }
            continue;
        }
        if class == "Reroute"
            || class == "SetNode"
            || class == "GetNode"
            || VIRTUAL_NO_OUTPUT.contains(&class.as_str())
        {
            state.virtual_skipped += 1;
            continue;
        }
        let execution_id = converter.exec_id(scope, node_id);
        if !converter.known_class(&class) {
            state.classes_unverified.insert(class.clone());
            state.unresolved(&execution_id, &class, "classe absente d'object_info");
            continue;
        }
        if node.get("widgets_values_named").is_some() {
            state.named_nodes += 1;
        }
        let (widgets, hidden, widget_mismatch) = converter.widget_values(scope, node);
        state.hidden_widgets += hidden;
        let mut inputs = widgets.clone();
        let required = converter.required_inputs(&class);
        let mut causes: Vec<String> = Vec::new();
        if let Some(cause) = widget_mismatch {
            causes.push(cause);
        }
        if let Some(array) = node.get("inputs").and_then(Value::as_array) {
            for (index, input) in array.iter().enumerate() {
                if input.get("link").and_then(as_u64).is_none() {
                    continue;
                }
                let name = name_of(input.get("name").unwrap_or(&Value::Null), "");
                match converter.resolve_input(scope, node_id, index, "", &mut BTreeSet::new()) {
                    Ok(Some(Resolved::Link { node, slot })) => {
                        inputs.insert(name, json!([node, slot]));
                    }
                    Ok(Some(Resolved::Value(value))) => {
                        inputs.insert(name, wrap_widget(&value));
                    }
                    Ok(Some(Resolved::UndefinedWidget)) => {
                        inputs.remove(&name);
                    }
                    Ok(None) => {
                        // An unresolved optional input is omitted like
                        // graphToPrompt does; only a required input blocks.
                        if !inputs.contains_key(&name) && required.contains_key(&name) {
                            causes.push(format!("entrée « {name} » non résolue"));
                        }
                    }
                    Err(error) => causes.push(format!("entrée « {name} » : {error}")),
                }
            }
        }
        if apply_known_widget_rules(&class, &mut inputs) {
            state.known_widget_rules += 1;
        }
        state.api.insert(
            execution_id.clone(),
            json!({
                "class_type": class,
                "inputs": inputs,
                "_meta": {"title": node.get("title").and_then(Value::as_str).unwrap_or("")},
            }),
        );
        state.widgets.insert(execution_id.clone(), widgets);
        for cause in causes {
            state.unresolved(&execution_id, &class, cause);
        }
    }
}

/// Converts a UI workflow to an API prompt with a qualification report.
///
/// The result always contains `api` (partial graph) and `report`.
/// `report.complete == false` means at least one element could not be
/// resolved faithfully; the caller must not present the graph as
/// executable.
pub fn convert_ui(source: &str, object_info: Option<&Value>) -> Result<Value> {
    let workflow: Value = serde_json::from_str(source)?;
    if workflow.get("nodes").and_then(Value::as_array).is_none() {
        return Err("workflow UI attendu : la clé `nodes` est absente".into());
    }
    let mut definitions = BTreeMap::new();
    if let Some(array) = workflow
        .pointer("/definitions/subgraphs")
        .and_then(Value::as_array)
    {
        for definition in array {
            if let Some(id) = definition.get("id").and_then(Value::as_str) {
                definitions.insert(id.to_owned(), definition.clone());
            }
        }
    }
    let root = build_scope(&workflow, Vec::new(), None, None);
    let mut scopes = vec![root];
    let mut stack = Vec::new();
    build_children(&mut scopes, &definitions, 0, &mut stack)?;

    let converter = Converter {
        scopes,
        definitions,
        object_info: object_info.cloned(),
    };

    let root_instances = converter.scopes[0]
        .order
        .iter()
        .filter(|id| {
            converter
                .node(0, id)
                .and_then(|n| n.get("type"))
                .and_then(Value::as_str)
                .map(|t| converter.definitions.contains_key(t))
                .unwrap_or(false)
        })
        .count();
    let total_instances: usize = converter
        .scopes
        .iter()
        .map(|s| {
            s.order
                .iter()
                .filter(|id| {
                    s.nodes
                        .get(*id)
                        .and_then(|n| n.get("type"))
                        .and_then(Value::as_str)
                        .map(|t| converter.definitions.contains_key(t))
                        .unwrap_or(false)
                })
                .count()
        })
        .sum();

    // Promoted values actually resolved: instance inputs carrying a
    // widget and a named value.
    let mut promoted_resolved = 0usize;
    for scope in &converter.scopes {
        let Some(instance) = &scope.instance else {
            continue;
        };
        let Some(parent) =
            converter.node(scope.instance.as_ref().unwrap().parent, &instance.node_id)
        else {
            continue;
        };
        let named = parent
            .get("widgets_values_named")
            .and_then(Value::as_object);
        if let Some(named) = named {
            if let Some(inputs) = parent.get("inputs").and_then(Value::as_array) {
                for input in inputs {
                    if let Some(widget) = input.get("widget").and_then(Value::as_object) {
                        if let Some(name) = widget.get("name").and_then(Value::as_str) {
                            if named.contains_key(name) {
                                promoted_resolved += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    let mut state = EmitState {
        api: Map::new(),
        widgets: BTreeMap::new(),
        unresolved: Vec::new(),
        muted: 0,
        bypassed: 0,
        virtual_skipped: 0,
        hidden_widgets: 0,
        named_nodes: 0,
        promoted_resolved,
        classes_unverified: BTreeSet::new(),
        known_widget_rules: 0,
    };
    emit_scope(&converter, 0, true, &mut state);

    // Links to nodes not emitted: the frontend removes them, then leaves the
    // widget value if it exists.
    let emitted: BTreeSet<String> = state.api.keys().cloned().collect();
    let mut final_inputs: Vec<(String, String, String, String)> = Vec::new();
    for (execution_id, node) in &mut state.api {
        let class = node
            .get("class_type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if let Some(inputs) = node.get_mut("inputs").and_then(Value::as_object_mut) {
            let mut dead = Vec::new();
            for (name, value) in inputs.iter() {
                if let Some(pair) = value.as_array() {
                    if pair.len() == 2 {
                        if let Some(origin) = pair.first().and_then(Value::as_str) {
                            if !emitted.contains(origin) {
                                dead.push(name.clone());
                            }
                        }
                    }
                }
            }
            for name in dead {
                inputs.remove(&name);
                if let Some(widget) = state.widgets.get(execution_id).and_then(|w| w.get(&name)) {
                    inputs.insert(name.clone(), widget.clone());
                } else {
                    final_inputs.push((
                        execution_id.clone(),
                        class.clone(),
                        name,
                        "lien vers un node non émis".into(),
                    ));
                }
            }
        }
    }
    for (execution_id, class, name, cause) in final_inputs {
        state.unresolved(
            &execution_id,
            &class,
            format!("entrée « {name} » : {cause}"),
        );
    }

    // Check of the schema's required inputs, if provided.
    let mut required_problems: Vec<(String, String, String)> = Vec::new();
    for (execution_id, node) in &state.api {
        let class = node
            .get("class_type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let inputs = node.get("inputs").and_then(Value::as_object);
        for (name, force) in converter.required_inputs(class) {
            let present = inputs.map(|i| i.contains_key(&name)).unwrap_or(false);
            if present {
                continue;
            }
            if force {
                required_problems.push((
                    execution_id.clone(),
                    class.to_owned(),
                    format!("entrée requise « {name} » absente (forceInput)"),
                ));
            } else if converter
                .object_info
                .as_ref()
                .and_then(|info| info.get(class))
                .and_then(|c| c.get("input"))
                .and_then(|i| i.get("required"))
                .and_then(|r| r.get(&name))
                .is_some()
            {
                required_problems.push((
                    execution_id.clone(),
                    class.to_owned(),
                    format!("entrée requise « {name} » absente"),
                ));
            }
        }
    }
    for (execution_id, class, cause) in required_problems {
        state.unresolved(&execution_id, &class, cause);
    }

    let complete = state.unresolved.is_empty();
    let api_nodes = state.api.len();
    let report = json!({
        "complete": complete,
        "validatedAgainstSchema": converter.object_info.is_some(),
        "apiNodes": api_nodes,
        "subgraphDefinitions": converter.definitions.len(),
        "topLevelInstances": root_instances,
        "nestedInstances": total_instances.saturating_sub(root_instances),
        "expandedScopes": converter.scopes.len() - 1,
        "mutedSkipped": state.muted,
        "bypassed": state.bypassed,
        "virtualSkipped": state.virtual_skipped,
        "hiddenWidgetsExcluded": state.hidden_widgets,
        "namedWidgetNodes": state.named_nodes,
        "knownWidgetRulesApplied": state.known_widget_rules,
        "promotedWidgetsResolved": state.promoted_resolved,
        "unverifiedClasses": state.classes_unverified,
        "unresolved": state.unresolved,
    });
    Ok(json!({"api": state.api, "report": report}))
}

/// Rebuild a phase-specific UI graph using the Saya Couple controller rules:
/// tagged nodes are active only for their phase (or marked shared), except the
/// phase-1 review gate. Original node files are not modified.
fn apply_phase_modes(scope: &mut Value, phase: u32) {
    fn collect_shared(scope: &Value, out: &mut std::collections::HashSet<String>) {
        if let Some(nodes) = scope.get("nodes").and_then(Value::as_array) {
            for node in nodes {
                if node["properties"]["saya_shared_base"].as_bool() == Some(true) {
                    if let Some(kind) = node["type"].as_str() {
                        out.insert(kind.to_owned());
                    }
                }
            }
        }
        if let Some(defs) = scope
            .pointer("/definitions/subgraphs")
            .and_then(Value::as_array)
        {
            for definition in defs {
                collect_shared(definition, out);
            }
        }
    }
    fn visit(
        scope: &mut Value,
        phase: u32,
        shared: &std::collections::HashSet<String>,
        forced: bool,
    ) {
        if let Some(nodes) = scope.get_mut("nodes").and_then(Value::as_array_mut) {
            for node in nodes {
                let props = node.get("properties").cloned().unwrap_or(Value::Null);
                let tagged = props.get("saya_phase").and_then(Value::as_u64).unwrap_or(0) as u32;
                if tagged == 0 {
                    continue;
                }
                let is_shared = forced
                    || props
                        .get("saya_shared_base")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                let original = if props.get("saya_original_mode").and_then(Value::as_u64) == Some(4)
                {
                    4
                } else {
                    0
                };
                let hidream_clip = phase == 3
                    && props
                        .get("saya_note")
                        .and_then(Value::as_str)
                        .is_some_and(|n| n.contains("HiDream"));
                let mode = if phase == 1 {
                    if tagged == 1 || is_shared {
                        original
                    } else {
                        2
                    }
                } else if phase == 3 {
                    if tagged == 3 || hidream_clip {
                        original
                    } else {
                        2
                    }
                } else if tagged == phase || is_shared {
                    original
                } else {
                    2
                };
                node["mode"] = json!(mode);
            }
        }
        if let Some(defs) = scope
            .pointer_mut("/definitions/subgraphs")
            .and_then(Value::as_array_mut)
        {
            for definition in defs {
                let force = definition["id"]
                    .as_str()
                    .is_some_and(|id| shared.contains(id));
                visit(definition, phase, shared, forced || force);
            }
        }
    }
    let mut shared = std::collections::HashSet::new();
    collect_shared(scope, &mut shared);
    visit(scope, phase, &shared, false);
}

fn prune_lazy_switches(api: &mut Value) {
    for node in api.as_object_mut().into_iter().flatten().map(|(_, n)| n) {
        if node["class_type"] != "LazySwitchKJ" {
            continue;
        }
        let Some(switch) = node["inputs"]["switch"].as_bool() else {
            continue;
        };
        let (selected, unused) = if switch {
            ("on_true", "on_false")
        } else {
            ("on_false", "on_true")
        };
        if let Some(value) = node["inputs"].get(selected).cloned() {
            node["inputs"][unused] = value;
        }
        if let Some(inputs) = node["inputs"].as_object_mut() {
            inputs.remove("switch");
        }
    }
}

/// Convert a UI workflow for exactly one Saya phase. Phase 1 keeps the custom
/// review gate; later phases use their own checkpoint loader/stop nodes.
pub fn convert_ui_phase(source: &str, object_info: Option<&Value>, phase: u32) -> Result<Value> {
    if !(1..=6).contains(&phase) {
        return Err("Phase Saya hors bornes".into());
    }
    let mut workflow: Value = serde_json::from_str(source)?;
    apply_phase_modes(&mut workflow, phase);
    let mut converted = convert_ui(&workflow.to_string(), object_info)?;
    prune_lazy_switches(&mut converted["api"]);
    converted["phase"] = json!(phase);
    Ok(converted)
}

/// Previews the conversion of a UI workflow to an API prompt.
///
/// The hash ties the confirmation to the exact state of the converted graph. No
/// write is done and the source is not modified.
pub fn prepare_api(source: &str, object_info: Option<&Value>) -> Result<Value> {
    let result = convert_ui(source, object_info)?;
    let api = result["api"].clone();
    let preview_hash = digest(api.to_string().as_bytes());
    let mut report = result["report"].clone();
    report["previewHash"] = json!(preview_hash);
    Ok(json!({
        "api": api,
        "report": report,
        "previewHash": preview_hash,
    }))
}

/// Creates a new API revision, without modifying the source UI workflow.
///
/// Strict refusal: explicit confirmation, complete conversion, schema
/// `object_info` actually verified and preview hash unchanged.
pub fn save_api_revision(
    db: &Database,
    source: &str,
    object_info: Option<&Value>,
    name: &str,
    expected_hash: &str,
    confirmed: bool,
) -> Result<Value> {
    if !confirmed {
        return Err("Confirmer explicitement la création de la révision API".into());
    }
    if name.trim().is_empty() || name.len() > 300 {
        return Err("Nom de workflow invalide".into());
    }
    let prepared = prepare_api(source, object_info)?;
    if prepared["report"]["complete"] != Value::Bool(true) {
        return Err("Conversion incomplète : création de la révision API refusée".into());
    }
    if prepared["report"]["validatedAgainstSchema"] != Value::Bool(true) {
        return Err("Schéma object_info non vérifié : création de la révision API refusée".into());
    }
    if prepared["previewHash"].as_str() != Some(expected_hash) {
        return Err("Conversion modifiée : prévisualiser à nouveau".into());
    }
    let api = prepared["api"].clone();
    let id = db.import_workflow(name, &api.to_string())?;
    Ok(json!({
        "workflowId": id,
        "nodeCount": api.as_object().map_or(0, |o| o.len()),
        "previewHash": prepared["previewHash"],
    }))
}
