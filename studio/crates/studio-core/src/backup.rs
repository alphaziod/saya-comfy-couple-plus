//! SQLite and managed checkpoint backups. Originals and caches remain external.
use crate::{
    gallery::{file_digest, read_json},
    Result,
};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
    time::Duration,
};

const DATABASE: &str = "studio.sqlite3";
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub schema_version: u32,
    pub database_file: String,
    pub sha256: String,
    pub byte_size: u64,
    pub originals_included: bool,
    #[serde(default)]
    pub checkpoint_files: Vec<CheckpointFile>,
    #[serde(default)]
    pub pass_states_included: bool,
    #[serde(default)]
    pub pass_state_files: Vec<CheckpointFile>,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckpointFile {
    pub file: String,
    pub sha256: String,
    pub byte_size: u64,
}
fn checkpoint_files(db: &Connection) -> Result<Vec<CheckpointFile>> {
    let mut files = BTreeMap::new();
    let mut statement = db.prepare("SELECT manifest_json,content_sha256 FROM pass_artifacts")?;
    let rows = statement.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
    })?;
    for row in rows {
        let (text, hash) = row?;
        let value: serde_json::Value = serde_json::from_str(&text)?;
        let format = value["format"].as_str().unwrap_or("");
        // External historical artifacts have no managed file contract.
        if !format.starts_with("saya-") {
            continue;
        }
        if format != "saya-image-checkpoint" || value["version"] != 1 {
            return Err("Format de checkpoint géré inconnu ; sauvegarde refusée".into());
        }
        let hash = hash.ok_or("Hash du checkpoint absent")?;
        let name = format!("{hash}.png");
        let size = value["bytes"]
            .as_u64()
            .ok_or("Taille du checkpoint absente")?;
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || value["sha256"] != hash
            || value["file"] != name
            || size > 128 * 1024 * 1024
        {
            return Err("Manifeste de checkpoint incohérent".into());
        }
        let file = CheckpointFile {
            file: name.clone(),
            sha256: hash,
            byte_size: size,
        };
        if let Some(previous) = files.insert(name, file.clone()) {
            if previous != file {
                return Err("Checkpoints contradictoires".into());
            }
        }
        if files.len() > 10_000 {
            return Err("Plus de 10000 fichiers checkpoints ; sauvegarde refusée".into());
        }
    }
    Ok(files.into_values().collect())
}
fn verify_checkpoint_file(root: &Path, file: &CheckpointFile) -> Result<()> {
    let path = root.join(&file.file);
    if !fs::symlink_metadata(root)?.file_type().is_dir()
        || !fs::symlink_metadata(&path)?.file_type().is_file()
        || path.metadata()?.len() != file.byte_size
        || file_digest(&path)? != file.sha256
    {
        return Err("Fichier checkpoint absent, symbolique ou corrompu".into());
    }
    Ok(())
}
fn copy_checkpoints(source: &Path, destination: &Path, files: &[CheckpointFile]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    new_directory(destination)?;
    for file in files {
        verify_checkpoint_file(source, file)?;
        let mut input = fs::File::open(source.join(&file.file))?.take(file.byte_size + 1);
        let mut output = new_file(&destination.join(&file.file))?;
        if std::io::copy(&mut input, &mut output)? != file.byte_size {
            return Err("Checkpoint modifié pendant la copie".into());
        }
        output.sync_all()?;
        verify_checkpoint_file(destination, file)?;
    }
    #[cfg(unix)]
    fs::File::open(destination)?.sync_all()?;
    Ok(())
}
fn pass_files(root: &Path) -> Result<Vec<CheckpointFile>> {
    match fs::symlink_metadata(root) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err("Racine d'états symbolique refusée".into())
        }
        _ => {}
    }
    let manifest = crate::pass_states::open_store(root)?;
    let mut files = BTreeMap::new();
    for entry in manifest.entries {
        let path = safe_pass_file(root, &entry.file)?;
        if path.metadata()?.len() != entry.bytes || file_digest(&path)? != entry.sha256 {
            return Err("État de passe corrompu".into());
        }
        files.insert(
            entry.file.clone(),
            CheckpointFile {
                file: entry.file,
                sha256: entry.sha256,
                byte_size: entry.bytes,
            },
        );
    }
    let path = safe_pass_file(root, "manifest.json")?;
    files.insert(
        "manifest.json".into(),
        CheckpointFile {
            file: "manifest.json".into(),
            sha256: file_digest(&path)?,
            byte_size: path.metadata()?.len(),
        },
    );
    if files.len() > 10000 {
        return Err("Plus de 10000 fichiers d'états ; sauvegarde refusée".into());
    }
    Ok(files.into_values().collect())
}

