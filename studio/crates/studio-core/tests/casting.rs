//! Slice U1 — 1-to-1 casting: the T1 runtime extended to casting, filling of the
//! pass-1 source and persistence of the YES picks through the Database struct.
//!
//! The real-engine test reuses the T1 pattern (`tests/engine_runtime.rs`): a direct
//! CLI run, a copy of the catalogue into a tmpdir (the engine always rewrites
//! `sortie/casting/catalogue.json` — never `sortie/dry-run/`), then the wrapped
//! command and the byte-for-byte comparison. Castings of one test binary
//! are serialized by a lock: never two casts in parallel.

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use studio_core::casting;
use studio_core::engine_runtime;

const SEED: u64 = 7;

static MOTEUR_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn engine_lock() -> MutexGuard<'static, ()> {
    MOTEUR_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|empoisonne| empoisonne.into_inner())
}

/// Copies a catalogue into a dedicated tmpdir (the engine always rewrites the same path).
fn copy_catalogue(source: &Path, dir: &str) -> PathBuf {
    let destination = std::env::temp_dir()
        .join("saya-studio-u1-casting")
        .join(dir);
    std::fs::create_dir_all(&destination).unwrap();
    let cible = destination.join("catalogue.json");
    std::fs::copy(source, &cible).unwrap();
    cible
}

fn shot_bny_fixture() -> Value {
    json!({
        "stem": "bny-00001-galatea-yorke-noobai",
        "name": "Galatea Yorke",
        "famille": "bny",
        "model": "noobai",
        "seed": 248919022623436u64,
        "casting": {
            "prefixe": "embedding:lazypos, embedding:lazyneg",
            "decor_libre": "night club, neon signs",
            "action": "standing, one leg up",
            "p1": "1girl, solo, elf, real lop ears, (long white floppy ears:1.3)",
            "anatomy": "(small wings:1.25), cute_tail",
            "negative": "embedding:lazyneg",
            "p2": "", "p2_anatomy": "",
            "checkpoint": "noobai.safetensors",
            "lora_loaders": {
                "662": {"loras": [], "text": ""},
                "663": {"loras": [{"name": "otoko no ko_illustrious_V2.0", "strength": "0.30", "clipStrength": "0.30", "active": true, "expanded": false, "selected": false, "locked": false}],
                        "text": "<lora:otoko no ko_illustrious_V2.0:0.30:0.30>"},
                "664": {"loras": [{"name": "sparkleAlpha", "strength": "0.60", "clipStrength": "0.70", "active": true, "expanded": false, "selected": false, "locked": false}],
                        "text": "<lora:sparkleAlpha:0.60:0.70>"}
            },
            "format_paysage": false,
            "seed": -1
        }
    })
}

fn source_fixture() -> String {
    let node = |id: u64, kind: &str, named: Value| json!({"id": id, "type": kind, "widgets_values_named": named});
    json!({
        "nodes": [
            node(100309, "SayaMainPromptFR", json!({"prefixe":"ancien prefixe","decor":"stock","decor_libre":"ancien decor","seed":0})),
            node(100300, "PrimitiveStringMultiline", json!({"value":"ancienne action"})),
            node(807, "PrimitiveStringMultiline", json!({"value":"ancien p1"})),
            node(100301, "PrimitiveStringMultiline", json!({"value":"ancienne anatomie"})),
            node(145, "PrimitiveStringMultiline", json!({"value":"ancien negatif"})),
            node(808, "PrimitiveStringMultiline", json!({"value":"ancien p2"})),
            node(100302, "PrimitiveStringMultiline", json!({"value":"ancienne p2 anatomie"})),
            node(662, "Lora Loader (LoraManager)", json!({"text":"<lora:old-style:1.00:1.00>","loras":[{"name":"old-style","strength":"1.00","clipStrength":"1.00","active":true}]})),
            node(663, "Lora Loader (LoraManager)", json!({"text":"","loras":[{"name":"otoko no ko_illustrious_V2.0","strength":"0.30","clipStrength":"0.30","active":false}]})),
            node(664, "Lora Loader (LoraManager)", json!({"text":"","loras":[{"name":"old-pose","strength":"0.80","clipStrength":"0.80","active":true}]})),
            node(654, "PrimitiveString", json!({"text":"directives manuelles"})),
            node(655, "PrimitiveString", json!({"text":"directives manuelles"})),
            node(100042, "SayaDims", json!({"dimensions":"Portrait - 832x1216","invert":true})),
            node(810, "hub-sous-graphe-uuid", json!({"ckpt_name":"illustrious.safetensors"}))
        ],
        "definitions": {"subgraphs": [
            {"id": "hub-sous-graphe-uuid", "nodes": [
                node(488, "CheckpointLoaderSimple", json!({"ckpt_name":"illustrious.safetensors"})),
                node(900, "SayaMultiCouple", json!({"solo":false}))
            ]}
        ]}
    })
    .to_string()
}

