//! Model library over the engine's own model folders (LoRA, checkpoints, embeddings, others).
//! Roots come from ComfyUI `/internal/folder_paths`; nothing is hardcoded. Model files are never
//! rewritten: only LoraManager-compatible sidecars (`<stem>.metadata.json`, `<stem>.<image>`) are
//! written, and moves (auto-organize) are explicit, journaled and reversible.
use crate::Result;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MODEL_EXT: [&str; 7] = ["safetensors", "ckpt", "pt", "pth", "bin", "gguf", "sft"];
const IMAGE_EXT: [&str; 4] = ["webp", "jpeg", "jpg", "png"];
/// Civitai sometimes ships a video as preview; one frame is shown through the system ffmpeg.
const VIDEO_EXT: [&str; 2] = ["mp4", "webm"];
const MAX_ENTRIES: usize = 30_000;
const MAX_DEPTH: usize = 10;
const CIVITAI_API: &str = "https://civitai.red/api/v1";
/// LoraManager's default category priority for `{base_model}/{first_tag}`.
pub const CATEGORY_PRIORITY: [&str; 13] = [
    "character",
    "concept",
    "clothing",
    "style",
    "poses",
    "background",
    "action",
    "tool",
    "vehicle",
    "buildings",
    "objects",
    "assets",
    "animal",
];

/// Folder keys of `/internal/folder_paths` for each library tab.
fn folder_keys(kind: &str) -> Result<&'static [&'static str]> {
    Ok(match kind {
        "loras" => &["loras"],
        "checkpoints" => &["checkpoints"],
        "embeddings" => &["embeddings"],
        "others" => &[
            "vae",
            "upscale_models",
            "diffusion_models",
            "controlnet",
            "text_encoders",
        ],
        _ => return Err("Type de modèle inconnu".into()),
    })
}

/// Existing roots for a tab, each with the folder key it belongs to.
pub fn roots(folder_paths: &Value, kind: &str) -> Result<Vec<(String, PathBuf)>> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for key in folder_keys(kind)? {
        for p in folder_paths[*key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            let path = PathBuf::from(p);
            if path.is_dir() && !out.iter().any(|(_, r)| r == &path) {
                out.push(((*key).to_owned(), path.canonicalize()?));
            }
        }
    }
    Ok(out)
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<()> {
    if depth > MAX_DEPTH || out.len() >= MAX_ENTRIES {
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            walk(&path, depth + 1, out)?;
        } else if kind.is_file() && MODEL_EXT.contains(&ext_of(&path).as_str()) {
            out.push(path);
        }
    }
    Ok(())
}

fn sidecar(path: &Path) -> PathBuf {
    path.with_extension("metadata.json")
}

fn read_metadata(path: &Path) -> Option<Value> {
    let file = sidecar(path);
    let meta = std::fs::metadata(&file).ok()?;
    if !meta.is_file() || meta.len() > 4 * 1024 * 1024 {
        return None;
    }
    serde_json::from_slice(&std::fs::read(file).ok()?).ok()
}

