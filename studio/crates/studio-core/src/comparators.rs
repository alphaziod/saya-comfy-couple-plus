//! Generic sampler-step discovery and intermediate-image comparison data.
//! Samplers are ordered from the real API graph; review/preview outputs are
//! never treated as permission to run another phase.
use crate::{Database, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

fn links(node: &Value) -> Vec<String> {
    node["inputs"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(_, v)| v.as_array())
        .filter(|a| a.len() == 2)
        .filter_map(|a| a[0].as_str().map(str::to_owned))
        .collect()
}

fn upstream(graph: &Value, start: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![start.to_owned()];
    while let Some(id) = stack.pop() {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(node) = graph.get(&id) {
            stack.extend(links(node));
        }
    }
    seen
}

fn sampler_order(graph: &Value) -> Vec<String> {
    let mut order = Vec::new();
    let mut seen = BTreeSet::new();
    fn visit(id: &str, graph: &Value, seen: &mut BTreeSet<String>, order: &mut Vec<String>) {
        if !seen.insert(id.to_owned()) {
            return;
        }
        if let Some(node) = graph.get(id) {
            for input in links(node) {
                visit(&input, graph, seen, order);
            }
            let class = node["class_type"].as_str().unwrap_or_default();
            if class.contains("Sampler") && !class.contains("SamplerCustomAdvanced") {
                order.push(id.to_owned());
            }
        }
    }
    for id in graph.as_object().into_iter().flatten().map(|(id, _)| id) {
        visit(id, graph, &mut seen, &mut order);
    }
    order
}

fn first_sampler(graph: &Value, node: &str) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    let order = sampler_order(graph);
    let ancestors = upstream(graph, node);
    for (index, sampler) in order.iter().enumerate() {
        if ancestors.contains(sampler) && best.as_ref().is_none_or(|(i, _)| index > *i) {
            best = Some((index, sampler.clone()));
        }
    }
    best.map(|(_, id)| id)
}

fn resources(graph: &Value, sampler: &str) -> (Vec<String>, Vec<String>) {
    let ancestors = upstream(graph, sampler);
    let mut models = Vec::new();
    let mut loras = Vec::new();
    for id in ancestors {
        let Some(node) = graph.get(&id) else { continue };
        let class = node["class_type"].as_str().unwrap_or_default();
        for (key, value) in node["inputs"].as_object().into_iter().flatten() {
            if key == "ckpt_name" {
                if let Some(name) = value.as_str() {
                    models.push(name.to_owned());
                }
            }
            if class.to_ascii_lowercase().contains("lora") {
                if key == "lora_name" {
                    if let Some(name) = value.as_str() {
                        loras.push(name.to_owned());
                    }
                }
                if key == "text" {
                    if let Some(text) = value.as_str() {
                        for token in text.split("<lora:").skip(1) {
                            if let Some(name) = token.split(':').next() {
                                loras.push(name.to_owned());
                            }
                        }
                    }
                }
            }
        }
    }
    models.sort();
    models.dedup();
    loras.sort();
    loras.dedup();
    (models, loras)
}

impl Database {
    pub fn sampler_steps(&self, job: &str) -> Result<Value> {
        let (snapshot, state, outputs): (String, String, String) = self
            .connection
            .query_row(
                "SELECT request_snapshot_json,state,outputs_json FROM generation_jobs WHERE id=?",
                [job],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|_| "Job inconnu")?;
        let snapshot: Value = serde_json::from_str(&snapshot)?;
        let graph = snapshot.get("prompt").ok_or("Snapshot sans graphe API")?;
        let outputs: Value = serde_json::from_str(&outputs)?;
        let order = sampler_order(graph);
        let mut by_sampler: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let mut errors = Vec::new();
        let mut unassigned = Vec::new();
        for output in outputs.as_array().into_iter().flatten() {
            if !output["assetId"].is_string() {
                continue;
            }
            let node = output["nodeId"].as_str().unwrap_or_default();
            match first_sampler(graph, node) {
                Some(sampler) => by_sampler.entry(sampler).or_default().push(json!({
                    "assetId":output["assetId"], "nodeId":node, "outputIndex":output["outputIndex"],
                    "kind":output["kind"], "source":output["source"],
                })),
                None => unassigned.push(json!({"nodeId":node,"assetId":output["assetId"]})),
            }
        }
        let mut steps = Vec::new();
        for (index, sampler) in order.iter().enumerate() {
            let node = &graph[sampler];
            let inputs = &node["inputs"];
            let (models, loras) = resources(graph, sampler);
            let images = by_sampler.remove(sampler).unwrap_or_default();
            if images.is_empty() {
                errors.push(format!(
                    "Aucune image intermédiaire collectée pour le sampler {sampler}"
                ));
            }
            steps.push(json!({
                "index":index+1, "sampler":sampler, "classType":node["class_type"],
                "title":node.pointer("/_meta/title"),
                "parameters":{"steps":inputs["steps"],"cfg":inputs["cfg"],"denoise":inputs["denoise"],
                    "sampler":inputs["sampler_name"],"scheduler":inputs["scheduler"],"seed":inputs["seed"]},
                "models":models,"loras":loras,
                "images":images,
                "mode": if index == 0 {"single"} else {"compare"},
                "previous": if index == 0 {Value::Null} else {json!(order[index-1])},
                "status":state,
            }));
        }
        if !unassigned.is_empty() {
            errors.push("Sorties sans sampler identifié".into());
        }
        Ok(
            json!({"job":job,"state":state,"steps":steps,"errors":errors,
            "pending": state != "succeeded", "previewOnly": true}),
        )
    }
}
