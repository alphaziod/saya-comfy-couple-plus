use studio_core::{contracts::Seed, Database};

#[test]
fn fresh_database_is_empty_and_migration_is_repeatable() {
    let mut db = Database::in_memory().unwrap();
    let before = db.status().unwrap();
    // Parallel migrations go beyond 16: number never frozen.
    assert!(before.schema_version >= 16, "{}", before.schema_version);
    assert_eq!(
        before.counts.characters + before.counts.assets + before.counts.jobs,
        0
    );
    db.migrate().unwrap();
    assert_eq!(db.status().unwrap(), before);
}

#[test]
fn unsigned_seed_survives_json_without_javascript_rounding() {
    let seed = Seed::parse("18446744073709551615").unwrap();
    assert_eq!(
        serde_json::to_string(&seed).unwrap(),
        "\"18446744073709551615\""
    );
    assert!(Seed::parse("18446744073709551616").is_err());
    assert!(Seed::parse("-1").is_err());
    assert!(Seed::parse("01").is_err());
    assert!(serde_json::from_str::<Seed>("\"-1\"").is_err());
    assert!(serde_json::from_str::<Seed>("123").is_err());
}

#[test]
fn core_status_matches_frontend_contract_fixture() {
    let db = Database::in_memory().unwrap();
    let actual = serde_json::to_value(db.status().unwrap()).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../../../contracts/bootstrap-v1.json")).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn database_survives_close_and_reopen() {
    let path =
        std::env::temp_dir().join(format!("saya-studio-test-{}.sqlite3", std::process::id()));
    {
        let db = Database::open(&path).unwrap();
        assert!(db.status().unwrap().schema_version >= 16);
    }
    let db = Database::open(&path).unwrap();
    assert_eq!(db.status().unwrap().counts.assets, 0);
    drop(db);
    std::fs::remove_file(path).unwrap();
}
