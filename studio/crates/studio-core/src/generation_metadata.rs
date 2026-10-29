//! Read saved data only. Unknown custom-node outputs are never executed or guessed.
use crate::{Database, Result};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

fn link(value: &Value) -> Option<(&str, u64)> {
    let a = value.as_array()?;
    (a.len() == 2).then_some(())?;
    Some((a[0].as_str()?, a[1].as_u64()?))
}
fn class(node: &Value) -> &str {
    node["class_type"].as_str().unwrap_or("")
}
fn switch(node: &Value) -> bool {
    matches!(class(node), "ComfySwitchNode" | "SayaLazyBooleanSelect")
}
fn coupled(node: &Value) -> bool {
    matches!(class(node), "SayaMultiCouple" | "SayaCoupleImprintPackV2")
}

fn resolve(value: &Value, graph: &Map<String, Value>, depth: usize) -> Option<Value> {
    if depth > 64 {
        return None;
    }
    let Some((id, slot)) = link(value) else {
        return (!value.is_null() && !value.is_array() && !value.is_object())
            .then(|| value.clone());
    };
    let node = graph.get(id)?;
    let i = &node["inputs"];
    let get = |key: &str| resolve(&i[key], graph, depth + 1);
    if switch(node) && slot == 0 {
        return get(if get("switch")?.as_bool()? {
            "on_true"
        } else {
            "on_false"
        });
    }
    match (class(node), slot) {
        (
            "PrimitiveString"
            | "PrimitiveStringMultiline"
            | "PrimitiveBoolean"
            | "PrimitiveInt"
            | "PrimitiveFloat",
            0,
        ) => get("value"),
        ("Seed (rgthree)", 0) => get("seed"),
        ("StringConcatenate", 0) => Some(json!(format!(
            "{}{}{}",
            get("string_a")?.as_str()?,
            get("delimiter")?.as_str()?,
            get("string_b")?.as_str()?
        ))),
        ("StringCompare", 0) => {
            let mut a = get("string_a")?.as_str()?.to_owned();
            let mut b = get("string_b")?.as_str()?.to_owned();
            if !i["case_sensitive"].as_bool().unwrap_or(true) {
                a = a.to_lowercase();
                b = b.to_lowercase();
            }
            Some(json!(match i["mode"].as_str()? {
                "Equal" => a == b,
                "Starts With" => a.starts_with(&b),
                "Ends With" => a.ends_with(&b),
                _ => return None,
            }))
        }
        ("SayaMainPromptFR", 4) => Some(json!(get("prefixe")?.as_str()?.trim_end())),
        ("SayaMainPrompt", 4) => Some(json!(get("prefix")?.as_str()?.trim_end())),
        _ => None,
    }
}

