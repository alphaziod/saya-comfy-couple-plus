//! V5 — generation browser: STRICTLY read-only listing of the
//! images produced by Saya's three sources:
//!
//! 1. the `generez` engine runs (`<engine>/sortie/<date>/<famille>/*.png`,
//!    root derived from `engine_runtime::engine_dir()`, so `SAYA_ENGINE_DIR`
//!    is still honoured for tests);
//! 2. the per-family copies of the SayaFamilySave node (`<ComfyUI output>/saya/…`,
//!    see `moteur/comfy.py:463` and `comfy.py:996` for the ComfyUI root);
//! 3. the app results (`<app_data>/results`, the same images as the
//!    gallery, written by `sync_jobs`).
//!
//! No write, no move, no subprocess: reads the names,
//! the mtimes, the tEXt chunk `saya_family` (reader shared with
//! `image_restore::inspect`) and the batch's optional `catalogue.json`. V3
//! (pass-1 sorts/filters) adds per image the short model (`modele`:
//! chunk > catalogue `model` > stem suffix) and the carried LoRAs (`loras`:
//! chunk > catalogue fields), each with ITS source — never an invented
//! field. The only display path goes through `gallery::image_data`, which
//! only writes to the app's own thumbnail cache — never into a
//! listed folder.

use crate::engine_runtime::engine_dir;
use crate::image_restore::text_chunks;
use crate::Result;
use png::Decoder;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where the ComfyUI output folder was found (detection order, first hit wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputSource {
    /// `SAYA_COMFY_OUTPUT_DIR`.
    Env,
    /// The `comfyOutputDir` user preference.
    Setting,
    /// The running ComfyUI (`/system_stats` argv).
    Running,
    /// `<ComfyUI install>/output`.
    Default,
    /// Nothing found.
    None,
}

impl OutputSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Setting => "setting",
            Self::Running => "running",
            Self::Default => "default",
            Self::None => "none",
        }
    }
}

/// Result of [`resolve_output_dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputResolution {
    pub path: Option<PathBuf>,
    pub source: OutputSource,
}

/// An absolute path naming an existing, readable folder.
pub fn usable_output_dir(text: &str) -> bool {
    let path = Path::new(text);
    path.is_absolute() && path.is_dir() && std::fs::read_dir(path).is_ok()
}

/// Output folder derived from the `sys.argv` ComfyUI reports: `--output-directory`
/// (both spellings) if present, else `<folder of main.py>/output`. Only absolute
/// paths are trusted: ComfyUI's working directory is unknown to us.
pub fn output_dir_from_argv(argv: &[String]) -> Option<PathBuf> {
    let mut explicit = None;
    let mut iter = argv.iter();
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--output-directory=") {
            explicit = Some(value.to_owned());
        } else if arg == "--output-directory" {
            explicit = iter.next().cloned();
        }
    }
    if let Some(value) = explicit {
        return Some(PathBuf::from(value));
    }
    let main = argv
        .iter()
        .find(|arg| Path::new(arg.as_str()).file_name().is_some_and(|name| name == "main.py"))?;
    let main = Path::new(main);
    main.is_absolute().then(|| main.parent().map(|dir| dir.join("output")))?
}

/// Pure resolution order: env, then user preference, then running ComfyUI, then the
/// default folder, else nothing. The env value is trusted as given (tests and other
/// installations point it at folders they own); the other sources must be existing folders.
pub fn resolve_output_dir(
    env: Option<&str>,
    preference: Option<&str>,
    running_argv: Option<&[String]>,
    default_dir: Option<&Path>,
) -> OutputResolution {
    let found = |path: PathBuf, source| OutputResolution { path: Some(path), source };
    if let Some(value) = env.filter(|value| !value.is_empty()) {
        return found(PathBuf::from(value), OutputSource::Env);
    }
    if let Some(value) = preference.filter(|value| usable_output_dir(value)) {
        return found(PathBuf::from(value), OutputSource::Setting);
    }
    if let Some(path) = running_argv
        .and_then(output_dir_from_argv)
        .filter(|path| path.is_absolute() && path.is_dir())
    {
        return found(path, OutputSource::Running);
    }
    if let Some(path) = default_dir.filter(|path| path.is_dir()) {
        return found(path.to_path_buf(), OutputSource::Default);
    }
    OutputResolution { path: None, source: OutputSource::None }
}

