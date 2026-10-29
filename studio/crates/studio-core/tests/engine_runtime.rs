//! Slice T1 — read runtime: the engine in read mode through the wrapped command.
//!
//! The catalogue produced by `engine_runtime::dry_run` must be identical, byte for
//! byte, to a direct CLI run with the same seed. The engine always writes
//! `sortie/dry-run/catalogue.json`: each catalogue is copied into a separate tmpdir
//! BEFORE the next dry run, and dry runs are never launched in parallel.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use studio_core::engine_runtime;

const SEED: u64 = 7;

/// Serializes the tests that touch the SAYA_ENGINE_DIR environment variable
/// (process-global: without a lock, one test can break another).
static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn env_lock() -> MutexGuard<'static, ()> {
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|empoisonne| empoisonne.into_inner())
}

/// Copies a catalogue into a dedicated tmpdir (the engine always rewrites the same path).
fn copy_catalogue(source: &Path, dir: &str) -> PathBuf {
    let destination = std::env::temp_dir()
        .join("saya-studio-t1-engine-runtime")
        .join(dir);
    std::fs::create_dir_all(&destination).unwrap();
    let cible = destination.join("catalogue.json");
    std::fs::copy(source, &cible).unwrap();
    cible
}

#[test]
#[ignore = "moteur GROK BUILD réel : lancer explicitement sur copie isolée"]
fn dry_run_catalogue_matches_cli_run_byte_for_byte() {
    let _garde = env_lock();
    let engine_dir = engine_runtime::engine_dir();
    assert!(
        engine_dir.join("test_solo.py").is_file(),
        "moteur absent dans {} (SAYA_ENGINE_DIR ?)",
        engine_dir.display()
    );

    // 1. Direct CLI run, independent of the code under test.
    let cli = Command::new("python3")
        .arg("test_solo.py")
        .arg("--dry-run")
        .arg("--graine")
        .arg(SEED.to_string())
        .current_dir(&engine_dir)
        .env("PYTHONHASHSEED", "0")
        .output()
        .expect("lancement du run CLI direct");
    assert!(
        cli.status.success(),
        "run CLI en échec (code {:?}) :\n{}",
        cli.status.code(),
        String::from_utf8_lossy(&cli.stderr)
    );
    // Copy BEFORE the next dry run: the engine always rewrites the same path.
    let catalogue_cli = copy_catalogue(&engine_dir.join("sortie/dry-run/catalogue.json"), "cli");

    // 2. Same seed through the wrapped command (the module under test).
    let outcome =
        engine_runtime::dry_run(SEED).unwrap_or_else(|e| panic!("dry_run en échec : {e}"));
    let catalogue_commande = copy_catalogue(&outcome.catalogue, "commande");

    // 3. Byte-for-byte comparison.
    let octets_cli = std::fs::read(&catalogue_cli).unwrap();
    let octets_commande = std::fs::read(&catalogue_commande).unwrap();
    assert_eq!(
        octets_cli.len(),
        octets_commande.len(),
        "tailles différentes : CLI {} contre commande {}",
        octets_cli.len(),
        octets_commande.len()
    );
    assert_eq!(
        octets_cli, octets_commande,
        "catalogues différents octet pour octet (CLI contre commande moteur, graine {SEED})"
    );
    assert!(
        outcome.stdout.contains("DRY-RUN"),
        "stdout du moteur sans trace de dry-run : {}",
        outcome.stdout
    );
    assert!(outcome.catalogue.ends_with("sortie/dry-run/catalogue.json"));
}

#[test]
fn missing_engine_dir_gives_clean_error() {
    let error = engine_runtime::dry_run_in(Path::new("/saya-studio-test/moteur-absent"), SEED)
        .expect_err("un chemin moteur absent doit donner une erreur");
    let message = error.to_string();
    assert!(
        message.contains("Moteur introuvable"),
        "message inattendu : {message}"
    );
    assert!(
        message.contains("moteur-absent"),
        "le message doit nommer le chemin fautif : {message}"
    );
}