/// Local preview next to the model: `<stem>.<image>` or `<stem>.preview.<image>`, then the
/// metadata's own absolute `preview_url` when it is an image beside the model.
fn preview_of(path: &Path, metadata: Option<&Value>) -> Option<PathBuf> {
    let stem = path.file_stem()?.to_str()?;
    let dir = path.parent()?;
    for ext in IMAGE_EXT.iter().chain(VIDEO_EXT.iter()) {
        for name in [format!("{stem}.{ext}"), format!("{stem}.preview.{ext}")] {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let url = PathBuf::from(metadata?["preview_url"].as_str()?);
    let ext = ext_of(&url);
    (url.is_absolute()
        && url.parent() == Some(dir)
        && (IMAGE_EXT.contains(&ext.as_str()) || VIDEO_EXT.contains(&ext.as_str()))
        && url.is_file())
    .then_some(url)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn item(key: &str, root: &Path, path: &Path) -> Result<Value> {
    let meta = std::fs::metadata(path)?;
    let metadata = read_metadata(path);
    let m = metadata.as_ref();
    let civitai = m.map(|m| &m["civitai"]);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("modèle");
    let parent = path
        .parent()
        .unwrap_or(root)
        .strip_prefix(root)
        .unwrap_or(Path::new(""));
    let mut folder = parent.to_string_lossy().replace('\\', "/");
    if !matches!(key, "loras" | "checkpoints" | "embeddings") {
        folder = if folder.is_empty() {
            key.to_owned()
        } else {
            format!("{key}/{folder}")
        };
    }
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut triggers = civitai
        .map(|c| strings(&c["trainedWords"]))
        .unwrap_or_default();
    triggers.retain(|t| !t.trim().is_empty());
    Ok(json!({
        "path": path.to_string_lossy(),
        "root": root.to_string_lossy(),
        "folder": folder,
        "file": path.file_name().and_then(|s| s.to_str()).unwrap_or(stem),
        "stem": stem,
        "name": m.and_then(|m| m["model_name"].as_str()).filter(|s| !s.trim().is_empty()).unwrap_or(stem),
        "ext": ext_of(path),
        "bytes": meta.len(),
        "modified": modified,
        "base": m.and_then(|m| m["base_model"].as_str()).unwrap_or(""),
        "tags": m.map(|m| strings(&m["tags"])).unwrap_or_default(),
        "triggers": triggers,
        "versionName": civitai.and_then(|c| c["name"].as_str()).unwrap_or(""),
        "modelId": civitai.and_then(|c| c["modelId"].as_u64()),
        "versionId": civitai.and_then(|c| c["id"].as_u64()),
        "favorite": m.and_then(|m| m["favorite"].as_bool()).unwrap_or(false),
        "sha256": m.and_then(|m| m["sha256"].as_str()).unwrap_or(""),
        "nsfw": m.and_then(|m| m["preview_nsfw_level"].as_u64()).unwrap_or(0),
        "fromCivitai": m.and_then(|m| m["from_civitai"].as_bool()).unwrap_or(false),
        "hasMetadata": m.is_some(),
        "preview": preview_of(path, m).map(|p| p.to_string_lossy().to_string()),
    }))
}

pub fn library(roots: &[(String, PathBuf)]) -> Result<Value> {
    let mut items = Vec::new();
    let mut truncated = false;
    for (key, root) in roots {
        let mut paths = Vec::new();
        walk(root, 0, &mut paths)?;
        truncated |= paths.len() >= MAX_ENTRIES;
        for path in paths {
            items.push(item(key, root, &path)?);
        }
    }
    Ok(json!({
        "roots": roots.iter().map(|(k, r)| json!({"key": k, "path": r.to_string_lossy()})).collect::<Vec<_>>(),
        "count": items.len(), "truncated": truncated, "items": items,
    }))
}

/// Resolve a model path and prove it lies inside one of the tab's roots.
fn inside(roots: &[(String, PathBuf)], path: &str) -> Result<(PathBuf, PathBuf)> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err("Chemin de modèle absolu requis".into());
    }
    let p = p.canonicalize().map_err(|_| "Modèle introuvable")?;
    let root = roots
        .iter()
        .map(|(_, r)| r)
        .find(|r| p.starts_with(r))
        .ok_or("Fichier hors des dossiers de modèles du moteur")?;
    Ok((p, root.clone()))
}

static THUMBS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn jpeg_data_url(bytes: &[u8], width: u32) -> Result<String> {
    use base64::Engine;
    let image = image::load_from_memory(bytes).map_err(|e| format!("Aperçu illisible : {e}"))?;
    let image = if image.width() > width {
        image.thumbnail(width, width * 3)
    } else {
        image
    };
    let mut out = std::io::Cursor::new(Vec::new());
    image
        .to_rgb8()
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(out.into_inner())
    ))
}

fn video_frame(path: &Path) -> Result<Vec<u8>> {
    let out = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-ss", "0.5", "-i"])
        .arg(path)
        .args(["-frames:v", "1", "-f", "image2pipe", "-vcodec", "png", "-"])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("ffmpeg indisponible : {e}"))?;
    if !out.status.success() || out.stdout.is_empty() {
        return Err("Image de la vidéo d'aperçu illisible".into());
    }
    Ok(out.stdout)
}

