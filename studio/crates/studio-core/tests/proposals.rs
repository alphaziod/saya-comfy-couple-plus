use serde_json::json;
use studio_core::Database;
#[test]
fn proposal_decisions_require_confirmation_exact_original_and_survive_restart() {
    let dir = std::env::temp_dir().join(format!("studio-proposals-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("studio.sqlite3");
    let db = Database::open(&path).unwrap();
    let original = "P1: exact,  tags\nBG: scene";
    let response = json!({"sourceSha256":"a".repeat(64),"result":{"original_prompt":original,"full_prompt":format!("{original}, addition"),"status":"proposed_unvalidated","structural_validation":{"valid":true}}});
    let id = db
        .save_proposal("/trusted/engine", &json!({"prompt":original}), &response)
        .unwrap();
    assert!(db.decide_proposal(&id, "accept", false, original).is_err());
    assert!(db
        .decide_proposal(&id, "accept", true, "changed since proposal")
        .is_err());
    assert_eq!(
        db.decide_proposal(&id, "accept", true, original).unwrap(),
        response["result"]["full_prompt"]
    );
    assert!(db.decide_proposal(&id, "reject", true, original).is_err());
    drop(db);
    let db = Database::open(&path).unwrap();
    let history = db.proposals().unwrap();
    assert_eq!(history[0]["decision"], "accept");
    assert_eq!(db.proposal(&id).unwrap()["response"], response);
    let mut bad = response.clone();
    bad["result"]["structural_validation"]["valid"] = json!(false);
    assert!(db
        .save_proposal("/trusted/engine", &json!({"prompt":original}), &bad)
        .is_err());
    drop(db);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn v4_migration_keeps_records_and_creates_recoverable_backup() {
    let dir = std::env::temp_dir().join(format!("studio-v4-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("studio.sqlite3");
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch(include_str!("../../../migrations/0001_initial.sql"))
        .unwrap();
    c.execute_batch(include_str!("../../../migrations/0002_functional.sql"))
        .unwrap();
    c.execute_batch(include_str!("../../../migrations/0003_gallery.sql"))
        .unwrap();
    c.execute_batch(include_str!(
        "../../../migrations/0004_character_history.sql"
    ))
    .unwrap();
    c.execute_batch("PRAGMA user_version=4; INSERT INTO characters(id,name,identity_json) VALUES('keep','Existing','{}');").unwrap();
    drop(c);
    let db = Database::open(&path).unwrap();
    // Parallel migrations go beyond 16: number never frozen.
    assert!(db.status().unwrap().schema_version >= 16);
    assert_eq!(db.status().unwrap().counts.characters, 1);
    drop(db);
    let backups: Vec<_> = std::fs::read_dir(dir.join("backups")).unwrap().collect();
    assert_eq!(backups.len(), 1);
    let backup = rusqlite::Connection::open(backups[0].as_ref().unwrap().path()).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        4
    );
    drop(backup);
    let db = Database::open(&path).unwrap();
    assert!(db.proposals().unwrap().is_empty());
    drop(db);
    assert_eq!(std::fs::read_dir(dir.join("backups")).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}
