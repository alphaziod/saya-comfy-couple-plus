//! Execution of the draw engine (`test_solo.py`) in read-only mode: dry-run without images.
//!
//! The engine is never modified by the application: we run
//! `python3 test_solo.py --dry-run --graine N` in its directory (which contains a space:
//! "<config>/GROK BUILD/generez", see `paths::engine_dir`), with `PYTHONHASHSEED=0` for a stable order
//! of Python `set`s, then return the path of the produced catalogue
//! `sortie/dry-run/catalogue.json`. No ComfyUI request is sent by this module.

use crate::Result;
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Catalogue produced by a dry-run, relative to the engine directory.
pub const DRY_RUN_CATALOGUE: &str = "sortie/dry-run/catalogue.json";

/// Catalogue produced by a 1-to-1 casting (slice U1), relative to the engine directory.
/// The engine always writes this fixed path; two castings must never run
/// in parallel (the last overwrites the first — same constraint as the dry-run).
pub const CASTING_CATALOGUE: &str = "sortie/casting/catalogue.json";

/// Sanity cap on the number of OCs in a casting (the engine refuses beyond its attempts).
pub const CASTING_MAX: u32 = 200;

/// Delay before interrupting the engine (a dry-run lasts ~40 s, longer with a cold cache).
pub const DRY_RUN_TIMEOUT_SECS: u64 = 600;

/// Engine flag excluding actions from the casting (Saya's feedback 6, 2026-10-26:
/// "exclude actions from the pull", complement of `--actions`). A/B SLICE CONVERGENCE
/// DONE on the name: the engine slice (W1, 2026-10-26) implements `--exclude
/// a,b,c` in `test_solo.py` — exclusions removed from the pool BEFORE the draw,
/// exclusion wins over `--actions`, also valid with `--pose-fixe` and `--famille`,
/// refused with `--persos` and the `--poses` rotations. Since the engine SILENTLY
/// IGNORES unknown arguments, [`exclusion_supported`] refuses to launch
/// as long as the engine at that path does not carry the flag — a lost and
/// silent exclusion is the opposite of the contract.
pub const CASTING_EXCLUDE_FLAG: &str = "--exclude";

/// Read cap for the engine's stdout/stderr.
const LIMIT: usize = 16_000_000;

/// Result of a dry-run: path of the produced catalogue and captured outputs.
#[derive(Debug, Clone)]
pub struct DryRunOutcome {
    pub graine: u64,
    pub catalogue: PathBuf,
    pub stdout: String,
    pub stderr: String,
}

/// Engine directory: `SAYA_ENGINE_DIR` if set, otherwise `<config>/GROK BUILD/generez` (see `paths`).
pub fn engine_dir() -> PathBuf {
    crate::paths::engine_dir()
}

/// Reads literal capability declarations; never imports or executes the engine here.
pub fn casting_capabilities(dir: &Path) -> Result<serde_json::Value> {
    casting_bridge(dir, None, None)
}

pub fn casting_mode(
    dir: &Path,
    mode: &str,
    shots: &[serde_json::Value],
) -> Result<Vec<serde_json::Value>> {
    let value = casting_bridge(dir, Some(mode), Some(shots))?;
    Ok(serde_json::from_value(value)?)
}

fn casting_bridge(
    dir: &Path,
    mode: Option<&str>,
    shots: Option<&[serde_json::Value]>,
) -> Result<serde_json::Value> {
    use std::io::Write;
    let mut command = Command::new("python3");
    command
        .args(["-B", "-c", include_str!("casting_bridge.py")])
        .arg(dir)
        .current_dir(dir);
    if let Some(mode) = mode {
        command.arg(mode);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().ok_or("stdout absent")?;
    let stderr = child.stderr.take().ok_or("stderr absent")?;
    let output = thread::spawn(move || read_bounded(stdout));
    let errors = thread::spawn(move || read_bounded(stderr));
    if let Some(shots) = shots {
        let input = serde_json::to_vec(shots)?;
        let mut stdin = child.stdin.take().ok_or("stdin absent")?;
        // Drain outputs concurrently, and never retain stdin after writing.
        thread::spawn(move || stdin.write_all(&input));
    } else {
        drop(child.stdin.take());
    }
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Lecture des capacités moteur trop longue".into());
        }
        thread::sleep(Duration::from_millis(25));
    };
    let stdout = output.join().map_err(|_| "Lecture moteur interrompue")??;
    let stderr = errors.join().map_err(|_| "Lecture erreurs interrompue")??;
    if stdout.len() > LIMIT || stderr.len() > LIMIT {
        return Err("Réponse moteur trop volumineuse".into());
    }
    if !status.success() {
        return Err(format!("Adaptateur casting : {}", String::from_utf8_lossy(&stderr)).into());
    }
    Ok(serde_json::from_slice(&stdout)?)
}

/// Runs the engine dry-run in the default directory (constant or `SAYA_ENGINE_DIR`).
pub fn dry_run(graine: u64) -> Result<DryRunOutcome> {
    dry_run_in_with_timeout(
        &engine_dir(),
        graine,
        Duration::from_secs(DRY_RUN_TIMEOUT_SECS),
    )
}

/// The same dry-run in an explicit engine directory (tests, configuration).
pub fn dry_run_in(dir: &Path, graine: u64) -> Result<DryRunOutcome> {
    dry_run_in_with_timeout(dir, graine, Duration::from_secs(DRY_RUN_TIMEOUT_SECS))
}

