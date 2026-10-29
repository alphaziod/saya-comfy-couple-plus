//! Slice V1 — character targeting: parameter validation, argv → engine wiring
//! via an isolated fake engine (tmpdir), and engine registry ↔ app sheets join.
//!
//! No test here launches the real engine (`tests/casting.rs` already does): refusals
//! are local (before any process), wiring goes through a stub written in a tmpdir
//! — never the real engine's `sortie/casting/`.

use serde_json::{json, Value};
use std::path::Path;
use studio_core::casting;
use studio_core::engine_runtime;

const SEED: u64 = 7;

fn refusal(result: studio_core::Result<engine_runtime::DryRunOutcome>) -> String {
    result.err().expect("le refus attendu").to_string()
}

#[test]
fn targeting_rejects_invalid_parameters() {
    assert!(
        refusal(engine_runtime::casting_characters(SEED, &[])).contains("nombre de persos ciblés"),
        "aucun id refusé"
    );
    let too_many = vec!["CH--9009".to_string(); (engine_runtime::CASTING_MAX as usize) + 1];
    assert!(
        refusal(engine_runtime::casting_characters(SEED, &too_many)).contains("nombre de persos ciblés"),
        "au-delà du plafond refusé"
    );
    assert!(
        refusal(engine_runtime::casting_characters(SEED, &["   ".into()])).contains("perso invalide")
    );
    assert!(refusal(engine_runtime::casting_characters(SEED, &["".into()])).contains("perso invalide"));
    assert!(
        refusal(engine_runtime::casting_characters(
            SEED,
            &["CH--9009; rm -rf".into()]
        ))
        .contains("perso invalide"),
        "un identifiant avec séparateur de commande est refusé"
    );
    assert!(
        refusal(engine_runtime::casting_characters(SEED, &["CH–0009".into()]))
            .contains("perso invalide")
    );
    let long = "CH--".repeat(17);
    assert!(refusal(engine_runtime::casting_characters(SEED, &[long])).contains("perso invalide"));
    // Clean ids pass local validation: the next refusal comes from the missing engine
    // (not from an id validation), proof that CH--9009/CH-09999 are accepted.
    let error = engine_runtime::casting_characters_in_with_timeout(
        Path::new("/saya-studio-test/moteur-absent"),
        SEED,
        &["CH--9009".into(), "CH-09999".into()],
        std::time::Duration::from_secs(2),
    )
    .expect_err("moteur absent");
    assert!(
        error.to_string().contains("Moteur introuvable")
            && error.to_string().contains("moteur-absent"),
        "{error}"
    );
}

/// argv → engine wiring proven by a real subprocess (isolated stub): targeting
/// receives `--casting N --persos ID1,ID2,… --graine G`, the stub catalog is re-read and
/// validated by the casting layer (U1).
#[test]
fn targeting_passes_correct_arguments_to_engine() {
    let engine_dir = std::env::temp_dir().join("saya-studio-v1-casting-persos-stub");
    let _ = std::fs::remove_dir_all(&engine_dir);
    std::fs::create_dir_all(&engine_dir).unwrap();
    let shot = json!({
        "id": "CH--9009",
        "stem": "hum--0009-greta-calder-noobai",
        "name": "Greta Calder",
        "famille": "hum",
        "model": "noobai",
        "seed": 248919022623436u64,
        "note": "negatif",
        "negatif_fige": true,
        "p1": "1girl, solo, elf",
        "anatomy": "(small wings:1.25), cute_tail",
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
ids = [x.strip() for x in valeur("--persos").split(",") if x.strip()]
modele = json.loads(Path("shot_fixture.json").read_text())
shots = []
for i, id_ in enumerate(ids):
    shot = json.loads(json.dumps(modele))
    shot["id"] = id_
    shot["name"] = f"Stub {id_}"
    shots.append(shot)
dossier = Path("sortie") / "casting"
dossier.mkdir(parents=True, exist_ok=True)
(dossier / "catalogue.json").write_text(json.dumps(shots, ensure_ascii=False))
print(f"CASTING PERSOS {dossier} — {len(shots)} persos ciblés ({' '.join(ids)}), rien envoyé")
print("CATALOGUE sortie/casting/catalogue.json")
"#,
    )
    .unwrap();
    std::fs::write(engine_dir.join("shot_fixture.json"), shot.to_string()).unwrap();

    let ids = ["CH--9009".to_string(), "CH--9010".to_string()];
    let outcome = engine_runtime::casting_characters_in_with_timeout(
        &engine_dir,
        SEED,
        &ids,
        std::time::Duration::from_secs(60),
    )
    .unwrap_or_else(|e| panic!("ciblage en échec : {e}"));
    let argv: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(engine_dir.join("argv.json")).unwrap()).unwrap();
    assert_eq!(
        argv,
        [
            "--casting",
            "2",
            "--persos",
            "CH--9009,CH--9010",
            "--graine",
            "7"
        ]
    );
    assert!(
        outcome.stdout.contains("CASTING PERSOS"),
        "{}",
        outcome.stdout
    );
    assert!(outcome.catalogue.ends_with("sortie/casting/catalogue.json"));

    // The catalog re-read by the casting layer (U1) validates EVERY targeted shot, in order.
    let shots = casting::read_catalogue(&outcome.catalogue).unwrap();
    assert_eq!(shots.len(), 2);
    assert_eq!(shots[0]["id"], json!("CH--9009"));
    assert_eq!(shots[1]["id"], json!("CH--9010"));
    assert_eq!(shots[0]["stem"], json!("hum--0009-greta-calder-noobai"));

    // An engine failure bubbles up cleanly (stub that crashes).
    std::fs::write(
        engine_dir.join("test_solo.py"),
        "raise RuntimeError('casting persos : CH-99999 absent du lot')\n",
    )
    .unwrap();
    let error = engine_runtime::casting_characters_in_with_timeout(
        &engine_dir,
        SEED,
        &["CH-99999".into()],
        std::time::Duration::from_secs(60),
    )
    .expect_err("un échec moteur doit remonter");
    assert!(error.to_string().contains("Échec du moteur"), "{error}");
}