/// Local preview as a small JPEG data URL (memory-cached by path and mtime).
pub fn thumbnail(roots: &[(String, PathBuf)], image: &str) -> Result<String> {
    let (path, _) = inside(roots, image)?;
    let video = VIDEO_EXT.contains(&ext_of(&path).as_str());
    if !IMAGE_EXT.contains(&ext_of(&path).as_str()) && !video {
        return Err("Aperçu image requis".into());
    }
    let meta = std::fs::metadata(&path)?;
    if !video && meta.len() > 32 * 1024 * 1024 {
        return Err("Aperçu trop lourd".into());
    }
    let key = format!("{}:{:?}", path.display(), meta.modified().ok());
    if let Some(hit) = THUMBS
        .lock()
        .ok()
        .and_then(|c| c.as_ref().and_then(|m| m.get(&key).cloned()))
    {
        return Ok(hit);
    }
    let bytes = if video {
        video_frame(&path)?
    } else {
        std::fs::read(&path)?
    };
    let url = jpeg_data_url(&bytes, 360)?;
    if let Ok(mut cache) = THUMBS.lock() {
        let map = cache.get_or_insert_with(HashMap::new);
        if map.len() > 3000 {
            map.clear();
        }
        map.insert(key, url.clone());
    }
    Ok(url)
}

/// Full sidecar for the detail panel (description, versions, tags…).
pub fn detail(roots: &[(String, PathBuf)], path: &str) -> Result<Value> {
    let (path, _) = inside(roots, path)?;
    Ok(json!({"metadata": read_metadata(&path), "sidecar": sidecar(&path).is_file()}))
}

fn write_sidecar(path: &Path, metadata: &Value) -> Result<()> {
    let file = sidecar(path);
    crate::fsutil::write_atomic(&file, &serde_json::to_vec_pretty(metadata)?)?;
    Ok(())
}

/// Toggle the LoraManager-compatible `favorite` flag; the model file is untouched.
pub fn set_favorite(roots: &[(String, PathBuf)], path: &str, favorite: bool) -> Result<Value> {
    let (path, _) = inside(roots, path)?;
    let mut metadata = read_metadata(&path).unwrap_or_else(|| json!({}));
    let object = metadata.as_object_mut().ok_or("Métadonnées illisibles")?;
    object.insert("favorite".into(), Value::Bool(favorite));
    write_sidecar(&path, &metadata)?;
    Ok(json!({"favorite": favorite}))
}

pub fn file_sha256(path: &Path) -> Result<String> {
    crate::fsutil::file_sha256(path)
}

