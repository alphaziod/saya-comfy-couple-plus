//! Durable pass-state storage and the future RAM/VRAM pass-boundary contract.
//!
//! States of a pipeline pass (IMAGE/LATENT for now) are saved as hash-addressed,
//! integrity-checked files under a managed store derived from the database
//! location, and registered in a versioned manifest. Nothing is deleted
//! implicitly: cleanup only removes single files explicitly registered here,
//! validated against the current manifest, and never a state pinned as needed
//! for resume. No automatic deletion happens at startup or store opening.
//!
//! The [`PassMemoryManager`] trait is the contract for the future real RAM/VRAM
//! lifecycle (ROCm/AMD release owned by the engine integration). The provided
//! [`NoopPassMemoryManager`] performs no release at all and never claims one.
use crate::{
    checkpoints::{create_file, store_lock},
    gallery::{digest, file_digest},
    Database, Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
const FORMAT: &str = "saya-pass-state-store";
const VERSION: u32 = 1;
const MANIFEST: &str = "manifest.json";
const DATA: &str = "data";
const LOCK_BUSY: &str = "Stockage d'états temporaires occupé";
const MAX_NAME: usize = 64;
const MAX_STEP: usize = 200;
const MAX_ENTRIES: usize = 100_000;
const MAX_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;
const MIN_STORE_BUDGET: u64 = 1024 * 1024;
const MAX_STORE_BUDGET: u64 = 16 * 1024 * 1024 * 1024 * 1024;
const MAX_STATE_BYTES: u64 = 16 * 1024 * 1024 * 1024;

/// Kind of saved pass state. Extensible only by adding a variant and versioning
/// the manifest: unknown kinds are refused on read, never silently reinterpreted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StateKind {
    Image,
    Latent,
}

impl StateKind {
    fn extension(self) -> &'static str {
        match self {
            Self::Image => "png",
            Self::Latent => "safetensors",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Latent => "latent",
        }
    }
}

/// Declarative retention of one state.
/// `Pinned` marks a state needed for resume: cleanup must never select it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Retention {
    Pinned,
    Normal,
    Ephemeral,
}

impl Retention {
    fn name(self) -> &'static str {
        match self {
            Self::Pinned => "pinned",
            Self::Normal => "normal",
            Self::Ephemeral => "ephemeral",
        }
    }
}

/// One registered state: hash-addressed file plus its producer context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateEntry {
    pub sha256: String,
    pub file: String,
    pub kind: StateKind,
    pub pipeline: String,
    pub pass: String,
    pub producer_step: String,
    pub bytes: u64,
    pub created_at: String,
    pub created_at_secs: u64,
    pub retention: Retention,
}

impl StateEntry {
    pub fn label(&self) -> String {
        format!("{}/{}/{}", self.pipeline, self.pass, self.sha256)
    }
}

/// Store-level registry. The final manifest is the only proof of registration:
/// files written before it are orphans, entries without files are missing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoreManifest {
    pub format: String,
    pub version: u32,
    pub budget_bytes: u64,
    pub entries: Vec<StateEntry>,
}

/// New state to register; content is provided fully and bounded.
pub struct NewState<'a> {
    pub pipeline: &'a str,
    pub pass: &'a str,
    pub producer_step: &'a str,
    pub kind: StateKind,
    pub retention: Retention,
    pub bytes: &'a [u8],
    pub max_state_bytes: u64,
}

/// Declarative retention bounds, per status. `Pinned` states have no policy:
/// they are always protected.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub ephemeral: StatusPolicy,
    pub normal: StatusPolicy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusPolicy {
    pub max_total_bytes: Option<u64>,
    pub max_age_secs: Option<u64>,
    pub max_count: Option<u32>,
}

/// Detection result of [`audit_store`]. Reports only; never deletes.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Issue {
    Orphan {
        file: String,
    },
    Missing {
        label: String,
    },
    SizeMismatch {
        label: String,
        expected: u64,
        actual: u64,
    },
    Corrupt {
        label: String,
    },
    PartialResidual {
        file: String,
    },
    Symlink {
        file: String,
    },
}

