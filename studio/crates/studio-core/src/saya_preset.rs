//! One editable working copy of the master and six split workflows.
//! The original per-phase topology is embedded so execution never depends on later disk edits.
use crate::Result;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};

pub fn assemble(source: &Value, dir: &Path) -> Result<Value> {
    if source["extra"].get("saya_preset").is_some() || source["extra"].get("saya_split").is_none() {
        return Ok(source.clone());
    }
    let master: Value = serde_json::from_slice(&std::fs::read(
        dir.join("ILUSTCOUPLE_00_MAITRE_CONFIG.json"),
    )?)?;
    let entries = std::fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
    let mut phases = Vec::new();
    for phase in 1..=6 {
        let prefix = format!("ILUSTCOUPLE_{phase:02}_PASSE_{phase}_");
        let paths: Vec<_> = entries
            .iter()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with(&prefix) && s.ends_with(".json"))
            })
            .collect();
        if paths.len() != 1 {
            return Err(format!(
                "Preset : une seule passe {phase} requise dans {}",
                dir.display()
            )
            .into());
        }
        let pass: Value = serde_json::from_slice(&std::fs::read(&paths[0])?)?;
        if pass["extra"]["saya_split"] != format!("passe {phase}") {
            return Err(format!("Marqueur de passe {phase} invalide").into());
        }
        phases.push(pass);
    }
    let mut combined = master.clone();
    let mut nodes = BTreeMap::<u64, Value>::new();
    let mut links = BTreeMap::<u64, Value>::new();
    let mut definitions = BTreeMap::<String, Value>::new();
    let mut groups = Vec::new();
    for doc in std::iter::once(&master).chain(phases.iter()) {
        for n in doc["nodes"].as_array().ok_or("Nodes absents du preset")? {
            let id = n["id"].as_u64().ok_or("ID de node invalide")?;
            if let Some(old) = nodes.get(&id) {
                if old["type"] != n["type"] {
                    return Err(format!("Types incompatibles pour le node {id}").into());
                }
            } else {
                nodes.insert(id, n.clone());
            }
        }
        for link in doc["links"].as_array().ok_or("Liens absents du preset")? {
            let id = link[0]
                .as_u64()
                .ok_or("Lien UI non pris en charge dans le preset")?;
            if links.get(&id).is_some_and(|old| old != link) {
                return Err(format!("Liens incompatibles : {id}").into());
            }
            links.insert(id, link.clone());
        }
        for def in doc["definitions"]["subgraphs"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let id = def["id"].as_str().ok_or("ID de subgraph absent")?;
            if definitions.get(id).is_some_and(|old| old != def) {
                return Err(format!("Subgraphs incompatibles : {id}").into());
            }
            definitions.insert(id.to_owned(), def.clone());
        }
        for group in doc["groups"].as_array().into_iter().flatten() {
            if !groups.contains(group) {
                groups.push(group.clone());
            }
        }
    }
    // Split files trim port links; reconstruct these only for the combined editor view.
    for node in nodes.values_mut() {
        for input in node["inputs"].as_array_mut().into_iter().flatten() {
            input["link"] = Value::Null;
        }
        for output in node["outputs"].as_array_mut().into_iter().flatten() {
            output["links"] = json!([]);
        }
    }
    for link in links.values() {
        let from = link[1].as_u64().ok_or("Origine de lien invalide")?;
        let to = link[3].as_u64().ok_or("Cible de lien invalide")?;
        let out = link[2].as_u64().ok_or("Port de sortie invalide")? as usize;
        let input = link[4].as_u64().ok_or("Port d'entrée invalide")? as usize;
        nodes
            .get_mut(&from)
            .and_then(|n| n["outputs"].get_mut(out))
            .and_then(|o| o["links"].as_array_mut())
            .ok_or("Sortie du preset introuvable")?
            .push(link[0].clone());
        let port = nodes
            .get_mut(&to)
            .and_then(|n| n["inputs"].get_mut(input))
            .ok_or("Entrée du preset introuvable")?;
        if !port["link"].is_null() && port["link"] != link[0] {
            return Err("Plusieurs liens vers la même entrée du preset".into());
        }
        port["link"] = link[0].clone();
    }
    combined["nodes"] = json!(nodes.into_values().collect::<Vec<_>>());
    combined["links"] = json!(links.into_values().collect::<Vec<_>>());
    combined["definitions"]["subgraphs"] = json!(definitions.into_values().collect::<Vec<_>>());
    combined["groups"] = json!(groups);
    overlay_widgets(&mut combined, &master);
    overlay_widgets(&mut combined, source);
    overlay_definitions(&mut combined, source);
    combined["extra"]
        .as_object_mut()
        .ok_or("Métadonnées du preset absentes")?
        .remove("saya_split");
    combined["extra"]["saya_preset"] = json!({"version":1,"phases":phases});
    Ok(combined)
}