fn named_node(source: &str, id: u64, widget: &str) -> Value {
    let doc: Value = serde_json::from_str(source).unwrap();
    doc["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"].as_u64() == Some(id))
        .expect("node présente dans la source")["widgets_values_named"][widget]
        .clone()
}

#[test]
fn fill_source_copies_pass_1_without_reinterpreting() {
    let shot = shot_bny_fixture();
    let filled = casting::fill_source(&source_fixture(), &shot).unwrap();

    let passe_un = &shot["casting"];
    assert_eq!(
        named_node(&filled, 100309, "prefixe").as_str().unwrap(),
        passe_un["prefixe"].as_str().unwrap()
    );
    assert_eq!(named_node(&filled, 100309, "decor").as_str().unwrap(), "libre");
    assert_eq!(
        named_node(&filled, 100309, "decor_libre").as_str().unwrap(),
        passe_un["decor_libre"].as_str().unwrap()
    );
    assert_eq!(named_node(&filled, 100300, "value"), passe_un["action"]);
    assert_eq!(named_node(&filled, 807, "value"), passe_un["p1"]);
    assert_eq!(named_node(&filled, 100301, "value"), passe_un["anatomy"]);
    assert_eq!(named_node(&filled, 145, "value"), passe_un["negative"]);
    assert_eq!(named_node(&filled, 808, "value").as_str().unwrap(), "");
    assert_eq!(named_node(&filled, 100302, "value").as_str().unwrap(), "");

    // Checkpoint: hub 810 AND node 488 of its subgraph definition.
    assert_eq!(
        named_node(&filled, 810, "ckpt_name").as_str().unwrap(),
        "noobai.safetensors"
    );
    let doc: Value = serde_json::from_str(&filled).unwrap();
    let sous_488 = &doc["definitions"]["subgraphs"][0]["nodes"][0];
    assert_eq!(
        sous_488["widgets_values_named"]["ckpt_name"]
            .as_str()
            .unwrap(),
        "noobai.safetensors"
    );
    // SayaMultiCouple repasse en solo (lot casting = solo).
    assert_eq!(
        doc["definitions"]["subgraphs"][0]["nodes"][1]["widgets_values_named"]["solo"],
        json!(true)
    );

    // LoRA 662: noobai → no style kept, the old line is disabled.
    let valeurs_662 = named_node(&filled, 662, "loras");
    let lignes_662 = valeurs_662.as_array().unwrap();
    assert!(lignes_662.iter().all(|l| l["active"] == json!(false)));
    assert_eq!(named_node(&filled, 662, "text").as_str().unwrap(), "");
    // 663: otoko re-enabled; 664: old pose disabled, sparkle line added.
    let valeurs_663 = named_node(&filled, 663, "loras");
    let lignes_663 = valeurs_663.as_array().unwrap();
    assert_eq!(
        lignes_663
            .iter()
            .find(|l| l["name"] == json!("otoko no ko_illustrious_V2.0"))
            .unwrap()["active"],
        json!(true)
    );
    let valeurs_664 = named_node(&filled, 664, "loras");
    let lignes_664 = valeurs_664.as_array().unwrap();
    assert_eq!(
        lignes_664
            .iter()
            .find(|l| l["name"] == json!("old-pose"))
            .unwrap()["active"],
        json!(false)
    );
    let sparkle = lignes_664
        .iter()
        .find(|l| l["name"] == json!("sparkleAlpha"))
        .unwrap();
    assert_eq!(sparkle["active"], json!(true));
    assert_eq!(sparkle["clipStrength"], json!("0.70"));
    assert_eq!(
        named_node(&filled, 664, "text").as_str().unwrap(),
        "<lora:sparkleAlpha:0.60:0.70>"
    );

    // Obsolete texts emptied, portrait format (invert false).
    assert_eq!(named_node(&filled, 654, "text").as_str().unwrap(), "");
    assert_eq!(named_node(&filled, 655, "text").as_str().unwrap(), "");
    assert_eq!(named_node(&filled, 100042, "invert"), json!(false));

    // The original source is never modified.
    assert_eq!(
        named_node(&source_fixture(), 807, "value").as_str().unwrap(),
        "ancien p1"
    );
}