/// Runs `python3 test_solo.py --dry-run --graine N` in `dir` with `PYTHONHASHSEED=0`,
/// a timeout, stdout/stderr captured, then returns the path of the produced
/// catalogue. The process is killed past the timeout and the error is clean.
pub fn dry_run_in_with_timeout(
    dir: &Path,
    graine: u64,
    timeout: Duration,
) -> Result<DryRunOutcome> {
    run_engine(
        dir,
        &["--dry-run".into(), "--graine".into(), graine.to_string()],
        DRY_RUN_CATALOGUE,
        timeout,
        graine,
    )
}

/// 1-to-1 casting (slice U1): `python3 test_solo.py --casting N [--famille X] --graine G`
/// in the default engine directory, `PYTHONHASHSEED=0`. No image: the catalogue
/// `sortie/casting/catalogue.json` carries the N OCs and their pass-1 values. Family
/// `None` (or empty after trimming) asks for the normal draw.
pub fn casting(graine: u64, nombre: u32, famille: Option<&str>) -> Result<DryRunOutcome> {
    casting_in_with_timeout(
        &engine_dir(),
        graine,
        nombre,
        famille,
        Duration::from_secs(DRY_RUN_TIMEOUT_SECS),
    )
}

/// Checks that the engine at `dir` knows [`CASTING_EXCLUDE_FLAG`]: reads the
/// real `test_solo.py` (the same file `run_engine` will execute). Without this guard,
/// the engine would ignore the unknown argument and draw the excluded actions silently.
pub fn exclusion_supported(dir: &Path) -> Result<bool> {
    let script = dir.join("test_solo.py");
    let raw = std::fs::read(&script)
        .map_err(|e| format!("Moteur illisible ({}): {e}", script.display()))?;
    Ok(String::from_utf8_lossy(&raw).contains(CASTING_EXCLUDE_FLAG))
}

/// Validation of the excluded action names (same charset as `--actions`: letters,
/// digits, `_`, deduplicated in order, empty ones ignored). The existence of an action
/// in the bank is still validated by the ENGINE (`valider_exclusions`, W1) — this layer
/// never reinterprets the bank.
fn clean_exclusions(exclusions: &[String]) -> Result<Vec<String>> {
    let mut clean = Vec::new();
    for exclusion in exclusions {
        let exclusion = exclusion.trim();
        if exclusion.is_empty() {
            continue;
        }
        if exclusion.len() > 100
            || !exclusion
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(format!(
                "Casting : nom d'action exclue invalide ({exclusion}) — attendu un nom de la banque du moteur (lettres, chiffres, _)"
            )
            .into());
        }
        if clean.iter().any(|v| v == exclusion) {
            return Err(format!("Casting : action exclue en doublon ({exclusion})").into());
        }
        clean.push(exclusion.to_owned());
    }
    if clean.is_empty() {
        return Err("Casting : aucune action exclue".into());
    }
    Ok(clean)
}

/// Validates then appends `--exclude a,b,c` to the engine arguments (W1 convergence):
/// shared by the three launches that accept exclusion — free draw
/// ([`casting_exclude_in_with_timeout`]), fixed pose ([`casting_pose_in_with_timeout`]) and
/// targeted actions (`actions_bank::casting_actions_in_with_timeout`, exclusion wins).
/// Refused BEFORE any spawn if the engine at that path ignores the flag (see
/// [`exclusion_supported`]). Empty list: no addition, no refusal.
pub(crate) fn push_exclusions(
    dir: &Path,
    args: &mut Vec<String>,
    exclusions: &[String],
) -> Result<()> {
    if exclusions.is_empty() {
        return Ok(());
    }
    let exclusions = clean_exclusions(exclusions)?;
    if !exclusion_supported(dir)? {
        return Err(format!(
            "Le moteur de ce chemin ne connaît pas « {CASTING_EXCLUDE_FLAG} » : exclusion refusée — le moteur ignore les arguments inconnus et l'exclusion serait perdue en silence (mettre à jour le moteur, tranche moteur W1)"
        )
        .into());
    }
    args.push(CASTING_EXCLUDE_FLAG.into());
    args.push(exclusions.join(","));
    Ok(())
}

/// Casting with EXCLUDED actions (Saya's feedback 6, 2026-10-26):
/// `python3 test_solo.py --casting N [--famille X] --exclude a,b,c --graine G` — the
/// REAL engine draw removes these actions from the pool BEFORE the draw (not an
/// interface mask). Family `None` (or empty) asks for the normal draw. Clean refusal
/// BEFORE any launch if the engine at `dir` does not know the flag (see
/// [`exclusion_supported`]).
pub fn casting_exclude(
    graine: u64,
    nombre: u32,
    famille: Option<&str>,
    exclusions: &[String],
) -> Result<DryRunOutcome> {
    casting_exclude_in_with_timeout(
        &engine_dir(),
        graine,
        nombre,
        famille,
        exclusions,
        Duration::from_secs(DRY_RUN_TIMEOUT_SECS),
    )
}