/// HTTPS GET through the system `curl` (the bundled HTTP client has no TLS). URL and token go
/// through a stdin config, so the token never appears in the process list.
pub fn https_get(url: &str, token: &str, max_bytes: u64) -> Result<(u16, Vec<u8>)> {
    use std::io::Write;
    let quote = |v: &str| v.replace('\\', "\\\\").replace('"', "\\\"");
    let mut config = format!(
        "url = \"{}\"\nuser-agent = \"SayaStudio/1.0\"\n",
        quote(url)
    );
    if !token.trim().is_empty() {
        config.push_str(&format!(
            "header = \"Authorization: Bearer {}\"\n",
            quote(token.trim())
        ));
    }
    let mut child = std::process::Command::new("curl")
        .args([
            "-sS",
            "-L",
            "--max-time",
            "60",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-filesize",
            &max_bytes.to_string(),
            "-w",
            "\n%{http_code}",
            "-K",
            "-",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl indisponible : {e}"))?;
    child
        .stdin
        .take()
        .ok_or("curl sans entrée")?
        .write_all(config.as_bytes())?;
    let out = child.wait_with_output()?;
    if !out.status.success() {
        return Err(format!("Réseau : {}", String::from_utf8_lossy(&out.stderr).trim()).into());
    }
    let split = out
        .stdout
        .iter()
        .rposition(|b| *b == b'\n')
        .ok_or("Réponse réseau invalide")?;
    let code = std::str::from_utf8(&out.stdout[split + 1..])
        .ok()
        .and_then(|c| c.trim().parse().ok())
        .unwrap_or(0);
    Ok((code, out.stdout[..split].to_vec()))
}

fn civitai_get(token: &str, path: &str) -> Result<Option<Value>> {
    let (code, body) = https_get(&format!("{CIVITAI_API}{path}"), token, 16 * 1024 * 1024)?;
    match code {
        404 => Ok(None),
        200..=299 => Ok(Some(serde_json::from_slice(&body)?)),
        _ => Err(format!("Civitai indisponible (HTTP {code})").into()),
    }
}

/// Civitai hosts: the apex domain itself or one of its subdomains (never `evilcivitai.com`).
pub fn civitai_host_allowed(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    ["civitai.com", "civitai.red"]
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Download a Civitai image (allowed hosts only) and return its bytes.
fn civitai_image_bytes(url: &str) -> Result<Vec<u8>> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "URL d'image invalide")?;
    let host = parsed.host_str().unwrap_or("");
    if parsed.scheme() != "https" || !civitai_host_allowed(host) {
        return Err("Image hors de Civitai refusée".into());
    }
    let (code, bytes) = https_get(parsed.as_str(), "", 24 * 1024 * 1024)?;
    if !(200..300).contains(&code) {
        return Err(format!("Image Civitai indisponible (HTTP {code})").into());
    }
    Ok(bytes)
}

/// Remote preview for the Civitai browser, as a small JPEG data URL (CSP forbids remote images).
pub fn civitai_thumbnail(url: &str) -> Result<String> {
    jpeg_data_url(&civitai_image_bytes(url)?, 360)
}

/// Fetch Civitai data by file hash and write a LoraManager-compatible sidecar plus a preview.
/// Existing user fields (favorite, notes, exclude) are kept; the previous sidecar is kept as `.bak`.
pub fn fetch_metadata(roots: &[(String, PathBuf)], path: &str, token: &str) -> Result<Value> {
    let (path, _) = inside(roots, path)?;
    let previous = read_metadata(&path);
    let sha = previous
        .as_ref()
        .and_then(|m| m["sha256"].as_str())
        .filter(|s| s.len() == 64)
        .map(str::to_owned)
        .map_or_else(|| file_sha256(&path), Ok)?;
    let Some(version) = civitai_get(token, &format!("/model-versions/by-hash/{sha}"))? else {
        return Ok(json!({"found": false, "sha256": sha}));
    };
    let model = version["modelId"]
        .as_u64()
        .map(|id| civitai_get(token, &format!("/models/{id}")))
        .transpose()?
        .flatten()
        .unwrap_or(Value::Null);
    let meta = std::fs::metadata(&path)?;
    let mut out = Map::new();
    let keep = |k: &str, d: Value| {
        previous
            .as_ref()
            .map(|p| p[k].clone())
            .filter(|v| !v.is_null())
            .unwrap_or(d)
    };
    out.insert(
        "file_name".into(),
        json!(path.file_stem().and_then(|s| s.to_str()).unwrap_or("")),
    );
    out.insert(
        "model_name".into(),
        json!(model["name"]
            .as_str()
            .or(version["model"]["name"].as_str())
            .unwrap_or("")),
    );
    out.insert("file_path".into(), json!(path.to_string_lossy()));
    out.insert("size".into(), json!(meta.len()));
    out.insert(
        "modified".into(),
        json!(meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())),
    );
    out.insert("sha256".into(), json!(sha));
    out.insert(
        "base_model".into(),
        json!(version["baseModel"].as_str().unwrap_or("")),
    );
    out.insert("from_civitai".into(), json!(true));
    out.insert("tags".into(), json!(strings(&model["tags"])));
    out.insert(
        "modelDescription".into(),
        json!(model["description"].as_str().unwrap_or("")),
    );
    out.insert("notes".into(), keep("notes", json!("")));
    out.insert("favorite".into(), keep("favorite", json!(false)));
    out.insert("exclude".into(), keep("exclude", json!(false)));
    out.insert("metadata_source".into(), json!("saya_civitai_api"));
    let mut civitai = version.clone();
    if let Some(o) = civitai.as_object_mut() {
        o.remove("files");
    }
    out.insert("civitai".into(), civitai);
    // Preview: first still image of the version, saved beside the model when none exists.
    let mut preview = preview_of(&path, previous.as_ref());
    let first = version["images"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|i| i["type"].as_str() != Some("video"));
    if preview.is_none() {
        if let Some(url) = first.and_then(|i| i["url"].as_str()) {
            if let Ok(bytes) = civitai_image_bytes(url) {
                let ext = match image::guess_format(&bytes) {
                    Ok(image::ImageFormat::Png) => "png",
                    Ok(image::ImageFormat::WebP) => "webp",
                    _ => "jpeg",
                };
                let target = path.with_extension(ext);
                if !target.exists() {
                    std::fs::write(&target, bytes)?;
                    preview = Some(target);
                }
            }
        }
    }
    out.insert(
        "preview_url".into(),
        json!(preview
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()),
    );
    out.insert(
        "preview_nsfw_level".into(),
        json!(first.and_then(|i| i["nsfwLevel"].as_u64()).unwrap_or(0)),
    );
    if sidecar(&path).is_file() {
        std::fs::copy(sidecar(&path), sidecar(&path).with_extension("json.bak"))?;
    }
    write_sidecar(&path, &Value::Object(out))?;
    Ok(json!({"found": true, "sha256": sha, "name": model["name"], "base": version["baseModel"]}))
}

