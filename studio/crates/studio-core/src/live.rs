//! ComfyUI's identified events; no inferred job ownership or simulated progress.
use crate::{Database, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, io::Cursor};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveJob {
    pub node: Option<String>,
    pub value: Option<f64>,
    pub max: Option<f64>,
    pub nodes: BTreeMap<String, Value>,
    pub error: Option<Value>,
    pub preview: Option<String>,
    pub finished: bool,
}
fn identifier(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.is_empty() && s.len() <= 256)
}
fn progress(data: &Value) -> Option<(f64, f64)> {
    let value = data["value"].as_f64()?;
    let max = data["max"].as_f64()?;
    (value.is_finite() && max.is_finite() && max > 0.0 && value >= 0.0 && value <= max)
        .then_some((value, max))
}
impl LiveJob {
    pub fn apply(&mut self, prompt: &str, event: &Value) -> bool {
        let data = &event["data"];
        if data["prompt_id"].as_str() != Some(prompt) {
            return false;
        }
        let kind = event["type"].as_str().unwrap_or("");
        if self.finished {
            return false;
        }
        match kind {
            "execution_start" => *self = Self::default(),
            "executing" => {
                if !data["node"].is_null() && identifier(&data["node"]).is_none() {
                    return false;
                }
                self.node = identifier(&data["node"]).map(str::to_owned);
                self.value = None;
                self.max = None;
            }
            "progress" => {
                let Some(node) = identifier(&data["node"]) else {
                    return false;
                };
                let Some((value, max)) = progress(data) else {
                    return false;
                };
                self.node = Some(node.into());
                self.value = Some(value);
                self.max = Some(max);
            }
            "progress_state" => {
                let Some(nodes) = data["nodes"].as_object().filter(|v| v.len() <= 256) else {
                    return false;
                };
                let mut checked = BTreeMap::new();
                for (id, node) in nodes {
                    if id.len() > 256
                        || progress(node).is_none()
                        || node["prompt_id"] != prompt
                        || !["running", "finished", "pending", "error"]
                            .contains(&node["state"].as_str().unwrap_or(""))
                    {
                        return false;
                    }
                    checked.insert(
                        id.clone(),
                        json!({"value":node["value"],"max":node["max"],"state":node["state"]}),
                    );
                }
                self.nodes = checked;
            }
            "execution_error" | "execution_interrupted" => {
                self.error = Some(
                    json!({"type":kind,"node":data["node_id"],"exception_type":data["exception_type"],"exception_message":data["exception_message"].as_str().unwrap_or("").chars().take(4000).collect::<String>()}),
                );
                self.finished = true;
                self.value = None;
                self.max = None;
            }
            "execution_success" => {
                self.finished = true;
                self.node = None;
                self.value = None;
                self.max = None;
            }
            _ => return false,
        }
        true
    }
}
#[derive(Debug)]
pub struct Preview {
    pub prompt_id: String,
    pub node_id: String,
    pub image: String,
}
pub fn decode_preview(bytes: &[u8]) -> Result<Option<Preview>> {
    if bytes.len() < 4 {
        return Err("En-tête preview tronqué".into());
    }
    // Legacy type 1 has no prompt_id: never guess which job owns it.
    if u32::from_be_bytes(bytes[..4].try_into()?) != 4 {
        return Ok(None);
    }
    if bytes.len() < 8 || bytes.len() > 2_000_000 {
        return Err("Taille preview invalide".into());
    }
    let size = u32::from_be_bytes(bytes[4..8].try_into()?) as usize;
    if size > 8192 || 8 + size >= bytes.len() {
        return Err("Métadonnées preview tronquées".into());
    }
    let metadata: Value = serde_json::from_slice(&bytes[8..8 + size])?;
    let prompt_id = identifier(&metadata["prompt_id"])
        .ok_or("Preview sans prompt_id")?
        .into();
    let node_id = identifier(&metadata["node_id"])
        .ok_or("Preview sans node_id")?
        .into();
    let format = match metadata["image_type"].as_str() {
        Some("image/png") => image::ImageFormat::Png,
        Some("image/jpeg") => image::ImageFormat::Jpeg,
        _ => return Err("Format preview invalide".into()),
    };
    let raw = &bytes[8 + size..];
    let (w, h) = image::ImageReader::with_format(Cursor::new(raw), format).into_dimensions()?;
    if u64::from(w) * u64::from(h) > 4_194_304 {
        return Err("Preview trop grande".into());
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(raw), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    reader.decode()?;
    Ok(Some(Preview {
        prompt_id,
        node_id,
        image: format!(
            "data:{};base64,{}",
            metadata["image_type"].as_str().unwrap(),
            STANDARD.encode(raw)
        ),
    }))
}
impl Database {
    pub fn comfy_client_id(&self) -> Result<String> {
        self.connection.execute(
            "INSERT OR IGNORE INTO preferences(key,value_json) VALUES('comfyClientId',?1)",
            [json!(uuid::Uuid::new_v4().to_string()).to_string()],
        )?;
        let text: String = self.connection.query_row(
            "SELECT value_json FROM preferences WHERE key='comfyClientId'",
            [],
            |r| r.get(0),
        )?;
        let id: String = serde_json::from_str(&text)?;
        uuid::Uuid::parse_str(&id)?;
        Ok(id)
    }
    pub fn tracked_prompt(&self, prompt: &str) -> Result<bool> {
        Ok(self.connection.query_row("SELECT EXISTS(SELECT 1 FROM generation_jobs WHERE comfy_prompt_id=?1 AND state IN ('queued','running','submitting','submission_unknown','awaiting_review'))",[prompt],|r|r.get(0))?)
    }
    pub fn record_execution_event(&self, event: &Value) -> Result<()> {
        let Some(prompt) = identifier(&event["data"]["prompt_id"]) else {
            return Ok(());
        };
        let kind = event["type"].as_str().unwrap_or("");
        let state = match kind {
            "execution_start" | "executing"
                if !event["data"]["node"].is_null() || kind == "execution_start" =>
            {
                "running"
            }
            "execution_error" => "failed",
            "execution_interrupted" => "interrupted",
            _ => return Ok(()),
        };
        let mut live = LiveJob::default();
        if !live.apply(prompt, event) {
            return Ok(());
        }
        let changed = self.connection.execute("UPDATE generation_jobs SET state=?2,failure_json=?3 WHERE comfy_prompt_id=?1 AND state IN ('queued','running','submitting','submission_unknown','awaiting_review')",rusqlite::params![prompt,state,live.error.as_ref().map(|v|v.to_string())])?;
        if changed > 0 && kind != "executing" {
            self.connection.execute("INSERT INTO job_events(job_id,event_type,payload_json) SELECT id,?2,?3 FROM generation_jobs WHERE comfy_prompt_id=?1",rusqlite::params![prompt,kind,json!({"promptId":prompt,"error":live.error}).to_string()])?;
        }
        Ok(())
    }
}
