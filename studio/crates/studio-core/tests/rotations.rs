//! Slice U2 — casting rotations: validation of the parameters of both modes and proof
//! of the argv → engine wiring via an isolated fake engine (tmpdir).
//!
//! Mode A (`casting_character`): `--poses <ID> --casting N --graine G` — 1 character × N times
//! the same pose. Mode B (`casting_pose`): `--casting N [--famille X] --pose-fixe
//! <action> --graine G` — 1 pose × N random characters. No test here launches the real
//! engine (`tests/casting.rs` already does): refusals are local, wiring goes through
//! a stub written in a tmpdir — never the real engine's `sortie/casting/`.

use serde_json::{json, Value};
use studio_core::engine_runtime;

const SEED: u64 = 7;

fn refusal(result: studio_core::Result<engine_runtime::DryRunOutcome>) -> String {
    result.err().expect("le refus attendu").to_string()
}

#[test]
fn rotations_mode_a_rejects_invalid_parameters() {
    assert!(refusal(engine_runtime::casting_character(SEED, "CH--9009", 0)).contains("nombre d'OC"));
    assert!(refusal(engine_runtime::casting_character(SEED, "CH--9009", 5000)).contains("nombre d'OC"));
    assert!(refusal(engine_runtime::casting_character(SEED, "   ", 2)).contains("perso invalide"));
    assert!(refusal(engine_runtime::casting_character(SEED, "", 2)).contains("perso invalide"));
    assert!(
        refusal(engine_runtime::casting_character(SEED, "CH--9009; rm -rf", 2))
            .contains("perso invalide"),
        "un identifiant avec séparateur de commande est refusé"
    );
    assert!(refusal(engine_runtime::casting_character(SEED, "CH–0009", 2)).contains("perso invalide"));
    let long = "CH--".repeat(17);
    assert!(refusal(engine_runtime::casting_character(SEED, &long, 2)).contains("perso invalide"));
}

#[test]
fn rotations_mode_b_rejects_invalid_parameters() {
    assert!(refusal(engine_runtime::casting_pose(
        SEED,
        0,
        None,
        "x_pinces_cheveux",
        &[]
    ))
    .contains("nombre d'OC"));
    assert!(refusal(engine_runtime::casting_pose(
        SEED,
        5000,
        None,
        "x_pinces_cheveux",
        &[]
    ))
    .contains("nombre d'OC"));
    assert!(
        refusal(engine_runtime::casting_pose(SEED, 2, None, "  ", &[]))
            .contains("pose fixe invalide")
    );
    assert!(
        refusal(engine_runtime::casting_pose(SEED, 2, None, "", &[]))
            .contains("pose fixe invalide")
    );
    assert!(
        refusal(engine_runtime::casting_pose(
            SEED,
            2,
            None,
            "hair clips",
            &[]
        ))
        .contains("pose fixe invalide"),
        "un nom d'action avec espace est refusé côté app (les noms du moteur sont [a-z0-9_])"
    );
    assert!(refusal(engine_runtime::casting_pose(
        SEED,
        2,
        None,
        "x_pinces_cheveux; drop",
        &[]
    ))
    .contains("pose fixe invalide"));
    assert!(refusal(engine_runtime::casting_pose(
        SEED,
        2,
        Some("ZOMBIE"),
        "x_pinces_cheveux",
        &[]
    ))
    .contains("famille invalide"));
}

