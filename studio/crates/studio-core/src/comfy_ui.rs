//! Loss-aware ComfyUI document adapter. UI source stays authoritative.
use crate::{graph_editor::*, Result};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub struct UiSource {
    pub original: Value,
    pub diagnostics: Vec<String>,
    // Exact positional widgets, never inferred from alphabetically sorted object keys.
    widgets: BTreeMap<String, Vec<(String, usize)>>,
}
fn id(v: &Value) -> Result<String> {
    if let Some(n) = v.as_i64() {
        Ok(n.to_string())
    } else if let Some(s) = v.as_str().filter(|s| !s.is_empty()) {
        Ok(s.into())
    } else {
        Err("Identifiant ComfyUI absent ou invalide".into())
    }
}
fn port_type(v: &Value) -> String {
    v.as_str().unwrap_or("*").to_owned()
}
pub fn widget_order(schema: &Value) -> Result<Vec<(String, Value)>> {
    let mut out = Vec::new();
    for section in ["required", "optional"] {
        let Some(fields) = schema
            .pointer(&format!("/input/{section}"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        let order = schema
            .pointer(&format!("/input_order/{section}"))
            .and_then(Value::as_array)
            .ok_or("Ordre des widgets absent de object_info")?;
        let mut seen = BTreeSet::new();
        for key in order {
            let key = key.as_str().ok_or("Nom de widget invalide")?;
            if !seen.insert(key) {
                return Err("Ordre de widgets dupliqué".into());
            }
            let declaration = fields.get(key).ok_or("Ordre de widgets incohérent")?;
            let kind = &declaration[0];
            if (kind.is_array()
                || matches!(
                    kind.as_str(),
                    Some("INT" | "FLOAT" | "STRING" | "BOOLEAN" | "COMBO")
                ))
                && declaration[1]["forceInput"] != true
            {
                out.push((key.into(), declaration.clone()));
            }
        }
        if seen.len() != fields.len() {
            return Err("Ordre de widgets incomplet".into());
        }
    }
    Ok(out)
}
fn widget_mapping(n: &Value, schema: &Value) -> Result<Vec<(String, usize)>> {
    let values = n["widgets_values"].as_array();
    let count = values.map_or(0, Vec::len);
    if n["type"] == "Seed (rgthree)" && count == 4 && n["widgets_values"][0].is_number() {
        return Ok(vec![("seed".into(), 0)]);
    }
    if n["type"] == "Lora Loader (LoraManager)"
        && count == 3
        && n["widgets_values"][0]["textWidgetName"] == "text"
        && n["widgets_values"][1].is_string()
        && crate::workflow::lora_manager::valid_loras(&json!({"__value__": n["widgets_values"][2]}))
    {
        return Ok(vec![("text".into(), 1), ("loras".into(), 2)]);
    }
    let order = widget_order(schema)?;
    let mut mapping = Vec::new();
    let mut index = 0;
    for (name, declaration) in order {
        if index >= count {
            return Err(format!("Widget absent : {name}").into());
        }
        if let Some(named) = n["widgets_values_named"].get(&name) {
            if Some(named) != values.and_then(|v| v.get(index)) {
                return Err(format!("Widget nommé/positionnel contradictoire : {name}").into());
            }
        }
        // Some frontends add seed controls without the object_info flag. Accept
        // only an explicitly named control that agrees with the adjacent slot.
        let named_control = matches!(name.as_str(), "seed" | "noise_seed")
            && n["widgets_values_named"]["control_after_generate"].is_string()
            && n["widgets_values_named"]["control_after_generate"]
                == n["widgets_values"][index + 1];
        mapping.push((name, index));
        index += 1;
        if declaration[1]["control_after_generate"] == true || named_control {
            let control = values.and_then(|v| v.get(index)).and_then(Value::as_str);
            if !matches!(
                control,
                Some("fixed" | "increment" | "decrement" | "randomize")
            ) {
                return Err("Contrôle après génération non reconnu".into());
            }
            index += 1;
        }
    }
    if index != count {
        return Err(format!(
            "{} widget(s) dynamique(s) sans correspondance",
            count.saturating_sub(index)
        )
        .into());
    }
    Ok(mapping)
}
fn xy(v: &Value) -> Option<Point> {
    Some(Point {
        x: v.get(0).or_else(|| v.get("0"))?.as_f64()?,
        y: v.get(1).or_else(|| v.get("1"))?.as_f64()?,
    })
}
impl UiSource {
    pub fn open(source: Value, info: &Value, name: &str) -> Result<(Self, EditorGraph)> {
        let raw = source["nodes"].as_array().ok_or("Liste UI nodes absente")?;
        if raw.len() > MAX_NODES {
            return Err("Trop de nodes UI".into());
        }
        let mut diagnostics = Vec::new();
        if source["version"] != json!(0.4) && source["version"] != json!(1) {
            diagnostics.push(format!("Version UI non qualifiée : {}", source["version"]));
        }
        let mut nodes = Vec::new();
        let mut widgets = BTreeMap::new();
        let definitions = source
            .pointer("/definitions/subgraphs")
            .and_then(Value::as_array);
        let mut subgraphs = Vec::new();
        for n in raw {
            let key = id(&n["id"])?;
            let class = n["type"].as_str().ok_or("Classe UI absente")?.to_owned();
            let mut inputs: Vec<InputPort> = n["inputs"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|p| {
                    Ok(InputPort {
                        name: p["name"].as_str().ok_or("Nom de port absent")?.into(),
                        port_type: port_type(&p["type"]),
                        optional: true,
                    })
                })
                .collect::<Result<_>>()?;
            let outputs = n["outputs"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|p| OutputPort {
                    name: p["name"].as_str().unwrap_or("sortie").into(),
                    port_type: port_type(&p["type"]),
                })
                .collect();
            let mut params = BTreeMap::new();
            if let Some(def) = definitions.and_then(|s| s.iter().find(|s| s["id"] == class)) {
                diagnostics.push(format!(
                    "#{key} {} : subgraph conservé, expansion et widgets promus non qualifiés",
                    def["name"].as_str().unwrap_or(&class)
                ));
                let mut mapping: Vec<(String, usize)> = Vec::new();
                let mut widget_index = 0usize;
                for port in def["inputs"].as_array().into_iter().flatten() {
                    let name = port["name"].as_str().unwrap_or("").to_owned();
                    let kind = port_type(&port["type"]);
                    if name.is_empty() {
                        continue;
                    }
                    // Typed boundary links without widgets occupy no positional value.
                    let has_widget = n["inputs"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|p| p["name"] == name && p["widget"]["name"] == name)
                        || n["widgets_values_named"].get(&name).is_some();
                    if !has_widget {
                        continue;
                    }
                    if matches!(
                        kind.as_str(),
                        "COMBO" | "STRING" | "INT" | "FLOAT" | "BOOLEAN"
                    ) {
                        let value = n["widgets_values"]
                            .get(widget_index)
                            .cloned()
                            .unwrap_or(Value::Null);
                        if !value.is_null() {
                            if !inputs.iter().any(|p| p.name == name) {
                                inputs.push(InputPort {
                                    name: name.clone(),
                                    port_type: kind.clone(),
                                    optional: true,
                                });
                            }
                            params.insert(name.clone(), value);
                            mapping.push((name, widget_index));
                        }
                        widget_index += 1;
                    }
                }
                if !mapping.is_empty() {
                    widgets.insert(key.clone(), mapping);
                }
                subgraphs.push(SubgraphContainer {
                    id: format!("sg-{key}"),
                    title: def["name"].as_str().unwrap_or(&class).into(),
                    node_ids: vec![key.clone()],
                    inputs: vec![],
                    outputs: vec![],
                });
            } else if let Some(schema) = info.get(&class) {
                match widget_mapping(n, schema) {
                    Ok(mapping) => {
                        for (name, index) in &mapping {
                            let mut value = n["widgets_values"][index].clone();
                            let lora_widget =
                                class == "Lora Loader (LoraManager)" && name == "loras";
                            if lora_widget {
                                value = json!({"__value__": value});
                            }
                            if !lora_widget
                                && !value.is_string()
                                && !value.is_number()
                                && !value.is_boolean()
                            {
                                diagnostics.push(format!("#{key}.{name} : widget composite conservé, conversion non qualifiée"));
                                continue;
                            }
                            if !inputs.iter().any(|p| &p.name == name) {
                                let decl = crate::workflow::schema_input(&class, schema, name)
                                    .ok_or("Widget inconnu")?;
                                inputs.push(InputPort {
                                    name: name.clone(),
                                    port_type: if decl[0].is_array() {
                                        "COMBO".into()
                                    } else {
                                        port_type(&decl[0])
                                    },
                                    optional: true,
                                });
                            }
                            params.insert(name.clone(), value);
                        }
                        for named in n["widgets_values_named"]
                            .as_object()
                            .into_iter()
                            .flat_map(|m| m.keys())
                        {
                            if !mapping.iter().any(|(key, _)| key == named) {
                                diagnostics.push(format!("#{key}.{named} : widget nommé supplémentaire conservé, sérialisation non qualifiée"));
                            }
                        }
                        widgets.insert(key.clone(), mapping);
                    }
                    Err(e) => diagnostics.push(format!("#{key} {class} : {e}")),
                }
            } else if !matches!(
                class.as_str(),
                "SetNode" | "GetNode" | "Reroute" | "Fast Groups Bypasser (rgthree)"
            ) {
                diagnostics.push(format!(
                    "#{key} {class} : classe virtuelle UI conservée (non exécutable seule)"
                ));
            }
            if n.get("mode").is_some_and(|m| m != 0) {
                diagnostics.push(format!(
                    "#{key} : mode {} conservé dans la source UI (mute/bypass)",
                    n["mode"]
                ));
            }
            nodes.push(EditorNode {
                id: key,
                class_type: class,
                title: n["title"].as_str().unwrap_or("").to_owned(),
                color: None,
                position: xy(&n["pos"]),
                size: None,
                inputs,
                outputs,
                params,
            });
        }
        let mut connections = Vec::new();
        for l in source["links"].as_array().ok_or("Liens UI absents")? {
            let (lid, from, output, to, input) = if l.is_array() {
                (
                    id(&l[0])?,
                    id(&l[1])?,
                    l[2].as_u64(),
                    id(&l[3])?,
                    l[4].as_u64(),
                )
            } else {
                (
                    id(&l["id"])?,
                    id(&l["origin_id"])?,
                    l["origin_slot"].as_u64(),
                    id(&l["target_id"])?,
                    l["target_slot"].as_u64(),
                )
            };
            let target = raw
                .iter()
                .find(|n| id(&n["id"]).ok().as_ref() == Some(&to))
                .ok_or("Cible de lien absente")?;
            let input_slot = input.ok_or("Slot entrant invalide")? as usize;
            let target_port = target["inputs"]
                .get(input_slot)
                .ok_or("Slot entrant hors limites")?;
            if id(&target_port["link"])? != lid {
                return Err(format!("Lien UI contradictoire : {lid}").into());
            }
            let input = target_port["name"]
                .as_str()
                .ok_or("Nom entrant absent")?
                .into();
            connections.push(Connection {
                id: format!("link-{lid}"),
                from: LinkEnd {
                    node: from,
                    output: output.ok_or("Slot sortant invalide")? as usize,
                },
                to: LinkEndInput { node: to, input },
            });
        }
        // Do not silently discard a linked input absent from the global links table.
        for n in raw {
            for p in n["inputs"].as_array().into_iter().flatten() {
                if !p["link"].is_null() {
                    let lid = format!("link-{}", id(&p["link"])?);
                    if !connections.iter().any(|c| c.id == lid) {
                        return Err(format!("Lien UI manquant : {lid}").into());
                    }
                }
            }
        }
        let mut assigned: BTreeSet<String> =
            subgraphs.iter().flat_map(|s| s.node_ids.clone()).collect();
        let mut groups = Vec::new();
        for (i, g) in source["groups"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let b = &g["bounding"];
            let (Some(x), Some(y), Some(w), Some(h)) =
                (b[0].as_f64(), b[1].as_f64(), b[2].as_f64(), b[3].as_f64())
            else {
                continue;
            };
            let members = nodes
                .iter()
                .filter(|n| {
                    n.position
                        .is_some_and(|p| p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h)
                })
                .filter_map(|n| assigned.insert(n.id.clone()).then_some(n.id.clone()))
                .collect();
            groups.push(EditorGroup {
                id: format!("group-{i}"),
                title: g["title"].as_str().unwrap_or("Groupe").into(),
                node_ids: members,
                bounds: Some(crate::graph_editor::GroupBounds {
                    x,
                    y,
                    width: w,
                    height: h,
                }),
                color: None,
            });
        }
        let graph = EditorGraph::from_ui_document(EditorDocument {
            format_version: FORMAT_VERSION,
            id: new_id(),
            name: name.into(),
            nodes,
            connections,
            groups,
            subgraphs,
        })?;
        Ok((
            Self {
                original: source,
                diagnostics,
                widgets,
            },
            graph,
        ))
    }
    pub fn original(&self) -> &Value {
        &self.original
    }
    pub fn can_edit_widget(&self, node: &str, input: &str) -> bool {
        self.widgets
            .get(node)
            .is_some_and(|m| m.iter().any(|(key, _)| key == input))
    }
    pub fn edited(&self, graph: &EditorGraph) -> Result<Value> {
        let mut out = self.original.clone();
        for raw in out["nodes"].as_array_mut().ok_or("Nodes absents")? {
            let key = id(&raw["id"])?;
            let n = graph
                .document()
                .nodes
                .iter()
                .find(|n| n.id == key)
                .ok_or("Structure UI modifiée")?;
            if let Some(p) = n.position {
                if xy(&raw["pos"]) != Some(p) {
                    raw["pos"] = json!([p.x, p.y]);
                }
            }
            if let Some(mapping) = self.widgets.get(&key) {
                for (key, index) in mapping {
                    if let Some(value) = n.params.get(key) {
                        let value = if raw["type"] == "Lora Loader (LoraManager)" && key == "loras"
                        {
                            value.get("__value__").ok_or("Liste LoRA invalide")?
                        } else {
                            value
                        };
                        raw["widgets_values"][*index] = value.clone();
                        if raw["widgets_values_named"].get(key).is_some() {
                            raw["widgets_values_named"][key] = value.clone();
                        }
                    }
                }
            }
        }
        Ok(out)
    }
    pub fn api(&self, graph: &EditorGraph, info: &Value) -> Result<Value> {
        if !self.diagnostics.is_empty() {
            return Err(self.diagnostics.join("\n").into());
        }
        let ui = self.edited(graph)?;
        let mut api = Map::new();
        for node in &graph.document().nodes {
            let mut inputs: Map<String, Value> = node.params.clone().into_iter().collect();
            for c in graph
                .document()
                .connections
                .iter()
                .filter(|c| c.to.node == node.id)
            {
                inputs.insert(c.to.input.clone(), json!([c.from.node, c.from.output]));
            }
            let raw = ui["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| id(&n["id"]).ok().as_ref() == Some(&node.id))
                .unwrap();
            api.insert(node.id.clone(),json!({"class_type":node.class_type,"inputs":inputs,"_meta":{"title":raw["title"].as_str().unwrap_or(&node.class_type)}}));
        }
        let api = Value::Object(api);
        crate::workflow::validate_graph(&api, info)?;
        Ok(api)
    }
}
/// Reconstructed layout is deterministic; absent visual metadata cannot be recovered.
pub fn from_api(
    api: &Value,
    document: &EditorDocument,
    original_ids: &std::collections::HashMap<String, String>,
    info: &Value,
) -> Result<Value> {
    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let mut names = BTreeMap::new();
    // Comfy UI numeric ids are separate from API ids (which may be arbitrary strings).
    let ids: BTreeMap<String, usize> = api
        .as_object()
        .ok_or("API invalide")?
        .keys()
        .enumerate()
        .map(|(i, id)| (id.clone(), i + 1))
        .collect();
    for (key, node) in api.as_object().unwrap() {
        let class = node["class_type"].as_str().ok_or("Classe absente")?;
        let schema = &info[class];
        let order = widget_order(schema)?;
        let mut values = Vec::new();
        let mut named = Map::new();
        for (name, decl) in order {
            let value = node["inputs"]
                .get(&name)
                .filter(|v| !v.is_array())
                .cloned()
                .or_else(|| decl[1].get("default").cloned())
                .or_else(|| decl[0].as_array().and_then(|a| a.first()).cloned())
                .ok_or_else(|| format!("Valeur UI manquante : {key}.{name}"))?;
            named.insert(name.clone(), value.clone());
            values.push(value);
            if decl[1]["control_after_generate"] == true {
                values.push(json!("fixed"));
            }
        }
        let mut ports = Vec::new();
        for (name, value) in node["inputs"].as_object().ok_or("Inputs absents")? {
            if !value.is_array() {
                continue;
            }
            let decl = crate::workflow::schema_input(class, schema, name).ok_or("Port inconnu")?;
            let kind = if decl[0].is_array() {
                "COMBO".into()
            } else {
                port_type(&decl[0])
            };
            let slot = ports.len();
            names.insert((key.clone(), name.clone()), slot);
            ports.push(json!({"name":name,"type":kind,"link":null}));
        }
        let outputs:Vec<Value> = schema["output"].as_array().ok_or("Sorties absentes")?.iter().enumerate().map(|(i,t)|json!({"name":schema["output_name"].get(i).and_then(Value::as_str).unwrap_or(t.as_str().unwrap_or("*")),"type":t,"links":[]})).collect();
        let display = document
            .nodes
            .iter()
            .find(|n| original_ids.get(&n.id).unwrap_or(&n.id) == key);
        let p = display.and_then(|n| n.position).unwrap_or(Point {
            x: ((ids[key] - 1) % 5) as f64 * 340.0,
            y: ((ids[key] - 1) / 5) as f64 * 300.0,
        });
        nodes.push(json!({"id":ids[key],"type":class,"pos":[p.x,p.y],"size":[280,160],"flags":{},"order":ids[key]-1,"mode":0,"inputs":ports,"outputs":outputs,"properties":{"Node name for S&R":class},"widgets_values":values,"widgets_values_named":named,"title":node["_meta"]["title"].as_str().unwrap_or(class)}));
    }
    for (key, node) in api.as_object().unwrap() {
        for (name, value) in node["inputs"].as_object().unwrap() {
            if let Some(link) = value.as_array() {
                let from = link[0].as_str().ok_or("Lien API invalide")?;
                let output = link[1].as_u64().ok_or("Slot API invalide")? as usize;
                let slot = names[&(key.clone(), name.clone())];
                let lid = links.len() + 1;
                let kind = nodes[ids[key] - 1]["inputs"][slot]["type"].clone();
                links.push(json!([lid, ids[from], output, ids[key], slot, kind]));
                nodes[ids[key] - 1]["inputs"][slot]["link"] = json!(lid);
                nodes[ids[from] - 1]["outputs"][output]["links"]
                    .as_array_mut()
                    .ok_or("Sortie inconnue")?
                    .push(json!(lid));
            }
        }
    }
    Ok(
        json!({"version":0.4,"last_node_id":nodes.len(),"last_link_id":links.len(),"nodes":nodes,"links":links,"groups":[],"config":{},"extra":{"sayaStudio":{"originalApi":api,"note":"Positions reconstruites ; métadonnées visuelles absentes du format API."}}}),
    )
}