fn verify_completed_pass_inventory(db: &Connection, files: &[CheckpointFile]) -> Result<()> {
    let table: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='pipeline_run_steps')", [], |r| r.get(0))?;
    if !table {
        return Ok(());
    }
    let listed: BTreeMap<_, _> = files
        .iter()
        .map(|f| (f.file.as_str(), f.sha256.as_str()))
        .collect();
    let mut query = db.prepare("SELECT run_id,step_id,job_id,output_sha256,output_kind FROM pipeline_run_steps WHERE state='completed'")?;
    let rows = query.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
        ))
    })?;
    for row in rows {
        let (run, step, job, sha, kind) = row?;
        let extension = match kind.as_str() {
            "image" => "png",
            "latent" => "safetensors",
            _ => return Err("Type de passe inconnu".into()),
        };
        let key = crate::orchestrator::attempt_key(&step, &job);
        let pipeline = crate::orchestrator::store_name(&run);
        let expected = format!("data/{pipeline}/{key}/{sha}.{extension}");
        if listed.get(expected.as_str()).copied() != Some(sha.as_str()) {
            return Err("Sortie de passe terminée absente de la sauvegarde".into());
        }
    }
    Ok(())
}

fn safe_pass_file(root: &Path, relative: &str) -> Result<std::path::PathBuf> {
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        return Err("Racine d'états symbolique refusée".into());
    }
    let mut path = root.to_path_buf();
    for component in Path::new(relative).components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err("Chemin d'état non relatif".into());
        }
        path.push(component);
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            return Err("Chemin d'état symbolique refusé".into());
        }
    }
    if !path.is_file() {
        return Err("État absent ou non régulier".into());
    }
    Ok(path)
}

fn copy_pass_files(source: &Path, destination: &Path, files: &[CheckpointFile]) -> Result<()> {
    if files.is_empty() {
        return Ok(());
    }
    if pass_files(source)? != files {
        return Err("Inventaire des états modifié pendant la sauvegarde".into());
    }
    new_directory(destination)?;
    for file in files {
        let path = safe_pass_file(source, &file.file)?;
        let target = destination.join(&file.file);
        let mut dirs = fs::DirBuilder::new();
        dirs.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            dirs.mode(0o700);
        }
        dirs.create(target.parent().ok_or("Dossier d'état absent")?)?;
        let mut input = fs::File::open(path)?;
        let mut output = new_file(&target)?;
        std::io::copy(&mut input, &mut output)?;
        output.sync_all()?;
        if target.metadata()?.len() != file.byte_size || file_digest(&target)? != file.sha256 {
            return Err("État modifié pendant la copie".into());
        }
    }
    if pass_files(destination)? != files {
        return Err("Copie des états incohérente".into());
    }
    Ok(())
}

pub fn estimate(source: &Path, include_pass_states: bool) -> Result<serde_json::Value> {
    let db = readonly(source)?;
    check(&db)?;
    let pages = u64::try_from(db.pragma_query_value(None, "page_count", |r| r.get::<_, i64>(0))?)?;
    let page_size =
        u64::try_from(db.pragma_query_value(None, "page_size", |r| r.get::<_, i64>(0))?)?;
    let root = source.parent().ok_or("Dossier source absent")?;
    let checkpoints = if include_pass_states {
        checkpoint_files(&db)?
    } else {
        Vec::new()
    };
    let passes = if include_pass_states {
        pass_files(&root.join("pass-states"))?
    } else {
        Vec::new()
    };
    let mut files = vec![serde_json::json!({"file": DATABASE, "byteSize": pages * page_size})];
    for (folder, list) in [("checkpoints", checkpoints), ("pass-states", passes)] {
        for file in list {
            files.push(serde_json::json!({"file": format!("{folder}/{}", file.file), "byteSize": file.byte_size}));
        }
    }
    let bytes: u64 = files.iter().filter_map(|f| f["byteSize"].as_u64()).sum();
    Ok(
        serde_json::json!({"estimatedBytes": bytes, "files": files, "passStatesIncluded": include_pass_states, "originalsIncluded": false, "externalReferences": "Images originales, modèles et fichiers non gérés restent externes. Sans l'option états, leurs références historiques sont conservées mais leurs fichiers sont exclus."}),
    )
}

