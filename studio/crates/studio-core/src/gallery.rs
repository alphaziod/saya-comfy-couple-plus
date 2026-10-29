use crate::{Database, Result};
use base64::Engine;
use rusqlite::{params, params_from_iter, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeSet, HashMap},
    fs::File,
    io::{BufRead, BufReader, Cursor},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

pub fn digest(bytes: &[u8]) -> String {
    crate::fsutil::sha256_hex(bytes)
}
pub fn read_json(path: &Path, max: u64) -> Result<Value> {
    if path.metadata()?.len() > max {
        return Err("Fichier JSON trop volumineux".into());
    }
    Ok(serde_json::from_reader(File::open(path)?)?)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Filters {
    #[serde(default)]
    pub availability: String,
    #[serde(default)]
    pub metadata_only: bool,
    #[serde(default)]
    pub checkpoint: String,
    #[serde(default)]
    pub checkpoints: Vec<String>,
    #[serde(default)]
    pub lora: String,
    #[serde(default)]
    pub character: String,
    #[serde(default)]
    pub characters: Vec<String>,
    #[serde(default)]
    pub variant: String,
    #[serde(default)]
    pub workflow: String,
    #[serde(default)]
    pub job: String,
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub offset: u32,
    #[serde(default)]
    pub loras: Vec<String>,
    #[serde(default)]
    pub lora_exact: bool,
    #[serde(default)]
    pub sort: String,
    #[serde(default)]
    pub metadata_state: String,
    #[serde(default)]
    pub duplicates: bool,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub review: String,
    /// Gallery view: '' (default) = passes 1 and finals only; 'all' =
    /// include the intermediate passes (2..N-1) of a multipass generation.
    #[serde(default)]
    pub capture: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: String,
    pub path: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub metadata_state: String,
    pub favorite: bool,
    pub note: String,
    pub review: String,
    pub content_sha256: Option<String>,
    /// Organized capture fields (migration 0028): NULL for imported assets
    /// or old deliveries, never invented.
    pub capture_role: Option<String>,
    pub capture_phase: Option<u32>,
    pub capture_day: Option<String>,
    pub capture_family: Option<String>,
}
#[derive(Serialize)]
pub struct GalleryPage {
    pub items: Vec<Asset>,
    pub total: u32,
}
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndexReport {
    pub cancelled: bool,
    pub error_count: u32,
    pub indexed: u32,
    pub unchanged: u32,
    pub errors: Vec<String>,
    pub characters: u32,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub width: u32,
    pub height: u32,
    pub text: Value,
    pub checkpoints: Vec<String>,
    pub loras: Vec<String>,
    pub warnings: Vec<String>,
}

struct IndexedFile {
    id: String,
    modified: Option<String>,
    size: Option<i64>,
    hashed: bool,
    library: String,
    relative: String,
}

pub fn resources(prompt: &Value) -> (Vec<String>, Vec<String>) {
    let mut checkpoints = BTreeSet::new();
    let mut loras = BTreeSet::new();
    if let Some(nodes) = prompt.as_object() {
        for node in nodes.values() {
            if let Some(inputs) = node.get("inputs").and_then(Value::as_object) {
                for (key, value) in inputs {
                    if key == "ckpt_name" {
                        if let Some(name) = value.as_str() {
                            checkpoints.insert(name.into());
                        }
                    }
                    if key == "lora_name"
                        && !(inputs.get("strength_model").and_then(Value::as_f64) == Some(0.0)
                            && inputs
                                .get("strength_clip")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0)
                                == 0.0)
                    {
                        if let Some(name) = value.as_str() {
                            loras.insert(name.into());
                        }
                    }
                    if key == "loras" {
                        let list = value.get("__value__").unwrap_or(value);
                        if let Some(list) = list.as_array() {
                            for l in list {
                                if l.get("active").and_then(Value::as_bool) == Some(false) {
                                    continue;
                                }
                                if let Some(name) = l.get("name").and_then(Value::as_str) {
                                    loras.insert(name.into());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    (
        checkpoints.into_iter().collect(),
        loras.into_iter().collect(),
    )
}
/// Saya pass of an image read from its own PNG metadata, for folders Studio did not deliver
/// itself (imported libraries, ComfyUI output): the `saya_role` chunk written by the Saya
/// save nodes (`{"role","phase","total"}`), otherwise the Saya phase node present in the
/// embedded API prompt (`SayaImageGenerationReview` = pass 1, `SayaImagePhase<N>Stop` = pass N).
/// Returns (capture role, pass); `None` when nothing identifies a Saya pass.
pub fn saya_role(text: &Value) -> Option<(&'static str, u32)> {
    if let Some(chunk) = text.get("saya_role").filter(|c| c.is_object()) {
        let phase = u32::try_from(chunk["phase"].as_u64()?).ok()?;
        let total = chunk["total"].as_u64().and_then(|t| u32::try_from(t).ok()).unwrap_or(6);
        let role = if chunk["role"] == "final" || phase >= total { "final" } else { "pull" };
        return Some((role, phase));
    }
    let prompt = text.get("prompt")?.as_object()?;
    let phase = prompt
        .values()
        .filter_map(|node| {
            let class = node["class_type"].as_str()?;
            if class == "SayaImageGenerationReview" {
                return Some(1);
            }
            class
                .strip_prefix("SayaImagePhase")?
                .strip_suffix("Stop")?
                .parse::<u32>()
                .ok()
        })
        .max()?;
    Some(if phase >= 6 { ("final", phase) } else { ("pull", phase) })
}

pub fn metadata(path: &Path) -> Result<Metadata> {
    let reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let (width, height) = reader.into_dimensions()?;
    let mut text = serde_json::Map::new();
    let mut warnings = Vec::new();
    if path
        .extension()
        .and_then(|v| v.to_str())
        .is_some_and(|v| v.eq_ignore_ascii_case("png"))
    {
        let mut decoder = png::Decoder::new(BufReader::new(File::open(path)?));
        decoder.set_limits(png::Limits {
            bytes: 8 * 1024 * 1024,
        });
        let reader = decoder.read_info()?;
        let info = reader.info();
        for item in &info.uncompressed_latin1_text {
            if item.text.len() <= 4_000_000 {
                text.insert(
                    item.keyword.clone(),
                    serde_json::from_str(&item.text).unwrap_or(json!(item.text)),
                );
            }
        }
        for item in &info.compressed_latin1_text {
            match item.get_text() {
                Ok(s) => {
                    text.insert(
                        item.keyword.clone(),
                        serde_json::from_str(&s).unwrap_or(json!(s)),
                    );
                }
                Err(_) => warnings.push("Texte PNG compressé illisible ou trop grand".into()),
            }
        }
        for item in &info.utf8_text {
            match item.get_text() {
                Ok(s) => {
                    text.insert(
                        item.keyword.clone(),
                        serde_json::from_str(&s).unwrap_or(json!(s)),
                    );
                }
                Err(_) => warnings.push("Texte PNG UTF-8 illisible ou trop grand".into()),
            }
        }
    }
    if text.get("prompt").is_some_and(|v| !v.is_object()) {
        warnings.push("Le champ prompt PNG n'est pas un graphe JSON".into());
    }
    let (checkpoints, loras) = resources(text.get("prompt").unwrap_or(&Value::Null));
    Ok(Metadata {
        width,
        height,
        text: Value::Object(text),
        checkpoints,
        loras,
        warnings,
    })
}
pub(crate) const ASSET_STATE: &str = "CASE WHEN a.catalog_removed=1 THEN 'removed' WHEN NOT EXISTS(SELECT 1 FROM libraries l WHERE l.enabled=1 AND substr(a.absolute_path,1,length(rtrim(l.root_path,'/'))+1)=rtrim(l.root_path,'/')||'/') THEN 'outside_roots' WHEN NOT EXISTS(SELECT 1 FROM libraries l WHERE l.enabled=1 AND l.scan_state!='inaccessible' AND substr(a.absolute_path,1,length(rtrim(l.root_path,'/'))+1)=rtrim(l.root_path,'/')||'/') THEN 'source_inaccessible' ELSE a.availability END";

fn gallery_order(sort: &str) -> Result<&'static str> {
    Ok(match sort {
        "" | "name_asc" => "a.relative_path ASC,a.id ASC",
        "name_desc" => "a.relative_path DESC,a.id DESC",
        "newest" => "length(a.modified_at) DESC,a.modified_at DESC,a.id DESC",
        "oldest" => "length(a.modified_at) ASC,a.modified_at ASC,a.id ASC",
        "favorites" => "a.favorite DESC,length(a.modified_at) DESC,a.modified_at DESC,a.id DESC",
        "accepted" => "CASE a.review WHEN 'accepted' THEN 0 WHEN 'unreviewed' THEN 1 ELSE 2 END,length(a.modified_at) DESC,a.modified_at DESC,a.id DESC",
        "resolution" => "coalesce(a.width,0)*coalesce(a.height,0) DESC,a.relative_path ASC,a.id ASC",
        "filesize" => "a.byte_size DESC,a.relative_path ASC,a.id ASC",
        // Assets without a linked character come last; ties keep the folder order.
        "character" => "(SELECT min(c.name) FROM asset_characters ac JOIN characters c ON c.id=ac.character_id WHERE ac.asset_id=a.id) IS NULL,(SELECT min(c.name) FROM asset_characters ac JOIN characters c ON c.id=ac.character_id WHERE ac.asset_id=a.id) COLLATE NOCASE,a.relative_path ASC,a.id ASC",
        // Stable shuffle: same order on every visit, so navigation and scrolling stay coherent.
        "shuffle" => "coalesce(a.content_sha256,a.id) ASC,a.id ASC",
        _ => return Err("Tri inconnu".into()),
    })
}

/// Shared WHERE clause for `gallery` and `gallery_window`.
fn gallery_clause(f: &Filters) -> Result<(String, Vec<rusqlite::types::Value>)> {
    if f.loras.len() > 32 || f.checkpoints.len() > 32 {
        return Err("Maximum 32 filtres modèles".into());
    }
    let state = if f.availability.is_empty() {
        "present"
    } else {
        &f.availability
    };
    if ![
        "present",
        "missing",
        "unreadable",
        "source_inaccessible",
        "outside_roots",
        "removed",
    ]
    .contains(&state)
    {
        return Err("État d’image inconnu".into());
    }
    // Single representative per path: the row with the minimal rowid. Same winning
    // row as the former `a.id=(SELECT d.id … ORDER BY d.rowid
    // LIMIT 1)` form, but answered by the assets_absolute_path index alone (the rowid
    // is the index payload): no row fetch in the table, whose inline
    // metadata_json weigh GiBs. Measured on a copy of the real database
    // (6107 assets): gallery_neighbors 787 ms → 12 ms with the index
    // covering assets_gallery_cover (migration 0028).
    let mut clause=format!(" WHERE ({ASSET_STATE})=? AND a.rowid=(SELECT min(d.rowid) FROM assets d WHERE d.absolute_path=a.absolute_path)");
    let mut values: Vec<rusqlite::types::Value> = vec![state.to_owned().into()];
    let mut checkpoints = f.checkpoints.clone();
    if !f.checkpoint.is_empty() && !checkpoints.contains(&f.checkpoint) {
        checkpoints.push(f.checkpoint.clone());
    }
    if !checkpoints.is_empty() {
        let marks = std::iter::repeat_n("EXISTS(SELECT 1 FROM asset_models am JOIN model_resources m ON m.id=am.model_id WHERE am.asset_id=a.id AND m.kind='checkpoint' AND m.name=?)", checkpoints.len()).collect::<Vec<_>>().join(" OR ");
        clause.push_str(&format!(" AND ({marks})"));
        for name in checkpoints {
            values.push(name.into());
        }
    }
    if !f.lora.is_empty() {
        clause.push_str(" AND EXISTS(SELECT 1 FROM asset_models am JOIN model_resources m ON m.id=am.model_id WHERE am.asset_id=a.id AND m.kind='lora' AND m.name=?)");
        values.push(f.lora.clone().into());
    }
    for name in &f.loras {
        clause.push_str(" AND EXISTS(SELECT 1 FROM asset_models am JOIN model_resources m ON m.id=am.model_id WHERE am.asset_id=a.id AND m.kind='lora' AND m.name=?)");
        values.push(name.clone().into());
    }
    if f.lora_exact {
        let names: BTreeSet<&str> = f
            .loras
            .iter()
            .map(String::as_str)
            .chain((!f.lora.is_empty()).then_some(f.lora.as_str()))
            .collect();
        clause.push_str(" AND (SELECT count(DISTINCT m.name) FROM asset_models am JOIN model_resources m ON m.id=am.model_id WHERE am.asset_id=a.id AND m.kind='lora')=?");
        values.push((names.len() as i64).into());
    }
    if f.duplicates {
        clause.push_str(" AND a.content_sha256 IS NOT NULL AND EXISTS(SELECT 1 FROM assets b JOIN libraries l ON l.id=b.library_id WHERE b.content_sha256=a.content_sha256 AND b.id!=a.id AND b.availability='present' AND l.enabled=1)");
    }
    if f.favorite {
        clause.push_str(" AND a.favorite=1");
    }
    if f.metadata_only {
        clause.push_str(" AND ((json_type(a.metadata_json,'$.text.parameters')='text' AND instr(json_extract(a.metadata_json,'$.text.parameters'),char(10)||'Steps: ')>0 AND (instr(json_extract(a.metadata_json,'$.text.parameters'),', Seed: ')>0 OR instr(json_extract(a.metadata_json,'$.text.parameters'),', CFG scale: ')>0)) OR EXISTS(SELECT 1 FROM job_outputs jo WHERE jo.asset_id=a.id) OR EXISTS(SELECT 1 FROM json_each(CASE WHEN json_type(a.metadata_json,'$.text.prompt')='object' THEN json_extract(a.metadata_json,'$.text.prompt') ELSE '{}' END) n WHERE n.type='object' AND CASE WHEN n.type='object' THEN json_type(n.value,'$.class_type')='text' AND (json_type(n.value,'$.inputs.steps') IS NOT NULL OR json_type(n.value,'$.inputs.text') IS NOT NULL OR json_type(n.value,'$.inputs.ckpt_name') IS NOT NULL OR json_type(n.value,'$.inputs.lora_name') IS NOT NULL OR json_type(n.value,'$.inputs.loras') IS NOT NULL) ELSE 0 END))");
    }
    if !f.metadata_state.is_empty() {
        if !["read", "absent", "invalid", "unknown"].contains(&f.metadata_state.as_str()) {
            return Err("État de métadonnées inconnu".into());
        }
        clause.push_str(" AND a.metadata_state=?");
        values.push(f.metadata_state.clone().into());
    }
    if !f.review.is_empty() {
        if !["unreviewed", "accepted", "rejected"].contains(&f.review.as_str()) {
            return Err("Validation inconnue".into());
        }
        clause.push_str(" AND a.review=?");
        values.push(f.review.clone().into());
    }
    // Main gallery (brief point 4): a multipass generation shows
    // PASS 1 or the FINAL image only; intermediate passes live in
    // the details and comparisons. Assets without a capture (imports, old
    // deliveries) stay visible. 'all' shows them all.
    match f.capture.as_str() {
        "" | "main" => {
            clause.push_str(" AND NOT (a.capture_role='pull' AND coalesce(a.capture_phase,0)>1)");
            // An imported pass 1 (not delivered by a Studio job) is hidden once the final
            // of the same lineage is indexed. Studio's own runs are compacted by `lineage`.
            // Computed once over the few pass-1 rows: referencing a.metadata_json in the
            // outer scan would read every row's inline metadata (GiBs) instead of the
            // covering index.
            clause.push_str(" AND a.id NOT IN (SELECT p.id FROM assets p WHERE p.capture_role='pull' AND p.capture_phase=1 AND json_extract(p.metadata_json,'$.text.saya_role.lineage') IS NOT NULL AND NOT EXISTS(SELECT 1 FROM job_outputs o WHERE o.asset_id=p.id) AND EXISTS(SELECT 1 FROM assets f WHERE json_extract(f.metadata_json,'$.text.saya_role.lineage')=json_extract(p.metadata_json,'$.text.saya_role.lineage') AND f.capture_role='final'))")
        }
        "all" => {}
        _ => return Err("Vue de capture inconnue".into()),
    }
    if !f.character.is_empty() {
        clause.push_str(
            " AND a.id IN (SELECT ac.asset_id FROM asset_characters ac WHERE ac.character_id=?)",
        );
        values.push(f.character.clone().into());
    }
    if f.characters.len() > 32 {
        return Err("Maximum 32 personnages par filtre".into());
    }
    for character in &f.characters {
        clause.push_str(
            " AND a.id IN (SELECT ac.asset_id FROM asset_characters ac WHERE ac.character_id=?)",
        );
        values.push(character.clone().into());
    }
    for (value,condition) in [(&f.variant,"EXISTS(SELECT 1 FROM job_outputs jo JOIN job_characters jc ON jc.job_id=jo.job_id WHERE jo.asset_id=a.id AND jc.variant_id=?)"),(&f.workflow,"EXISTS(SELECT 1 FROM job_outputs jo JOIN generation_jobs j ON j.id=jo.job_id WHERE jo.asset_id=a.id AND j.workflow_revision_id=?)"),(&f.job,"EXISTS(SELECT 1 FROM job_outputs jo WHERE jo.asset_id=a.id AND jo.job_id=?)")] {
        if !value.is_empty() {clause.push_str(" AND ");clause.push_str(condition);values.push(value.clone().into());}
    }
    if !f.search.is_empty() {
        clause.push_str(" AND instr(lower(a.relative_path),lower(?))>0");
        values.push(f.search.clone().into());
    }
    Ok((clause, values))
}
fn visit(
    dir: &Path,
    files: &mut Vec<PathBuf>,
    journals: &mut Vec<PathBuf>,
    errors: &mut Vec<String>,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        if cancelled() {
            return Ok(());
        }
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(e.to_string());
                continue;
            }
        };
        let p = entry.path();
        let kind = match entry.file_type() {
            Ok(kind) => kind,
            Err(error) => {
                errors.push(format!("{} : {error}", p.display()));
                continue;
            }
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if let Err(e) = visit(&p, files, journals, errors, cancelled) {
                errors.push(format!("{} : {e}", p.display()));
            }
        } else if kind.is_file() {
            if p.file_name().is_some_and(|v| v == "journal.jsonl") {
                journals.push(p.clone());
            }
            if p.extension().and_then(|s| s.to_str()).is_some_and(|s| {
                ["png", "jpg", "jpeg", "webp"].contains(&s.to_ascii_lowercase().as_str())
            }) {
                files.push(p);
            }
        }
    }
    Ok(())
}
impl Database {
    pub fn index_library(&mut self, root: &Path) -> Result<IndexReport> {
        self.index_library_progress(root, |_, _| {})
    }
    pub fn index_library_progress(
        &mut self,
        root: &Path,
        progress: impl FnMut(u32, u32),
    ) -> Result<IndexReport> {
        self.index_library_controlled(root, progress, || false)
    }
    pub fn index_library_controlled(
        &mut self,
        root: &Path,
        mut progress: impl FnMut(u32, u32),
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<IndexReport> {
        let _lock = self.index_lock_waiting(std::time::Duration::from_secs(15))?;
        let root = match crate::library_sources::checked_directory(root) {
            Ok(path) => path,
            Err(error) => {
                self.connection.execute("UPDATE libraries SET scan_state='inaccessible',scan_error=?2 WHERE root_path=?1",params![root.to_string_lossy(),error.to_string()])?;
                return Err(error);
            }
        };
        let run = self.start_index_run(&root)?;
        let result = self.index_library_run(&root, run, &mut progress, &mut cancelled);
        if let Err(error) = &result {
            self.index_error(run, &root, &error.to_string())?;
        }
        self.finish_index_run(
            run,
            match &result {
                Ok(report) if report.cancelled => "interrupted",
                Ok(_) => "completed",
                Err(_) => "failed",
            },
        )?;
        let (state, error) = match &result {
            Ok(r) if r.cancelled => ("interrupted", None),
            Ok(r) if r.error_count > 0 => ("partial", Some(r.errors.join("\n"))),
            Ok(_) => ("ready", None),
            Err(e) => ("inaccessible", Some(e.to_string())),
        };
        self.connection.execute(
            "UPDATE libraries SET scan_state=?2,scan_error=?3 WHERE root_path=?1",
            params![root.to_string_lossy(), state, error],
        )?;
        result
    }
    fn index_library_run(
        &mut self,
        root: &Path,
        run: i64,
        progress: &mut impl FnMut(u32, u32),
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<IndexReport> {
        let root_text = root.to_str().ok_or("Chemin non UTF-8")?;
        let library: Option<String> = self
            .connection
            .query_row(
                "SELECT id FROM libraries WHERE root_path=?",
                [root_text],
                |r| r.get(0),
            )
            .optional()?;
        let library = library.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.connection.execute("INSERT INTO libraries(id,label,root_path,platform) VALUES(?1,?2,?2,?3) ON CONFLICT(root_path) DO NOTHING",params![library,root_text,if cfg!(target_os="windows"){"windows"}else if cfg!(target_os="macos"){"macos"}else{"linux"}])?;
        let mut report = IndexReport::default();
        let source_identity = std::fs::metadata(root)?;
        let mut files = Vec::new();
        let mut journals = Vec::new();
        visit(
            root,
            &mut files,
            &mut journals,
            &mut report.errors,
            cancelled,
        )?;
        if cancelled() {
            report.cancelled = true;
            return Ok(report);
        }
        for error in &report.errors {
            self.index_error(run, root, error)?;
        }
        report.error_count = report.errors.len() as u32;
        files.sort();
        let scan_complete = report.errors.is_empty();
        let present: BTreeSet<_> = files
            .iter()
            .filter_map(|p| p.to_str().map(str::to_owned))
            .collect();
        let total = files.len() as u32;
        self.index_checkpoint(run, 0, total, None, &report)?;
        progress(0, total);
        let catalogue_path = root.join("characters_ready.json");
        let catalogue = match read_json(&catalogue_path, 32_000_000) {
            Ok(value) => value,
            Err(error) if catalogue_path.exists() => {
                self.index_error(run, &catalogue_path, &error.to_string())?;
                report.error_count += 1;
                if report.errors.len() < 30 {
                    report
                        .errors
                        .push(format!("{} : {error}", catalogue_path.display()));
                }
                Value::Null
            }
            Err(_) => Value::Null,
        };
        let catalogue_hash = digest(catalogue.to_string().as_bytes());
        let characters: HashMap<String, Value> = catalogue
            .get("characters")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| {
                v.get("id")
                    .and_then(Value::as_str)
                    .map(|k| (k.into(), v.clone()))
            })
            .collect();
        let mut links = HashMap::new();
        for path in journals {
            let result = (|| -> Result<()> {
                if path.metadata()?.len() > 64_000_000 {
                    return Err("Journal trop volumineux".into());
                }
                for (number, line) in BufReader::new(File::open(&path)?).lines().enumerate() {
                    let parsed = (|| -> Result<Value> {
                        let line = line?;
                        if line.len() > 1_000_000 {
                            return Err("Ligne trop volumineuse".into());
                        }
                        if line.trim().is_empty() {
                            return Ok(Value::Null);
                        }
                        Ok(serde_json::from_str(&line)?)
                    })();
                    let v = match parsed {
                        Ok(value) => value,
                        Err(error) => {
                            let error = format!("Ligne {} : {error}", number + 1);
                            self.index_error(run, &path, &error)?;
                            report.error_count += 1;
                            if report.errors.len() < 30 {
                                report.errors.push(format!("{} : {error}", path.display()));
                            }
                            continue;
                        }
                    };
                    if v["event"] == "saved" {
                        if let (Some(image), Some(id)) = (v["image"].as_str(), v["id"].as_str()) {
                            if let Ok(p) = Path::new(image).canonicalize() {
                                if p.starts_with(root) {
                                    links.insert(p, id.to_string());
                                }
                            }
                        }
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.index_error(run, &path, &error.to_string())?;
                report.error_count += 1;
                if report.errors.len() < 30 {
                    report.errors.push(format!("{} : {error}", path.display()));
                }
            }
        }
        let mut imported: HashMap<String, String> = HashMap::new();
        for (position, path) in files.into_iter().enumerate() {
            if cancelled() {
                report.cancelled = true;
                report.characters = imported.len() as u32;
                return Ok(report);
            }
            let result = (|| -> Result<()> {
                let canonical = path.canonicalize()?;
                if !canonical.starts_with(root) {
                    return Err("Image hors bibliothèque".into());
                }
                let relative = path
                    .strip_prefix(root)?
                    .to_str()
                    .ok_or("Chemin non UTF-8")?;
                let stat = path.metadata()?;
                let modified = stat
                    .modified()?
                    .duration_since(UNIX_EPOCH)?
                    .as_nanos()
                    .to_string();
                let old: Option<IndexedFile> = self.connection.query_row(
                    "SELECT id,modified_at,byte_size,content_sha256 IS NOT NULL,library_id,relative_path FROM assets WHERE absolute_path=?1 ORDER BY rowid LIMIT 1",
                    [canonical.to_str().ok_or("Chemin non UTF-8")?],
                    |r| Ok(IndexedFile { id:r.get(0)?, modified:r.get(1)?,size:r.get(2)?,hashed:r.get(3)?,library:r.get(4)?,relative:r.get(5)? }),
                ).optional()?;
                let (library, relative) = old
                    .as_ref()
                    .map(|r| (r.library.as_str(), r.relative.as_str()))
                    .unwrap_or((&library, relative));
                let unchanged = old.as_ref().is_some_and(|r| {
                    r.modified.as_deref() == Some(&modified)
                        && r.size == Some(stat.len() as i64)
                        && r.hashed
                });
                let id = old
                    .as_ref()
                    .map(|r| r.id.clone())
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                if !unchanged {
                    let meta = metadata(&path)?;
                    let hash = file_digest(&path)?;
                    // Reject a torn read if a writer changed the file during metadata/hash extraction.
                    let after = path.metadata()?;
                    if stat.len() != after.len() || stat.modified()? != after.modified()? {
                        return Err("Image modifiée pendant l'indexation ; réessayer".into());
                    }
                    // Incremental vectorization on add: a decoding failure
                    // (truncated file, limits exceeded) does not prevent
                    // indexing; the image stays listed without a vector for visual
                    // search and can be retried later.
                    let features = crate::visual_search::extract_file(&path).ok();
                    let state = if !meta.warnings.is_empty() {
                        "invalid"
                    } else if meta.text.get("prompt").is_none()
                        && meta.text.get("workflow").is_none()
                        && !crate::generation_metadata::is_parameters(&meta.text["parameters"])
                    {
                        "absent"
                    } else {
                        "read"
                    };
                    let tx = self.connection.transaction()?;
                    tx.execute("INSERT INTO assets(id,library_id,relative_path,byte_size,modified_at,availability,metadata_json,metadata_state,width,height,content_sha256) VALUES(?1,?2,?3,?4,?5,'present',?6,?7,?8,?9,?10) ON CONFLICT(library_id,relative_path) DO UPDATE SET byte_size=excluded.byte_size,modified_at=excluded.modified_at,availability='present',metadata_json=excluded.metadata_json,metadata_state=excluded.metadata_state,width=excluded.width,height=excluded.height,content_sha256=excluded.content_sha256",params![id,library,relative,stat.len() as i64,modified,serde_json::to_string(&meta)?,state,meta.width,meta.height,hash])?;
                    // Imported Saya passes: the image's own metadata says which pass it is,
                    // so intermediate passes stay out of the main gallery view.
                    if let Some((role, phase)) = saya_role(&meta.text) {
                        tx.execute("UPDATE assets SET capture_role=?3,capture_phase=?4 WHERE library_id=?1 AND relative_path=?2 AND capture_role IS NULL",params![library,relative,role,phase])?;
                    }
                    tx.execute(
                        "DELETE FROM asset_models WHERE asset_id=? AND evidence='embedded'",
                        [&id],
                    )?;
                    for (kind, names) in [("checkpoint", &meta.checkpoints), ("lora", &meta.loras)]
                    {
                        for name in names {
                            let model_id = digest(format!("{kind}:{name}").as_bytes());
                            tx.execute("INSERT INTO model_resources(id,kind,name) VALUES(?1,?2,?3) ON CONFLICT DO NOTHING",params![model_id,kind,name])?;
                            tx.execute("INSERT INTO asset_models VALUES(?1,?2,'embedded') ON CONFLICT DO NOTHING",params![id,model_id])?;
                        }
                    }
                    if let Some(vector) = &features {
                        tx.execute("INSERT INTO image_features(asset_id,extractor_version,phash,vector) VALUES(?1,?2,?3,?4) ON CONFLICT(asset_id) DO UPDATE SET extractor_version=excluded.extractor_version,phash=excluded.phash,vector=excluded.vector,created_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",params![id,crate::visual_search::EXTRACTOR_VERSION,vector.phash_bytes().as_slice(),vector.to_bytes()])?;
                    } else {
                        // A changed file whose extraction failed must not keep the
                        // previous content's vector: it would match the old image.
                        tx.execute("DELETE FROM image_features WHERE asset_id=?1", params![id])?;
                    }
                    tx.commit()?;
                    report.indexed += 1;
                } else {
                    self.connection.execute("UPDATE assets SET availability='present' WHERE id=? AND availability!='present'", [&id])?;
                    report.unchanged += 1;
                }
                if let Some(key) = links.get(&canonical) {
                    if let Some(record) = characters.get(key) {
                        let character = if let Some(id) = imported.get(key) {
                            id.clone()
                        } else {
                            let id = self.import_character(
                                catalogue_path.to_str().ok_or("Chemin non UTF-8")?,
                                &catalogue_hash,
                                key,
                                record,
                            )?;
                            imported.insert(key.clone(), id.clone());
                            id
                        };
                        self.connection.execute("INSERT INTO asset_characters VALUES(?1,?2,'depicted','sidecar') ON CONFLICT DO NOTHING",params![id,character])?;
                    }
                }
                Ok(())
            })();
            if let Err(e) = result {
                self.index_error(run, &path, &e.to_string())?;
                report.error_count += 1;
                if let Some(relative) = path.strip_prefix(root).ok().and_then(|p| p.to_str()) {
                    self.connection.execute(
                        "UPDATE assets SET availability='unreadable' WHERE absolute_path=?1",
                        [root.join(relative).to_string_lossy()],
                    )?;
                }
                if report.errors.len() < 30 {
                    report.errors.push(format!("{} : {e}", path.display()));
                }
            }
            self.index_checkpoint(run, position as u32 + 1, total, path.to_str(), &report)?;
            progress(position as u32 + 1, total);
        }
        if cancelled() {
            report.cancelled = true;
            return Ok(report);
        }
        // Recheck the source after traversal: an unplug/replacement is not a complete scan.
        let after = std::fs::metadata(root)?;
        std::fs::read_dir(root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if source_identity.dev() != after.dev() || source_identity.ino() != after.ino() {
                return Err("Source remplacée pendant le scan ; réindexer".into());
            }
        }
        #[cfg(not(unix))]
        let _ = (source_identity, after);
        if scan_complete && report.error_count == 0 {
            let mut q = self.connection.prepare(
                "SELECT id,absolute_path FROM assets WHERE substr(absolute_path,1,length(?1))=?1",
            )?;
            let rows = q
                .query_map([format!("{}/", root_text.trim_end_matches('/'))], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(q);
            let tx = self.connection.transaction()?;
            for (id, path) in rows {
                if !present.contains(&path) {
                    tx.execute("UPDATE assets SET availability='missing' WHERE id=?", [id])?;
                }
            }
            tx.commit()?;
        }
        progress(total, total);
        report.characters = imported.len() as u32;
        Ok(report)
    }
    // Check only the displayed window; never stat the whole library on each scroll.
    fn refresh_gallery_assets(&self, ids: &[String]) -> Result<bool> {
        let mut changes = Vec::new();
        for id in ids {
            let (path, previous): (String, String) = self.connection.query_row(
                "SELECT absolute_path,availability FROM assets WHERE id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if previous != "present" {
                continue;
            }
            let missing = match std::fs::metadata(path) {
                Ok(stat) => !stat.is_file(),
                Err(e) => e.kind() == std::io::ErrorKind::NotFound,
            };
            if missing {
                changes.push(id);
            }
        }
        if changes.is_empty() {
            return Ok(false);
        }
        let tx = self.connection.unchecked_transaction()?;
        for id in changes {
            tx.execute("UPDATE assets SET availability='missing' WHERE id=?", [id])?;
        }
        tx.commit()?;
        Ok(true)
    }
    fn gallery_visible_clause(&self, f: &Filters) -> Result<(String, Vec<rusqlite::types::Value>)> {
        let (mut clause, mut values) = gallery_clause(f)?;
        if f.capture == "all" || self.connection.path().is_none_or(|path| path.is_empty()) {
            return Ok((clause, values));
        }
        // Old deliveries have no capture_phase. Read their run manifests without
        // rewriting the catalogue; cache only job IDs, never the large workflows.
        type Stamp = Vec<(std::path::PathBuf, std::time::SystemTime, u64)>;
        static CACHE: std::sync::Mutex<Option<(Stamp, String)>> = std::sync::Mutex::new(None);
        let root = self.pass_state_root()?.join("saya-runs");
        let mut stamp = Vec::new();
        if root.is_dir() {
            for entry in std::fs::read_dir(root)? {
                let entry = entry?;
                if entry.path().extension().is_none_or(|e| e != "json") {
                    continue;
                }
                let Ok(meta) = entry.metadata() else { continue };
                if let (true, Ok(modified)) = (meta.is_file(), meta.modified()) {
                    stamp.push((entry.path(), modified, meta.len()));
                }
            }
        }
        stamp.sort_by(|a, b| a.0.cmp(&b.0));
        let mut cache = CACHE.lock().map_err(|_| "Cache des phases indisponible")?;
        if cache.as_ref().is_none_or(|(old, _)| old != &stamp) {
            #[derive(serde::Deserialize)]
            struct Phase {
                job: Option<String>,
                asset: Option<String>,
                state: Option<String>,
            }
            #[derive(serde::Deserialize)]
            struct Run {
                #[serde(default)]
                phases: std::collections::BTreeMap<String, Phase>,
                #[serde(default)]
                casting: Value,
                #[serde(default, rename = "retiredJobs")]
                retired_jobs: Vec<String>,
                #[serde(default, rename = "retiredFinals")]
                retired_finals: Vec<String>,
            }
            let mut hidden = Vec::new();
            for (path, _, _) in &stamp {
                let parsed = std::fs::File::open(path)
                    .map_err(|e| e.to_string())
                    .and_then(|f| {
                        serde_json::from_reader::<_, Run>(std::io::BufReader::new(f))
                            .map_err(|e| e.to_string())
                    });
                let run = match parsed {
                    Ok(run) => run,
                    Err(error) => {
                        eprintln!("Manifeste Saya ignoré {}: {error}", path.display());
                        continue;
                    }
                };
                // A superseded final stays visible until the new attempt validates phase 6.
                let new_final = run.phases.get("6").is_some_and(|p| {
                    p.state.as_deref() == Some("succeeded") && p.asset.is_some()
                });
                hidden.extend(
                    run.retired_jobs
                        .iter()
                        .filter(|job| new_final || !run.retired_finals.contains(job))
                        .cloned(),
                );
                let finished = run
                    .phases
                    .get("6")
                    .is_some_and(|p| p.state.as_deref() == Some("succeeded") && p.asset.is_some());
                let unapproved = run.casting.is_object() && run.casting["published"] != true;
                for (phase, info) in run.phases {
                    if unapproved
                        || matches!(phase.as_str(), "2" | "3" | "4" | "5")
                        || (phase == "1" && finished)
                    {
                        if let Some(job) = info.job {
                            hidden.push(job);
                        }
                    }
                }
            }
            *cache = Some((stamp, serde_json::to_string(&hidden)?));
        }
        if let Some((_, hidden)) = cache.as_ref() {
            clause.push_str(" AND NOT EXISTS (SELECT 1 FROM job_outputs jo WHERE jo.asset_id=a.id AND jo.job_id IN (SELECT value FROM json_each(?)))");
            // A preview copy (Saya review gate) is hidden when its job also saved a real image.
            clause.push_str(" AND NOT EXISTS (SELECT 1 FROM job_outputs jo WHERE jo.asset_id=a.id AND jo.output_kind='preview' AND EXISTS (SELECT 1 FROM job_outputs j2 WHERE j2.job_id=jo.job_id AND j2.output_kind<>'preview'))");
            values.push(hidden.clone().into());
        }
        Ok((clause, values))
    }

    pub fn gallery(&self, f: &Filters) -> Result<GalleryPage> {
        self.gallery_window(f, 48)
    }
    /// Windowed loading for infinite scroll: same filters and ordering as
    /// `gallery`, with an explicit limit (offset+limit contract, matching the
    /// historical pagination; metadata only, thumbnails stay lazy).
    pub fn gallery_window(&self, f: &Filters, limit: u32) -> Result<GalleryPage> {
        if limit == 0 || limit > 250 {
            return Err("Limite de fenêtre invalide (1 à 250)".into());
        }
        loop {
            let order = gallery_order(&f.sort)?;
            let (clause, mut values) = self.gallery_visible_clause(f)?;
            let total = self.connection.query_row(
                &format!("SELECT count(*) FROM assets a{clause}"),
                params_from_iter(values.iter()),
                |r| r.get(0),
            )?;
            values.push(f.offset.into());
            let mut q=self.connection.prepare(&format!("SELECT a.id,a.relative_path,a.width,a.height,a.metadata_state,a.favorite,a.note,a.review,a.content_sha256,a.capture_role,a.capture_phase,a.capture_day,a.capture_famille FROM assets a{clause} ORDER BY {order} LIMIT {limit} OFFSET ?"))?;
            let items: Vec<Asset> = q
                .query_map(params_from_iter(values.iter()), |r| {
                    Ok(Asset {
                        id: r.get(0)?,
                        path: r.get(1)?,
                        width: r.get(2)?,
                        height: r.get(3)?,
                        metadata_state: r.get(4)?,
                        favorite: r.get(5)?,
                        note: r.get(6)?,
                        review: r.get(7)?,
                        content_sha256: r.get(8)?,
                        capture_role: r.get(9)?,
                        capture_phase: r.get(10)?,
                        capture_day: r.get(11)?,
                        capture_family: r.get(12)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            drop(q);
            if (f.availability.is_empty() || f.availability == "present")
                && self.refresh_gallery_assets(
                    &items.iter().map(|a| a.id.clone()).collect::<Vec<_>>(),
                )?
            {
                continue;
            }
            return Ok(GalleryPage { items, total });
        }
    }
    /// One SQLite snapshot, at most three metadata records; never load full images.
    pub fn gallery_neighbors(&self, f: &Filters, id: &str, fallback: u32) -> Result<Value> {
        loop {
            let order = gallery_order(&f.sort)?;
            let (clause, mut values) = self.gallery_visible_clause(f)?;
            values.push(id.to_owned().into());
            values.push(fallback.into());
            let sql=format!("WITH ranked AS (SELECT a.id,a.relative_path,a.width,a.height,a.metadata_state,a.favorite,a.note,a.review,a.content_sha256,a.capture_role,a.capture_phase,a.capture_day,a.capture_famille,row_number() OVER(ORDER BY {order})-1 AS idx,count(*) OVER() AS total FROM assets a{clause}), anchor AS (SELECT coalesce((SELECT idx FROM ranked WHERE id=?),min(?,max(0,(SELECT count(*)-1 FROM ranked)))) AS idx) SELECT r.id,r.relative_path,r.width,r.height,r.metadata_state,r.favorite,r.note,r.review,r.content_sha256,r.capture_role,r.capture_phase,r.capture_day,r.capture_famille,r.idx,r.total,anchor.idx FROM ranked r CROSS JOIN anchor WHERE r.idx BETWEEN anchor.idx-1 AND anchor.idx+1 ORDER BY r.idx");
            let mut result = json!({"previous":null,"current":null,"next":null,"position":0,"total":0,"found":false});
            let mut query = self.connection.prepare(&sql)?;
            let mut rows = query.query(params_from_iter(values.iter()))?;
            while let Some(r) = rows.next()? {
                let index: u32 = r.get(13)?;
                let anchor: u32 = r.get(15)?;
                let asset = Asset {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    width: r.get(2)?,
                    height: r.get(3)?,
                    metadata_state: r.get(4)?,
                    favorite: r.get(5)?,
                    note: r.get(6)?,
                    review: r.get(7)?,
                    content_sha256: r.get(8)?,
                    capture_role: r.get(9)?,
                    capture_phase: r.get(10)?,
                    capture_day: r.get(11)?,
                    capture_family: r.get(12)?,
                };
                result["total"] = json!(r.get::<_, u32>(14)?);
                result["position"] = json!(anchor);
                if index == anchor {
                    result["found"] = json!(asset.id == id);
                }
                result[if index < anchor {
                    "previous"
                } else if index > anchor {
                    "next"
                } else {
                    "current"
                }] = serde_json::to_value(asset)?;
            }
            drop(rows);
            drop(query);
            let ids = ["previous", "current", "next"]
                .iter()
                .filter_map(|key| result[key]["id"].as_str().map(str::to_owned))
                .collect::<Vec<_>>();
            if (f.availability.is_empty() || f.availability == "present")
                && self.refresh_gallery_assets(&ids)?
            {
                continue;
            }
            return Ok(result);
        }
    }
    pub fn annotate_asset(&self, id: &str, favorite: bool, note: &str, review: &str) -> Result<()> {
        if note.chars().count() > 20_000 || !["unreviewed", "accepted", "rejected"].contains(&review) {
            return Err("Annotation invalide".into());
        }
        if self.connection.execute(
            "UPDATE assets SET favorite=?2,note=?3,review=?4 WHERE id=?1",
            params![id, favorite, note, review],
        )? != 1
        {
            return Err("Image inconnue".into());
        }
        Ok(())
    }
    pub fn facets(&self) -> Result<Value> {
        let mut result = json!({});
        for kind in ["checkpoint", "lora"] {
            let mut q = self
                .connection
                .prepare("SELECT DISTINCT m.name FROM model_resources m JOIN asset_models am ON am.model_id=m.id JOIN assets a ON a.id=am.asset_id JOIN libraries l ON l.id=a.library_id WHERE m.kind=? AND a.availability='present' AND l.enabled=1 ORDER BY m.name")?;
            let list = q
                .query_map([kind], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            result[kind] = json!(list);
        }
        for (key,sql) in [
            ("variant","SELECT jc.variant_id,min(json_extract(jc.snapshot_json,'$.name') || ' · ' || json_extract(jc.snapshot_json,'$.variant.name')) FROM job_characters jc JOIN job_outputs jo ON jo.job_id=jc.job_id WHERE jc.variant_id IS NOT NULL GROUP BY jc.variant_id ORDER BY 2"),
            ("workflow","SELECT DISTINCT w.id,w.name FROM workflow_revisions w JOIN generation_jobs j ON j.workflow_revision_id=w.id JOIN job_outputs jo ON jo.job_id=j.id ORDER BY w.name"),
            ("job","SELECT DISTINCT j.id,w.name || ' · ' || j.created_at FROM generation_jobs j JOIN workflow_revisions w ON w.id=j.workflow_revision_id JOIN job_outputs jo ON jo.job_id=j.id ORDER BY j.created_at DESC LIMIT 200")
        ] {
            let mut query=self.connection.prepare(sql)?;
            let rows=query.query_map([],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
            result[key]=json!(rows);
        }
        Ok(result)
    }
    pub fn asset_detail(&self, id: &str) -> Result<Value> {
        let (relative,meta,root):(String,String,String)=self.connection.query_row("SELECT a.relative_path,a.metadata_json,l.root_path FROM assets a JOIN libraries l ON l.id=a.library_id WHERE a.id=?",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let asset=self.connection.query_row("SELECT width,height,metadata_state,favorite,note,review,content_sha256,capture_role,capture_phase,capture_day,capture_famille FROM assets WHERE id=?",[id],|r|Ok(Asset{id:id.into(),path:relative.clone(),width:r.get(0)?,height:r.get(1)?,metadata_state:r.get(2)?,favorite:r.get(3)?,note:r.get(4)?,review:r.get(5)?,content_sha256:r.get(6)?,capture_role:r.get(7)?,capture_phase:r.get(8)?,capture_day:r.get(9)?,capture_family:r.get(10)?}))?;
        let availability: String = self.connection.query_row(
            &format!("SELECT {ASSET_STATE} FROM assets a WHERE a.id=?"),
            [id],
            |r| r.get(0),
        )?;
        let reason = self.asset_path(id).err().map(|e| e.to_string());
        Ok(
            json!({"id":id,"asset":asset,"path":Path::new(&root).join(relative),"availability":availability,"reason":reason,"metadata":serde_json::from_str::<Value>(&meta)?,"generations":self.asset_generations(id)?}),
        )
    }
    pub fn asset_path(&self, id: &str) -> Result<PathBuf> {
        let(relative,root):(String,String)=self.connection.query_row("SELECT a.relative_path,l.root_path FROM assets a JOIN libraries l ON l.id=a.library_id WHERE a.id=?",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let root = Path::new(&root);
        crate::fsutil::ensure_within(root, &root.join(relative))
    }

    // ---- Organized captures (brief point 4: the app owns the images) ----
    //
    // On delivery of a job, the app moves its results from the `results/`
    // buffer to the configurable tree
    // `<root>/<day>/pull/<family>/` (trials, passes of a run)
    // or `<root>/<day>/pull/Final/` (completed full workflow), then records
    // provenance and phase in the capture columns (migration 0028).
    // Files already in the database never move; content already present
    // (content_sha256) is not stored a second time — the delivery is
    // linked to the existing asset, the physical duplicate is removed from the buffer.
    // Any organization failure leaves the file in the buffer: it stays
    // indexed as before, no image is ever lost.

    /// Default capture root (brief point 4), changeable through the
    /// `captureRoot` preference. This finding is DISPLAYED by the UI, which enables it
    /// by writing the preference; the library NEVER creates this folder
    /// on its own: without a preference, deliveries stay in the historical
    /// buffer (tests and tools never leak to the real disk).
    /// Proposed capture root for a new profile: `<pictures dir>/saya studio`.
    pub fn default_capture_root() -> Result<PathBuf> {
        Ok(crate::paths::pictures_dir()?.join("saya studio"))
    }

    /// Configured capture root, created if needed. `Ok(None)`: no
    /// preference — historical behaviour, no organization. `Err`:
    /// preference present but the path cannot become a folder;
    /// the caller keeps the buffer and the error is reported per file.
    pub fn capture_root(&self) -> Result<Option<PathBuf>> {
        let configured: Option<String> = self
            .connection
            .query_row(
                "SELECT value_json FROM preferences WHERE key='captureRoot'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|v| serde_json::from_str::<Value>(&v).ok())
            .and_then(|v| v.as_str().map(str::to_owned))
            .filter(|v| !v.trim().is_empty());
        let Some(text) = configured else {
            return Ok(None);
        };
        // A value stored before validation existed is refused here too, never used.
        let path = crate::library_sources::checked_capture_root(&text)?;
        if let Some(sandbox) = test_sandbox() {
            // Isolated test or smoke: any write outside the sandbox fails,
            // never a silent fallback to the user's real folders.
            if !path.starts_with(&sandbox) {
                return Err(format!(
                    "Test isolé : racine de capture {} hors du bac à sable {}",
                    path.display(),
                    sandbox.display()
                )
                .into());
            }
        }
        std::fs::create_dir_all(&path)?;
        // Canonicalized: the libraries and absolute_path are built
        // on the resolved root, the organized destinations must match.
        Ok(Some(path.canonicalize()?))
    }

    /// Phase of a job: (pass, number of passes) when it belongs to a multipass
    /// run — pipeline step (position in the database) or Saya
    /// casting manifest (saya-runs files) — `(None, None)` for an isolated job, which
    /// is by definition a complete workflow. Nothing is ever invented.
    /// Family of a casting job, read from its run manifest (`casting.shot.famille`).
    /// Studio submits casting trials itself, so their renders carry no `saya_family`
    /// chunk (only engine-submitted shots get the SayaFamilySave node). Only a plain
    /// folder-safe name is accepted.
    /// Also returns the trial's character name, used in readable file names.
    fn casting_job_identity(&self, job: &str) -> Result<(Option<String>, Option<String>)> {
        for run in self.saya_runs()? {
            if (1..=6).any(|phase| run["phases"][phase.to_string()]["job"].as_str() == Some(job)) {
                let shot = &run["casting"]["shot"];
                let family = shot["famille"]
                    .as_str()
                    .filter(|f| {
                        !f.is_empty()
                            && f.chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                    })
                    .map(str::to_owned);
                return Ok((family, shot["name"].as_str().map(str::to_owned)));
            }
        }
        Ok((None, None))
    }

    /// Language of the folder and file names the app writes: preference
    /// `fileNamingLanguage`, otherwise the interface language, otherwise French.
    fn naming_language(&self) -> String {
        let read = |key: &str| -> Option<String> {
            self.connection
                .query_row("SELECT value_json FROM preferences WHERE key=?", [key], |r| {
                    r.get::<_, String>(0)
                })
                .ok()
                .and_then(|v| serde_json::from_str::<Value>(&v).ok())
                .and_then(|v| v.as_str().map(str::to_owned))
                .filter(|v| !v.trim().is_empty())
        };
        read("fileNamingLanguage")
            .or_else(|| read("language"))
            .unwrap_or_else(|| "fr".into())
    }

    pub fn capture_phase(&self, job: &str) -> Result<(Option<u64>, Option<u64>)> {
        let pipeline: Option<(i64, Option<i64>)> = self
            .connection
            .query_row(
                "SELECT rs.position,(SELECT max(r2.position) FROM pipeline_run_steps r2 WHERE r2.run_id=rs.run_id) FROM pipeline_run_steps rs WHERE rs.job_id=?1",
                [job],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((position, max)) = pipeline {
            return Ok((
                Some(position.max(0) as u64 + 1),
                max.map(|m| (m.max(0) as u64) + 1),
            ));
        }
        for run in self.saya_runs()? {
            for phase in 1..=6 {
                if run["phases"][phase.to_string()]["job"].as_str() == Some(job) {
                    return Ok((Some(phase), Some(6)));
                }
            }
        }
        Ok((None, None))
    }
}
/// Test sandbox (`SAYA_STUDIO_SANDBOX`): when it is set, Saya
/// Studio refuses to write images outside this folder.
pub fn test_sandbox() -> Option<PathBuf> {
    std::env::var_os("SAYA_STUDIO_SANDBOX")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

pub fn image_data(path: &Path, cache: &Path, preview: bool) -> Result<String> {
    // Simultaneous decoding/encoding bounded by the frontend limiter (3 concurrent
    // `image` calls, api.ts) — never by a global lock: measured G5,
    // a single 1600 px thumbnail blocked ALL the app's thumbnails
    // during its entire encoding (88 calls, 5.9 s cumulative, most of it
    // waiting for the lock). Only the disk cache eviction stays
    // serialized: two concurrent evictions would fight over the same
    // files; remove_file NotFound is then tolerated.
    static TRIM: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let stat = path.metadata()?;
    if stat.len() > 128 * 1024 * 1024 {
        return Err("Image supérieure à 128 Mio".into());
    }
    let size = if preview { 1600 } else { 320 };
    let key = digest(
        format!(
            "{}:{}:{:?}:{size}",
            path.display(),
            stat.len(),
            stat.modified()?
        )
        .as_bytes(),
    );
    std::fs::create_dir_all(cache)?;
    let dest = cache.join(format!("{key}.jpg"));
    let bytes = if dest.exists() {
        std::fs::read(&dest)?
    } else {
        let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16384);
        limits.max_image_height = Some(16384);
        limits.max_alloc = Some(256 * 1024 * 1024);
        reader.limits(limits);
        let thumb = reader.decode()?.thumbnail(size, size).to_rgb8();
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(thumb).write_to(&mut bytes, image::ImageFormat::Jpeg)?;
        let bytes = bytes.into_inner();
        crate::fsutil::write_atomic(&dest, &bytes)?;
        bytes
    };
    let guard = TRIM.lock().map_err(|_| "Cache lock unavailable")?;
    trim_cache(cache, 256 * 1024 * 1024, 2048)?;
    drop(guard);
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

pub fn file_digest(path: &Path) -> Result<String> {
    crate::fsutil::file_sha256(path)
}

/// Generation day `YYYY-MM-DD` (UTC): the engine also dates its batches by
/// day; no local calendar is available without an added dependency, the
/// UTC day is written as-is and never re-guessed.
pub fn capture_day(now: std::time::SystemTime) -> String {
    let days = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    // civil_from_days algorithm (H. Hinnant): proleptic Gregorian.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    // The algorithm works in years starting in March: January and
    // February still belong to the previous civil year.
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02}")
}

/// UTC time of day for capture file names (`HH-MM-SS`, same clock as `capture_day`).
pub fn capture_clock(now: std::time::SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) % 86_400;
    format!("{:02}-{:02}-{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// Folder and file words of the capture tree, per naming language (preference
/// `fileNamingLanguage`). French keeps the historical names (`pull`, `Final`, `sans-famille`).
pub struct NamingWords {
    pub pull: &'static str,
    pub final_dir: &'static str,
    pub no_family: &'static str,
    pub final_file: &'static str,
}

pub fn naming_words(language: &str) -> NamingWords {
    let (pull, final_dir, no_family, final_file) = match language {
        "en" => ("trials", "Final", "no-family", "final"),
        "es" => ("pruebas", "Final", "sin-familia", "final"),
        "pt-BR" => ("testes", "Final", "sem-familia", "final"),
        "tr" => ("denemeler", "Final", "ailesiz", "final"),
        "id" => ("percobaan", "Final", "tanpa-keluarga", "final"),
        "vi" => ("thu-nghiem", "Final", "khong-ho", "final"),
        "zh-CN" => ("试样", "最终", "无家族", "最终"),
        "zh-TW" => ("試樣", "最終", "無家族", "最終"),
        "ja" => ("試行", "最終", "ファミリーなし", "最終"),
        "ko" => ("시도", "최종", "패밀리없음", "최종"),
        "th" => ("ทดลอง", "สุดท้าย", "ไม่มีตระกูล", "สุดท้าย"),
        "ru" => ("пробы", "Финал", "без-семьи", "финал"),
        "ar" => ("تجارب", "نهائي", "بلا-عائلة", "نهائي"),
        _ => ("pull", "Final", "sans-famille", "finale"),
    };
    NamingWords { pull, final_dir, no_family, final_file }
}

/// One file-name word: letters/digits of any script kept, everything else becomes '-'.
fn file_word(text: &str) -> String {
    let word: String = text
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let word: String = word.split('-').filter(|s| !s.is_empty()).collect::<Vec<_>>().join("-");
    word.chars().take(40).collect()
}

/// `a_b_c.png` from the non-empty parts.
fn readable_name(parts: &[&str]) -> String {
    let name: Vec<&str> = parts.iter().copied().filter(|p| !p.is_empty()).collect();
    format!("{}.png", name.join("_"))
}

/// `folder/name`, or `folder/name-2.png`, `-3`… when it already exists.
fn unique_path(folder: &Path, name: &str) -> PathBuf {
    let first = folder.join(name);
    if !first.exists() {
        return first;
    }
    let stem = name.trim_end_matches(".png");
    (2..)
        .map(|n| folder.join(format!("{stem}-{n}.png")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Family read from the `saya_family` chunk of the PNG (written by SayaFamilySave, same
/// reader as `generation_browser`). `"sans-famille"` for captures
/// without a chunk: no family is ever invented.
pub fn capture_family(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let reader = png::Decoder::new(BufReader::new(file)).read_info()?;
    for (keyword, value) in crate::image_restore::text_chunks(reader.info()) {
        if keyword == "saya_family" {
            if let Ok(chunk) = serde_json::from_str::<Value>(&value) {
                if let Some(famille) = chunk.get("famille").and_then(Value::as_str) {
                    if !famille.is_empty() {
                        return Ok(famille.to_owned());
                    }
                }
            }
        }
    }
    Ok("sans-famille".into())
}

/// A delivery placement: either an organized file (`organized`), or a
/// content duplicate reduced to the existing asset (`dedup_asset`), or a
/// failure that stayed in the buffer (`error` — never lost, never hidden).
#[derive(Debug)]
pub struct DeliveryPlacement {
    pub buffer_name: String,
    pub dedup_asset: Option<String>,
    pub organized: Option<PathBuf>,
    pub famille: String,
    pub error: Option<String>,
}

/// Capture of a delivery: day, role ('pull' | 'final') and known pass.
#[derive(Debug, Clone)]
pub struct DeliveryCapture {
    pub day: String,
    pub role: &'static str,
    pub phase: Option<u64>,
    pub total: Option<u64>,
}

impl DeliveryCapture {
    /// Gallery-readable provenance: "PASSE 1", "PASSE 2/6", "Finale".
    pub fn label(&self) -> String {
        match (self.role, self.phase, self.total) {
            ("final", _, _) => "Finale".into(),
            (_, Some(phase), Some(total)) => format!("PASSE {phase}/{total}"),
            (_, Some(phase), None) => format!("PASSE {phase}"),
            _ => "Essai".into(),
        }
    }
}

impl Database {
    /// Organizes the files of THIS job from the buffer into the capture
    /// tree. Returns the placements and, when at least one file has joined
    /// the tree, the capture to index then annotate (`index_library` +
    /// `mark_delivery_captures` by the caller). Per-file failures
    /// stay in the buffer and are reported, never killed.
    pub fn organize_delivery(
        &self,
        buffer: &Path,
        job: &str,
    ) -> Result<(Vec<DeliveryPlacement>, Option<(DeliveryCapture, PathBuf)>)> {
        let mut placements = Vec::new();
        let mut names = Vec::new();
        for entry in std::fs::read_dir(buffer)?.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            // Only the image outputs of THIS job (`<job>-<node>-<index>.png`,
            // download naming) are organized; latents and the files of
            // other deliveries stay in the buffer.
            if name.starts_with(&format!("{job}-"))
                && path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("png"))
            {
                names.push(name);
            }
        }
        if names.is_empty() {
            return Ok((placements, None));
        }
        let root = match self.capture_root() {
            Ok(Some(root)) => root,
            // No root configured: historical delivery in the buffer,
            // without organization or error — nothing is ever lost.
            Ok(None) => return Ok((placements, None)),
            Err(error) => {
                for name in names {
                    placements.push(DeliveryPlacement {
                        buffer_name: name,
                        dedup_asset: None,
                        organized: None,
                        famille: String::new(),
                        error: Some(error.to_string()),
                    });
                }
                return Ok((placements, None));
            }
        };
        let day = capture_day(std::time::SystemTime::now());
        let (phase, total) = self.capture_phase(job)?;
        let capture = DeliveryCapture {
            role: match total {
                Some(total) if phase == Some(total) => "final",
                Some(_) => "pull",
                None => "final",
            },
            phase,
            total,
            day: day.clone(),
        };
        let (job_family, job_name) = self.casting_job_identity(job)?;
        let words = naming_words(&self.naming_language());
        let clock = capture_clock(std::time::SystemTime::now());
        let mut ordinal = 0;
        let mut organized_any = false;
        for name in names {
            let source = buffer.join(&name);
            let placement = (|| -> Result<DeliveryPlacement> {
                let sha = file_digest(&source)?;
                // Zero duplicates: content already present in the database (active asset)
                // is not stored a second time; the delivery is linked
                // to the existing asset and the buffer file is removed.
                let existing: Option<String> = self
                    .connection
                    .query_row(
                        "SELECT a.id FROM assets a JOIN libraries l ON l.id=a.library_id WHERE a.content_sha256=?1 AND a.availability='present' AND l.enabled=1 ORDER BY a.rowid LIMIT 1",
                        [&sha],
                        |r| r.get(0),
                    )
                    .optional()?
                    // `availability` is only refreshed for visible items: re-hash the kept
                    // file so a vanished or changed copy never causes the new render to be deleted.
                    .filter(|asset: &String| {
                        self.asset_path(asset)
                            .ok()
                            .and_then(|kept| file_digest(&kept).ok())
                            .is_some_and(|kept_sha| kept_sha == sha)
                    });
                let mut famille = capture_family(&source)?;
                if famille == "sans-famille" {
                    if let Some(known) = &job_family {
                        famille = known.clone();
                    }
                }
                if let Some(asset) = existing {
                    std::fs::remove_file(&source)?;
                    return Ok(DeliveryPlacement {
                        buffer_name: name.clone(),
                        dedup_asset: Some(asset),
                        organized: None,
                        famille,
                        error: None,
                    });
                }
                let family_dir = if famille == "sans-famille" { words.no_family } else { famille.as_str() };
                let folder = match capture.role {
                    "final" => root.join(&day).join(words.pull).join(words.final_dir),
                    _ => root.join(&day).join(words.pull).join(family_dir),
                };
                std::fs::create_dir_all(&folder)?;
                ordinal += 1;
                let pass = match (capture.role, capture.phase) {
                    ("final", _) => words.final_file.to_owned(),
                    (_, Some(phase)) => format!("p{phase}"),
                    _ => "p1".to_owned(),
                };
                let destination = unique_path(&folder, &readable_name(&[
                    &format!("{day}_{clock}"),
                    family_dir,
                    &job_name.as_deref().map(file_word).unwrap_or_default(),
                    &pass,
                    &format!("{}{}", &job[..job.len().min(8)], if ordinal > 1 { format!("-{ordinal}") } else { String::new() }),
                ]));
                // Same filesystem: atomic rename; otherwise copy
                // then delete (the content is already validated on the buffer side).
                if std::fs::rename(&source, &destination).is_err() {
                    std::fs::copy(&source, &destination)?;
                    std::fs::remove_file(&source)?;
                }
                Ok(DeliveryPlacement {
                    buffer_name: name.clone(),
                    dedup_asset: None,
                    organized: Some(destination),
                    famille,
                    error: None,
                })
            })();
            match placement {
                Ok(placement) => {
                    if placement.organized.is_some() {
                        organized_any = true;
                    }
                    placements.push(placement);
                }
                Err(error) => placements.push(DeliveryPlacement {
                    buffer_name: name,
                    dedup_asset: None,
                    organized: None,
                    famille: String::new(),
                    error: Some(error.to_string()),
                }),
            }
        }
        let used = organized_any.then(|| (capture, root));
        Ok((placements, used))
    }

    /// Provenance + phase in the database, after indexing the capture root:
    /// the capture columns are set on the freshly created assets.
    pub fn mark_delivery_captures(
        &self,
        placements: &[DeliveryPlacement],
        capture: &DeliveryCapture,
    ) -> Result<()> {
        for placement in placements {
            if let Some(destination) = &placement.organized {
                self.connection.execute(
                    "UPDATE assets SET capture_role=?2,capture_phase=?3,capture_day=?4,capture_famille=?5 WHERE absolute_path=?1",
                    params![
                        destination.to_string_lossy(),
                        capture.role,
                        capture.phase.map(|p| p as i64),
                        capture.day,
                        placement.famille,
                    ],
                )?;
            }
        }
        Ok(())
    }
}

fn trim_cache(cache: &Path, max_bytes: u64, max_files: usize) -> Result<()> {
    let mut entries = Vec::new();
    let mut total = 0;
    for entry in std::fs::read_dir(cache)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.len() != 68
            || !name.ends_with(".jpg")
            || !name.as_bytes()[..64].iter().all(u8::is_ascii_hexdigit)
            || !entry.file_type()?.is_file()
        {
            continue;
        }
        let metadata = entry.metadata()?;
        total += metadata.len();
        entries.push((metadata.modified()?, metadata.len(), entry.path()));
    }
    entries.sort_by_key(|entry| entry.0);
    let mut count = entries.len();
    for (_, size, path) in entries {
        if total <= max_bytes && count <= max_files {
            break;
        }
        // A concurrent eviction (another image_data call) may already have
        // removed this file: NotFound is the nominal case of sharing.
        if let Err(error) = std::fs::remove_file(&path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                return Err(error.into());
            }
        }
        total -= size;
        count -= 1;
    }
    Ok(())
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    #[test]
    fn cache_evicts_only_generated_jpegs_to_fit_both_limits() {
        let root = std::env::temp_dir().join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&root).unwrap();
        for i in 0..4 {
            std::fs::write(root.join(format!("{}.jpg", digest(&[i]))), [0; 20]).unwrap();
        }
        std::fs::write(root.join("personal.jpg"), [0; 100]).unwrap();
        trim_cache(&root, 45, 2).unwrap();
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 3);
        assert!(root.join("personal.jpg").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "studio-capture-test-{name}-{}",
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

    /// 2×2 PNG with a `saya_family` tEXt chunk (same writing as the
    /// SayaFamilySave node on the engine side).
    fn png_with_family(path: &Path, famille: &str) {
        png_with_family_seed(path, famille, "42");
    }
    /// Distinct seed = distinct result (different bytes), never merged.
    fn png_with_family_seed(path: &Path, famille: &str, seed: &str) {
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .add_text_chunk(
                "saya_family".into(),
                json!({"famille": famille, "seed": seed}).to_string(),
            )
            .unwrap();
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[9; 12])
            .unwrap();
    }

    fn png_with_role(path: &Path, phase: u32, lineage: &str) {
        let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 2, 2);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let role = if phase == 6 { "final" } else { "preview" };
        encoder
            .add_text_chunk(
                "saya_role".into(),
                json!({"role": role, "phase": phase, "total": 6, "lineage": lineage}).to_string(),
            )
            .unwrap();
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[phase as u8; 12])
            .unwrap();
    }

    #[test]
    fn imported_pass_one_is_hidden_once_its_final_is_indexed() {
        let dir = TempDir::new("lineage-import");
        let mut db = Database::open(&dir.path().join("studio.sqlite3")).unwrap();
        let library = dir.path().join("library");
        std::fs::create_dir_all(&library).unwrap();
        png_with_role(&library.join("x-p1.png"), 1, "casting-tests/lot/1");
        png_with_role(&library.join("y-p1.png"), 1, "casting-tests/lot/2");
        db.index_library(&library).unwrap();
        let names = |db: &Database| {
            let mut v: Vec<String> = db.gallery(&Filters::default()).unwrap().items.into_iter().map(|a| a.path).collect();
            v.sort();
            v
        };
        assert_eq!(names(&db), ["x-p1.png", "y-p1.png"], "no final yet: both pass 1 visible");
        png_with_role(&library.join("x-final.png"), 6, "casting-tests/lot/1");
        db.index_library(&library).unwrap();
        assert_eq!(names(&db), ["x-final.png", "y-p1.png"], "pass 1 of lineage x replaced by its final");
        let all = db.gallery(&Filters { capture: "all".into(), ..Default::default() }).unwrap();
        assert_eq!(all.total, 3, "'all' still shows the hidden pass 1");
    }

    #[test]
    fn capture_day_matches_known_dates() {
        // 2026-10-26T00:00:00Z = 1793049600 ; 2026-01-01T00:00:00Z = 1767225600.
        assert_eq!(
            capture_day(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_793_049_600)),
            "2026-10-26"
        );
        assert_eq!(
            capture_day(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_767_225_600)),
            "2026-01-01"
        );
        assert_eq!(capture_day(UNIX_EPOCH), "1970-01-01");
    }

    #[test]
    fn capture_names_follow_the_file_naming_language() {
        let dir = TempDir::new("naming");
        let mut db = Database::open(&dir.path().join("studio.sqlite3")).unwrap();
        let root = dir.path().join("captures");
        db.set_preference("captureRoot", json!(root.to_str().unwrap())).unwrap();
        db.set_preference("fileNamingLanguage", json!("en")).unwrap();
        let job = "2d71cc9f-2222-4333-8444-0000000000c2";
        crate::saya::write_run(&db, "33333333-2222-4333-8444-0000000000c2",
            &json!({"id":"33333333-2222-4333-8444-0000000000c2","phases":{"1":{"job":job}},
                    "casting":{"shot":{"famille":"hum","name":"Kira Brook"}}})).unwrap();
        let buffer = dir.path().join("results");
        std::fs::create_dir_all(&buffer).unwrap();
        png_with_family(&buffer.join(format!("{job}-abcd1234ef56-0.png")), "hum");
        let (placements, _) = db.organize_delivery(&buffer, job).unwrap();
        let organized = placements[0].organized.clone().unwrap();
        assert!(organized.parent().unwrap().ends_with("trials/hum"), "{organized:?}");
        let name = organized.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.ends_with("_hum_Kira-Brook_p1_2d71cc9f.png"), "{name}");
        assert_eq!(placements[0].famille, "hum", "database keeps the family code");
        // French (default) keeps the historical folder names.
        assert_eq!(naming_words("fr").pull, "pull");
        assert_eq!(naming_words("fr").no_family, "sans-famille");
        assert_eq!(naming_words("ja").no_family, "ファミリーなし");
    }

    #[test]
    fn saya_pass_is_read_from_the_tag_or_the_embedded_prompt() {
        assert_eq!(saya_role(&json!({"saya_role":{"role":"preview","phase":3,"total":6}})), Some(("pull", 3)));
        assert_eq!(saya_role(&json!({"saya_role":{"role":"final","phase":6,"total":6}})), Some(("final", 6)));
        let pass = |class: &str| json!({"prompt":{"9":{"class_type":"SaveImage"},"54":{"class_type":class}}});
        assert_eq!(saya_role(&pass("SayaImageGenerationReview")), Some(("pull", 1)));
        assert_eq!(saya_role(&pass("SayaImagePhase4Stop")), Some(("pull", 4)));
        assert_eq!(saya_role(&pass("SayaImagePhase6Stop")), Some(("final", 6)));
        assert_eq!(saya_role(&json!({"prompt":{"9":{"class_type":"SaveImage"}}})), None, "not a Saya pass");
        assert_eq!(saya_role(&json!({})), None);
    }

    #[test]
    fn studio_casting_render_without_family_chunk_uses_the_run_family() {
        let dir = TempDir::new("casting-family");
        let mut db = Database::open(&dir.path().join("studio.sqlite3")).unwrap();
        let root = dir.path().join("captures");
        db.set_preference("captureRoot", json!(root.to_str().unwrap()))
            .unwrap();
        let job = "11111111-2222-4333-8444-0000000000c1";
        crate::saya::write_run(
            &db,
            "22222222-2222-4333-8444-0000000000c1",
            &json!({"id":"22222222-2222-4333-8444-0000000000c1","phases":{"1":{"job":job,"state":"succeeded"}},
                    "casting":{"shot":{"famille":"bny","stem":"bny-00001-test-noobai"}}}),
        )
        .unwrap();
        let buffer = dir.path().join("results");
        std::fs::create_dir_all(&buffer).unwrap();
        let mut encoder = png::Encoder::new(
            std::fs::File::create(buffer.join(format!("{job}-abcd1234ef56-0.png"))).unwrap(),
            2,
            2,
        );
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().unwrap().write_image_data(&[7; 12]).unwrap();
        let (placements, _) = db.organize_delivery(&buffer, job).unwrap();
        assert_eq!(placements[0].famille, "bny");
        let organized = placements[0].organized.as_ref().unwrap();
        assert!(organized.parent().unwrap().ends_with("pull/bny"), "{organized:?}");
    }

    #[test]
    fn delivery_never_deletes_a_new_render_when_the_known_copy_vanished() {
        let dir = TempDir::new("vanished");
        let mut db = Database::open(&dir.path().join("studio.sqlite3")).unwrap();
        let root = dir.path().join("captures");
        db.set_preference("captureRoot", json!(root.to_str().unwrap()))
            .unwrap();
        let buffer = dir.path().join("results");
        std::fs::create_dir_all(&buffer).unwrap();
        let first = "11111111-2222-4333-8444-000000000001";
        png_with_family(&buffer.join(format!("{first}-abcd1234ef56-0.png")), "bny");
        let (placements, used) = db.organize_delivery(&buffer, first).unwrap();
        let (capture, _) = used.unwrap();
        db.index_library(&root).unwrap();
        db.mark_delivery_captures(&placements, &capture).unwrap();
        // The catalogue still says "present", but the file is gone.
        std::fs::remove_file(placements[0].organized.as_ref().unwrap()).unwrap();
        let second = "11111111-2222-4333-8444-000000000002";
        png_with_family(&buffer.join(format!("{second}-abcd1234ef56-0.png")), "bny");
        let (again, _) = db.organize_delivery(&buffer, second).unwrap();
        assert!(again[0].dedup_asset.is_none());
        assert!(
            again[0].organized.as_ref().unwrap().is_file(),
            "the new render is kept"
        );
    }

    #[test]
    fn delivery_routes_plain_job_to_final_and_marks_provenance() {
        let dir = TempDir::new("final");
        let mut db = Database::open(&dir.path().join("studio.sqlite3")).unwrap();
        let root = dir.path().join("captures");
        db.set_preference("captureRoot", json!(root.to_str().unwrap()))
            .unwrap();
        let job = "11111111-2222-4333-8444-555555555555";
        let buffer = dir.path().join("results");
        std::fs::create_dir_all(&buffer).unwrap();
        png_with_family(&buffer.join(format!("{job}-abcd1234ef56-0.png")), "bny");
        png_with_family_seed(
            &buffer.join(format!("{job}-abcd1234ef56-1.png")),
            "bny",
            "43",
        );
        // A latent and another job's file never leave.
        std::fs::write(buffer.join(format!("{job}-abcd1234ef56-2.lat")), b"latent").unwrap();
        std::fs::write(buffer.join("autre-job-abcd1234ef56-0.png"), b"png").unwrap();
        let (placements, used) = db.organize_delivery(&buffer, job).unwrap();
        assert_eq!(placements.len(), 2, "seuls les PNG de ce job");
        let (capture, indexed_root) = used.expect("au moins un fichier organisé");
        assert_eq!(capture.role, "final", "job isolé = workflow complet");
        assert_eq!(capture.phase, None);
        assert_eq!(indexed_root, root.canonicalize().unwrap());
        assert!(
            !buffer.join(format!("{job}-abcd1234ef56-0.png")).exists(),
            "le tampon est vidé du fichier déplacé"
        );
        assert!(
            buffer.join("autre-job-abcd1234ef56-0.png").exists(),
            "le fichier d'un autre job reste dans le tampon"
        );
        assert!(buffer.join(format!("{job}-abcd1234ef56-2.lat")).exists());
        let destination = placements[0].organized.as_ref().unwrap();
        assert_eq!(
            destination.parent().unwrap(),
            root.join(&capture.day).join("pull").join("Final"),
            "<jour>/pull/Final/, pas de sous-dossier famille"
        );
        assert_eq!(placements[0].famille, "bny", "famille lue du chunk");
        // Indexing then annotation: the gallery knows provenance + phase.
        db.index_library(&root).unwrap();
        db.mark_delivery_captures(&placements, &capture).unwrap();
        let page = db
            .gallery(&Filters {
                capture: "all".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.items[0].capture_role.as_deref(), Some("final"));
        assert_eq!(
            page.items[0].capture_day.as_deref(),
            Some(capture.day.as_str())
        );
        assert_eq!(page.items[0].capture_family.as_deref(), Some("bny"));
        assert_eq!(page.items[0].capture_phase, None);
    }

    #[test]
    fn delivery_deduplicates_content_and_intermediate_passes_stay_hidden() {
        let dir = TempDir::new("dedup");
        let mut db = Database::open(&dir.path().join("studio.sqlite3")).unwrap();
        let root = dir.path().join("captures");
        db.set_preference("captureRoot", json!(root.to_str().unwrap()))
            .unwrap();
        let job = "aaaa1111-2222-4333-8444-555555555555";
        // Pass 2 of a 3-pass pipeline run (positions 0/1/2 in the database).
        db.connection.execute("INSERT INTO pipeline_definitions(id,name,manifest_json,manifest_sha256) VALUES('run-def','Déf','{}','abc')", []).unwrap();
        db.connection.execute("INSERT INTO pipeline_runs(id,pipeline_id,manifest_sha256,state) VALUES('run-1','run-def','abc','running')", []).unwrap();
        for (step, position) in [("p1", 0), ("p2", 1), ("p3", 2)] {
            db.connection
                .execute(
                    "INSERT INTO pipeline_run_steps(run_id,step_id,position) VALUES('run-1',?1,?2)",
                    rusqlite::params![step, position],
                )
                .unwrap();
        }
        // The job must exist: job_id references generation_jobs.
        db.connection
            .execute(
                "INSERT INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES('wf','Test','h','comfy_api','{}')",
                [],
            )
            .unwrap();
        db.connection
            .execute(
                "INSERT INTO generation_jobs(id,workflow_revision_id,seed,request_snapshot_json,state) VALUES(?1,'wf','42','{}','queued')",
                [job],
            )
            .unwrap();
        db.connection
            .execute(
                "UPDATE pipeline_run_steps SET job_id=?1 WHERE step_id='p2'",
                [job],
            )
            .unwrap();
        let (phase, total) = db.capture_phase(job).unwrap();
        assert_eq!((phase, total), (Some(2), Some(3)), "passe lue de la base");
        let buffer = dir.path().join("results");
        std::fs::create_dir_all(&buffer).unwrap();
        png_with_family(&buffer.join(format!("{job}-bbbb11112222-0.png")), "kmn");
        let (placements, used) = db.organize_delivery(&buffer, job).unwrap();
        let (capture, root_used) = used.unwrap();
        assert_eq!(capture.role, "pull", "passe intermédiaire = essai");
        assert_eq!(
            placements[0].organized.as_ref().unwrap().parent().unwrap(),
            root.join(&capture.day).join("pull").join("kmn"),
            "<jour>/pull/<famille>/"
        );
        db.index_library(&root_used).unwrap();
        db.mark_delivery_captures(&placements, &capture).unwrap();
        // Main view: pass 2 (>1) is hidden; PASS 1 and finals
        // stay visible; 'all' shows them all.
        let main = db.gallery(&Filters::default()).unwrap();
        assert_eq!(main.total, 0, "passe 2 cachée de la galerie principale");
        let all = db
            .gallery(&Filters {
                capture: "all".into(),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(all.total, 1);
        assert_eq!(all.items[0].capture_role.as_deref(), Some("pull"));
        assert_eq!(all.items[0].capture_phase, Some(2));
        db.connection
            .execute(
                "INSERT INTO job_outputs(job_id,asset_id,output_kind) VALUES(?1,?2,'saved')",
                rusqlite::params![job, all.items[0].id],
            )
            .unwrap();
        // Legacy output: null capture labels must still obey the run's phase.
        db.connection
            .execute("UPDATE assets SET capture_role=NULL,capture_phase=NULL", [])
            .unwrap();
        let run = "legacy-gallery-run";
        crate::saya::write_run(
            &db,
            run,
            &json!({"phases":{"2":{"job":job,"state":"succeeded"}}}),
        )
        .unwrap();
        assert_eq!(db.gallery(&Filters::default()).unwrap().total, 0);
        let neighbors = db
            .gallery_neighbors(&Filters::default(), &all.items[0].id, 0)
            .unwrap();
        assert_eq!(neighbors["total"], 0);
        // A changed manifest invalidates the read-only cache.
        crate::saya::write_run(
            &db,
            run,
            &json!({"phases":{"1":{"job":job,"state":"succeeded","asset":all.items[0].id}}}),
        )
        .unwrap();
        assert_eq!(db.gallery(&Filters::default()).unwrap().total, 1);
        let unchanged: Option<i64> = db
            .connection
            .query_row("SELECT capture_phase FROM assets LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(unchanged, None, "filtering never modifies the catalogue");
        // Zero duplicates: re-delivering the SAME content links the existing asset and
        // does not store a second file.
        png_with_family(&buffer.join(format!("{job}-cccc11112222-0.png")), "kmn");
        let (again, used_again) = db.organize_delivery(&buffer, job).unwrap();
        assert_eq!(
            again[0].dedup_asset.as_deref(),
            Some(all.items[0].id.as_str())
        );
        assert!(again[0].organized.is_none(), "pas de second fichier");
        assert!(used_again.is_none(), "rien de nouveau à indexer");
        assert!(!buffer.join(format!("{job}-cccc11112222-0.png")).exists());
        assert_eq!(
            db.gallery(&Filters {
                capture: "all".into(),
                ..Default::default()
            })
            .unwrap()
            .total,
            1,
            "toujours une seule image en base pour ce contenu"
        );
    }

    #[test]
    fn capture_root_failure_keeps_files_in_buffer_with_reported_error() {
        let dir = TempDir::new("echec");
        let db = Database::open(&dir.path().join("studio.sqlite3")).unwrap();
        // Capture root under a FILE path is impossible: the parent is a
        // file, create_dir_all fails.
        let blocking = dir.path().join("bloquant");
        std::fs::write(&blocking, b"x").unwrap();
        db.set_preference(
            "captureRoot",
            json!(blocking.join("captures").to_str().unwrap()),
        )
        .unwrap();
        let job = "bbbb2222-3333-4444-8555-666666666666";
        let buffer = dir.path().join("results");
        std::fs::create_dir_all(&buffer).unwrap();
        png_with_family(&buffer.join(format!("{job}-dddd11112222-0.png")), "lcs");
        let (placements, used) = db.organize_delivery(&buffer, job).unwrap();
        assert!(used.is_none(), "rien n'est organisé");
        assert!(
            buffer.join(format!("{job}-dddd11112222-0.png")).exists(),
            "le fichier reste dans le tampon : aucune image perdue"
        );
        let error = placements[0].error.as_deref().expect("erreur rapportée");
        assert!(!error.is_empty());
    }

    #[test]
    fn unconfigured_root_stays_legacy_and_never_touches_the_real_disk() {
        let dir = TempDir::new("sans-racine");
        let db = Database::open(&dir.path().join("studio.sqlite3")).unwrap();
        assert!(
            db.capture_root().unwrap().is_none(),
            "sans préférence, la bibliothèque ne choisit aucun dossier"
        );
        // Invariant: the library never CREATES the default on its own. The probe
        // is relative to the state BEFORE the test: on Saya's machine the folder
        // legitimately exists (real G1 captures) — an absolute check would fail
        // forever from the first real use.
        let default_before = Database::default_capture_root().unwrap().exists();
        let job = "cccc3333-4444-5555-8666-777777777777";
        let buffer = dir.path().join("results");
        std::fs::create_dir_all(&buffer).unwrap();
        png_with_family(&buffer.join(format!("{job}-eeee11112222-0.png")), "bny");
        let (placements, used) = db.organize_delivery(&buffer, job).unwrap();
        assert!(
            used.is_none() && placements.is_empty(),
            "aucune organisation"
        );
        assert_eq!(
            Database::default_capture_root().unwrap().exists(),
            default_before,
            "le défaut n'est jamais créé par la bibliothèque"
        );
        assert!(
            buffer.join(format!("{job}-eeee11112222-0.png")).exists(),
            "livraison historique : le fichier reste dans le tampon"
        );
    }

    #[test]
    fn pass1_and_finals_label_is_readable() {
        let pass_number = DeliveryCapture {
            day: "2026-10-26".into(),
            role: "pull",
            phase: Some(1),
            total: Some(6),
        };
        assert_eq!(pass_number.label(), "PASSE 1/6");
        let finale = DeliveryCapture {
            day: "2026-10-26".into(),
            role: "final",
            phase: Some(6),
            total: Some(6),
        };
        assert_eq!(finale.label(), "Finale");
        let isole = DeliveryCapture {
            day: "2026-10-26".into(),
            role: "final",
            phase: None,
            total: None,
        };
        assert_eq!(isole.label(), "Finale");
    }
}