/// Selection result of [`evaluate_cleanup`]. Selection only; nothing is deleted
/// until [`apply_cleanup`] is explicitly called.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPlan {
    pub selected: Vec<StateEntry>,
    pub protected: Vec<String>,
    pub freed_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CleanupReport {
    pub removed: Vec<String>,
    pub freed_bytes: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// UTC RFC3339 rendering of an epoch second count, without external dependencies.
fn format_utc(secs: u64) -> String {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    // Howard Hinnant's civil_from_days, valid over the whole u64 epoch range.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

fn valid_name(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_NAME
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("Segment de chemin d'état invalide (alphanumérique, _ ou -)".into());
    }
    Ok(())
}

fn valid_step(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > MAX_STEP {
        return Err("Étape productrice absente ou trop longue".into());
    }
    Ok(())
}

fn valid_hash(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Empreinte d'état invalide".into());
    }
    Ok(())
}

fn entry_file(kind: StateKind, pipeline: &str, pass: &str, sha256: &str) -> String {
    format!("{DATA}/{pipeline}/{pass}/{sha256}.{}", kind.extension())
}

/// The manifest path is never trusted: it is recomputed from the entry fields
/// and compared, so a tampered registry cannot point outside the store.
fn validate_entry(entry: &StateEntry) -> Result<()> {
    valid_name(&entry.pipeline)?;
    valid_name(&entry.pass)?;
    valid_hash(&entry.sha256)?;
    valid_step(&entry.producer_step)?;
    if entry.bytes == 0 {
        return Err("État vide enregistré".into());
    }
    if entry.created_at.is_empty() || entry.created_at.len() > 32 {
        return Err("Date de création d'état invalide".into());
    }
    let expected = entry_file(entry.kind, &entry.pipeline, &entry.pass, &entry.sha256);
    if entry.file != expected {
        return Err("Chemin d'état incohérent avec son contenu déclaré".into());
    }
    Ok(())
}

fn validate_root(root: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(root)?;
    if meta.file_type().is_symlink() {
        return Err("Racine d'états symbolique refusée".into());
    }
    if !meta.is_dir() {
        return Err("Racine d'états non régulière".into());
    }
    Ok(())
}

fn read_manifest(root: &Path) -> Result<StoreManifest> {
    let path = root.join(MANIFEST);
    let meta = fs::symlink_metadata(&path)?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err("Manifeste du stockage d'états absent ou non régulier".into());
    }
    if meta.len() > MAX_MANIFEST_BYTES {
        return Err("Manifeste d'états trop volumineux".into());
    }
    let manifest: StoreManifest = serde_json::from_slice(&fs::read(&path)?)?;
    if manifest.format != FORMAT || manifest.version != VERSION {
        return Err("Stockage d'états temporaires inconnu".into());
    }
    if manifest.entries.len() > MAX_ENTRIES {
        return Err("Trop d'états enregistrés".into());
    }
    if !(MIN_STORE_BUDGET..=MAX_STORE_BUDGET).contains(&manifest.budget_bytes) {
        return Err("Budget d'états hors bornes".into());
    }
    for entry in &manifest.entries {
        validate_entry(entry)?;
    }
    Ok(manifest)
}

/// Atomic publication: fsync of a `.part` file then rename. An interrupted
/// publication leaves a residual `.part` file, detected by [`audit_store`].
fn write_manifest(root: &Path, manifest: &StoreManifest) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(manifest)?;
    let temporary = root.join(format!(".{MANIFEST}.part-{}", uuid::Uuid::new_v4()));
    {
        let mut file = create_file(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    fs::rename(&temporary, root.join(MANIFEST))?;
    #[cfg(unix)]
    File::open(root)?.sync_all()?;
    Ok(())
}

fn state_path(root: &Path, entry: &StateEntry) -> PathBuf {
    root.join(&entry.file)
}

fn check_state_file(root: &Path, entry: &StateEntry) -> Result<()> {
    let path = state_path(root, entry);
    let meta = fs::symlink_metadata(&path)?;
    if meta.file_type().is_symlink() {
        return Err("État symbolique refusé".into());
    }
    if !meta.is_file() {
        return Err("État absent ou non régulier".into());
    }
    if meta.len() != entry.bytes {
        return Err("Taille d'état divergente".into());
    }
    if file_digest(&path)? != entry.sha256 {
        return Err("Empreinte d'état invalide".into());
    }
    Ok(())
}

fn create_state_directory(root: &Path, pipeline: &str, pass: &str) -> Result<PathBuf> {
    let dir = root.join(DATA).join(pipeline).join(pass);
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&dir)?;
    for path in [root.join(DATA), root.join(DATA).join(pipeline), dir.clone()] {
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return Err("Chemin d'états non régulier refusé".into());
        }
    }
    Ok(dir)
}