fn readonly(path: &Path) -> Result<Connection> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.busy_timeout(Duration::from_secs(3))?;
    Ok(db)
}
fn check(db: &Connection) -> Result<u32> {
    let version: u32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if !(1..=crate::storage::VERSION).contains(&version) {
        return Err("Version SQLite inconnue pour cette sauvegarde".into());
    }
    let identity: String = db.query_row(
        "SELECT name FROM schema_migrations WHERE version=1",
        [],
        |r| r.get(0),
    )?;
    if identity != "saya-studio-initial" {
        return Err("Base étrangère à Saya Studio".into());
    }
    let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    let foreign_keys: i64 =
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })?;
    if integrity != "ok" || foreign_keys != 0 {
        return Err("Intégrité SQLite ou références invalides".into());
    }
    Ok(version)
}
fn new_directory(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    Ok(())
}
fn new_file(path: &Path) -> Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}
fn finish(directory: &Path, manifest: &Manifest) -> Result<()> {
    fs::File::open(directory.join(DATABASE))?.sync_all()?;
    // A directory without this final manifest is incomplete, never a valid backup.
    let mut file = new_file(&directory.join("manifest.json"))?;
    let bytes = serde_json::to_vec_pretty(manifest)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("Manifeste de sauvegarde trop volumineux".into());
    }
    file.write_all(&bytes)?;
    file.sync_all()?;
    #[cfg(unix)]
    fs::File::open(directory)?.sync_all()?;
    Ok(())
}
pub fn create(source: &Path, destination: &Path) -> Result<Manifest> {
    create_with_options(source, destination, false)
}
pub fn create_with_options(
    source: &Path,
    destination: &Path,
    include_pass_states: bool,
) -> Result<Manifest> {
    let root = source.parent().ok_or("Dossier source absent")?;
    let mut locks = Vec::new();
    if include_pass_states {
        for folder in ["checkpoints", "pass-states"] {
            let path = root.join(folder);
            if path.try_exists()? {
                locks.push(crate::checkpoints::store_lock(
                    &path,
                    "Stockage occupé ; réessayez la sauvegarde",
                )?);
            }
        }
    }
    let db = readonly(source)?;
    check(&db)?;
    new_directory(destination)?;
    let path = destination.join(DATABASE);
    drop(new_file(&path)?);
    db.backup("main", &path, None)?;
    let snapshot = Connection::open(&path)?;
    snapshot.pragma_update(None, "journal_mode", "DELETE")?;
    let schema_version = check(&snapshot)?;
    let checkpoint_files = if include_pass_states {
        checkpoint_files(&snapshot)?
    } else {
        Vec::new()
    };
    let pass_state_files = if include_pass_states {
        pass_files(&root.join("pass-states"))?
    } else {
        Vec::new()
    };
    if include_pass_states {
        verify_completed_pass_inventory(&snapshot, &pass_state_files)?;
    }
    drop(snapshot);
    copy_checkpoints(
        &source
            .parent()
            .ok_or("Dossier source absent")?
            .join("checkpoints"),
        &destination.join("checkpoints"),
        &checkpoint_files,
    )?;
    copy_pass_files(
        &root.join("pass-states"),
        &destination.join("pass-states"),
        &pass_state_files,
    )?;
    let manifest = Manifest {
        format: "saya-studio-sqlite-backup".into(),
        version: 3,
        schema_version,
        database_file: DATABASE.into(),
        sha256: file_digest(&path)?,
        byte_size: path.metadata()?.len(),
        originals_included: false,
        checkpoint_files,
        pass_states_included: include_pass_states,
        pass_state_files,
    };
    finish(destination, &manifest)?;
    Ok(manifest)
}
pub fn verify(directory: &Path) -> Result<Manifest> {
    let manifest: Manifest = serde_json::from_value(read_json(
        &directory.join("manifest.json"),
        4 * 1024 * 1024,
    )?)?;
    if manifest.format != "saya-studio-sqlite-backup"
        || ![1, 2, 3].contains(&manifest.version)
        || manifest.database_file != DATABASE
        || manifest.originals_included
    {
        return Err("Format de sauvegarde non pris en charge".into());
    }
    let path = directory.join(DATABASE);
    if directory.join("studio.sqlite3-wal").exists() {
        return Err("Sauvegarde modifiée : journal WAL inattendu".into());
    }
    if path.metadata()?.len() != manifest.byte_size || file_digest(&path)? != manifest.sha256 {
        return Err("Empreinte de sauvegarde incorrecte".into());
    }
    if check(&readonly(&path)?)? != manifest.schema_version {
        return Err("Version de sauvegarde contradictoire".into());
    }
    if manifest.version < 3
        && (manifest.pass_states_included || !manifest.pass_state_files.is_empty())
    {
        return Err("États de passes inattendus dans un ancien format".into());
    }
    if manifest.version == 1 || (manifest.version == 3 && !manifest.pass_states_included) {
        if !manifest.checkpoint_files.is_empty() || !manifest.pass_state_files.is_empty() {
            return Err("Ancien format SQLite avec fichiers inattendus".into());
        }
    } else {
        if checkpoint_files(&readonly(&path)?)? != manifest.checkpoint_files {
            return Err("Inventaire checkpoints différent du snapshot SQLite".into());
        }
        for file in &manifest.checkpoint_files {
            verify_checkpoint_file(&directory.join("checkpoints"), file)?;
        }
    }
    if manifest.version == 3
        && manifest.pass_states_included
        && pass_files(&directory.join("pass-states"))? != manifest.pass_state_files
    {
        return Err("Inventaire des états différent du manifeste de sauvegarde".into());
    }
    if manifest.version == 3 && manifest.pass_states_included {
        verify_completed_pass_inventory(&readonly(&path)?, &manifest.pass_state_files)?;
    }
    Ok(manifest)
}
pub fn restore(source: &Path, destination: &Path) -> Result<Manifest> {
    let manifest = verify(source)?;
    new_directory(destination)?;
    let path = destination.join(DATABASE);
    let mut input = fs::File::open(source.join(DATABASE))?;
    let mut output = new_file(&path)?;
    std::io::copy(&mut input, &mut output)?;
    output.sync_all()?;
    drop(output);
    // Check the copied bytes too: a source can change after its initial verification.
    if path.metadata()?.len() != manifest.byte_size
        || file_digest(&path)? != manifest.sha256
        || check(&readonly(&path)?)? != manifest.schema_version
    {
        return Err("Copie incohérente ; restauration non validée".into());
    }
    copy_checkpoints(
        &source.join("checkpoints"),
        &destination.join("checkpoints"),
        &manifest.checkpoint_files,
    )?;
    copy_pass_files(
        &source.join("pass-states"),
        &destination.join("pass-states"),
        &manifest.pass_state_files,
    )?;
    finish(destination, &manifest)?;
    Ok(manifest)
}