#[test]
fn stuck_engine_is_interrupted_by_timeout() {
    let dir = std::env::temp_dir().join("saya-studio-t1-moteur-bloque");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("test_solo.py"),
        "import time\nprint('demarrage', flush=True)\ntime.sleep(120)\n",
    )
    .unwrap();
    let debut = Instant::now();
    let error = engine_runtime::dry_run_in_with_timeout(&dir, SEED, Duration::from_secs(2))
        .expect_err("un moteur bloqué doit être interrompu");
    assert!(
        error.to_string().contains("délai dépassé"),
        "message inattendu : {error}"
    );
    assert!(
        debut.elapsed() < Duration::from_secs(20),
        "le processus dormait 120 s : il n'a pas été tué au délai"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn engine_dir_follows_saya_engine_dir() {
    let _garde = env_lock();
    let sauvegarde = std::env::var_os("SAYA_ENGINE_DIR");
    std::env::remove_var("SAYA_ENGINE_DIR");
    // Default: the engine inside the user's config folder, else the former location.
    let config = std::env::temp_dir().join(format!("config-moteur-{}", std::process::id()));
    let config_sauvegarde = std::env::var_os("SAYA_CONFIG_DIR");
    std::env::set_var("SAYA_CONFIG_DIR", &config);
    assert_eq!(
        engine_runtime::engine_dir(),
        PathBuf::from(std::env::var("HOME").unwrap()).join("Documents/GROK BUILD/generez"),
        "no engine in the config folder: former location"
    );
    std::fs::create_dir_all(config.join("GROK BUILD/generez")).unwrap();
    assert_eq!(engine_runtime::engine_dir(), config.join("GROK BUILD/generez"));
    std::fs::remove_dir_all(&config).ok();
    match config_sauvegarde {
        Some(v) => std::env::set_var("SAYA_CONFIG_DIR", v),
        None => std::env::remove_var("SAYA_CONFIG_DIR"),
    }
    std::env::set_var("SAYA_ENGINE_DIR", "/tmp/moteur-de-test");
    assert_eq!(
        engine_runtime::engine_dir(),
        PathBuf::from("/tmp/moteur-de-test")
    );
    match sauvegarde {
        Some(valeur) => std::env::set_var("SAYA_ENGINE_DIR", valeur),
        None => std::env::remove_var("SAYA_ENGINE_DIR"),
    }
}

/// Fake engine with the three modules the `--poses-all` bridge mode uses.
fn poses_engine() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("moteur-poses-{}", uuid_like()));
    std::fs::create_dir_all(dir.join("moteur")).unwrap();
    std::fs::write(dir.join("test_solo.py"), "").unwrap();
    std::fs::write(dir.join("moteur/noyau.py"), "SEED = []\ndef graine_tirage(g):\n    SEED.append(g)\n").unwrap();
    std::fs::write(
        dir.join("moteur/tirage.py"),
        "import noyau\ndef draw_poses(cible):\n    if cible != 'CH--0001':\n        raise RuntimeError('perso ' + cible + ' absent du lot')\n    return [{'id': cible + '-pose-%03d' % i, 'stem': 'poses-%d' % i, 'name': 'Test', 'famille': 'poses', 'model': 'noobai', 'seed': 10 + i, 'p1': 'neutral', 'pose': name, 'action_name': name, 'graine': noyau.SEED[-1]} for i, name in enumerate(['debout', 'assise', 'bras_leves'], 1)]\n",
    )
    .unwrap();
    std::fs::write(dir.join("moteur/comfy.py"), "def casting_passe_un(shot):\n    return {'action': shot['pose'], 'seed': shot['seed']}\n").unwrap();
    dir
}

fn uuid_like() -> String {
    format!("{}-{}", std::process::id(), Instant::now().elapsed().as_nanos())
}

#[test]
fn one_character_on_every_action_goes_through_the_bridge() {
    let dir = poses_engine();
    let timeout = Duration::from_secs(60);
    let read = |outcome: engine_runtime::DryRunOutcome| -> Vec<serde_json::Value> {
        serde_json::from_slice(&std::fs::read(outcome.catalogue).unwrap()).unwrap()
    };
    let all = read(engine_runtime::casting_character_actions_in_with_timeout(&dir, SEED, "CH--0001", &[], timeout).unwrap());
    assert_eq!(all.len(), 3, "every action");
    assert!(all.iter().all(|s| s["casting"]["action"] == s["pose"] && s["lazy"] == true));
    assert_eq!(all[0]["graine"], SEED, "draw seeded by the casting seed");
    let some = read(
        engine_runtime::casting_character_actions_in_with_timeout(&dir, SEED, "CH--0001", &["assise".into(), "debout".into()], timeout).unwrap(),
    );
    let names: Vec<_> = some.iter().map(|s| s["action_name"].as_str().unwrap()).collect();
    assert_eq!(names, ["debout", "assise"], "engine order kept, filter applied");
    let unknown = engine_runtime::casting_character_actions_in_with_timeout(&dir, SEED, "CH--0001", &["inconnue".into()], timeout).unwrap_err();
    assert!(unknown.to_string().contains("inconnue"), "{unknown}");
    assert!(engine_runtime::casting_character_actions_in_with_timeout(&dir, SEED, "../etc", &[], timeout).is_err());
    assert!(engine_runtime::casting_character_actions_in_with_timeout(&dir, SEED, "CH--9999", &[], timeout).is_err());
    std::fs::remove_dir_all(&dir).ok();
}