fn active_nodes(graph: &Map<String, Value>, output: Option<&str>) -> (BTreeSet<String>, bool) {
    let mut pending = if let Some(output) = output.filter(|id| graph.contains_key(*id)) {
        vec![output.to_owned()]
    } else {
        graph
            .iter()
            .filter(|(_, n)| {
                matches!(
                    class(n),
                    "SaveImage" | "PreviewImage" | "DaSiWa_MetadataImageSaver"
                )
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>()
    };
    let grounded = !pending.is_empty();
    if !grounded {
        pending = graph.keys().cloned().collect();
    }
    let mut active = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !active.insert(id.clone()) {
            continue;
        }
        let Some(node) = graph.get(&id) else {
            continue;
        };
        let Some(inputs) = node["inputs"].as_object() else {
            continue;
        };
        let selector = if switch(node) {
            resolve(&node["inputs"]["switch"], graph, 0).and_then(|v| v.as_bool())
        } else {
            None
        };
        let solo = coupled(node)
            && resolve(&node["inputs"]["solo"], graph, 0).and_then(|v| v.as_bool()) == Some(true);
        for (name, value) in inputs {
            if switch(node)
                && selector.is_some()
                && ((name == "on_true" && selector == Some(false))
                    || (name == "on_false" && selector == Some(true)))
            {
                continue;
            }
            if solo
                && [
                    "pos_2",
                    "neg_2",
                    "mask_2",
                    "model_2",
                    "p2_text",
                    "person_2_prompt",
                ]
                .contains(&name.as_str())
            {
                continue;
            }
            if let Some((parent, _)) = link(value) {
                pending.push(parent.into());
            }
        }
    }
    (active, grounded)
}
fn role(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.contains("negative")
        || n.contains("negatif")
        || n == "neg"
        || n.starts_with("neg ")
        || n.contains("· neg")
    {
        "NEG"
    } else if n.contains("person 2") || n.contains("person_2") || n.contains("p2") {
        if n.contains("anatom") {
            "P2 ANATOMY"
        } else {
            "P2"
        }
    } else if n.contains("person 1") || n.contains("person_1") || n.contains("p1") {
        if n.contains("anatom") {
            "P1 ANATOMY"
        } else {
            "P1"
        }
    } else if n.contains("quality") || n.contains("qualité") || n == "prefixe" || n == "prefix" {
        "QUALITY"
    } else if n.contains("action") {
        "ACTION"
    } else if n.contains("background")
        || n == "bg"
        || n.starts_with("bg ")
        || n.contains("decor")
        || n.contains("main_prompt")
    {
        "BG"
    } else {
        "PROMPT"
    }
}
fn scalar(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
fn section(
    id: &str,
    input: &str,
    title: &str,
    value: &Value,
    graph: &Map<String, Value>,
    grounded: bool,
) -> Value {
    let content = resolve(value, graph, 0).and_then(|v| v.as_str().map(str::to_owned));
    let status = match content.as_deref() {
        Some("") => "empty",
        Some(_) => "present",
        None => "unparseable",
    };
    json!({"node":id,"input":input,"name":title,"role":role(title),"status":status,"content":content,"activity":if grounded{"reachable"}else{"unverified"}})
}

pub fn is_parameters(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.contains("\nSteps: ") && (s.contains(", Seed: ") || s.contains(", CFG scale: "))
    })
}

fn exported_parameters(text: &Value, sections: &mut Vec<Value>, parameters: &mut Vec<Value>) {
    let Some(raw) = text["parameters"]
        .as_str()
        .filter(|_| is_parameters(&text["parameters"]))
    else {
        return;
    };
    let Some((prompts, settings)) = raw.rsplit_once("\nSteps: ") else {
        return;
    };
    if prompts.matches("\nNegative prompt:").count() > 1 {
        return;
    }
    let (positive, negative) = match prompts.split_once("\nNegative prompt:") {
        Some((p, n)) => (p, Some(n.strip_prefix(' ').unwrap_or(n))),
        None => (prompts, None),
    };
    for (role, content) in [("POS", Some(positive)), ("NEG", negative)] {
        if let Some(content) = content {
            sections.push(json!({"node":null,"input":"parameters","name":format!("{role} · parameters exportés"),"role":role,"status":if content.is_empty(){"empty"}else{"present"},"content":content,"activity":"recorded"}));
        }
    }
    let settings = format!("Steps: {settings}");
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut quote = false;
    let mut escaped = false;
    for (i, c) in settings.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quote && c == '\\' {
            escaped = true;
            continue;
        }
        if c == '"' {
            quote = !quote;
            continue;
        }
        if quote {
            continue;
        }
        if matches!(c, '{' | '[') {
            depth += 1;
        } else if matches!(c, '}' | ']') {
            depth = depth.saturating_sub(1);
        }
        if c == ',' && depth == 0 && settings[i..].starts_with(", ") {
            tokens.push(&settings[start..i]);
            start = i + 2;
        }
    }
    tokens.push(&settings[start..]);
    let values = tokens
        .into_iter()
        .filter_map(|v| v.split_once(": "))
        .map(|(name, value)| json!({"name":name,"value":value,"resolved":!quote&&depth==0}))
        .collect::<Vec<_>>();
    parameters.push(json!({"node":"parameters","name":"Paramètres exportés A1111 / DaSiWa","class":"Métadonnées déclarées dans l’image","values":values}));
}