/// Open (or idempotently create) the managed store. A foreign directory is
/// never adopted: without this store's manifest the operation is refused.
pub fn init_store(root: &Path, budget_bytes: u64) -> Result<StoreManifest> {
    if !(MIN_STORE_BUDGET..=MAX_STORE_BUDGET).contains(&budget_bytes) {
        return Err("Budget d'états temporaires hors bornes".into());
    }
    match fs::symlink_metadata(root) {
        Err(_) => {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(root)?;
        }
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err("Racine d'états symbolique refusée".into());
            }
            if !meta.is_dir() {
                return Err("Racine d'états non régulière".into());
            }
            if root.join(MANIFEST).exists() {
                let existing = read_manifest(root)?;
                if existing.budget_bytes != budget_bytes {
                    return Err("Budget déclaré contradictoire avec le stockage existant".into());
                }
                return Ok(existing);
            }
            if fs::read_dir(root)?.next().is_some() {
                return Err("Dossier étranger non vide ; initialisation d'états refusée".into());
            }
        }
    }
    let manifest = StoreManifest {
        format: FORMAT.into(),
        version: VERSION,
        budget_bytes,
        entries: Vec::new(),
    };
    write_manifest(root, &manifest)?;
    Ok(manifest)
}

/// Open an existing managed store and validate every registered entry.
pub fn open_store(root: &Path) -> Result<StoreManifest> {
    validate_root(root)?;
    read_manifest(root)
}

/// Explicit budget resize; refused while registered states exceed the target.
pub fn resize_budget(root: &Path, budget_bytes: u64) -> Result<StoreManifest> {
    if !(MIN_STORE_BUDGET..=MAX_STORE_BUDGET).contains(&budget_bytes) {
        return Err("Budget d'états temporaires hors bornes".into());
    }
    let _lock = store_lock(root, LOCK_BUSY)?;
    let mut manifest = read_manifest(root)?;
    let used: u64 = manifest.entries.iter().map(|e| e.bytes).sum();
    if used > budget_bytes {
        return Err("Budget plus faible que les états enregistrés".into());
    }
    manifest.budget_bytes = budget_bytes;
    write_manifest(root, &manifest)?;
    Ok(manifest)
}

/// Save one state durably, then register it. The file is written and verified
/// before manifest publication; an interruption leaves a detectable orphan,
/// never a registered-but-absent state. Identical (pipeline, pass, content)
/// saves are deduplicated to the existing entry.
pub fn save_state(root: &Path, state: &NewState) -> Result<StateEntry> {
    valid_name(state.pipeline)?;
    valid_name(state.pass)?;
    valid_step(state.producer_step)?;
    if state.bytes.is_empty() {
        return Err("Contenu d'état vide refusé".into());
    }
    if state.max_state_bytes == 0 || state.max_state_bytes > MAX_STATE_BYTES {
        return Err("Borne de taille d'état hors bornes".into());
    }
    let size = u64::try_from(state.bytes.len())?;
    if size > state.max_state_bytes {
        return Err("État au-delà de la borne de taille déclarée".into());
    }
    validate_root(root)?;
    let _lock = store_lock(root, LOCK_BUSY)?;
    let mut manifest = read_manifest(root)?;
    let sha256 = digest(state.bytes);
    if let Some(index) = manifest
        .entries
        .iter()
        .position(|e| e.pipeline == state.pipeline && e.pass == state.pass && e.sha256 == sha256)
    {
        // A duplicate is only a success if the stored state is still valid,
        // of the same kind, and at least as protected as requested: callers
        // such as `pass_boundary` release memory on this success.
        let existing = &manifest.entries[index];
        if existing.kind != state.kind {
            return Err("État déjà enregistré avec un autre type ; aucun remplacement".into());
        }
        check_state_file(root, existing)?;
        if state.retention == Retention::Pinned && existing.retention != Retention::Pinned {
            manifest.entries[index].retention = Retention::Pinned;
            write_manifest(root, &manifest)?;
        }
        return Ok(manifest.entries[index].clone());
    }
    let used: u64 = manifest.entries.iter().map(|e| e.bytes).sum();
    if used
        .checked_add(size)
        .is_none_or(|total| total > manifest.budget_bytes)
    {
        return Err("Budget d'états temporaires dépassé ; aucun fichier supprimé".into());
    }
    let file = entry_file(state.kind, state.pipeline, state.pass, &sha256);
    let dir = create_state_directory(root, state.pipeline, state.pass)?;
    let destination = root.join(&file);
    if destination.exists() {
        // Pre-existing bytes with this exact name must already match their hash,
        // otherwise the store is inconsistent and nothing is overwritten.
        if !fs::symlink_metadata(&destination)?.file_type().is_file()
            || file_digest(&destination)? != sha256
        {
            return Err("Fichier d'état existant incohérent".into());
        }
    } else {
        let temporary = dir.join(format!(".partial-{}", uuid::Uuid::new_v4()));
        {
            let mut output = create_file(&temporary)?;
            output.write_all(state.bytes)?;
            output.sync_all()?;
        }
        if file_digest(&temporary)? != sha256 {
            return Err("État modifié pendant l'écriture ; fichier partiel conservé".into());
        }
        fs::hard_link(&temporary, &destination)?;
        fs::remove_file(&temporary)?;
        #[cfg(unix)]
        File::open(&dir)?.sync_all()?;
    }
    let entry = StateEntry {
        sha256,
        file,
        kind: state.kind,
        pipeline: state.pipeline.into(),
        pass: state.pass.into(),
        producer_step: state.producer_step.into(),
        bytes: size,
        created_at: format_utc(now_secs()),
        created_at_secs: now_secs(),
        retention: state.retention,
    };
    manifest.entries.push(entry.clone());
    write_manifest(root, &manifest)?;
    Ok(entry)
}

