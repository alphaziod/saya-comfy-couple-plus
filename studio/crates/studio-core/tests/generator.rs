use serde_json::json;
use studio_core::generator::{generate, GeneratorInput};
fn input(value: serde_json::Value) -> GeneratorInput {
    serde_json::from_value(value).unwrap()
}
#[test]
fn generator_is_deterministic_preserves_locks_and_explains_constraints() {
    let mut request = input(
        json!({"seed":"18446744073709551615","required":{"species":"elf","role":"scholar"},"forbidden":{"hairColor":["black"]},"locked":{"name":"Mira","eyeColor":"green"},"checkpointFamily":"illustrious (declared)"}),
    );
    let first = generate(&request).unwrap();
    assert_eq!(first, generate(&request).unwrap());
    assert_eq!(first["attributes"]["world"], "fantasy");
    assert_eq!(first["attributes"]["species"], "elf");
    assert_eq!(first["attributes"]["eyeColor"], "green");
    assert!(first["prompts"]["P1"].as_str().unwrap().contains("robe"));
    assert!(first["prompts"]["P1"]
        .as_str()
        .unwrap()
        .contains("pointy ears"));
    assert_eq!(
        first["identity"]["generator"]["checkpointFamily"],
        request.checkpoint_family
    );
    assert!(first["trace"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["attribute"] == "eyeColor" && t["reason"] == "locked"));
    for n in 0..32 {
        request.seed = n.to_string();
        let next = generate(&request).unwrap();
        assert_eq!(next["attributes"]["name"], "Mira");
        assert_eq!(next["attributes"]["eyeColor"], "green");
        assert_ne!(next["attributes"]["hairColor"], "black");
        assert_eq!(next["attributes"]["world"], "fantasy");
    }
}
#[test]
fn generator_rejects_unknown_impossible_and_conflicting_rules() {
    for value in [
        json!({"seed":"1","required":{"world":"modern","species":"elf"}}),
        json!({"seed":"1","required":{"species":"elf"},"locked":{"species":"human"}}),
        json!({"seed":"1","required":{"eyeColor":"blue"},"forbidden":{"eyeColor":["blue"]}}),
        json!({"seed":"1","forbidden":{"world":["modern","fantasy","science-fiction"]}}),
        json!({"seed":"1","required":{"silentlyIgnored":"x"}}),
        json!({"seed":"-1"}),
    ] {
        assert!(generate(&input(value.clone())).is_err(), "{value}");
    }
    assert!(serde_json::from_value::<GeneratorInput>(json!({"seed":"1","extra":true})).is_err());
}
#[test]
fn generated_identity_and_exact_prompt_survive_record_roundtrip() {
    let dir = std::env::temp_dir().join(format!("studio-generator-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("db.sqlite3");
    let request = input(
        json!({"seed":"42","required":{"world":"science-fiction","species":"android"},"locked":{"hairColor":"silver"},"checkpointFamily":"local-family"}),
    );
    let generated = generate(&request).unwrap();
    let db = studio_core::Database::open(&path).unwrap();
    let id = db
        .save_character(
            None,
            generated["name"].as_str().unwrap().into(),
            generated["identity"].clone(),
        )
        .unwrap();
    let export = db.export_character(&id).unwrap();
    drop(db);
    let db = studio_core::Database::open(&path).unwrap();
    let imported = db.import_character_document(&export.to_string()).unwrap();
    let records = db.characters().unwrap();
    let record = records.iter().find(|c| c.id == imported).unwrap();
    assert_eq!(record.identity, generated["identity"]);
    assert_eq!(
        record.identity["identity_prompt"],
        generated["prompts"]["P1"]
    );
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn generated_variants_keep_identity_and_require_confirmation_and_fresh_revision() {
    let dir =
        std::env::temp_dir().join(format!("studio-generated-variant-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("db.sqlite3");
    let db = studio_core::Database::open(&path).unwrap();
    let original = generate(&input(
        json!({"seed":"11","required":{"species":"elf","role":"scholar"}}),
    ))
    .unwrap();
    let id = db
        .save_character(None, "Source".into(), original["identity"].clone())
        .unwrap();
    let variation = input(json!({"seed":"99","required":{"role":"guardian"}}));
    let prepared = db.prepare_generated_variant(&id, &variation).unwrap();
    let revision = prepared["revision"].as_u64().unwrap() as u32;
    for key in ["name", "species", "hairColor", "eyeColor"] {
        assert_eq!(
            prepared["result"]["attributes"][key],
            original["attributes"][key]
        );
    }
    assert!(prepared["result"]["prompts"]["P1"]
        .as_str()
        .unwrap()
        .contains("plate armor"));
    assert!(db.variants(&id).unwrap().is_empty());
    assert!(db
        .save_generated_variant(&id, "Armor", &variation, revision, false)
        .is_err());
    assert!(db
        .prepare_generated_variant(
            &id,
            &input(json!({"seed":"99","locked":{"species":"human"}}))
        )
        .is_err());
    let variant = db
        .save_generated_variant(&id, "Armor", &variation, revision, true)
        .unwrap();
    assert_eq!(db.characters().unwrap()[0].identity, original["identity"]);
    assert_eq!(
        db.variants(&id).unwrap()[0]["promptBlocks"],
        prepared["result"]["prompts"]
    );
    assert!(db
        .save_generated_variant(&id, "Stale", &variation, revision, true)
        .is_err());
    assert_eq!(db.variants(&id).unwrap().len(), 1);
    db.set_character_archived(&id, true).unwrap();
    assert!(db.prepare_generated_variant(&id, &variation).is_err());
    db.set_character_archived(&id, false).unwrap();
    let mut manual = original["identity"].clone();
    manual["identity_prompt"] = json!(format!(
        "{}, exact custom tag",
        manual["identity_prompt"].as_str().unwrap()
    ));
    db.save_character(Some(id.clone()), "Manual".into(), manual.clone())
        .unwrap();
    assert!(db
        .prepare_generated_variant(&id, &variation)
        .unwrap_err()
        .to_string()
        .contains("manuellement"));
    assert_eq!(db.characters().unwrap()[0].identity, manual);
    drop(db);
    let db = studio_core::Database::open(&path).unwrap();
    let variants = db.variants(&id).unwrap();
    assert_eq!(variants[0]["id"], variant);
    assert_eq!(
        variants[0]["constraints"]["basePrompt"],
        original["prompts"]["P1"]
    );
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