/// The same exclusion casting in an explicit engine directory (tests, configuration).
pub fn casting_exclude_in_with_timeout(
    dir: &Path,
    graine: u64,
    nombre: u32,
    famille: Option<&str>,
    exclusions: &[String],
    timeout: Duration,
) -> Result<DryRunOutcome> {
    if nombre == 0 || nombre > CASTING_MAX {
        return Err(format!(
            "Casting : nombre d'OC hors limites ({nombre}, attendu 1..={CASTING_MAX})"
        )
        .into());
    }
    let famille = famille.map(str::trim).filter(|f| !f.is_empty());
    if let Some(famille) = famille {
        if famille.chars().all(|c| c.is_ascii_lowercase())
            && !if dir.join("moteur/registre_ids.py").is_file() {
                casting_capabilities(dir)?["families"]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|v| v == famille))
            } else {
                crate::casting::CASTING_FAMILIES.contains(&famille)
            }
        {
            return Err(format!("famille de casting inconnue : {famille}").into());
        }
        if !famille
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return Err(format!("Casting : famille invalide ({famille})").into());
        }
    }
    let mut args = vec!["--casting".into(), nombre.to_string()];
    if let Some(famille) = famille {
        args.push("--famille".into());
        args.push(famille.to_owned());
    }
    push_exclusions(dir, &mut args, exclusions)?;
    args.push("--graine".into());
    args.push(graine.to_string());
    run_engine(dir, &args, CASTING_CATALOGUE, timeout, graine)
}

/// Casting with an explicit timeout. Empty `famille` = normal draw.
pub fn casting_in_with_timeout(
    dir: &Path,
    graine: u64,
    nombre: u32,
    famille: Option<&str>,
    timeout: Duration,
) -> Result<DryRunOutcome> {
    if nombre == 0 || nombre > CASTING_MAX {
        return Err(format!(
            "Casting : nombre d'OC hors limites ({nombre}, attendu 1..={CASTING_MAX})"
        )
        .into());
    }
    let famille = famille.map(str::trim).filter(|f| !f.is_empty());
    if let Some(famille) = famille {
        if famille.chars().all(|c| c.is_ascii_lowercase())
            && !if dir.join("moteur/registre_ids.py").is_file() {
                casting_capabilities(dir)?["families"]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|v| v == famille))
            } else {
                crate::casting::CASTING_FAMILIES.contains(&famille)
            }
        {
            return Err(format!("famille de casting inconnue : {famille}").into());
        }
        if !famille
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return Err(format!("Casting : famille invalide ({famille})").into());
        }
    }
    let mut args = vec!["--casting".into(), nombre.to_string()];
    if let Some(famille) = famille {
        args.push("--famille".into());
        args.push(famille.to_owned());
    }
    args.push("--graine".into());
    args.push(graine.to_string());
    run_engine(dir, &args, CASTING_CATALOGUE, timeout, graine)
}

/// Mode A rotations (slice U2, 2026-10-25): `python3 test_solo.py --poses <ID>
/// --casting N --graine G` in the default engine directory — ONE character of the batch
/// (id `CH--NNNN`, Saya sheet `CH-…` or `bank_id`) replayed N times with THE SAME pose, identical
/// prompts, pass-1 catalogue in `sortie/casting/catalogue.json`.
pub fn casting_character(graine: u64, perso: &str, nombre: u32) -> Result<DryRunOutcome> {
    casting_character_in_with_timeout(
        &engine_dir(),
        graine,
        perso,
        nombre,
        Duration::from_secs(DRY_RUN_TIMEOUT_SECS),
    )
}

/// The same Mode A rotations in an explicit engine directory (tests, configuration).
pub fn casting_character_in_with_timeout(
    dir: &Path,
    graine: u64,
    perso: &str,
    nombre: u32,
    timeout: Duration,
) -> Result<DryRunOutcome> {
    if nombre == 0 || nombre > CASTING_MAX {
        return Err(format!(
            "Casting : nombre d'OC hors limites ({nombre}, attendu 1..={CASTING_MAX})"
        )
        .into());
    }
    let perso = perso.trim();
    if perso.is_empty()
        || perso.len() > 64
        || !perso.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(format!(
            "Casting : identifiant de perso invalide ({perso}) — attendu CH--NNNN, CH-NNNNN ou CH-NNNNN (banque)"
        )
        .into());
    }
    let args = vec![
        "--poses".into(),
        perso.to_owned(),
        "--casting".into(),
        nombre.to_string(),
        "--graine".into(),
        graine.to_string(),
    ];
    run_engine(dir, &args, CASTING_CATALOGUE, timeout, graine)
}

/// Character targeting (slice V1, 2026-10-25): `python3 test_solo.py --casting N
/// --persos ID1,ID2,… --graine G` in the default engine directory — EXACTLY these
/// characters (ids of the validated `CH--NNNN`, always in the batch, Saya sheets `CH-…`), one shot
/// each, in the requested order, pass-1 catalogue in `sortie/casting/catalogue.json`.
pub fn casting_characters(graine: u64, ids: &[String]) -> Result<DryRunOutcome> {
    casting_characters_in_with_timeout(
        &engine_dir(),
        graine,
        ids,
        Duration::from_secs(DRY_RUN_TIMEOUT_SECS),
    )
}

/// The same targeting in an explicit engine directory (tests, configuration).
pub fn casting_characters_in_with_timeout(
    dir: &Path,
    graine: u64,
    ids: &[String],
    timeout: Duration,
) -> Result<DryRunOutcome> {
    if ids.is_empty() || ids.len() > CASTING_MAX as usize {
        return Err(format!(
            "Casting : nombre de persos ciblés hors limites ({}, attendu 1..={CASTING_MAX})",
            ids.len()
        )
        .into());
    }
    let mut clean = Vec::with_capacity(ids.len());
    for id in ids {
        let id = id.trim();
        if id.is_empty()
            || id.len() > 64
            || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(format!(
                "Casting : identifiant de perso invalide ({id}) — attendu CH--NNNN (validée) ou CH-… (fiche)"
            )
            .into());
        }
        clean.push(id.to_owned());
    }
    let args = vec![
        "--casting".into(),
        clean.len().to_string(),
        "--persos".into(),
        clean.join(","),
        "--graine".into(),
        graine.to_string(),
    ];
    run_engine(dir, &args, CASTING_CATALOGUE, timeout, graine)
}