/// `pub(crate)`: the split path (`saya.rs::split_phase_source_in`) shares the SAME
/// overlay semantics as the preset path — both widget fields, never the array
/// alone (casting fill only writes `widgets_values_named`).
pub(crate) fn overlay_widgets(target: &mut Value, source: &Value) {
    for node in target["nodes"].as_array_mut().into_iter().flatten() {
        if let Some(n) = source["nodes"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|n| n["id"] == node["id"] && n["type"] == node["type"])
        {
            for field in ["widgets_values", "widgets_values_named"] {
                if let Some(value) = n.get(field) {
                    node[field] = value.clone();
                }
            }
            // Bypass set from Generation: each active pass runs it too
            // (mode 4); a pass that already mutes this node (mute) stays unchanged.
            if n["properties"]["saya_bypass"] == true && node["mode"].as_i64().unwrap_or(0) == 0 {
                node["mode"] = json!(4);
                if !node["properties"].is_object() {
                    node["properties"] = json!({});
                }
                node["properties"]["saya_original_mode"] = json!(4);
            }
        }
    }
}
/// `pub(crate)`: the split path also propagates the tuned DEFINITIONS
/// (hub 810 + 488 in its subgraph definition), like the engine that splits
/// a master ALREADY tuned by the shot.
pub(crate) fn overlay_definitions(target: &mut Value, source: &Value) {
    for def in target["definitions"]["subgraphs"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        if let Some(edited) = source["definitions"]["subgraphs"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|d| d["id"] == def["id"])
        {
            *def = edited.clone();
        }
    }
}

pub fn phase_source(source: &Value, phase: u32) -> Result<Option<Value>> {
    let Some(preset) = source["extra"].get("saya_preset") else {
        return Ok(None);
    };
    if preset["version"] != 1 || !(1..=6).contains(&phase) {
        return Err("Version ou phase du preset invalide".into());
    }
    let phases = preset["phases"]
        .as_array()
        .filter(|p| p.len() == 6)
        .ok_or("Les six passes du preset sont requises")?;
    let mut pass = phases[(phase - 1) as usize].clone();
    overlay_widgets(&mut pass, source);
    overlay_definitions(&mut pass, source);
    Ok(Some(pass))
}

#[cfg(test)]
mod tests {
    use super::phase_source;
    use serde_json::{json, Value};

    /// REAL path of casting finishing (runs carry `extra.saya_preset`):
    /// each projected phase 1-6 carries the SHOT CHECKPOINT (hub 810 instantiated AND
    /// 488 in its definition), not that of the passes embedded when the preset was built.
    /// Sampler 2 (489) stays on the fixed checkpoint.
    #[test]
    fn each_projected_phase_carries_the_shot_checkpoint() {
        const GENERATION: &str = "Illustrious/Sayapacito-Alpha (config Generation)";
        const SHOT: &str = "Noobai/MKLAN POISONED NOOB AI (shot)";
        const FIXE: &str = "SDXL/Mklan-AnimeTest6SDXL (sampler 2 fixe)";
        let hub = |ckpt: &str| {
            json!({"id": 810, "type": "hub-test", "mode": 0,
                "widgets_values": [ckpt], "widgets_values_named": {"ckpt_name": ckpt}})
        };
        let definition = |ckpt488: &str| {
            json!({"id": "hub-test", "nodes": [
                {"id": 488, "type": "CheckpointLoaderSimple", "mode": 0,
                 "widgets_values": [ckpt488], "widgets_values_named": {"ckpt_name": ckpt488}},
                {"id": 489, "type": "CheckpointLoaderSimple", "mode": 0,
                 "widgets_values": [FIXE], "widgets_values_named": {"ckpt_name": FIXE}}
            ]})
        };
        let source = json!({
            "extra": {"saya_preset": {"version": 1, "phases": [
                json!({"nodes": [hub(GENERATION)], "definitions": {"subgraphs": [definition(GENERATION)]}}),
                json!({"nodes": [hub(GENERATION)], "definitions": {"subgraphs": [definition(GENERATION)]}}),
                json!({"nodes": [hub(GENERATION)], "definitions": {"subgraphs": [definition(GENERATION)]}}),
                json!({"nodes": [hub(GENERATION)], "definitions": {"subgraphs": [definition(GENERATION)]}}),
                json!({"nodes": [hub(GENERATION)], "definitions": {"subgraphs": [definition(GENERATION)]}}),
                json!({"nodes": [hub(GENERATION)], "definitions": {"subgraphs": [definition(GENERATION)]}})
            ]}},
            // Filled by casting: named field = shot, array frozen on Generation.
            "nodes": [{"id": 810, "type": "hub-test", "mode": 0,
                "widgets_values": [GENERATION], "widgets_values_named": {"ckpt_name": SHOT}}],
            "definitions": {"subgraphs": [definition(SHOT)]}
        });
        let checkpoint = |projected: &Value| {
            let def = &projected["definitions"]["subgraphs"][0];
            (
                projected["nodes"][0]["widgets_values_named"]["ckpt_name"].clone(),
                def["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|n| n["id"] == 488)
                    .unwrap()["widgets_values_named"]["ckpt_name"]
                    .clone(),
                def["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|n| n["id"] == 489)
                    .unwrap()["widgets_values_named"]["ckpt_name"]
                    .clone(),
            )
        };
        for phase in 1..=6 {
            let projected = phase_source(&source, phase).unwrap().unwrap();
            let (hub810, n488, n489) = checkpoint(&projected);
            assert_eq!(hub810, json!(SHOT), "phase {phase} : hub 810 = shot");
            assert_eq!(n488, json!(SHOT), "phase {phase} : 488 = shot");
            assert_eq!(n489, json!(FIXE), "phase {phase} : 489 fixe");
        }
    }
}