static PREFERRED_OUTPUT: Mutex<Option<String>> = Mutex::new(None);
static RUNNING_ARGV: Mutex<Option<Vec<String>>> = Mutex::new(None);

fn locked<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Remembers the `comfyOutputDir` preference for [`comfy_output_resolution`] ('' = automatic).
pub fn set_preferred_output_dir(value: &str) {
    *locked(&PREFERRED_OUTPUT) = Some(value.to_owned());
}

/// Asks the running ComfyUI for its `sys.argv` (short timeouts, never an error) and
/// caches it; an unreachable ComfyUI clears the cache.
pub fn refresh_running_argv(port: u16) {
    let argv = crate::comfy::Comfy::loopback_with_timeout(
        port,
        std::time::Duration::from_millis(500),
        std::time::Duration::from_secs(2),
    )
    .and_then(|comfy| comfy.get("/system_stats"))
    .ok()
    .and_then(|stats| {
        stats["system"]["argv"].as_array().map(|items| {
            items.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect::<Vec<_>>()
        })
    });
    *locked(&RUNNING_ARGV) = argv;
}

/// Current resolution with the live inputs (env, remembered preference and ComfyUI argv).
pub fn comfy_output_resolution() -> OutputResolution {
    let env = std::env::var_os("SAYA_COMFY_OUTPUT_DIR").map(|v| v.to_string_lossy().into_owned());
    let preference = locked(&PREFERRED_OUTPUT).clone();
    let argv = locked(&RUNNING_ARGV).clone();
    let default_dir = crate::paths::comfyui_dir().ok().map(|dir| dir.join("output"));
    resolve_output_dir(env.as_deref(), preference.as_deref(), argv.as_deref(), default_dir.as_deref())
}

/// ComfyUI output folder, never hard-coded; `None` when no source yields one.
pub fn comfy_output_dir() -> Option<PathBuf> {
    comfy_output_resolution().path
}

/// Canonical engine families (`moteur/registre_ids.py:41`) + legacy `ch`.
const FAMILIES: [&str; 16] = [
    "hum", "bny", "kmn", "slm", "lcs", "cat", "dem", "elf", "cute", "edgy", "hor", "ard", "div",
    "sop", "lra", "ch",
];

/// Bounds of the walk: depth and volume capped so a listing never
/// turns into an endless scan.
const MAX_DEPTH: usize = 4;
const MAX_DIRS: usize = 4_000;
const MAX_IMAGES: usize = 50_000;

/// A listable root: kind (for the UI filter), label and path.
pub struct Root {
    pub kind: &'static str,
    pub label: &'static str,
    pub path: PathBuf,
}

/// The three browser roots. `app_results` = `<app_data>/results`,
/// the same folder as the one passed to `saya_status`/`sync_jobs`.
/// `captures`: the configured capture root, where deliveries are organized (the
/// results folder is then only the buffer of files that could not be organized).
pub fn roots(app_results: &Path, captures: Option<&Path>) -> Vec<Root> {
    let mut roots = vec![Root {
        kind: "moteur",
        label: "Moteur generez · sortie",
        path: engine_dir().join("sortie"),
    }];
    if let Some(captures) = captures {
        roots.push(Root {
            kind: "app",
            label: "Images livrées",
            path: captures.to_path_buf(),
        });
    }
    roots.push(Root {
        kind: "app",
        label: "Résultats de l'app",
        path: app_results.to_path_buf(),
    });
    // No ComfyUI output folder found: the copies root is simply absent.
    if let Some(output) = comfy_output_dir() {
        roots.insert(
            1,
            Root {
                kind: "comfyui",
                label: "Copies ComfyUI · output/saya",
                path: output.join("saya"),
            },
        );
    }
    roots
}