/// Mode B rotations (slice U2, 2026-10-25): `python3 test_solo.py --casting N
/// [--famille X] --pose-fixe <action> --graine G` in the default engine directory —
/// N OCs of the draw ALL carry THE SAME pose (validated solo action of the engine), pass-1
/// catalogue in `sortie/casting/catalogue.json`. Family `None` (or empty) asks for the
/// normal draw.
pub fn casting_pose(
    graine: u64,
    nombre: u32,
    famille: Option<&str>,
    pose: &str,
    exclusions: &[String],
) -> Result<DryRunOutcome> {
    casting_pose_in_with_timeout(
        &engine_dir(),
        graine,
        nombre,
        famille,
        pose,
        exclusions,
        Duration::from_secs(DRY_RUN_TIMEOUT_SECS),
    )
}

/// The same Mode B rotations in an explicit engine directory (tests, configuration).
pub fn casting_pose_in_with_timeout(
    dir: &Path,
    graine: u64,
    nombre: u32,
    famille: Option<&str>,
    pose: &str,
    exclusions: &[String],
    timeout: Duration,
) -> Result<DryRunOutcome> {
    if nombre == 0 || nombre > CASTING_MAX {
        return Err(format!(
            "Casting : nombre d'OC hors limites ({nombre}, attendu 1..={CASTING_MAX})"
        )
        .into());
    }
    let pose = pose.trim();
    if pose.is_empty()
        || pose.len() > 100
        || !pose
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(format!(
            "Casting : nom de pose fixe invalide ({pose}) — attendu un nom d'action du moteur (lettres, chiffres, _)"
        )
        .into());
    }
    let famille = famille.map(str::trim).filter(|f| !f.is_empty());
    if let Some(famille) = famille {
        if famille.chars().all(|c| c.is_ascii_lowercase())
            && !if dir.join("moteur/registre_ids.py").is_file() {
                casting_capabilities(dir)?["families"]
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|v| v == famille))
            } else {
                crate::casting::CASTING_FAMILIES.contains(&famille)
            }
        {
            return Err(format!("famille de casting inconnue : {famille}").into());
        }
        if !famille
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return Err(format!("Casting : famille invalide ({famille})").into());
        }
    }
    let mut args = vec!["--casting".into(), nombre.to_string()];
    if let Some(famille) = famille {
        args.push("--famille".into());
        args.push(famille.to_owned());
    }
    args.push("--pose-fixe".into());
    args.push(pose.to_owned());
    // W1 exclusion: the engine accepts it with --pose-fixe (the identity's action
    // pool is filtered, the imposed pose stays).
    push_exclusions(dir, &mut args, exclusions)?;
    args.push("--graine".into());
    args.push(graine.to_string());
    run_engine(dir, &args, CASTING_CATALOGUE, timeout, graine)
}

/// The five real modes of the `generez` fish menu (`~/.config/fish/functions/generez.fish`),
/// encoded EXACTLY: `attente` (case 0), `poses` (case 2), `lcs` (case 3), `duo`
/// (case 4), `bny20` (case 5). No fake "test" mode (Saya's feedback 2, 2026-10-26):
/// these are Saya's GPU commands; the engine handles its ComfyUI submission itself.
pub const ENGINE_SCRIPTS: [&str; 5] = ["attente", "poses", "lcs", "duo", "bny20"];

/// Default timeout of a real engine script (`engine_script`): beyond it, the process
/// is killed and the captured partial log is returned as is (`timeout:true`).
pub const SCRIPT_TIMEOUT_SECS: u64 = 3600;

