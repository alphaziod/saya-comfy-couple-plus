use serde_json::json;
use studio_core::{
    grok::{parse_characters, Choice},
    Database,
};
fn choice(e: &studio_core::grok::Entry, character: Option<String>) -> Choice {
    Choice {
        path: e.path.clone(),
        key: e.key.clone(),
        sha256: e.sha256.clone(),
        mode: if character.is_some() {
            "variant"
        } else {
            "create"
        }
        .into(),
        character,
    }
}
#[test]
fn observed_formats_preserve_text_variants_duplicates_and_atomicity() {
    let dir = std::env::temp_dir().join(format!("saya-grok-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let a = dir.join("characters_ready.json");
    let b = dir.join("lot.json");
    std::fs::write(&a,json!({"characters":[{"id":"CH-001","name":"Same name","identity_prompt":"  exact, (tag:1.2)\nété 🌸\n","anatomy_prompt":" anatomy  ","bg_options":[{"bg_id":"A","bg_name":"Garden","scene_bg":" garden\n"},{"bg_id":"B","bg_name":"Night","scene_bg":" night  "}],"unknown":{"seed":18446744073709551615u64}}]}).to_string()).unwrap();
    std::fs::write(&b,json!([{"id":"test-01","name":"Same name","bank_id":"CH-001","p1":"variant  \n","anatomy":"variant anatomy","pose":" action  ","bg":" bg\n"}]).to_string()).unwrap();
    let db = Database::in_memory().unwrap();
    let entries = parse_characters(&a).unwrap().0;
    let e = &entries[0];
    assert_eq!(e.variants.len(), 2);
    // P1 kept as written, the anatomy appended; no action, quality or negative.
    assert_eq!(e.blocks["P1"], "  exact, (tag:1.2)\nété 🌸, anatomy");
    let roles: Vec<&String> = e.blocks.as_object().unwrap().keys().collect();
    assert!(roles.iter().all(|r| ["P1", "BG"].contains(&r.as_str())), "{roles:?}");
    assert!(e.identity.to_string().contains("18446744073709551615"));
    assert!(db.grok_import(&[choice(e, None)], false).is_err());
    let result = db.grok_import(&[choice(e, None)], true).unwrap();
    let id = result["imported"][0]["characterId"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(db.variants(&id).unwrap().len(), 3);
    assert_eq!(
        db.grok_import(&[choice(e, None)], true).unwrap()["skipped"],
        1
    );
    let other = parse_characters(&b).unwrap().0;
    let bad = choice(&other[0], Some("absent".into()));
    assert!(db.grok_import(&[bad], true).is_err());
    assert_eq!(db.variants(&id).unwrap().len(), 3);
    db.grok_import(&[choice(&other[0], Some(id.clone()))], true)
        .unwrap();
    let sheet = db.character_sheet(&id).unwrap();
    assert_eq!(sheet["identityPrompt"], "  exact, (tag:1.2)\nété 🌸\n");
    assert_eq!(db.variants(&id).unwrap().len(), 4);
    let old = e.clone();
    std::fs::write(&a, "{}").unwrap();
    assert!(db.grok_import(&[choice(&old, None)], true).is_err());
    for (name, data) in [
        (
            "valides",
            json!({"valides":[{"slug":"v","nom":"Validé","p1":" p1 ","anatomy":" a "}]}),
        ),
        (
            "persos",
            json!({"persos":[{"id":"p","nom":"Perso","p1":" p1 ","anatomie":" a ","decors":["bg one","bg two"]}]}),
        ),
    ] {
        let path = dir.join(format!("{name}.json"));
        std::fs::write(&path, data.to_string()).unwrap();
        assert_eq!(parse_characters(&path).unwrap().0.len(), 1);
    }
    let code = dir.join("evil.py");
    std::fs::write(&code, "raise SystemExit('must never execute')").unwrap();
    assert!(parse_characters(&code).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn bad_second_entry_rolls_back_first_creation() {
    let file = std::env::temp_dir().join(format!("saya-grok-{}.json", uuid::Uuid::new_v4()));
    std::fs::write(
        &file,
        json!([{"id":"one","name":"One","p1":"x"},{"id":"two","name":"Two","p1":"y"}]).to_string(),
    )
    .unwrap();
    let rows = parse_characters(&file).unwrap().0;
    let db = Database::in_memory().unwrap();
    assert!(db
        .grok_import(
            &[
                choice(&rows[0], None),
                choice(&rows[1], Some("missing".into()))
            ],
            true
        )
        .is_err());
    assert_eq!(db.character_cards().unwrap().len(), 0);
    std::fs::remove_file(file).unwrap();
}
#[test]
#[ignore = "Read-only opt-in verification of local Grok formats"]
fn actual_local_grok_exports() {
    let Some(root) = std::env::var_os("SAYA_REAL_GROK_DIR").map(std::path::PathBuf::from) else {
        eprintln!("skip: régler SAYA_REAL_GROK_DIR (dossier des exports du moteur de tirage)");
        return;
    };
    for path in [
        "PACKAGE FOR BIG PROMPT/characters_ready.json",
        "test_solo/reference/lot_graine7.json",
        "test_solo/negatifs_figes.json",
        "context/persos_saya/persos_obligatoires.json",
    ] {
        let (entries, warnings) = parse_characters(&root.join(path)).unwrap();
        assert!(!entries.is_empty(), "{path}");
        println!(
            "{path}: {} entries, {} warnings",
            entries.len(),
            warnings.len()
        );
    }
}