/// Family read from the file stem (`bny-00001-…` / `lcs--0004-…`), `_PREFIXE`
/// convention of the engine (`registre_ids.py:44`). `None` if no known prefix:
/// no family is ever invented.
fn family_from_stem(stem: &str) -> Option<String> {
    let lettres: String = stem
        .chars()
        .take_while(|c| c.is_ascii_lowercase())
        .collect();
    let rest = stem.strip_prefix(lettres.as_str())?;
    let chiffres = rest.trim_start_matches('-');
    // One or two dashes before the number ("bny-00001", "lcs--0004"),
    // `_PREFIXE` convention of the engine (`registre_ids.py:44`).
    if FAMILIES.contains(&lettres.as_str())
        && matches!(rest.len() - chiffres.len(), 1..=2)
        && chiffres.starts_with(|c: char| c.is_ascii_digit())
    {
        Some(lettres)
    } else {
        None
    }
}

/// Number in JSON: literal as is, never guessed.
fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        autre => autre.to_string(),
    }
}

/// LoRA names from an engine JSON value: list of `{name}` objects (chunk
/// `saya_family.loras`, written by SayaFamilySave) or literal catalogue field
/// (`action_lora` text, `pose_loras` list, `other_lora`/`stack_lora`/`cute_lora`
/// objects — same shapes as `sortie/dry-run/catalogue.json`). An empty or
/// missing name is never invented.
fn lora_names(value: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut pousser = |v: &Value| {
        let nom = v
            .as_str()
            .or_else(|| v.get("name").and_then(Value::as_str))
            .unwrap_or_default();
        if !nom.is_empty() && !out.iter().any(|e| e == nom) {
            out.push(nom.to_owned());
        }
    };
    match value {
        Some(Value::Array(list)) => {
            for v in list {
                pousser(v);
            }
        }
        // The catalogue's `action_lora` is a bare string; the `loras` chunk a list
        // of objects; `other_lora`/`stack_lora`/`cute_lora` an object.
        Some(v @ (Value::String(_) | Value::Object(_))) => pousser(v),
        _ => {}
    }
    out
}

/// LoRAs carried by a batch catalogue entry: the shot's LoRA fields,
/// literal as is — never deduced from prompt text.
fn catalogue_loras(entry: &Value) -> Vec<String> {
    let mut out = lora_names(entry.get("pose_loras"));
    for field in ["action_lora", "other_lora", "stack_lora", "cute_lora"] {
        for nom in lora_names(entry.get(field)) {
            if !out.contains(&nom) {
                out.push(nom);
            }
        }
    }
    out
}

/// Short model read from the file name (`<stem>-<modele>_<compteur:05>_.png`,
/// SayaFamilySave naming, see the node) — only `noobai`/`illustrious`,
/// never guessed beyond that; `None` otherwise.
fn model_from_stem(stem: &str) -> Option<&'static str> {
    // The final ComfyUI counter `_NNNNN_` is removed if present; without a counter,
    // the model suffix may be glued directly to the stem (renamed copies).
    let sans_compteur = match stem.strip_suffix('_').and_then(|t| t.rfind('_')) {
        Some(pos) => {
            let chiffres = &stem[pos + 1..stem.len() - 1];
            if !chiffres.is_empty() && chiffres.chars().all(|c| c.is_ascii_digit()) {
                &stem[..pos]
            } else {
                stem
            }
        }
        None => stem,
    };
    if sans_compteur.ends_with("-noobai") {
        return Some("noobai");
    }
    if sans_compteur.ends_with("-illustrious") {
        return Some("illustrious");
    }
    None
}

/// Chunk tEXt/zTXt/iTXt `saya_family` of the PNG (written by SayaFamilySave,
/// see `moteur/comfy.py:463`), parsed JSON, or `None`.
fn chunk_saya_family(file_path: &Path) -> Option<Value> {
    let file = std::fs::File::open(file_path).ok()?;
    let lecteur = Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .ok()?;
    text_chunks(lecteur.info())
        .into_iter()
        .find(|(k, _)| k == "saya_family")
        .and_then(|(_, v)| serde_json::from_str::<Value>(&v).ok())
}

/// Normalized stem: the model suffix is removed on both sides (the
/// SayaFamilySave node does the same, `moteur/comfy.py:484`).
fn normalized_stem(stem: &str) -> &str {
    stem.strip_suffix("-noobai")
        .or_else(|| stem.strip_suffix("-illustrious"))
        .unwrap_or(stem)
}

