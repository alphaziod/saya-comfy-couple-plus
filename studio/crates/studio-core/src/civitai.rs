//! Local Saya Civitai bridge client. The token is never logged or exposed to UI.
use crate::Result;
use serde_json::Value;

pub fn jobs(token: &str) -> Result<Value> {
    let response = reqwest::blocking::Client::new()
        .get("http://127.0.0.1:47821/jobs")
        .header("X-Saya-Token", token.trim())
        .send()?;
    if !response.status().is_success() {
        return Err("Bridge Civitai indisponible".into());
    }
    Ok(response.json()?)
}

fn control(token: &str, action: &str, job: &str) -> Result<Value> {
    if job.is_empty() || !job.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err("Identifiant de tâche Civitai invalide".into());
    }
    let response = reqwest::blocking::Client::new()
        .post(format!("http://127.0.0.1:47821/{}/{}", action, job))
        .header("X-Saya-Token", token.trim())
        .send()?;
    if !response.status().is_success() {
        return Err(format!(
            "Bridge Civitai refuse {} (HTTP {})",
            action,
            response.status()
        )
        .into());
    }
    Ok(response.json()?)
}

pub fn cancel(token: &str, job: &str) -> Result<Value> {
    control(token, "cancel", job)
}

pub fn pause(token: &str, job: &str) -> Result<Value> {
    control(token, "pause", job)
}

pub fn resume(token: &str, job: &str) -> Result<Value> {
    control(token, "resume", job)
}

pub fn api_token() -> String {
    let path = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .map(|home| home.join(".config/civitai/token"));
    path.and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default()
        .trim()
        .to_owned()
}

pub fn search(query: &str, types: &str, base_models: &str, limit: u32) -> Result<Value> {
    let token = api_token();
    let mut params = vec![("query", query.to_owned()), ("limit", limit.to_string())];
    if !types.trim().is_empty() {
        params.push(("types", types.to_owned()));
    }
    if !base_models.trim().is_empty() {
        params.push(("baseModels", base_models.to_owned()));
    }
    let query = params
        .iter()
        .map(|(k, v)| format!("{k}={}", url_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let (code, body) = crate::model_library::https_get(
        &format!("https://civitai.red/api/v1/models?{query}"),
        &token,
        16 * 1024 * 1024,
    )?;
    if !(200..300).contains(&code) {
        return Err(format!("Civitai indisponible (HTTP {code})").into());
    }
    Ok(serde_json::from_slice(&body)?)
}

fn url_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn download(
    token: &str,
    version_id: u64,
    file_id: Option<u64>,
    destination: &str,
) -> Result<Value> {
    let response = reqwest::blocking::Client::new()
        .post("http://127.0.0.1:47821/download")
        .header("X-Saya-Token", token.trim())
        .json(
            &serde_json::json!({"versionId":version_id,"fileId":file_id,"destination":destination}),
        )
        .send()?;
    if !response.status().is_success() {
        return Err("Bridge Civitai refuse le téléchargement".into());
    }
    Ok(response.json()?)
}