/// argv → engine wiring proven by a real subprocess (isolated stub): Mode A
/// receives `--poses <ID> --casting N --graine G`, Mode B `--casting N --famille F
/// --pose-fixe <action> --graine G`, and the stub catalog is re-read and validated.
#[test]
fn rotations_pass_correct_arguments_to_engine() {
    let engine_dir = std::env::temp_dir().join("saya-studio-u2-rotations-stub");
    let _ = std::fs::remove_dir_all(&engine_dir);
    std::fs::create_dir_all(&engine_dir).unwrap();
    let shot = json!({
        "stem": "bny-00001-stub-oc-noobai",
        "name": "Stub OC",
        "famille": "bny",
        "model": "noobai",
        "seed": 248919022623436u64,
        "casting": {
            "prefixe": "embedding:lazypos, embedding:lazyneg",
            "decor_libre": "night club, neon signs",
            "action": "standing, one leg up",
            "p1": "1girl, solo, elf",
            "anatomy": "(small wings:1.25), cute_tail",
            "negative": "embedding:lazyneg",
            "p2": "", "p2_anatomy": "",
            "checkpoint": "noobai.safetensors",
            "lora_loaders": {"662": {"loras": [], "text": ""}, "663": {"loras": [], "text": ""}, "664": {"loras": [], "text": ""}},
            "format_paysage": false,
            "seed": -1
        }
    });
    std::fs::write(
        engine_dir.join("test_solo.py"),
        r#"import json, sys
from pathlib import Path
args = sys.argv[1:]
Path("argv.json").write_text(json.dumps(args))
def valeur(flag, defaut=None):
    return args[args.index(flag) + 1] if flag in args else defaut
perso = valeur("--poses")
fixe = valeur("--pose-fixe")
# Mêmes entêtes que le CLI réel (test_solo.py) : le mode est visible sur stdout.
if perso is not None:
    entete = f"CASTING ROTATIONS {perso} × N fois la même pose"
elif fixe is not None:
    entete = f"CASTING N OC, pose « {fixe} »"
else:
    entete = "CASTING stub"
shot = json.loads(Path("shot_fixture.json").read_text())
dossier = Path("sortie") / "casting"
dossier.mkdir(parents=True, exist_ok=True)
(dossier / "catalogue.json").write_text(json.dumps([shot], ensure_ascii=False))
print(f"{entete}, rien envoyé")
print("CATALOGUE sortie/casting/catalogue.json")
"#,
    )
    .unwrap();
    std::fs::write(engine_dir.join("shot_fixture.json"), shot.to_string()).unwrap();

    // Mode A: ONE character × N times the same pose.
    let outcome = engine_runtime::casting_character_in_with_timeout(
        &engine_dir,
        SEED,
        "CH--9201",
        2,
        std::time::Duration::from_secs(60),
    )
    .unwrap_or_else(|e| panic!("mode A en échec : {e}"));
    let argv: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(engine_dir.join("argv.json")).unwrap()).unwrap();
    assert_eq!(
        argv,
        ["--poses", "CH--9201", "--casting", "2", "--graine", "7"]
    );
    assert!(
        outcome.stdout.contains("CASTING ROTATIONS"),
        "{}",
        outcome.stdout
    );
    assert!(outcome.catalogue.ends_with("sortie/casting/catalogue.json"));

    // Mode B: 1 pose × N characters, filtered family.
    let outcome = engine_runtime::casting_pose_in_with_timeout(
        &engine_dir,
        SEED,
        2,
        Some("bny"),
        "x_pinces_cheveux",
        &[],
        std::time::Duration::from_secs(60),
    )
    .unwrap_or_else(|e| panic!("mode B en échec : {e}"));
    let argv: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(engine_dir.join("argv.json")).unwrap()).unwrap();
    assert_eq!(
        argv,
        [
            "--casting",
            "2",
            "--famille",
            "bny",
            "--pose-fixe",
            "x_pinces_cheveux",
            "--graine",
            "7"
        ]
    );
    assert!(
        outcome.stdout.contains("pose « x_pinces_cheveux »"),
        "{}",
        outcome.stdout
    );

    // The catalog re-read by the casting layer (U1) validates the mode's shot.
    let shots: Vec<Value> = studio_core::casting::read_catalogue(&outcome.catalogue).unwrap();
    assert_eq!(shots.len(), 1);
    assert_eq!(shots[0]["famille"], json!("bny"));

    // An engine failure bubbles up cleanly (stub that crashes).
    std::fs::write(
        engine_dir.join("test_solo.py"),
        "raise RuntimeError('pose de casting inconnue : zombie')\n",
    )
    .unwrap();
    let error = engine_runtime::casting_pose_in_with_timeout(
        &engine_dir,
        SEED,
        2,
        None,
        "x_pinces_cheveux",
        &[],
        std::time::Duration::from_secs(60),
    )
    .expect_err("un échec moteur doit remonter");
    assert!(error.to_string().contains("Échec du moteur"), "{error}");
    std::fs::write(engine_dir.join("test_solo.py"), "").unwrap();
}