/// Log of a real engine script: exact argv, exit code, captured outputs.
/// A script failure is a valid log (never turned into a success).
#[derive(Debug, Clone)]
pub struct ScriptOutcome {
    pub script: String,
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub success: bool,
    pub code: Option<i32>,
    pub timeout: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs ONE of the five real scripts from the engine directory `dir`, without a shell:
/// `Command::new` receives the program and a closed argv (the engine path contains a
/// space — everything goes through process arguments, never through an interpreter).
/// `PYTHONHASHSEED=0` on every engine run. `id_perso` is required by `poses`
/// (`lancer_poses.bash ID`), refused for the others. The `solo_x7/...` paths are the
/// neighbours of the engine directory, as in the fish function.
pub fn run_engine_script(
    dir: &Path,
    script: &str,
    id_perso: Option<&str>,
    timeout: Duration,
) -> Result<ScriptOutcome> {
    let script = script.trim();
    if !ENGINE_SCRIPTS.contains(&script) {
        return Err(format!(
            "Script moteur inconnu ({script}) — attendus : {}",
            ENGINE_SCRIPTS.join(", ")
        )
        .into());
    }
    let solo_x7 = dir
        .parent()
        .ok_or("Répertoire moteur sans parent")?
        .join("solo_x7");
    let (programme, args, envs): (String, Vec<String>, Vec<(&'static str, String)>) = match script {
        "attente" => (
            "python3".into(),
            vec![
                solo_x7
                    .join("attente_solo")
                    .join("tester.py")
                    .to_string_lossy()
                    .into_owned(),
                "--gpu".into(),
            ],
            vec![],
        ),
        "poses" => {
            let id = id_perso
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .ok_or("Script poses : identifiant du perso requis (ex. CH--9062, CH-09724)")?;
            if id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return Err(format!(
                    "Script poses : identifiant de perso invalide ({id}) — attendu CH--NNNN ou CH-NNNNN"
                )
                .into());
            }
            (
                "bash".into(),
                vec![
                    dir.join("lancer_poses.bash").to_string_lossy().into_owned(),
                    id.to_owned(),
                ],
                vec![],
            )
        }
        "lcs" => {
            if id_perso.is_some() {
                return Err("Script lcs : id_perso ne s'applique qu'au script poses".into());
            }
            (
                "bash".into(),
                vec![
                    dir.join("lancer_test_solo.bash")
                        .to_string_lossy()
                        .into_owned(),
                    "--reprise".into(),
                ],
                vec![
                    ("SOLO_MODE", "futa".to_owned()),
                    ("SOLO_LOT", "lcs".to_owned()),
                ],
            )
        }
        "duo" => {
            if id_perso.is_some() {
                return Err("Script duo : id_perso ne s'applique qu'au script poses".into());
            }
            (
                "python3".into(),
                vec![
                    dir.join("outils")
                        .join("lot_duo.py")
                        .to_string_lossy()
                        .into_owned(),
                    "--gpu".into(),
                ],
                vec![],
            )
        }
        "bny20" => {
            if id_perso.is_some() {
                return Err("Script bny20 : id_perso ne s'applique qu'au script poses".into());
            }
            (
                "python3".into(),
                vec![
                    solo_x7.join("test_bny20.py").to_string_lossy().into_owned(),
                    "--gpu".into(),
                ],
                vec![],
            )
        }
        _ => unreachable!("script filtré par ENGINE_SCRIPTS"),
    };
    // The first argument is the target script (python3/bash afterwards): existence
    // checked BEFORE any spawn for a clear error.
    let target = Path::new(&args[0]);
    if !target.is_file() {
        return Err(format!(
            "Script moteur introuvable : {} (chemin configurable par SAYA_ENGINE_DIR, défaut « ~/Documents/saya studio config/GROK BUILD/generez »)",
            target.display()
        )
        .into());
    }
    let mut commande = Command::new(&programme);
    commande
        .args(&args)
        .current_dir(dir)
        .env("PYTHONHASHSEED", "0");
    for (key, valeur) in &envs {
        commande.env(key, valeur);
    }
    let mut child = commande
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "Lancement du script moteur impossible dans {} : {e}",
                dir.display()
            )
        })?;
    let stdout = child.stdout.take().ok_or("stdout du script absent")?;
    let stderr = child.stderr.take().ok_or("stderr du script absent")?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout));
    let stderr_reader = thread::spawn(move || read_bounded(stderr));
    let started = Instant::now();
    let (status, timeout_atteint) = loop {
        match child.try_wait() {
            Ok(Some(status)) => break (Some(status), false),
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break (None, true);
            }
        }
    };
    let stdout = String::from_utf8_lossy(
        &stdout_reader
            .join()
            .map_err(|_| "Lecture script interrompue")??,
    )
    .into_owned();
    let stderr = String::from_utf8_lossy(
        &stderr_reader
            .join()
            .map_err(|_| "Lecture erreur script interrompue")??,
    )
    .into_owned();
    Ok(ScriptOutcome {
        script: script.to_owned(),
        argv: {
            let mut complet = vec![programme];
            complet.extend(args);
            complet
        },
        cwd: dir.to_path_buf(),
        success: status.is_some_and(|s| s.success()),
        code: status.as_ref().and_then(|s| s.code()),
        timeout: timeout_atteint,
        stdout,
        stderr,
    })
}

/// Runs `python3 test_solo.py <args>` in `dir` with `PYTHONHASHSEED=0`, a
/// timeout, stdout/stderr captured, then returns the path of the produced catalogue.
/// The process is killed past the timeout and the error is clean.
/// `pub(crate)`: the encodings of the neighbouring engine modes (casting.rs, actions_bank.rs)
/// reuse the same pipe — never a home-grown second spawn.
pub fn casting_subfamily(
    graine: u64,
    nombre: u32,
    family: &str,
    subfamily: &str,
    actions: &[String],
    exclusions: &[String],
    pose: Option<&str>,
) -> Result<DryRunOutcome> {
    let dir = engine_dir();
    if nombre == 0 || nombre > CASTING_MAX {
        return Err("Nombre de casting hors limites".into());
    }
    let capabilities = casting_capabilities(&dir)?;
    if !capabilities["subfamilies"][family]
        .as_array()
        .is_some_and(|a| a.iter().any(|v| v == subfamily))
    {
        return Err("Sous-famille non fournie par le moteur".into());
    }
    let mut args = vec![
        "--casting".into(),
        nombre.to_string(),
        "--famille".into(),
        family.into(),
        "--graine".into(),
        graine.to_string(),
    ];
    if !actions.is_empty() || pose.is_some() {
        let available = crate::actions_bank::list_actions(&dir)?;
        for name in actions.iter().map(String::as_str).chain(pose) {
            if !available
                .iter()
                .any(|a| a["name"] == name && a["disponible"] == true)
            {
                return Err(format!("Action indisponible : {name}").into());
            }
        }
        if let Some(pose) = pose {
            args.extend(["--pose-fixe".into(), pose.into()]);
        } else {
            args.extend(["--actions".into(), actions.join(",")]);
        }
    }
    push_exclusions(&dir, &mut args, exclusions)?;
    let bridge = ["--subfamily".to_owned(), family.to_owned(), subfamily.to_owned()];
    run_engine_selected(
        &dir,
        &args,
        CASTING_CATALOGUE,
        Duration::from_secs(DRY_RUN_TIMEOUT_SECS),
        graine,
        Some(&bridge),
    )
}