pub fn inspect(text: &Value, output: Option<&str>) -> Value {
    let raw = text.to_string();
    let graph = text["prompt"].as_object();
    let mut sections = Vec::new();
    let mut parameters = Vec::new();
    let mut models = Vec::new();
    let mut warnings = Vec::new();
    let mut mode = "indeterminate";
    let mut mode_evidence = Vec::new();
    if let Some(graph) = graph.filter(|g| g.len() <= 10000) {
        let (active, grounded) = active_nodes(graph, output);
        if !grounded {
            warnings.push("Sortie non identifiée : activité des nodes non démontrée.");
        }
        let mut modes = BTreeSet::new();
        let mut uncertain = false;
        for id in &active {
            let Some(node) = graph.get(id) else {
                warnings.push("Une connexion désigne un node absent.");
                uncertain = true;
                continue;
            };
            let title = node["_meta"]["title"].as_str().unwrap_or(class(node));
            let Some(inputs) = node["inputs"].as_object() else {
                continue;
            };
            if coupled(node) {
                match resolve(&node["inputs"]["solo"], graph, 0).and_then(|v| v.as_bool()) {
                    Some(solo) => {
                        modes.insert(if solo { "solo" } else { "couple" });
                        mode_evidence.push(format!("{id} · {title} · solo={solo}"));
                    }
                    None => uncertain = true,
                }
            }
            if switch(node)
                && resolve(&node["inputs"]["switch"], graph, 0)
                    .and_then(|v| v.as_bool())
                    .is_none()
            {
                uncertain = true;
            }
            let solo = coupled(node)
                && resolve(&node["inputs"]["solo"], graph, 0).and_then(|v| v.as_bool())
                    == Some(true);
            let mut values = Vec::new();
            for (name, value) in inputs {
                if solo && ["p2_text", "person_2_prompt"].contains(&name.as_str()) {
                    continue;
                }
                let is_prompt = (class(node).starts_with("CLIPTextEncode")
                    && ["text", "text_g", "text_l"].contains(&name.as_str()))
                    || (class(node).starts_with("PrimitiveString")
                        && name == "value"
                        && (role(title) != "PROMPT" || title.to_lowercase().contains("prompt")))
                    || name.ends_with("_prompt")
                    || ["prompt", "p1_text", "p2_text"].contains(&name.as_str());
                let main_source = matches!(class(node), "SayaMainPromptFR" | "SayaMainPrompt")
                    && [
                        "prefixe",
                        "prefix",
                        "decor_libre",
                        "custom_background",
                        "complement",
                        "extra",
                    ]
                    .contains(&name.as_str());
                if is_prompt || main_source {
                    let label = if main_source {
                        format!(
                            "{} · {title} · {name} (entrée source)",
                            if ["prefixe", "prefix"].contains(&name.as_str()) {
                                "QUALITY"
                            } else {
                                "BG"
                            }
                        )
                    } else {
                        format!("{title} · {name}")
                    };
                    let mut item = section(id, name, &label, value, graph, grounded);
                    if item["role"] == "PROMPT" {
                        item["role"] = json!(role(name));
                    }
                    if item["role"] == "PROMPT" && class(node).starts_with("CLIPTextEncode") {
                        let negative = active.iter().filter_map(|id| graph.get(id)).any(|n| {
                            ["negative", "neg_1", "neg_2"].iter().any(|key| {
                                link(&n["inputs"][key]).is_some_and(|(source, _)| source == id)
                            })
                        });
                        if negative {
                            item["role"] = json!("NEG");
                        }
                    }
                    sections.push(item);
                } else if !name.starts_with("__") {
                    let resolved = resolve(value, graph, 0);
                    values.push(json!({"name":name,"value":resolved.as_ref().map(scalar).unwrap_or_else(||value.to_string()),"resolved":resolved.is_some()}));
                }
                if [
                    "ckpt_name",
                    "vae_name",
                    "unet_name",
                    "model_name",
                    "clip_name",
                    "clip_name1",
                    "clip_name2",
                    "clip_name3",
                    "clip_name4",
                    "lora_name",
                ]
                .contains(&name.as_str())
                {
                    models.push(json!({"node":id,"kind":name,"name":resolve(value,graph,0).as_ref().map(scalar),"strengthModel":inputs.get("strength_model").map(scalar),"strengthClip":inputs.get("strength_clip").map(scalar)}));
                }
                if name == "loras" {
                    if let Some(loras) = value.get("__value__").unwrap_or(value).as_array() {
                        for lora in loras {
                            if lora["active"].as_bool() == Some(false) {
                                continue;
                            }
                            models.push(json!({"node":id,"kind":"lora","name":lora.get("name").map(scalar),"strengthModel":lora.get("strength").or_else(||lora.get("strength_model")).map(scalar),"strengthClip":lora.get("strength_clip").map(scalar),"raw":lora.to_string()}));
                        }
                    }
                }
            }
            if !values.is_empty() {
                parameters
                    .push(json!({"node":id,"name":title,"class":class(node),"values":values}));
            }
        }
        if grounded && !uncertain && modes.len() == 1 {
            mode = modes.iter().next().copied().unwrap();
        }
        if uncertain {
            warnings.push("Branche conditionnelle non résolue : le mode et certaines participations restent indéterminés.");
        }
    } else {
        warnings.push("Graphe API absent, invalide ou trop volumineux. Données brutes conservées.");
    }
    exported_parameters(text, &mut sections, &mut parameters);
    if !sections.iter().any(|s| s["role"] == "NEG") {
        sections.push(json!({"node":null,"input":null,"name":"NEG","role":"NEG","status":"absent","content":null,"activity":"unknown"}));
    }
    if sections.iter().any(|s| s["status"] == "unparseable") {
        warnings.push("Certaines sorties de custom nodes ne sont pas reconstruites ; les entrées sources et les données brutes restent consultables.");
    }
    json!({"mode":mode,"modeEvidence":mode_evidence,"sections":sections,"parameters":parameters,"models":models,"warnings":warnings,"rawJson":raw,"workflowJson":text.get("workflow").map(Value::to_string)})
}