fn has_table(db: &Connection, name: &str) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?)",
        [name],
        |r| r.get(0),
    )?)
}

/// External dependencies of a backup or restored directory, read-only (the
/// database is never migrated or written). Lists what the copy does not
/// contain: library roots and original images missing on this machine,
/// managed checkpoints and completed pipeline states absent from the copy,
/// and the models/LoRA referenced by workflows (never backed up; to be
/// checked in ComfyUI). Nothing is guessed or repaired.
pub fn dependency_report(directory: &Path) -> Result<serde_json::Value> {
    use serde_json::json;
    let db = readonly(&directory.join(DATABASE))?;
    let schema = check(&db)?;
    let mut libraries = Vec::new();
    let mut missing_images = Vec::new();
    let mut checked_images = 0u64;
    if has_table(&db, "libraries")? && has_table(&db, "assets")? {
        let mut q = db.prepare("SELECT id,root_path,enabled FROM libraries ORDER BY root_path")?;
        let rows: Vec<(String, String, bool)> = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (id, root, enabled) in rows {
            let present = Path::new(&root).is_dir();
            let mut q = db.prepare(
                "SELECT relative_path FROM assets WHERE library_id=? AND availability='present'",
            )?;
            let paths: Vec<String> = q
                .query_map([&id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let mut missing = 0u64;
            for relative in &paths {
                checked_images += 1;
                if !Path::new(&root).join(relative).is_file() {
                    missing += 1;
                    if missing_images.len() < 50 {
                        missing_images.push(format!("{root}/{relative}"));
                    }
                }
            }
            libraries.push(json!({"root": root, "enabled": enabled, "presentOnDisk": present, "images": paths.len(), "missingImages": missing}));
        }
    }
    let mut missing_checkpoints = Vec::new();
    if has_table(&db, "pass_artifacts")? {
        for file in checkpoint_files(&db)? {
            let path = directory.join("checkpoints").join(&file.file);
            if verify_checkpoint_file(&directory.join("checkpoints"), &file).is_err() {
                missing_checkpoints.push(path.display().to_string());
            }
        }
    }
    let mut missing_states = Vec::new();
    if has_table(&db, "pipeline_run_steps")? {
        let mut q = db.prepare("SELECT run_id,step_id,job_id,output_sha256 FROM pipeline_run_steps WHERE state='completed' AND output_sha256 IS NOT NULL")?;
        let rows: Vec<(String, String, Option<String>, String)> = q
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let store = directory.join("pass-states");
        for (run, step, job, sha) in rows {
            let key = crate::orchestrator::attempt_key(&step, job.as_deref().unwrap_or_default());
            if crate::pass_states::verify_state(
                &store,
                &crate::orchestrator::store_name(&run),
                &key,
                &sha,
            )
            .is_err()
            {
                missing_states.push(json!({"run": run, "step": step}));
            }
        }
    }
    let mut models = std::collections::BTreeSet::new();
    if has_table(&db, "workflow_revisions")? {
        let mut q = db.prepare("SELECT document_json FROM workflow_revisions")?;
        let docs: Vec<String> = q
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for doc in docs {
            let graph: serde_json::Value = serde_json::from_str(&doc).unwrap_or_default();
            for node in graph.as_object().into_iter().flat_map(|o| o.values()) {
                for key in [
                    "ckpt_name",
                    "lora_name",
                    "vae_name",
                    "unet_name",
                    "model_name",
                ] {
                    if let Some(name) = node["inputs"][key].as_str() {
                        models.insert(format!("{key}: {name}"));
                    }
                }
            }
        }
    }
    let complete =
        missing_images.is_empty() && missing_checkpoints.is_empty() && missing_states.is_empty();
    Ok(json!({
        "directory": directory.display().to_string(),
        "schemaVersion": schema,
        "libraries": libraries,
        "imagesChecked": checked_images,
        "missingImages": missing_images,
        "missingCheckpoints": missing_checkpoints,
        "missingPipelineStates": missing_states,
        "externalModels": models.into_iter().collect::<Vec<_>>(),
        "selfContained": false,
        "complete": complete,
        "note": "Une sauvegarde n'est jamais autonome : les images originales, modèles et LoRA restent des références externes.",
    }))
}

/// Free space required before writing a full database copy, as a fraction of the database size.
pub const BACKUP_SPACE_FACTOR: f64 = 1.2;

/// Keep only the `keep` most recent files in `dir` whose name starts with `prefix`
/// and ends with `suffix`. Recency is the modification time, then the file name.
/// Returns the number of deleted files. Other files are never touched.
pub fn rotate_backups(dir: &Path, prefix: &str, suffix: &str, keep: usize) -> Result<usize> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(prefix) || !name.ends_with(suffix) {
            continue;
        }
        let meta = entry.metadata()?;
        if !meta.is_file() {
            continue;
        }
        found.push((meta.modified()?, name, entry.path()));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let mut removed = 0;
    for (_, _, path) in found.into_iter().skip(keep) {
        for sidecar in ["-wal", "-shm"] {
            let mut name = path.clone().into_os_string();
            name.push(sidecar);
            let _ = fs::remove_file(name);
        }
        fs::remove_file(path)?;
        removed += 1;
    }
    Ok(removed)
}

/// Free bytes on the filesystem holding `path`, or `None` when it cannot be determined
/// (no dependency on libc: this asks `df`, and an unknown answer never blocks).
pub fn available_space(path: &Path) -> Option<u64> {
    let out = std::process::Command::new("df")
        .args(["-Pk", "--"])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let kib: u64 = text.lines().nth(1)?.split_whitespace().nth(3)?.parse().ok()?;
    kib.checked_mul(1024)
}

/// Refuse a backup that would not fit: `available` must cover `db_bytes * 1.2`.
/// `available` is injectable; `None` (unknown) lets the operation proceed.
pub fn ensure_free_space(db_bytes: u64, available: Option<u64>) -> Result<()> {
    let Some(available) = available else {
        return Ok(());
    };
    let required = (db_bytes as f64 * BACKUP_SPACE_FACTOR).ceil() as u64;
    if available < required {
        return Err(format!(
            "Not enough free disk space for a database backup: {required} bytes required, {available} available; operation refused"
        )
        .into());
    }
    Ok(())
}

/// Check the real free space of `dir` for a backup of the database file `db`.
pub fn ensure_backup_space(db: &Path, dir: &Path) -> Result<()> {
    let size = fs::metadata(db).map(|m| m.len()).unwrap_or(0);
    ensure_free_space(size, available_space(dir))
}
