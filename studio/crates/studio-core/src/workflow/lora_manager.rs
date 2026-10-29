//! Explicit installed LoraManager widget contract; never rewrites graph values.
use serde_json::{json, Value};

fn supported(class: &str, schema: &Value) -> bool {
    class == "Lora Loader (LoraManager)"
        && schema.pointer("/input/required/model/0") == Some(&json!("MODEL"))
        && schema.pointer("/input/required/text/0") == Some(&json!("AUTOCOMPLETE_TEXT_LORAS"))
        && schema.pointer("/input/required/loras/0") == Some(&json!("LORAS"))
        && schema["output"] == json!(["MODEL", "CLIP", "STRING", "STRING"])
        && schema
            .pointer("/input/optional/__lm_autocomplete_meta_text/0")
            .is_none_or(|kind| kind == "STRING")
}

pub(crate) fn optional_field(class: &str, schema: &Value, key: &str) -> Option<Value> {
    if !supported(class, schema) {
        return None;
    }
    // FlexibleOptionalInputType serializes to {} in object_info. Only these known inputs are supported.
    match key {
        "clip" => Some(json!(["CLIP"])),
        "__lm_autocomplete_meta_text" => Some(json!(["STRING"])),
        _ => None,
    }
}

pub(super) fn literal(class: &str, schema: &Value, key: &str, value: &Value) -> Option<bool> {
    if !supported(class, schema) {
        return None;
    }
    match key {
        "text" | "__lm_autocomplete_meta_text" => Some(value.is_string()),
        "loras" => Some(valid_loras(value)),
        _ => None,
    }
}

fn finite(value: &Value) -> bool {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse::<f64>().ok())
        .is_some_and(f64::is_finite)
}

pub(crate) fn valid_loras(value: &Value) -> bool {
    // Require the API wrapper: unwrapped arrays are reserved for links by ComfyUI validation.
    let Some(wrapper) = value.as_object().filter(|v| v.len() == 1) else {
        return false;
    };
    let Some(entries) = wrapper.get("__value__").and_then(Value::as_array) else {
        return false;
    };
    entries.len() <= 100
        && entries.iter().all(|entry| {
            let Some(fields) = entry.as_object() else {
                return false;
            };
            fields.keys().all(|key| {
                [
                    "name",
                    "strength",
                    "clipStrength",
                    "active",
                    "expanded",
                    "locked",
                    "selected",
                ]
                .contains(&key.as_str())
            }) && entry["name"]
                .as_str()
                .is_some_and(|name| !name.trim().is_empty() && name.len() <= 4096)
                && finite(&entry["strength"])
                && entry.get("clipStrength").is_none_or(finite)
                && entry["active"].is_boolean()
                && ["expanded", "locked", "selected"]
                    .iter()
                    .all(|key| entry.get(key).is_none_or(Value::is_boolean))
        })
}