fn find_entry<'m>(
    manifest: &'m StoreManifest,
    pipeline: &str,
    pass: &str,
    sha256: &str,
) -> Result<&'m StateEntry> {
    valid_name(pipeline)?;
    valid_name(pass)?;
    valid_hash(sha256)?;
    manifest
        .entries
        .iter()
        .find(|e| e.pipeline == pipeline && e.pass == pass && e.sha256 == sha256)
        .ok_or_else(|| "État temporaire introuvable".into())
}

/// Verify one state on disk (regular file, exact size, exact hash).
pub fn verify_state(root: &Path, pipeline: &str, pass: &str, sha256: &str) -> Result<StateEntry> {
    validate_root(root)?;
    let manifest = read_manifest(root)?;
    let entry = find_entry(&manifest, pipeline, pass, sha256)?;
    check_state_file(root, entry)?;
    Ok(entry.clone())
}

/// Reload one state after full integrity verification (save → reload contract).
pub fn load_state(
    root: &Path,
    pipeline: &str,
    pass: &str,
    sha256: &str,
) -> Result<(StateEntry, Vec<u8>)> {
    let entry = verify_state(root, pipeline, pass, sha256)?;
    let file = File::open(state_path(root, &entry))?;
    let expected = usize::try_from(entry.bytes)?;
    let mut bytes = Vec::with_capacity(expected);
    let mut limit = file.take(entry.bytes + 1);
    limit.read_to_end(&mut bytes)?;
    if bytes.len() != expected {
        return Err("État modifié pendant la relecture".into());
    }
    if digest(&bytes) != entry.sha256 {
        return Err("Empreinte de l'état relu invalide".into());
    }
    Ok((entry, bytes))
}

/// Change the declarative retention of one state (pin for resume, or release).
pub fn set_retention(
    root: &Path,
    pipeline: &str,
    pass: &str,
    sha256: &str,
    retention: Retention,
) -> Result<StateEntry> {
    validate_root(root)?;
    let _lock = store_lock(root, LOCK_BUSY)?;
    let mut manifest = read_manifest(root)?;
    let entry = find_entry(&manifest, pipeline, pass, sha256)?.clone();
    let mut updated = entry.clone();
    updated.retention = retention;
    manifest
        .entries
        .retain(|e| e.pipeline != pipeline || e.pass != pass || e.sha256 != sha256);
    manifest.entries.push(updated.clone());
    write_manifest(root, &manifest)?;
    Ok(updated)
}