fn category(tags: &[String]) -> Option<&'static str> {
    CATEGORY_PRIORITY
        .iter()
        .copied()
        .find(|c| tags.iter().any(|t| t.eq_ignore_ascii_case(c)))
}

fn safe_segment(s: &str) -> String {
    s.chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '\0') {
                '_'
            } else {
                c
            }
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_owned()
}

/// Files that travel with a model: same stem, any sidecar/preview extension.
fn companions(path: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(stem)) = (path.parent(), path.file_stem().and_then(|s| s.to_str())) else {
        return Vec::new();
    };
    let mut out = vec![path.to_path_buf()];
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if p != path && p.is_file() && name.starts_with(&format!("{stem}.")) {
            let rest = &name[stem.len() + 1..];
            if rest == "metadata.json"
                || rest == "metadata.json.bak"
                || rest == "civitai.info"
                || rest == "json"
                || IMAGE_EXT.contains(&rest)
                || VIDEO_EXT.contains(&rest)
                || rest.starts_with("preview.")
            {
                out.push(p);
            }
        }
    }
    out
}

/// Proposed moves `{base_model}/{category}` for models with Civitai metadata. Read-only.
pub fn organize_plan(roots: &[(String, PathBuf)]) -> Result<Value> {
    let mut moves = Vec::new();
    let mut skipped = 0;
    for (key, root) in roots {
        if !matches!(key.as_str(), "loras" | "checkpoints" | "embeddings") {
            continue;
        }
        let mut paths = Vec::new();
        walk(root, 0, &mut paths)?;
        for path in paths {
            let Some(m) = read_metadata(&path) else {
                skipped += 1;
                continue;
            };
            let base = safe_segment(m["base_model"].as_str().unwrap_or(""));
            if base.is_empty() {
                skipped += 1;
                continue;
            }
            let mut dir = root.join(&base);
            if let Some(c) = category(&strings(&m["tags"])) {
                dir = dir.join(c);
            }
            if path.parent() == Some(dir.as_path()) {
                continue;
            }
            let target = dir.join(path.file_name().unwrap_or_default());
            let conflict = companions(&path)
                .iter()
                .any(|c| dir.join(c.file_name().unwrap_or_default()).exists());
            moves.push(json!({
                "from": path.to_string_lossy(), "to": target.to_string_lossy(),
                "fromFolder": path.parent().unwrap_or(root).strip_prefix(root).unwrap_or(Path::new("")).to_string_lossy(),
                "toFolder": dir.strip_prefix(root).unwrap_or(Path::new("")).to_string_lossy(),
                "conflict": conflict,
            }));
        }
    }
    Ok(json!({"moves": moves, "skipped": skipped}))
}

/// Apply selected moves (model + companions), journaled for undo. Never overwrites.
pub fn organize_apply(
    roots: &[(String, PathBuf)],
    moves: &[(String, String)],
    journal_dir: &Path,
) -> Result<Value> {
    let mut done = Vec::new();
    let mut errors = Vec::new();
    for (from, to) in moves {
        let result = (|| -> Result<Vec<(String, String)>> {
            let (src, root) = inside(roots, from)?;
            let to = PathBuf::from(to);
            let dir = to.parent().ok_or("Destination invalide")?;
            if !to.is_absolute()
                || has_parent_component(&to)
                || !dir.starts_with(&root)
                || to.file_name() != src.file_name()
            {
                return Err("Destination hors du dossier de modèles".into());
            }
            let files = companions(&src);
            if files
                .iter()
                .any(|f| dir.join(f.file_name().unwrap_or_default()).exists())
            {
                return Err("Un fichier du même nom existe déjà à destination".into());
            }
            std::fs::create_dir_all(dir)?;
            let mut moved = Vec::new();
            for f in files {
                let target = dir.join(f.file_name().unwrap_or_default());
                std::fs::rename(&f, &target)?;
                moved.push((
                    f.to_string_lossy().to_string(),
                    target.to_string_lossy().to_string(),
                ));
            }
            Ok(moved)
        })();
        match result {
            Ok(moved) => done.extend(moved),
            Err(e) => errors.push(json!({"from": from, "error": e.to_string()})),
        }
    }
    std::fs::create_dir_all(journal_dir)?;
    let journal = journal_dir.join(format!("organize-{}.json", uuid::Uuid::new_v4()));
    std::fs::write(
        &journal,
        serde_json::to_vec_pretty(&json!({"moved": done}))?,
    )?;
    Ok(json!({"moved": done.len(), "errors": errors, "journal": journal.to_string_lossy()}))
}

