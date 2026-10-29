use serde_json::json;
use studio_core::{
    grok_scenes::{parse_scenes, SceneChoice},
    Database,
};
#[test]
fn literal_tables_never_execute_code_and_preserve_grouping_revisions() {
    let dir = std::env::temp_dir().join(format!("saya-scenes-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("source.py");
    let marker = dir.join("MUST_NOT_EXIST");
    let script=format!("from pathlib import Path\nPath({:?}).write_text('executed')\nPOLE_PERSO={{'CH-1':{{'scene':'  garden, (light:1.2)\\nété 🌸','intime':' room  '}},'CH-2':'SAME'}}\nPOLE_STAGES={{'theme':'{{accent}} stage'}}\nPOLE_INTIMES={{'theme':'room'}}\nraise RuntimeError('must not run')\n",marker.to_str().unwrap());
    std::fs::write(&path, &script).unwrap();
    let (entries, warnings) = parse_scenes(&path).unwrap();
    assert!(!marker.exists());
    assert_eq!(entries.len(), 2);
    assert!(warnings.iter().any(|s| s.contains("alias")));
    let e = &entries[0];
    assert_eq!(e.variants.len(), 2);
    assert_eq!(
        e.variants[0]["blocks"]["BG"],
        "  garden, (light:1.2)\nété 🌸"
    );
    let db = Database::in_memory().unwrap();
    let choice = || SceneChoice {
        path: e.path.clone(),
        sha256: e.sha256.clone(),
        key: e.key.clone(),
    };
    assert!(db.grok_scene_import(&[choice()], false).is_err());
    let ids = db.grok_scene_import(&[choice()], true).unwrap();
    assert_eq!(db.grok_scene_import(&[choice()], true).unwrap(), ids);
    std::fs::write(&path, script.replace("garden", "forest")).unwrap();
    assert!(db.grok_scene_import(&[choice()], true).is_err());
    let e2 = parse_scenes(&path).unwrap().0.remove(0);
    db.grok_scene_import(
        &[SceneChoice {
            path: e2.path,
            sha256: e2.sha256,
            key: e2.key,
        }],
        true,
    )
    .unwrap();
    assert_eq!(db.grok_scenes().unwrap().as_array().unwrap().len(), 2);
    assert_eq!(
        db.grok_scene(&ids[0]).unwrap()["variants"][0]["blocks"]["BG"],
        "  garden, (light:1.2)\nété 🌸"
    );
    let jsonpath = dir.join("export.json");
    std::fs::write(&jsonpath,json!([{"id":"one","quality":" quality ","p1":" person ","anatomy":" anatomy ","pose":" pose\n","bg":" bg ","negative":""}]).to_string()).unwrap();
    let rows = parse_scenes(&jsonpath).unwrap().0;
    assert_eq!(rows[0].variants[0]["blocks"]["ACTION"], " pose\n");
    assert_eq!(rows[0].variants[0]["blocks"]["NEG"], "");
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn migration_v20_preserves_character_and_creates_backed_up_scene_storage() {
    let dir = std::env::temp_dir().join(format!("saya-grok-migration-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("studio.sqlite3");
    let db = Database::open(&path).unwrap();
    let id = db
        .save_character(None, "Keep".into(), json!({"identity_prompt":" exact\n"}))
        .unwrap();
    drop(db);
    let conn = rusqlite::Connection::open(&path).unwrap();
    // The rewind also removes the v28 artifacts (G1): capture columns and
    // covering index have no IF NOT EXISTS equivalent in SQLite.
    conn.execute_batch("DROP TABLE grok_scenes; DROP INDEX IF EXISTS assets_gallery_cover;DROP INDEX IF EXISTS assets_capture; ALTER TABLE assets DROP COLUMN capture_role; ALTER TABLE assets DROP COLUMN capture_phase; ALTER TABLE assets DROP COLUMN capture_day; ALTER TABLE assets DROP COLUMN capture_famille; DELETE FROM schema_migrations WHERE version>=21; PRAGMA user_version=20;").unwrap();
    drop(conn);
    let db = Database::open(&path).unwrap();
    assert_eq!(
        db.character_sheet(&id).unwrap()["identityPrompt"],
        " exact\n"
    );
    assert!(db.grok_scenes().unwrap().as_array().unwrap().is_empty());
    drop(db);
    assert!(std::fs::read_dir(&dir).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains("backup")));
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
#[ignore = "Read-only observed local scene formats"]
fn actual_local_grok_scenes() {
    let Some(root) = std::env::var_os("SAYA_REAL_GROK_DIR").map(std::path::PathBuf::from) else {
        eprintln!("skip: régler SAYA_REAL_GROK_DIR (dossier des scènes du moteur de tirage)");
        return;
    };
    for rel in [
        "test_solo/test_solo.py",
        "test_solo/reference/lot_graine7.json",
        "PACKAGE FOR BIG PROMPT/scenes_reference.json",
    ] {
        let path = root.join(rel).display().to_string();
        let (entries, warnings) = parse_scenes(std::path::Path::new(&path)).unwrap();
        assert!(!entries.is_empty());
        println!(
            "{path}: {} scenes, {} diagnostics",
            entries.len(),
            warnings.len()
        );
    }
}
