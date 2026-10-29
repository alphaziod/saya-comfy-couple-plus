//! Conservative pre-pass residency policy. Two SDXL/Illustrious checkpoints may
//! coexist; a third distinct model or an explicitly large architecture unloads
//! first. This is a planning decision; the memory manager performs the unload.
use crate::Result;
use serde_json::{json, Value};

fn model_name(class: &str, input: &str, value: &Value) -> Option<String> {
    let value = value.as_str()?.trim();
    if value.is_empty() {
        return None;
    }
    let relevant = input == "ckpt_name"
        || input == "unet_name"
        || input == "model_name"
        || class.contains("Checkpoint")
        || class.contains("UNET")
        || class.contains("Unet");
    relevant.then(|| value.to_owned())
}

fn is_large(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [
        "flux",
        "hidream",
        "sd3",
        "stable-diffusion-3",
        "qwen",
        "kolors",
        "wan",
        "hunyuan",
    ]
    .iter()
    .any(|k| n.contains(k))
}

pub fn plan(api: &Value, available_mib: Option<u64>) -> Result<Value> {
    let mut models: Vec<String> = Vec::new();
    for node in api.as_object().into_iter().flatten().map(|(_, n)| n) {
        let class = node["class_type"].as_str().unwrap_or_default();
        for (input, value) in node["inputs"].as_object().into_iter().flatten() {
            if let Some(name) = model_name(class, input, value) {
                if !models.contains(&name) {
                    models.push(name);
                }
            }
        }
    }
    let large: Vec<&String> = models.iter().filter(|m| is_large(m)).collect();
    let estimated_mib = models
        .iter()
        .map(|m| if is_large(m) { 12_000 } else { 6_500 })
        .sum::<u64>();
    let measured_shortage = available_mib.is_some_and(|free| free < estimated_mib);
    let within_budget = available_mib.is_some_and(|free| free >= estimated_mib);
    let should_unload =
        models.len() > 2 || (!large.is_empty() && measured_shortage) || measured_shortage;
    Ok(
        json!({"models":models,"largeModels":large,"distinct":models.len(),
        "allowedResident":2,"availableMib":available_mib,"estimatedMib":estimated_mib,"withinBudget":within_budget,"requiresOffload":available_mib.is_some() && !within_budget,"shouldUnloadBeforePass":should_unload,
        "reason": if models.len()>2 { "plus de deux modèles distincts" } else if measured_shortage { "VRAM libre insuffisante pour les modèles requis" } else if !large.is_empty() { "modèle lourd évalué, capacité suffisante" } else { "deux modèles SDXL/Illustrious tiennent dans la VRAM" }}),
    )
}