fn has_parent_component(path: &Path) -> bool {
    path.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Resolve a possibly missing absolute path: canonicalize the deepest existing ancestor of its
/// parent and append the remaining components, the file name kept as is (a symlink is moved,
/// never followed). Refuses relative paths and `..`.
fn resolve_lenient(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() || has_parent_component(path) {
        return Err("Chemin du journal invalide".into());
    }
    let mut rest = vec![path.file_name().ok_or("Chemin du journal invalide")?];
    let mut existing = path.parent().ok_or("Chemin du journal invalide")?;
    while !existing.exists() {
        rest.push(existing.file_name().ok_or("Chemin du journal invalide")?);
        existing = existing.parent().ok_or("Chemin du journal invalide")?;
    }
    let mut out = existing.canonicalize()?;
    for part in rest.into_iter().rev() {
        out.push(part);
    }
    Ok(out)
}

/// Undo a journaled organize run, newest move first. Never overwrites.
/// Every source and destination of the journal must resolve inside `roots`; otherwise the
/// whole undo is refused before any file is touched.
pub fn organize_undo(roots: &[(String, PathBuf)], journal: &Path) -> Result<Value> {
    let data: Value = serde_json::from_slice(&std::fs::read(journal)?)?;
    let within = |p: &Path| roots.iter().any(|(_, r)| p != r && p.starts_with(r));
    let mut pairs = Vec::new();
    for pair in data["moved"].as_array().into_iter().flatten() {
        let (Some(from), Some(to)) = (pair[0].as_str(), pair[1].as_str()) else {
            continue;
        };
        let (from_r, to_r) = (
            resolve_lenient(Path::new(from))?,
            resolve_lenient(Path::new(to))?,
        );
        if !within(&from_r) || !within(&to_r) || from_r == to_r {
            return Err(
                "Journal de rangement hors des dossiers de modèles : annulation refusée".into(),
            );
        }
        pairs.push((from_r, to_r));
    }
    let mut restored = 0;
    let mut errors = Vec::new();
    for (from, to) in pairs.iter().rev() {
        let (from, to) = (from.as_path(), to.as_path());
        if from.exists() {
            errors.push(
                json!({"file": from.to_string_lossy(), "error": "Emplacement d'origine occupé"}),
            );
            continue;
        }
        if let Some(parent) = from.parent() {
            std::fs::create_dir_all(parent)?;
        }
        match std::fs::rename(to, from) {
            Ok(()) => restored += 1,
            Err(e) => errors.push(json!({"file": to.to_string_lossy(), "error": e.to_string()})),
        }
    }
    Ok(json!({"restored": restored, "errors": errors}))
}

/// Move a finished Civitai download (bridge folder) into the tab root, under `{base}/{category}`.
pub fn import_download(
    roots: &[(String, PathBuf)],
    downloaded: &Path,
    base: &str,
    tags: &[String],
) -> Result<PathBuf> {
    let root = roots
        .first()
        .map(|(_, r)| r.clone())
        .ok_or("Aucun dossier de modèles pour ce type")?;
    let mut dir = root.clone();
    let base = safe_segment(base);
    if !base.is_empty() {
        dir = dir.join(base);
        if let Some(c) = category(tags) {
            dir = dir.join(c);
        }
    }
    let target = dir.join(
        downloaded
            .file_name()
            .ok_or("Fichier téléchargé invalide")?,
    );
    if target.exists() {
        return Err("Un modèle du même nom existe déjà à destination".into());
    }
    std::fs::create_dir_all(&dir)?;
    if std::fs::rename(downloaded, &target).is_err() {
        std::fs::copy(downloaded, &target)?;
        std::fs::remove_file(downloaded)?;
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::civitai_host_allowed;

    #[test]
    fn civitai_host_check_rejects_lookalikes() {
        for ok in [
            "civitai.com",
            "image.civitai.com",
            "CIVITAI.COM",
            "civitai.red",
            "a.b.civitai.red",
        ] {
            assert!(civitai_host_allowed(ok), "{ok}");
        }
        for bad in [
            "evilcivitai.com",
            "civitai.com.evil.net",
            "notcivitai.red",
            "",
            "civitai.co",
        ] {
            assert!(!civitai_host_allowed(bad), "{bad}");
        }
    }
}