/// One character on the engine's validated solo actions (`draw_poses`, the `--poses <ID>`
/// script mode) as a pass-1 casting catalogue, through the bridge: nothing is submitted.
/// `actions` keeps only those names; empty = every action the character can play.
pub fn casting_character_actions(graine: u64, perso: &str, actions: &[String]) -> Result<DryRunOutcome> {
    casting_character_actions_in_with_timeout(
        &engine_dir(),
        graine,
        perso,
        actions,
        Duration::from_secs(DRY_RUN_TIMEOUT_SECS),
    )
}

/// The same in an explicit engine directory (tests).
pub fn casting_character_actions_in_with_timeout(
    dir: &Path,
    graine: u64,
    perso: &str,
    actions: &[String],
    timeout: Duration,
) -> Result<DryRunOutcome> {
    let perso = perso.trim();
    if perso.is_empty()
        || perso.len() > 64
        || !perso.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(format!("Identifiant de perso invalide ({perso})").into());
    }
    if actions.len() > CASTING_MAX as usize
        || actions.iter().any(|a| {
            a.is_empty()
                || !a
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
    {
        return Err("Liste d'actions invalide".into());
    }
    let bridge = [
        "--poses-all".to_owned(),
        perso.to_owned(),
        graine.to_string(),
        actions.join(","),
    ];
    run_engine_selected(dir, &[], CASTING_CATALOGUE, timeout, graine, Some(&bridge))
}

pub(crate) fn run_engine(
    dir: &Path,
    args: &[String],
    catalogue_rel: &str,
    timeout: Duration,
    graine: u64,
) -> Result<DryRunOutcome> {
    run_engine_selected(dir, args, catalogue_rel, timeout, graine, None)
}

fn run_engine_selected(
    dir: &Path,
    args: &[String],
    catalogue_rel: &str,
    timeout: Duration,
    graine: u64,
    // Arguments of the app-side bridge (casting_bridge.py) when the engine is driven through
    // it instead of `test_solo.py` directly: `--subfamily F B`, `--poses-all ID G a,b`.
    bridge: Option<&[String]>,
) -> Result<DryRunOutcome> {
    let script = dir.join("test_solo.py");
    if !script.is_file() {
        return Err(format!(
            "Moteur introuvable : {} absent (chemin configurable par SAYA_ENGINE_DIR, défaut « ~/Documents/saya studio config/GROK BUILD/generez »)",
            script.display()
        )
        .into());
    }
    let catalogue = dir.join(catalogue_rel);
    let mut command = Command::new("python3");
    if let Some(bridge) = bridge {
        command
            .args(["-B", "-c", include_str!("casting_bridge.py")])
            .arg(dir)
            .args(bridge);
    } else {
        command.arg("test_solo.py");
    }
    let mut child = command
        .args(args)
        .current_dir(dir)
        .env("PYTHONHASHSEED", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "Lancement du moteur impossible dans {} : {e}",
                dir.display()
            )
        })?;
    let stdout = child.stdout.take().ok_or("stdout du moteur absent")?;
    let stderr = child.stderr.take().ok_or("stderr du moteur absent")?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout));
    let stderr_reader = thread::spawn(move || read_bounded(stderr));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(50)),
            autre => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(match autre {
                    Ok(None) => format!("Moteur interrompu : délai dépassé ({timeout:?})"),
                    autre => format!("Moteur interrompu (erreur processus) : {autre:?}"),
                });
            }
        }
    };
    let stdout = String::from_utf8_lossy(
        &stdout_reader
            .join()
            .map_err(|_| "Lecture moteur interrompue")??,
    )
    .into_owned();
    let stderr = String::from_utf8_lossy(
        &stderr_reader
            .join()
            .map_err(|_| "Lecture erreur interrompue")??,
    )
    .into_owned();
    let status = status?;
    if !status.success() {
        return Err(format!("Échec du moteur (code {status}) : {}", tail_chars(&stderr, 4000)).into());
    }
    if !catalogue.is_file() {
        return Err(format!(
            "Run moteur terminé sans catalogue : {} absent.\nstdout: {}\nstderr: {}",
            catalogue.display(),
            tail_chars(&stdout, 2000),
            tail_chars(&stderr, 2000)
        )
        .into());
    }
    Ok(DryRunOutcome {
        graine,
        catalogue,
        stdout,
        stderr,
    })
}

fn read_bounded(reader: impl Read) -> std::io::Result<Vec<u8>> {
    let mut data = Vec::new();
    reader.take((LIMIT + 1) as u64).read_to_end(&mut data)?;
    Ok(data)
}

/// Last `max` characters of a text (Python errors are read at the end).
fn tail_chars(text: &str, max: usize) -> &str {
    if text.len() <= max {
        text
    } else {
        &text[text.len() - max..]
    }
}