/// A usable entry of a `catalogue.json`: stem, family, seed, name, short
/// model (`model` of the engine shot: noobai/illustrious, empty if absent) and the carried
/// LoRAs (literal fields). A catalogue of another shape is simply ignored.
#[derive(Clone)]
struct CatalogueEntry {
    stem: String,
    family: String,
    seed: String,
    nom: String,
    model: String,
    loras: Vec<String>,
}

/// Searched in the folder then up to 3 parents within the root (the engine batch
/// writes its catalogue at the run root, the images in <famille>/).
fn folder_catalogue(folder: &Path, root: &Path) -> Vec<CatalogueEntry> {
    let mut file_path = folder.to_path_buf();
    for _ in 0..=3 {
        if let Ok(raw) = std::fs::read(file_path.join("catalogue.json")) {
            return serde_json::from_slice::<Value>(&raw)
                .ok()
                .and_then(|v| v.as_array().cloned())
                .map(|list| {
                    list
                        .iter()
                        .filter_map(|e| {
                            Some(CatalogueEntry {
                                stem: e.get("stem")?.as_str()?.to_string(),
                                family: e.get("famille")?.as_str()?.to_string(),
                                seed: e.get("seed").map(text).unwrap_or_default(),
                                nom: e
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                model: e
                                    .get("model")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                loras: catalogue_loras(e),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        if file_path == root || !file_path.pop() {
            return Vec::new();
        }
    }
    Vec::new()
}

/// Attribution of an image: `saya_family` chunk first, batch catalogue
/// next, stem prefix last; each field carries ITS source ("chunk" /
/// "catalogue" / "stem" / "aucune") so the UI stays honest about
/// the origin. No field is ever invented: absent from the sources = `None`/empty.
struct Attribution {
    family: Option<String>,
    source: &'static str,
    character: Option<String>,
    seed: Option<String>,
    short_model: Option<String>,
    model_source: &'static str,
    loras: Vec<String>,
    loras_source: &'static str,
}

fn attribution(file_path: &Path, catalogue: &[CatalogueEntry]) -> Attribution {
    let stem = file_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if let Some(chunk) = chunk_saya_family(file_path) {
        let loras = lora_names(chunk.get("loras"));
        return Attribution {
            family: chunk
                .get("famille")
                .and_then(Value::as_str)
                .map(str::to_owned),
            source: "chunk",
            character: chunk
                .get("perso_id")
                .and_then(Value::as_str)
                .map(str::to_owned),
            seed: chunk.get("seed").map(text),
            short_model: chunk
                .get("modele")
                .and_then(Value::as_str)
                .filter(|m| !m.is_empty())
                .map(str::to_owned),
            model_source: "chunk",
            loras_source: if loras.is_empty() { "aucune" } else { "chunk" },
            loras,
        };
    }
    let normalized = normalized_stem(&stem);
    if let Some(entry) = catalogue
        .iter()
        .filter(|e| normalized.starts_with(normalized_stem(&e.stem)))
        .max_by_key(|e| e.stem.len())
    {
        return Attribution {
            family: Some(entry.family.clone()),
            source: "catalogue",
            character: if entry.nom.is_empty() {
                None
            } else {
                Some(entry.nom.clone())
            },
            seed: if entry.seed.is_empty() {
                None
            } else {
                Some(entry.seed.clone())
            },
            short_model: if entry.model.is_empty() {
                None
            } else {
                Some(entry.model.clone())
            },
            model_source: "catalogue",
            loras_source: if entry.loras.is_empty() {
                "aucune"
            } else {
                "catalogue"
            },
            loras: entry.loras.clone(),
        };
    }
    Attribution {
        family: family_from_stem(&stem),
        source: "stem",
        character: None,
        seed: None,
        short_model: model_from_stem(&stem).map(str::to_owned),
        model_source: "stem",
        loras: Vec::new(),
        loras_source: "aucune",
    }
}

fn millis(modified: std::io::Result<std::time::SystemTime>) -> u64 {
    modified
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Lists all the images of a root, grouped by container folder,
/// each folder sorted from oldest to most recent (mtime, name as tie-break):
/// "from the first made to the last". The walk only does `read_dir`
/// and `metadata` — nothing is ever written.
pub fn list_roots(racines: &[Root]) -> Result<Value> {
    let mut roots_json = Vec::new();
    let mut folders = Vec::new();
    let mut total = 0usize;
    for root in racines {
        let existe = root.path.is_dir();
        roots_json.push(json!({
            "kind": root.kind, "label": root.label,
            "path": root.path.to_string_lossy(), "exists": existe,
        }));
        if !existe {
            continue;
        }
        let canonique = root.path.canonicalize()?;
        let mut a_voir = vec![(canonique.clone(), 0usize)];
        let mut visites = 0usize;
        while let Some((folder, profondeur)) = a_voir.pop() {
            visites += 1;
            if visites > MAX_DIRS || total >= MAX_IMAGES {
                break;
            }
            let mut images: Vec<PathBuf> = Vec::new();
            for entry in std::fs::read_dir(&folder)? {
                let entry = entry?;
                let file_path = entry.path();
                let nom = entry.file_name().to_string_lossy().into_owned();
                if file_path.is_dir() {
                    if profondeur < MAX_DEPTH && !nom.starts_with('.') && nom != "_backups" {
                        a_voir.push((file_path, profondeur + 1));
                    }
                } else if file_path.extension().and_then(|e| e.to_str()) == Some("png") {
                    images.push(file_path);
                }
            }
            if images.is_empty() {
                continue;
            }
            let catalogue = folder_catalogue(&folder, &canonique);
            let mut line = Vec::new();
            for file_path in &images {
                if total >= MAX_IMAGES {
                    break;
                }
                total += 1;
                let a = attribution(file_path, &catalogue);
                line.push(json!({
                    "path": file_path.to_string_lossy(),
                    "name": file_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
                    "modifiedMs": millis(file_path.metadata().and_then(|m| m.modified())),
                    "famille": a.family,
                    "familleSource": a.source,
                    "perso": a.character,
                    "seed": a.seed,
                    "modele": a.short_model,
                    "modeleSource": a.model_source,
                    "loras": a.loras,
                    "lorasSource": a.loras_source,
                }));
            }
            line.sort_by_key(|i| {
                (
                    i["modifiedMs"].as_u64().unwrap_or(0),
                    i["name"].as_str().unwrap_or_default().to_string(),
                )
            });
            let rel = folder
                .strip_prefix(&canonique)
                .map(|r| r.to_string_lossy().into_owned())
                .unwrap_or_default();
            folders.push(json!({
                "root": root.kind,
                "path": folder.to_string_lossy(),
                "label": if rel.is_empty() { root.label.to_string() } else { rel },
                "imageCount": line.len(),
                "images": line,
            }));
        }
    }
    folders.sort_by_key(|f| {
        (
            match f["root"].as_str().unwrap_or_default() {
                "moteur" => 0,
                "comfyui" => 1,
                _ => 2,
            },
            f["label"].as_str().unwrap_or_default().to_string(),
        )
    });
    Ok(json!({"roots": roots_json, "folders": folders, "totalImages": total}))
}

/// Displays a listed image: `canonicalize` + `starts_with` (same jail as
/// `gallery::asset_path`) — impossible to read outside the browser roots,
/// and only PNGs are. The rendering goes through the app's thumbnail
/// cache (`gallery::image_data`), never written into a listed folder.
pub fn read_image(file_path: &Path, racines: &[Root], cache: &Path, apercu: bool) -> Result<String> {
    let file = file_path.canonicalize()?;
    if file.extension().and_then(|e| e.to_str()) != Some("png") {
        return Err("Seules les images PNG du navigateur sont lisibles".into());
    }
    let dedans = racines
        .iter()
        .filter(|r| r.path.is_dir())
        .any(|r| crate::fsutil::ensure_within(&r.path, &file).is_ok());
    if !dedans {
        return Err("Chemin hors des dossiers du navigateur de générations".into());
    }
    crate::gallery::image_data(&file, cache, apercu)
}

#[cfg(test)]
mod tests {

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn argv_with_and_without_output_directory() {
        let explicit = args(&["/c/main.py", "--output-directory", "/data/out", "--port", "1"]);
        assert_eq!(output_dir_from_argv(&explicit), Some(PathBuf::from("/data/out")));
        let equals = args(&["/c/main.py", "--output-directory=/data/eq"]);
        assert_eq!(output_dir_from_argv(&equals), Some(PathBuf::from("/data/eq")));
        let plain = args(&["/c/main.py", "--listen"]);
        assert_eq!(output_dir_from_argv(&plain), Some(PathBuf::from("/c/output")));
        let found_later = args(&["python", "/c/main.py"]);
        assert_eq!(output_dir_from_argv(&found_later), Some(PathBuf::from("/c/output")));
        assert_eq!(output_dir_from_argv(&args(&["main.py"])), None);
        assert_eq!(output_dir_from_argv(&args(&["other.py"])), None);
    }

    #[test]
    fn resolution_order_and_fallbacks() {
        let tmp = TempDir::new("resolve");
        let mk = |name: &str| {
            let dir = tmp.path().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        };
        let (pref, run, default) = (mk("pref"), mk("run/output"), mk("def"));
        let pref_s = pref.to_string_lossy().into_owned();
        let argv = args(&[&tmp.path().join("run/main.py").to_string_lossy()]);
        let explicit = args(&["main.py", "--output-directory", &pref_s]);

        let r = resolve_output_dir(Some("/env/x"), Some(&pref_s), Some(&argv), Some(&default));
        assert_eq!((r.source, r.path), (OutputSource::Env, Some(PathBuf::from("/env/x"))));
        let r = resolve_output_dir(Some(""), Some(&pref_s), Some(&argv), Some(&default));
        assert_eq!((r.source, r.path), (OutputSource::Setting, Some(pref.clone())));
        // Invalid preferences are ignored: relative, missing, empty.
        for bad in ["relative/dir", "/definitely/missing/dir", ""] {
            let r = resolve_output_dir(None, Some(bad), Some(&argv), Some(&default));
            assert_eq!((r.source, r.path), (OutputSource::Running, Some(run.clone())), "{bad}");
        }
        let r = resolve_output_dir(None, None, Some(&explicit), Some(&default));
        assert_eq!((r.source, r.path), (OutputSource::Running, Some(pref.clone())));
        // Running ComfyUI pointing at a missing folder falls through to the default.
        let ghost = args(&["/missing/main.py"]);
        let r = resolve_output_dir(None, None, Some(&ghost), Some(&default));
        assert_eq!((r.source, r.path), (OutputSource::Default, Some(default.clone())));
        let r = resolve_output_dir(None, None, None, Some(&tmp.path().join("nope")));
        assert_eq!((r.source, r.path), (OutputSource::None, None));
        assert_eq!(resolve_output_dir(None, None, None, None).source, OutputSource::None);
    }
    use super::*;
    use std::io::Write;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(nom: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "studio-browser-test-{nom}-{}",
                uuid::Uuid::new_v4().simple()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Minimal 2×2 PNG with arbitrary tEXt chunks (same writing as
    /// `tests/index_recovery.rs`).
    fn png_with_chunks(file_path: &Path, chunks: &[(&str, String)]) {
        let mut encoder = png::Encoder::new(std::fs::File::create(file_path).unwrap(), 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        for (mot, text) in chunks {
            encoder
                .add_text_chunk((*mot).into(), text.clone())
                .unwrap();
        }
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[10; 12])
            .unwrap();
    }

    /// Explicit mtime: the listing order must owe nothing to the file
    /// write order.
    fn set_mtime(file_path: &Path, seconds: u64) {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(file_path)
            .unwrap();
        file
            .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(seconds))
            .unwrap();
        let _ = file.write(&[]);
    }

    #[test]
    fn family_chunk_then_catalogue_then_stem() {
        let root = TempDir::new("famille");
        let run = root.path().join("2026-10-25_120000");
        let batch = run.join("bny");
        std::fs::create_dir_all(&batch).unwrap();
        std::fs::write(
            run.join("catalogue.json"),
            r#"[{"stem":"kmn-00002-zed","famille":"kmn","seed":42,"name":"Zed"},{"stem":"lcs--0007-ame","famille":"lcs","seed":7,"name":"Ame"}]"#,
        )
        .unwrap();
        png_with_chunks(
            &batch.join("kmn-00002-zed-noobai_00001_.png"),
            &[(
                "saya_family",
                r#"{"famille":"bny","perso_id":"kmn-00002-zed","seed":"99"}"#.into(),
            )],
        );
        png_with_chunks(&batch.join("lcs--0007-ame-noobai_00002_.png"), &[]);
        png_with_chunks(&batch.join("edgy--0009-nyx_00003_.png"), &[]);
        png_with_chunks(&batch.join("sans-attribution.png"), &[]);
        let racines = vec![Root {
            kind: "moteur",
            label: "L",
            path: root.path().to_path_buf(),
        }];
        let value = list_roots(&racines).unwrap();
        let dossiers = value["folders"].as_array().unwrap();
        assert_eq!(dossiers.len(), 1, "un seul dossier conteneur");
        assert_eq!(dossiers[0]["imageCount"], 4);
        let images = dossiers[0]["images"].as_array().unwrap();
        let by_name = |nom: &str| {
            images
                .iter()
                .find(|i| i["name"].as_str() == Some(nom))
                .unwrap_or_else(|| panic!("image absente : {nom}"))
        };
        let chunk = by_name("kmn-00002-zed-noobai_00001_.png");
        assert_eq!(chunk["famille"], "bny", "le chunk gagne sur le catalogue");
        assert_eq!(chunk["familleSource"], "chunk");
        assert_eq!(chunk["perso"], "kmn-00002-zed");
        let catalogue = by_name("lcs--0007-ame-noobai_00002_.png");
        assert_eq!(catalogue["famille"], "lcs");
        assert_eq!(catalogue["familleSource"], "catalogue");
        assert_eq!(catalogue["seed"], "7");
        let stem = by_name("edgy--0009-nyx_00003_.png");
        assert_eq!(stem["famille"], "edgy");
        assert_eq!(stem["familleSource"], "stem");
        let rien = by_name("sans-attribution.png");
        assert!(rien["famille"].is_null(), "pas de famille inventée");
    }

    #[test]
    fn model_and_loras_chunk_then_catalogue_then_stem() {
        let root = TempDir::new("modele");
        let run = root.path().join("2026-10-25_130000");
        let batch = run.join("bny");
        std::fs::create_dir_all(&batch).unwrap();
        // Batch catalogue: model + literal LoRA fields (same shapes as the
        // real engine catalogue: action_lora text, pose_loras list, other_lora object).
        std::fs::write(
            run.join("catalogue.json"),
            json!([
                {"stem": "lcs--0007-ame", "famille": "lcs", "seed": 7, "name": "Ame",
                 "model": "illustrious", "action_lora": "otoko no ko_illustrious_V2.0",
                 "pose_loras": [{"name": "pole dancing_illustrious_V2.0", "strength": 0.6}],
                 "other_lora": {"name": "naked ribbon_illustrious_V2.0", "strength": 0.5}},
                {"stem": "bny-00008-quilla", "famille": "bny", "seed": 8, "name": "Quilla",
                 "model": "noobai", "cute_lora": {"name": "cute_v1.5"}}
            ])
            .to_string(),
        )
        .unwrap();
        // 1. Chunk wins over the catalogue: modele AND loras come from the chunk.
        png_with_chunks(
            &batch.join("lcs--0007-ame-noobai_00001_.png"),
            &[(
                "saya_family",
                json!({"famille": "lcs", "modele": "noobai",
                       "loras": [{"name": "cute_v1.5"}, {"name": "cute_v1.5"}, {"strength": 0.5}],
                       "checkpoints": ["NoobAI/no tags/MKLAN POISONED NOOB AI FUTA_00001_.safetensors"]})
                .to_string(),
            )],
        );
        // 2. Without a chunk: catalogue → model illustrious, LoRAs from the shot fields,
        //    deduplicated (pose_loras + other_lora carry distinct names).
        png_with_chunks(&batch.join("lcs--0007-ame-illustrious_00002_.png"), &[]);
        // 3. Outside the catalogue: stem suffix (with ComfyUI counter) → noobai, no LoRA.
        png_with_chunks(&batch.join("edgy--0009-nyx-noobai_00003_.png"), &[]);
        // 4. Without counter, suffix or catalogue: no model invented.
        png_with_chunks(&batch.join("cat-00010-mira_00004_.png"), &[]);
        let racines = vec![Root {
            kind: "moteur",
            label: "L",
            path: root.path().to_path_buf(),
        }];
        let value = list_roots(&racines).unwrap();
        let images = value["folders"][0]["images"].as_array().unwrap();
        let by_name = |nom: &str| {
            images
                .iter()
                .find(|i| i["name"].as_str() == Some(nom))
                .unwrap_or_else(|| panic!("image absente : {nom}"))
        };
        let chunk = by_name("lcs--0007-ame-noobai_00001_.png");
        assert_eq!(chunk["modele"], "noobai", "le chunk gagne sur le catalogue");
        assert_eq!(chunk["modeleSource"], "chunk");
        assert_eq!(
            chunk["loras"].as_array().map(|l| l.as_slice()),
            Some([json!("cute_v1.5")].as_slice()),
            "loras du chunk, dédoublonnées, entrée sans name ignorée"
        );
        assert_eq!(chunk["lorasSource"], "chunk");
        let catalogue = by_name("lcs--0007-ame-illustrious_00002_.png");
        assert_eq!(
            catalogue["modele"], "illustrious",
            "model du catalogue quand le chunk absent"
        );
        assert_eq!(catalogue["modeleSource"], "catalogue");
        assert_eq!(
            catalogue["loras"].as_array().map(|l| l.len()),
            Some(3),
            "action_lora + pose_loras + other_lora lus tels quels"
        );
        assert_eq!(catalogue["lorasSource"], "catalogue");
        let stem = by_name("edgy--0009-nyx-noobai_00003_.png");
        assert_eq!(stem["modele"], "noobai", "suffixe du stem avec compteur");
        assert_eq!(stem["modeleSource"], "stem");
        assert!(
            stem["loras"].as_array().unwrap().is_empty(),
            "pas de LoRA inventée"
        );
        assert_eq!(stem["lorasSource"], "aucune");
        let rien = by_name("cat-00010-mira_00004_.png");
        assert!(rien["modele"].is_null(), "pas de modèle inventé");
        assert_eq!(rien["modeleSource"], "stem");
    }

    #[test]
    fn images_from_first_to_last() {
        let root = TempDir::new("chrono");
        let folder = root.path().join("lot");
        std::fs::create_dir_all(&folder).unwrap();
        png_with_chunks(&folder.join("b-recente.png"), &[]);
        png_with_chunks(&folder.join("a-ancienne.png"), &[]);
        set_mtime(&folder.join("b-recente.png"), 60);
        set_mtime(&folder.join("a-ancienne.png"), 600);
        let racines = vec![Root {
            kind: "app",
            label: "L",
            path: root.path().to_path_buf(),
        }];
        let value = list_roots(&racines).unwrap();
        let images = value["folders"][0]["images"].as_array().unwrap();
        let names: Vec<_> = images.iter().map(|i| i["name"].as_str().unwrap()).collect();
        assert_eq!(
            names,
            ["a-ancienne.png", "b-recente.png"],
            "première faite en premier, mtime croissant"
        );
        assert!(
            images[0]["modifiedMs"].as_u64().unwrap() < images[1]["modifiedMs"].as_u64().unwrap()
        );
    }

    #[test]
    fn read_refused_outside_roots_and_non_png() {
        let root = TempDir::new("jail");
        let folder = root.path().join("lot");
        std::fs::create_dir_all(&folder).unwrap();
        png_with_chunks(&folder.join("ok.png"), &[]);
        let dehors = TempDir::new("dehors");
        png_with_chunks(&dehors.path().join("fuite.png"), &[]);
        let racines = vec![Root {
            kind: "app",
            label: "L",
            path: root.path().to_path_buf(),
        }];
        let cache = TempDir::new("cache");
        let lu = read_image(&folder.join("ok.png"), &racines, cache.path(), false).unwrap();
        assert!(lu.starts_with("data:image/jpeg;base64,"));
        assert!(read_image(
            &dehors.path().join("fuite.png"),
            &racines,
            cache.path(),
            false
        )
        .is_err());
        let text = root.path().join("note.txt");
        std::fs::write(&text, "pas une image").unwrap();
        assert!(read_image(&text, &racines, cache.path(), false).is_err());
    }
}