fn list_issues_of_entries(root: &Path, manifest: &StoreManifest) -> Result<Vec<Issue>> {
    let mut issues = Vec::new();
    for entry in &manifest.entries {
        let path = state_path(root, entry);
        match fs::symlink_metadata(&path) {
            Err(_) => issues.push(Issue::Missing {
                label: entry.label(),
            }),
            Ok(meta) if meta.file_type().is_symlink() => issues.push(Issue::Symlink {
                file: entry.file.clone(),
            }),
            Ok(meta) => {
                if !meta.is_file() {
                    issues.push(Issue::Missing {
                        label: entry.label(),
                    });
                } else if meta.len() != entry.bytes {
                    issues.push(Issue::SizeMismatch {
                        label: entry.label(),
                        expected: entry.bytes,
                        actual: meta.len(),
                    });
                } else if file_digest(&path)? != entry.sha256 {
                    issues.push(Issue::Corrupt {
                        label: entry.label(),
                    });
                }
            }
        }
    }
    Ok(issues)
}

fn is_partial(name: &str) -> bool {
    name.starts_with(".partial-") || name.starts_with(&format!(".{MANIFEST}.part-"))
}

/// Detect orphans and incomplete states: file without entry, entry without
/// file, divergent size, divergent hash, residual `.part` file, symlink where
/// a regular file is expected. Detection only; no deletion here.
pub fn audit_store(root: &Path) -> Result<Vec<Issue>> {
    validate_root(root)?;
    let manifest = read_manifest(root)?;
    let mut issues = list_issues_of_entries(root, &manifest)?;
    let mut managed: std::collections::BTreeSet<String> =
        manifest.entries.iter().map(|e| e.file.clone()).collect();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for child in fs::read_dir(&dir)? {
            let child = child?;
            let path = child.path();
            let meta = fs::symlink_metadata(&path)?;
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            if meta.file_type().is_symlink() {
                // Registered files are already reported by the entry checks
                // above; an unregistered symlink is dangerous on its own.
                if !managed.contains(&relative) {
                    issues.push(Issue::Symlink { file: relative });
                }
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
                continue;
            }
            if is_partial(&child.file_name().to_string_lossy()) {
                issues.push(Issue::PartialResidual { file: relative });
                continue;
            }
            if relative == MANIFEST || relative == ".store.lock" {
                continue;
            }
            if managed.remove(&relative) {
                // Registered file: integrity already verified above.
                continue;
            }
            issues.push(Issue::Orphan { file: relative });
        }
    }
    issues.sort_by(|a, b| {
        serde_json::to_string(a)
            .unwrap_or_default()
            .cmp(&serde_json::to_string(b).unwrap_or_default())
    });
    Ok(issues)
}

fn policy_for(policy: &RetentionPolicy, retention: Retention) -> Option<&StatusPolicy> {
    match retention {
        Retention::Pinned => None,
        Retention::Normal => Some(&policy.normal),
        Retention::Ephemeral => Some(&policy.ephemeral),
    }
}

/// Evaluate the declarative policy against the current store. Oldest states
/// are selected first within each status; ephemeral states are the natural
/// first candidates of their own policy. Pinned states are always protected.
/// This selects only: no deletion happens here, and nothing runs automatically.
pub fn evaluate_cleanup(
    root: &Path,
    policy: &RetentionPolicy,
    now_secs: u64,
) -> Result<CleanupPlan> {
    validate_root(root)?;
    let manifest = read_manifest(root)?;
    let mut selected: Vec<StateEntry> = Vec::new();
    let push = |entry: &StateEntry, selected: &mut Vec<StateEntry>| {
        if !selected.iter().any(|s| {
            s.pipeline == entry.pipeline && s.pass == entry.pass && s.sha256 == entry.sha256
        }) {
            selected.push(entry.clone());
        }
    };
    for retention in [Retention::Ephemeral, Retention::Normal] {
        let Some(bound) = policy_for(policy, retention) else {
            continue;
        };
        let mut group: Vec<&StateEntry> = manifest
            .entries
            .iter()
            .filter(|e| e.retention == retention)
            .collect();
        group.sort_by(|a, b| {
            (a.created_at_secs, &a.pipeline, &a.pass, &a.sha256).cmp(&(
                b.created_at_secs,
                &b.pipeline,
                &b.pass,
                &b.sha256,
            ))
        });
        if let Some(max_total) = bound.max_total_bytes {
            let mut running: u64 = group.iter().map(|e| e.bytes).sum();
            for entry in &group {
                if running <= max_total {
                    break;
                }
                running = running.saturating_sub(entry.bytes);
                push(entry, &mut selected);
            }
        }
        if let Some(max_age) = bound.max_age_secs {
            for entry in &group {
                if now_secs.saturating_sub(entry.created_at_secs) > max_age {
                    push(entry, &mut selected);
                }
            }
        }
        if let Some(max_count) = bound.max_count {
            // Oldest-first ordering: the overflow beyond the newest kept states.
            let keep = usize::try_from(max_count)?;
            let overflow = group.len().saturating_sub(keep);
            for entry in group.iter().take(overflow) {
                push(entry, &mut selected);
            }
        }
    }
    selected.sort_by_key(|e| e.label());
    let freed_bytes = selected.iter().map(|e| e.bytes).sum();
    let protected = manifest
        .entries
        .iter()
        .filter(|e| e.retention == Retention::Pinned)
        .map(|e| e.label())
        .collect();
    Ok(CleanupPlan {
        selected,
        protected,
        freed_bytes,
    })
}