#[cfg(test)]
mod exclusion_and_scripts_tests {
    use super::*;
    use serde_json::json;

    /// Engine stub: writes a test_solo.py that logs its argv and drops the catalogue.
    /// `with_flag` decides whether the stub knows CASTING_EXCLUDE_FLAG (support guard).
    fn engine_stub(dir: &Path, with_flag: bool) {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap();
        // The flag token must appear IN test_solo.py when with_flag: guard B
        // (`exclusion_supported`) reads exactly this file, as in production.
        let token = if with_flag { CASTING_EXCLUDE_FLAG } else { "" };
        let script = format!(
            r#"import json, sys
from pathlib import Path
# {token}
args = sys.argv[1:]
Path("argv.json").write_text(json.dumps(args))
dossier = Path("sortie") / "casting"
dossier.mkdir(parents=True, exist_ok=True)
shot = json.loads(Path("shot_fixture.json").read_text())
(dossier / "catalogue.json").write_text(json.dumps([shot], ensure_ascii=False))
print("STUB moteur, rien envoyé")
"#
        );
        std::fs::write(dir.join("test_solo.py"), script).unwrap();
    }

    fn shot() -> String {
        json!({
            "stem": "bny-00001-stub-oc-noobai",
            "name": "Stub OC",
            "famille": "bny",
            "model": "noobai",
            "seed": 248919022623436u64,
            "casting": {
                "prefixe": "embedding:lazypos",
                "decor_libre": "night club",
                "action": "hands clasped behind head",
                "p1": "1girl, solo, elf",
                "anatomy": "(small wings:1.25), cute_tail",
                "negative": "embedding:lazyneg",
                "checkpoint": "noobai.safetensors",
                "lora_loaders": {}
            }
        })
        .to_string()
    }

    #[test]
    fn exclusion_passes_the_real_flag_to_the_engine() {
        let engine = std::env::temp_dir().join("saya-studio-exclu-stub");
        engine_stub(&engine, true);
        std::fs::write(engine.join("shot_fixture.json"), shot()).unwrap();
        let outcome = casting_exclude_in_with_timeout(
            &engine,
            7,
            2,
            Some("bny"),
            &["seins".to_owned(), "masturbation".to_owned()],
            Duration::from_secs(30),
        )
        .expect("casting à exclusions via stub");
        let argv: Vec<String> =
            serde_json::from_slice(&std::fs::read(engine.join("argv.json")).unwrap()).unwrap();
        let expected = vec![
            "--casting".to_owned(),
            "2".to_owned(),
            "--famille".to_owned(),
            "bny".to_owned(),
            CASTING_EXCLUDE_FLAG.to_owned(),
            "seins,masturbation".to_owned(),
            "--graine".to_owned(),
            "7".to_owned(),
        ];
        assert_eq!(argv, expected, "argv exact transmis au moteur");
        assert!(outcome.stdout.contains("STUB"));
        // Validation refused BEFORE any spawn: duplicate, charset, empty list, number.
        let erreur = |result: crate::Result<DryRunOutcome>| result.err().unwrap().to_string();
        assert!(erreur(casting_exclude_in_with_timeout(
            &engine,
            7,
            2,
            None,
            &["a".into(), "a".into()],
            Duration::from_secs(1)
        ))
        .contains("doublon"));
        assert!(erreur(casting_exclude_in_with_timeout(
            &engine,
            7,
            2,
            None,
            &["Seins!".into()],
            Duration::from_secs(1)
        ))
        .contains("exclue invalide"));
        // Empty list: no exclusion requested, the draw starts WITHOUT the flag (W1
        // convergence: the guard only refuses a real exclusion).
        casting_exclude_in_with_timeout(&engine, 7, 2, None, &[], Duration::from_secs(30))
            .expect("tirage libre sans exclusion");
        // List containing only empty entries: refused (a requested exclusion must be real).
        assert!(erreur(casting_exclude_in_with_timeout(
            &engine,
            7,
            2,
            None,
            &["".into()],
            Duration::from_secs(1)
        ))
        .contains("aucune action exclue"));
        assert!(erreur(casting_exclude_in_with_timeout(
            &engine,
            7,
            0,
            None,
            &["a".into()],
            Duration::from_secs(1)
        ))
        .contains("hors limites"));
    }

    #[test]
    fn exclusion_refuses_an_engine_that_ignores_the_flag() {
        let engine = std::env::temp_dir().join("saya-studio-exclu-vieux");
        engine_stub(&engine, false);
        let erreur = casting_exclude_in_with_timeout(
            &engine,
            7,
            2,
            None,
            &["seins".to_owned()],
            Duration::from_secs(30),
        )
        .unwrap_err()
        .to_string();
        assert!(erreur.contains("exclusion refusée"), "{erreur}");
        assert!(
            !engine.join("argv.json").exists(),
            "refus AVANT spawn : le tirage ne part jamais sans exclusion garantie"
        );
    }