impl Database {
    pub fn generation_metadata(&self, id: &str) -> Result<Value> {
        let metadata: String = self.connection.query_row(
            "SELECT metadata_json FROM assets WHERE id=?",
            [id],
            |r| r.get(0),
        )?;
        let meta: Value = serde_json::from_str(&metadata)?;
        let generations = self.asset_generations(id)?;
        let mut text = meta
            .get("text")
            .filter(|v| v.is_object())
            .cloned()
            .unwrap_or_else(|| json!({}));
        let mut source = "image";
        if !text["prompt"].is_object() {
            if let Some(job) = generations.first() {
                if let Some(raw) = job["requestJson"].as_str() {
                    let request: Value = serde_json::from_str(raw)?;
                    if request["prompt"].is_object() {
                        text["prompt"] = request["prompt"].clone();
                        source = "job";
                    }
                }
            }
        }
        let mut result = inspect(
            &text,
            generations.first().and_then(|g| g["nodeId"].as_str()),
        );
        result["source"] = json!(source);
        result["generations"] = json!(generations);
        result["width"] = meta["width"].clone();
        result["height"] = meta["height"].clone();
        result["fileModifiedUtc"]=self.connection.query_row("SELECT CASE WHEN length(modified_at)>=16 AND modified_at NOT GLOB '*[^0-9]*' THEN datetime(CAST(modified_at AS REAL)/1000000000,'unixepoch') ELSE modified_at END FROM assets WHERE id=?",[id],|r|r.get::<_,Option<String>>(0))?.map(Value::String).unwrap_or(Value::Null);
        result["originalMetadataJson"] = json!(metadata);
        Ok(result)
    }
}