/// Delete exactly the selected registered files. Every target is re-validated
/// against the current manifest before any removal: registered entry, not
/// pinned, regular non-symlink file strictly under the managed root, exact
/// size and hash. Only single registered files are removed — never the root,
/// never a recursive sweep, never an unmanaged path. A pinned state is refused
/// even if a caller hand-crafted a plan containing it.
pub fn apply_cleanup(root: &Path, plan: &CleanupPlan) -> Result<CleanupReport> {
    validate_root(root)?;
    let _lock = store_lock(root, LOCK_BUSY)?;
    let mut manifest = read_manifest(root)?;
    let mut targets: Vec<StateEntry> = Vec::new();
    for wanted in &plan.selected {
        let entry = find_entry(&manifest, &wanted.pipeline, &wanted.pass, &wanted.sha256)?;
        if entry.retention == Retention::Pinned {
            return Err(format!(
                "État épinglé nécessaire à la reprise ; suppression refusée : {}",
                entry.label()
            )
            .into());
        }
        if !targets.iter().any(|t| {
            t.pipeline == entry.pipeline && t.pass == entry.pass && t.sha256 == entry.sha256
        }) {
            targets.push(entry.clone());
        }
    }
    for target in &targets {
        check_state_file(root, target)
            .map_err(|e| format!("Nettoyage refusé pour {} : {e}", target.label()))?;
    }
    let mut removed: Vec<String> = Vec::new();
    let mut freed_bytes: u64 = 0;
    for target in &targets {
        let path = state_path(root, target);
        if let Err(error) = fs::remove_file(&path) {
            manifest
                .entries
                .retain(|e| !removed.iter().any(|r| *r == e.label()));
            write_manifest(root, &manifest)?;
            return Err(format!(
                "Suppression interrompue après {} fichiers ; registre reconcilié : {error}",
                removed.len()
            )
            .into());
        }
        freed_bytes = freed_bytes.saturating_add(target.bytes);
        removed.push(target.label());
        let _ = fs::remove_dir(root.join(DATA).join(&target.pipeline).join(&target.pass));
    }
    manifest
        .entries
        .retain(|e| !removed.iter().any(|r| *r == e.label()));
    write_manifest(root, &manifest)?;
    Ok(CleanupReport {
        removed,
        freed_bytes,
    })
}

/// Compact summary of the store for engines and future UI surfaces.
pub fn store_summary(root: &Path) -> Result<Value> {
    let manifest = open_store(root)?;
    let mut by_kind: BTreeMap<&str, u64> = BTreeMap::new();
    let mut by_retention: BTreeMap<&str, u64> = BTreeMap::new();
    for entry in &manifest.entries {
        *by_kind.entry(entry.kind.name()).or_default() += 1;
        *by_retention.entry(entry.retention.name()).or_default() += 1;
    }
    Ok(json!({
        "format": manifest.format,
        "version": manifest.version,
        "budgetBytes": manifest.budget_bytes,
        "usedBytes": manifest.entries.iter().map(|e| e.bytes).sum::<u64>(),
        "entries": manifest.entries.len(),
        "byKind": by_kind,
        "byRetention": by_retention,
    }))
}