    /// Engine directory + solo_x7 neighbour for the scripts (the app resolves solo_x7
    /// as the sibling of the engine directory, see run_engine_script).
    fn stub_scripts(dir: &Path) -> PathBuf {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir.join("outils")).unwrap();
        let solo_x7 = dir.parent().unwrap().join("solo_x7");
        std::fs::create_dir_all(solo_x7.join("attente_solo")).unwrap();
        std::fs::write(dir.join("test_solo.py"), "# stub\n").unwrap();
        std::fs::write(
            solo_x7.join("attente_solo").join("tester.py"),
            "print(\"ATTENTE STUB ok\")\n",
        )
        .unwrap();
        std::fs::write(
            solo_x7.join("test_bny20.py"),
            "import sys\nprint(\"BNY20 STUB\", sys.argv[1])\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("outils").join("lot_duo.py"),
            "print(\"DUO STUB ok\")\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("lancer_poses.bash"),
            "echo \"POSES STUB $1 PYTHONHASHSEED=$PYTHONHASHSEED\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("lancer_test_solo.bash"),
            "echo \"LCS STUB $1 SOLO_MODE=$SOLO_MODE SOLO_LOT=$SOLO_LOT PYTHONHASHSEED=$PYTHONHASHSEED\"\n",
        )
        .unwrap();
        solo_x7
    }

    #[test]
    fn engine_script_runs_the_five_modes_without_shell() {
        let dir =
            std::env::temp_dir().join(format!("saya-studio-scripts-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        stub_scripts(&dir);
        let deadline = Duration::from_secs(30);

        // attente (fish case 0): python3 "<engine>/../solo_x7/attente_solo/tester.py" --gpu
        let waiting_run = run_engine_script(&dir, "attente", None, deadline).unwrap();
        assert!(waiting_run.success, "{}", outcome_error(&waiting_run));
        assert_eq!(waiting_run.argv[0], "python3");
        assert!(waiting_run.argv[1].ends_with("solo_x7/attente_solo/tester.py"));
        assert_eq!(waiting_run.argv[2], "--gpu");
        assert!(waiting_run.stdout.contains("ATTENTE STUB"));

        // poses (case 2): bash <engine>/lancer_poses.bash ID + PYTHONHASHSEED=0
        assert!(run_engine_script(&dir, "poses", None, deadline)
            .unwrap_err()
            .to_string()
            .contains("identifiant du perso requis"));
        assert!(run_engine_script(&dir, "poses", Some("../etc/passwd"), deadline).is_err());
        let poses = run_engine_script(&dir, "poses", Some("CH--9062"), deadline).unwrap();
        assert!(poses.success);
        assert_eq!(
            poses.argv[1..3],
            vec![
                dir.join("lancer_poses.bash").to_string_lossy().into_owned(),
                "CH--9062".to_owned()
            ]
        );
        assert!(poses.stdout.contains("POSES STUB CH--9062"));
        assert!(
            poses.stdout.contains("PYTHONHASHSEED=0"),
            "{}",
            poses.stdout
        );

        // lcs (case 3): env SOLO_MODE=futa SOLO_LOT=lcs bash lancer_test_solo.bash --reprise
        assert!(run_engine_script(&dir, "lcs", Some("CH--9062"), deadline)
            .unwrap_err()
            .to_string()
            .contains("ne s'applique qu'au script poses"));
        let lcs = run_engine_script(&dir, "lcs", None, deadline).unwrap();
        assert!(lcs.success);
        assert!(
            lcs.stdout
                .contains("LCS STUB --reprise SOLO_MODE=futa SOLO_LOT=lcs PYTHONHASHSEED=0"),
            "{}",
            lcs.stdout
        );

        // duo (case 4) and bny20 (case 5), same shape checks.
        let duo = run_engine_script(&dir, "duo", None, deadline).unwrap();
        assert!(duo.success && duo.stdout.contains("DUO STUB"));
        assert!(duo.argv[1].ends_with("outils/lot_duo.py") && duo.argv[2] == "--gpu");
        let bny = run_engine_script(&dir, "bny20", None, deadline).unwrap();
        assert!(bny.success && bny.stdout.contains("BNY20 STUB --gpu"));
        assert!(bny.argv[1].ends_with("solo_x7/test_bny20.py"));

        // Unknown script: refused (never a free command).
        assert!(run_engine_script(&dir, "rm -rf /", None, deadline)
            .unwrap_err()
            .to_string()
            .contains("Script moteur inconnu"));
        // Target file missing: clear error before spawn.
        std::fs::remove_file(dir.join("outils").join("lot_duo.py")).unwrap();
        assert!(run_engine_script(&dir, "duo", None, deadline)
            .unwrap_err()
            .to_string()
            .contains("introuvable"));
        std::fs::remove_dir_all(&dir).unwrap();
        let _ = std::fs::remove_dir_all(dir.parent().unwrap().join("solo_x7"));
    }

    #[test]
    fn engine_script_kills_on_timeout_and_logs() {
        let dir = std::env::temp_dir().join(format!(
            "saya-studio-script-timeout-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("test_solo.py"), "# stub\n").unwrap();
        std::fs::write(
            dir.join("outils2.py"),
            "import time\nprint(\"DEBUT\", flush=True)\ntime.sleep(30)\n",
        )
        .unwrap();
        // Via the lcs script, replacing the launcher with a slow script.
        std::fs::write(dir.join("lancer_test_solo.bash"), "python3 outils2.py\n").unwrap();
        let outcome = run_engine_script(&dir, "lcs", None, Duration::from_secs(1)).unwrap();
        assert!(!outcome.success);
        assert!(outcome.timeout, "délai signalé honnêtement");
        assert!(outcome.stdout.contains("DEBUT"), "{}", outcome.stdout);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn outcome_error(outcome: &ScriptOutcome) -> String {
        format!("stdout: {}\nstderr: {}", outcome.stdout, outcome.stderr)
    }
}