#[test]
fn fill_source_rejects_a_non_saya_source() {
    let error = casting::fill_source(r#"{"nodes":[]}"#, &shot_bny_fixture())
        .expect_err("une source sans node Saya doit être refusée");
    assert!(
        error.to_string().contains("100309"),
        "message inattendu : {error}"
    );
}

#[test]
fn read_catalogue_validates_shots() {
    let dir = std::env::temp_dir().join("saya-studio-u1-catalogue");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("catalogue.json");
    std::fs::write(&path, json!([shot_bny_fixture()]).to_string()).unwrap();
    let shots = casting::read_catalogue(&path).unwrap();
    assert_eq!(shots.len(), 1);

    let mut sans_stem = shot_bny_fixture();
    sans_stem["stem"] = json!("");
    std::fs::write(&path, json!([sans_stem]).to_string()).unwrap();
    let error = casting::read_catalogue(&path).expect_err("shot sans stem refusé");
    assert!(error.to_string().contains("stem"), "{error}");

    let mut unknown_family = shot_bny_fixture();
    unknown_family["famille"] = json!("../invalid");
    std::fs::write(&path, json!([unknown_family]).to_string()).unwrap();
    assert!(casting::read_catalogue(&path).is_err());

    let mut new_family = shot_bny_fixture();
    new_family["famille"] = json!("nouvelle_classe");
    std::fs::write(&path, json!([new_family]).to_string()).unwrap();
    assert_eq!(
        casting::read_catalogue(&path).unwrap()[0]["famille"],
        "nouvelle_classe"
    );

    std::fs::write(&path, "[]").unwrap();
    assert!(casting::read_catalogue(&path).is_err());
}

#[test]
fn shot_seed_requires_a_u64() {
    assert_eq!(
        casting::shot_seed(&json!({"seed": 248919022623436u64})).unwrap(),
        "248919022623436"
    );
    assert!(casting::shot_seed(&json!({"seed": "abc"})).is_err());
    assert!(casting::shot_seed(&json!({})).is_err());
}

#[test]
fn casting_validate_persists_yes_through_the_database() {
    let db = studio_core::Database::in_memory().unwrap();
    let shot = shot_bny_fixture();
    let result = db
        .casting_validate("run-casting-1", "7", "sortie/casting/catalogue.json", &shot)
        .unwrap();
    assert_eq!(result["persiste"], json!(true));
    let lignes = db.casting_choices().unwrap();
    assert_eq!(lignes.len(), 1);
    assert_eq!(lignes[0]["stem"], json!("bny-00001-galatea-yorke-noobai"));
    assert_eq!(lignes[0]["famille"], json!("bny"));
    assert_eq!(lignes[0]["graine"], json!("7"));
    assert_eq!(
        lignes[0]["pose"].as_str().unwrap(),
        "standing, one leg up",
        "la pose vient des valeurs passe 1"
    );

    // Revalidating the same (run, stem): update, never a duplicate.
    db.casting_validate("run-casting-1", "7", "sortie/casting/catalogue.json", &shot)
        .unwrap();
    assert_eq!(db.casting_choices().unwrap().len(), 1);

    // Un autre run garde sa propre ligne.
    db.casting_validate("run-casting-2", "7", "sortie/casting/catalogue.json", &shot)
        .unwrap();
    assert_eq!(db.casting_choices().unwrap().len(), 2);

    // Refus propres : run trop court, famille inconnue, p1 vide.
    assert!(db.casting_validate("court", "7", "", &shot).is_err());
    let mut unknown_family = shot.clone();
    unknown_family["famille"] = json!("../invalid");
    assert!(db
        .casting_validate("run-casting-1", "7", "", &unknown_family)
        .is_err());
    let mut sans_p1 = shot.clone();
    sans_p1["casting"]["p1"] = json!("   ");
    assert!(db
        .casting_validate("run-casting-1", "7", "", &sans_p1)
        .is_err());
    assert_eq!(
        db.casting_choices().unwrap().len(),
        2,
        "aucun refus n'écrit"
    );
}

/// The real engine through the wrapped command: CLI casting and runtime casting
/// give the same catalogue, byte for byte (T1 pattern). ~2 full draws.
#[test]
#[ignore = "moteur GROK BUILD réel : lancer explicitement sur copie isolée"]
fn casting_catalogue_matches_cli_run_byte_for_byte() {
    let _garde = engine_lock();
    let engine_dir = engine_runtime::engine_dir();
    assert!(
        engine_dir.join("test_solo.py").is_file(),
        "moteur absent dans {} (SAYA_ENGINE_DIR ?)",
        engine_dir.display()
    );

    // 1. Direct CLI run, independent of the code under test (never sortie/dry-run/).
    let cli = std::process::Command::new("python3")
        .arg("test_solo.py")
        .arg("--casting")
        .arg("5")
        .arg("--famille")
        .arg("bny")
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
    let catalogue_cli = copy_catalogue(&engine_dir.join(engine_runtime::CASTING_CATALOGUE), "cli");

    // 2. Same seed through the wrapped command (the module under test).
    let outcome = engine_runtime::casting(SEED, 5, Some("bny"))
        .unwrap_or_else(|e| panic!("casting en échec : {e}"));
    let catalogue_commande = copy_catalogue(&outcome.catalogue, "commande");

    // 3. Byte-for-byte comparison + content.
    let octets_cli = std::fs::read(&catalogue_cli).unwrap();
    let octets_commande = std::fs::read(&catalogue_commande).unwrap();
    assert_eq!(
        octets_cli, octets_commande,
        "catalogues différents octet pour octet (CLI contre commande moteur, graine {SEED})"
    );
    assert!(
        outcome.stdout.contains("CASTING"),
        "stdout du moteur sans trace de casting : {}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains("sortie/casting"),
        "le casting doit écrire dans sortie/casting : {}",
        outcome.stdout
    );
    let shots: Vec<Value> = serde_json::from_slice(&octets_commande).unwrap();
    assert_eq!(shots.len(), 5, "5 OC demandés");
    for shot in &shots {
        assert_eq!(shot["note"], json!("oc"), "{}", shot["stem"]);
        assert_eq!(shot["famille"], json!("bny"), "{}", shot["stem"]);
        assert!(shot["casting"].is_object(), "{} sans passe 1", shot["stem"]);
    }
    assert!(outcome.catalogue.ends_with("sortie/casting/catalogue.json"));
}

/// The runtime rejects an out-of-range count and a non-normalized family without starting
/// the engine; an unknown but well-formed family fails ON THE ENGINE SIDE (fast failure
/// in draw_casting, before any draw).
#[test]
fn casting_runtime_rejects_invalid_parameters() {
    let _garde = engine_lock();
    let error = engine_runtime::casting(SEED, 0, None).expect_err("nombre nul refusé");
    assert!(error.to_string().contains("nombre d'OC"), "{error}");
    let error = engine_runtime::casting(SEED, 5000, None).expect_err("nombre excessif refusé");
    assert!(error.to_string().contains("nombre d'OC"), "{error}");
    let error =
        engine_runtime::casting(SEED, 5, Some("ZOMBIE")).expect_err("famille non normalisée");
    assert!(error.to_string().contains("famille invalide"), "{error}");
    let error = engine_runtime::casting_in_with_timeout(
        Path::new("/saya-studio-test/moteur-absent"),
        SEED,
        5,
        Some("bny"),
        std::time::Duration::from_secs(2),
    )
    .expect_err("moteur absent");
    assert!(
        error.to_string().contains("Moteur introuvable")
            && error.to_string().contains("moteur-absent"),
        "{error}"
    );
    // Unknown well-formed family: the engine fails fast, no catalogue rewritten.
    let engine_dir = engine_runtime::engine_dir();
    let avant = std::fs::read(engine_dir.join(engine_runtime::CASTING_CATALOGUE)).ok();
    let error = engine_runtime::casting(SEED, 2, Some("zombie"))
        .expect_err("famille inconnue doit échouer côté moteur");
    assert!(
        error.to_string().contains("famille de casting inconnue"),
        "{error}"
    );
    let apres = std::fs::read(engine_dir.join(engine_runtime::CASTING_CATALOGUE)).ok();
    assert_eq!(
        avant, apres,
        "un casting en échec ne doit pas réécrire le catalogue"
    );
}