/// Outcome of a memory-lifecycle call. `released: false` means nothing was
/// freed: callers must never assume resources disappeared.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryOutcome {
    pub backend: String,
    pub released: bool,
    pub details: String,
}

/// Contract for the RAM/VRAM lifecycle around passes (steps 3 and 4 of the
/// mission: release unused resources, load the next pass's resources). The
/// real ROCm/AMD implementation belongs to the engine integration; this trait
/// only fixes the boundary and the honesty rule: an implementation reports
/// `released: true` only for resources it actually released.
pub trait PassMemoryManager: Send + Sync {
    fn backend(&self) -> &'static str;
    /// Called only after the pass states are durably saved and registered.
    fn release_after_pass(&self, pipeline: &str, pass: &str) -> Result<MemoryOutcome>;
    /// Called before executing a pass; prepares or loads its resources.
    fn prepare_pass(&self, pipeline: &str, pass: &str) -> Result<MemoryOutcome>;
}

/// Documented no-op until the real ROCm/AMD backend exists. It performs no
/// RAM/VRAM release and no load, and never reports `released: true`.
pub struct NoopPassMemoryManager;

impl PassMemoryManager for NoopPassMemoryManager {
    fn backend(&self) -> &'static str {
        "noop"
    }
    fn release_after_pass(&self, pipeline: &str, pass: &str) -> Result<MemoryOutcome> {
        Ok(MemoryOutcome {
            backend: "noop".into(),
            released: false,
            details: format!("no-op : aucune libération RAM/VRAM réelle pour {pipeline}/{pass}"),
        })
    }
    fn prepare_pass(&self, pipeline: &str, pass: &str) -> Result<MemoryOutcome> {
        Ok(MemoryOutcome {
            backend: "noop".into(),
            released: false,
            details: format!("no-op : aucun chargement mémoire pour {pipeline}/{pass}"),
        })
    }
}

/// Pass boundary: durably save every declared state first, then request the
/// resource release. If any save fails, nothing is released and the error is
/// returned: disk always precedes memory release.
pub fn pass_boundary(
    manager: &dyn PassMemoryManager,
    root: &Path,
    pipeline: &str,
    pass: &str,
    states: &[NewState],
) -> Result<(Vec<StateEntry>, MemoryOutcome)> {
    for state in states {
        if state.pipeline != pipeline || state.pass != pass {
            return Err("État hors de la frontière de passe déclarée".into());
        }
    }
    let mut saved = Vec::with_capacity(states.len());
    for state in states {
        saved.push(save_state(root, state)?);
    }
    let outcome = manager.release_after_pass(pipeline, pass)?;
    Ok((saved, outcome))
}

impl Database {
    /// Managed pass-state root derived from the database location, mirroring
    /// the existing `checkpoints` and `backups` conventions: no hardcoded path.
    pub fn pass_state_root(&self) -> Result<PathBuf> {
        let path = self
            .connection
            .path()
            .filter(|p| !p.is_empty())
            .ok_or("Base en mémoire : aucune racine fichier d'états")?;
        Ok(Path::new(path)
            .parent()
            .ok_or("Base sans dossier parent")?
            .join("pass-states"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_utc_timestamps() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(format_utc(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn rejects_names_hashes_and_files_outside_convention() {
        assert!(valid_name("pipeline-1").is_ok());
        for bad in ["", "a/b", "..", ".", ".hidden", "espace interdit", "a\\b"] {
            assert!(valid_name(bad).is_err(), "accepted {bad}");
        }
        assert!(valid_hash(&"a".repeat(64)).is_ok());
        for bad in [&"A".repeat(64), &"g".repeat(64), "abc"] {
            assert!(valid_hash(bad).is_err(), "accepted {bad}");
        }
        let entry = StateEntry {
            sha256: "a".repeat(64),
            file: "data/p/x/".to_owned() + &"a".repeat(64) + ".png",
            kind: StateKind::Image,
            pipeline: "p".into(),
            pass: "x".into(),
            producer_step: "step".into(),
            bytes: 1,
            created_at: "2026-10-16T00:00:00Z".into(),
            created_at_secs: 1,
            retention: Retention::Normal,
        };
        assert!(validate_entry(&entry).is_ok());
        let mut escaped = entry.clone();
        escaped.file = "data/p/x/../../etc.png".into();
        assert!(validate_entry(&escaped).is_err());
    }
}
