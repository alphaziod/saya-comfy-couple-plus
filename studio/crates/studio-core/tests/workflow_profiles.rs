use serde_json::{json, Value};
use studio_core::{workflow::Edit, Database};

fn profile(db: &Database, id: &str) -> Value {
    db.workflows()
        .unwrap()
        .into_iter()
        .find(|w| w["id"] == id)
        .unwrap()["promptProfile"]
        .clone()
}
fn binding(node: &str, role: &str) -> Edit {
    Edit {
        node_id: node.into(),
        input_name: "text".into(),
        value: "unused".into(),
        section: Some(role.into()),
    }
}

#[test]
fn workflow_profiles_follow_only_saved_mapping_and_survive_reopen() {
    let directory = std::env::temp_dir().join(format!("studio-profiles-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("studio.sqlite3");
    let mut db = Database::open(&path).unwrap();
    let graph = json!({"a":{"class_type":"Text","inputs":{"text":"one,  exact\n"}},"b":{"class_type":"Text","inputs":{"text":"two"}}}).to_string();
    let id = db
        .import_workflow("COUPLE name is not evidence", &graph)
        .unwrap();
    assert_eq!(profile(&db, &id), "unconfigured");
    db.save_bindings(&id, &[binding("b", "P2")]).unwrap();
    assert_eq!(profile(&db, &id), "unconfigured");
    db.save_bindings(&id, &[binding("a", "P1"), binding("b", "P2")])
        .unwrap();
    assert_eq!(profile(&db, &id), "couple");
    db.save_bindings(&id, &[binding("a", "P1"), binding("b", "BG")])
        .unwrap();
    assert_eq!(profile(&db, &id), "solo");
    assert!(db.save_bindings(&id, &[binding("missing", "P2")]).is_err());
    assert_eq!(profile(&db, &id), "solo");
    drop(db);
    let mut db = Database::open(&path).unwrap();
    assert_eq!(profile(&db, &id), "solo");
    db.save_bindings(&id, &[]).unwrap();
    assert_eq!(profile(&db, &id), "unconfigured");
    drop(db);
    let sql =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let original: String = sql
        .query_row(
            "SELECT document_json FROM workflow_revisions WHERE id=?",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&original).unwrap(),
        serde_json::from_str::<Value>(&graph).unwrap()
    );
    drop(sql);
    std::fs::remove_dir_all(directory).unwrap();
}