/// The registry engine ↔ app sheets join: IMPORT KEY first (`characters:<neg_id>`
/// set by the base preset — insensitive to sheet renames), EXACT NAME as
/// fallback. A validated one without a sheet stays listed with `ficheId: null`: castable, tile
/// without a portrait.
#[test]
fn available_characters_join_registry_to_sheets_by_key_then_by_name() {
    let db = studio_core::Database::in_memory().unwrap();
    let greta = db
        .save_character(
            None,
            "Greta Calder".into(),
            json!({"identity_prompt": "p1 greta"}),
        )
        .expect("fiche Greta créée");
    let _ = db
        .save_character(None, "Autre fiche".into(), json!({}))
        .expect("fiche sans rapport créée");

    // A sheet imported by the preset (key characters:CH--9002) then RENAMED:
    // the import key still links it to the registry.
    let dir = std::env::temp_dir().join("saya-studio-v1-persos-disponibles");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let export = dir.join("grok-fiches.json");
    std::fs::write(
        &export,
        json!({"characters": [
            {"id": "CH--9002", "name": "Nouveau nom", "identity_prompt": "p1 importé", "anatomy_prompt": "anatomie importée"}
        ]})
        .to_string(),
    )
    .unwrap();
    let (entries, _) = studio_core::grok::parse_characters(&export).expect("export grok lisible");
    assert_eq!(entries.len(), 1);
    let choices: Vec<studio_core::grok::Choice> = entries
        .iter()
        .map(|e| studio_core::grok::Choice {
            path: e.path.clone(),
            sha256: e.sha256.clone(),
            key: e.key.clone(),
            mode: "create".into(),
            character: None,
        })
        .collect();
    let imported = db.grok_import(&choices, true).expect("import grok");
    assert_eq!(imported["imported"].as_array().map(Vec::len), Some(1));
    let renamed: String = imported["imported"][0]["characterId"]
        .as_str()
        .expect("fiche importée")
        .to_owned();

    std::fs::write(
        dir.join("negatifs_figes.json"),
        json!({
            "valides": [
                {"neg_id": "CH--9009", "nom": "Greta Calder", "slug": "greta-calder",
                 "model": "noobai", "source": "oc", "bank_id": ""},
                {"neg_id": "CH--9002", "nom": "Ancien nom", "slug": "ancien-nom",
                 "model": "illustrious", "source": "banque", "bank_id": "CH-00006"},
                {"neg_id": "CH--9010", "nom": "Sans Fiche", "slug": "sans-fiche",
                 "model": "noobai", "source": "banque", "bank_id": ""}
            ]
        })
        .to_string(),
    )
    .unwrap();

    let rows = db.available_characters(&dir).expect("registre lisible");
    assert_eq!(rows.len(), 3, "chaque validée devient une ligne");
    // Exact-name fallback: the non-imported sheet carries the registry name.
    assert_eq!(rows[0]["negId"], json!("CH--9009"));
    assert_eq!(rows[0]["ficheId"], json!(greta), "jointure par nom exact");
    // Import key: the sheet was renamed, the key still finds it (priority).
    assert_eq!(rows[1]["negId"], json!("CH--9002"));
    assert_eq!(
        rows[1]["ficheId"],
        json!(renamed),
        "clé d'import characters:CH--9002 gagne sur le nom changé"
    );
    // Neither key nor name: null, no error.
    assert_eq!(rows[2]["negId"], json!("CH--9010"));
    assert_eq!(
        rows[2]["ficheId"],
        Value::Null,
        "sans fiche : null, pas d'erreur"
    );

    // Empty or missing registry: clean error, never a silent empty list.
    let error = db
        .available_characters(Path::new("/saya-studio-test/moteur-absent"))
        .expect_err("registre absent");
    assert!(
        error.to_string().contains("Registre des persos validés"),
        "{error}"
    );
    std::fs::write(
        dir.join("negatifs_figes.json"),
        json!({"valides": []}).to_string(),
    )
    .unwrap();
    let error = db.available_characters(&dir).expect_err("registre vide");
    assert!(
        error.to_string().contains("aucun perso validé"),
        "{error}"
    );
}
