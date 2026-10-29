use serde_json::{json, Value};
use studio_core::{gallery::Filters, Database};
#[test]
fn v5_migration_backfills_provenance_without_rewriting_snapshots_and_keeps_backup() {
    let dir = std::env::temp_dir().join(format!("studio-provenance-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("studio.sqlite3");
    let old = rusqlite::Connection::open(&path).unwrap();
    for sql in [
        include_str!("../../../migrations/0001_initial.sql"),
        include_str!("../../../migrations/0002_functional.sql"),
        include_str!("../../../migrations/0003_gallery.sql"),
        include_str!("../../../migrations/0004_character_history.sql"),
        include_str!("../../../migrations/0005_prompt_proposals.sql"),
    ] {
        old.execute_batch(sql).unwrap();
    }
    old.execute_batch("PRAGMA user_version=5;INSERT INTO characters(id,name,identity_json) VALUES('A','Alpha','{}'),('B','Beta','{}');INSERT INTO libraries(id,label,root_path,platform) VALUES('lib','Synthetic','/synthetic','linux');INSERT INTO assets(id,library_id,relative_path,availability) VALUES('img','lib','one.png','present');INSERT INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES('wf','Synthetic','hash','comfy_api','{}');").unwrap();
    let snapshot=json!({"characters":[{"role":"P1","characterId":"A","name":"Alpha","variant":{"id":"historical-variant","name":"Old"}},{"role":"P2","characterId":"B","name":"Beta","variant":null}],"prompt":{"seed":18446744073709551615u64}}).to_string();
    let outputs = json!([{"assetId":"img","source":{"filename":"one.png"}}]).to_string();
    old.execute("INSERT INTO generation_jobs(id,workflow_revision_id,seed,state,request_snapshot_json,outputs_json) VALUES('job','wf','18446744073709551615','succeeded',?1,?2)",rusqlite::params![snapshot,outputs]).unwrap();
    image::RgbImage::new(2, 2)
        .save(dir.join("one.png"))
        .unwrap();
    old.execute(
        "UPDATE libraries SET root_path=? WHERE id='lib'",
        [dir.to_str().unwrap()],
    )
    .unwrap();
    drop(old);
    let db = Database::open(&path).unwrap();
    // Parallel migrations go beyond 16: number never frozen.
    assert!(db.status().unwrap().schema_version >= 16);
    assert_eq!(db.job_details("job").unwrap()["requestJson"], snapshot);
    let f = Filters {
        characters: vec!["A".into(), "B".into()],
        variant: "historical-variant".into(),
        workflow: "wf".into(),
        job: "job".into(),
        ..Default::default()
    };
    assert_eq!(db.gallery(&f).unwrap().total, 1);
    let detail = db.asset_detail("img").unwrap();
    assert!(detail["generations"][0]["nodeId"].is_null());
    assert_eq!(detail["generations"][0]["kind"], "unknown");
    assert_eq!(detail["generations"][0]["seed"], "18446744073709551615");
    let backups = std::fs::read_dir(dir.join("backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let backup = rusqlite::Connection::open(&backups[0]).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        5
    );
    assert_eq!(
        backup
            .query_row(
                "SELECT request_snapshot_json FROM generation_jobs WHERE id='job'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        snapshot
    );
    drop(backup);
    drop(db);
    let db = Database::open(&path).unwrap();
    assert_eq!(db.gallery(&f).unwrap().total, 1);
    assert_eq!(std::fs::read_dir(dir.join("backups")).unwrap().count(), 1);
    drop(db);
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT outputs_json FROM generation_jobs WHERE id='job'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        outputs
    );
    assert_eq!(
        sql.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(sql);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn solo_snapshot_and_role_validation_never_invent_a_second_identity() {
    let mut db = Database::in_memory().unwrap();
    // Use the real preparation contract before attaching synthetic output records in a test database.
    let graph = json!({"a":{"class_type":"Text","inputs":{"text":"alpha, exact\n"}},"b":{"class_type":"Text","inputs":{"text":"beta, exact"}}});
    let wf = db.import_workflow("Pair", &graph.to_string()).unwrap();
    let edits = vec![
        studio_core::workflow::Edit {
            node_id: "a".into(),
            input_name: "text".into(),
            value: "alpha, exact\n".into(),
            section: Some("P1".into()),
        },
        studio_core::workflow::Edit {
            node_id: "b".into(),
            input_name: "text".into(),
            value: "beta, exact".into(),
            section: Some("P2".into()),
        },
    ];
    db.save_bindings(&wf, &edits).unwrap();
    let a = db.save_character(None, "Alpha".into(), json!({})).unwrap();
    let b = db.save_character(None, "Beta".into(), json!({})).unwrap();
    let variant = db
        .save_variant(&a, None, "Blue", json!({"P1":"alpha, exact\n"}), json!({}))
        .unwrap();
    let schema = json!({"Text":{"input":{"required":{"text":["STRING"]}},"output":["STRING"]}});
    let solo = db
        .prepare_job_with_characters(
            &wf,
            &edits,
            &schema,
            &[studio_core::workflow::JobCharacter {
                role: "P1".into(),
                character_id: a.clone(),
                variant_id: Some(variant.clone()),
            }],
        )
        .unwrap();
    let snap: Value = serde_json::from_str(&solo.request_json).unwrap();
    assert_eq!(snap["characters"].as_array().unwrap().len(), 1);
    assert_eq!(snap["characters"][0]["role"], "P1");
    assert_eq!(snap["prompt"], graph);
    let count = db.jobs().unwrap().len();
    assert!(db
        .prepare_job_with_characters(
            &wf,
            &edits,
            &schema,
            &[
                studio_core::workflow::JobCharacter {
                    role: "P1".into(),
                    character_id: a.clone(),
                    variant_id: None
                },
                studio_core::workflow::JobCharacter {
                    role: "P1".into(),
                    character_id: b.clone(),
                    variant_id: None
                }
            ]
        )
        .is_err());
    assert_eq!(db.jobs().unwrap().len(), count);
    db.set_character_archived(&b, true).unwrap();
    assert!(db
        .prepare_job_with_characters(
            &wf,
            &edits,
            &schema,
            &[studio_core::workflow::JobCharacter {
                role: "P2".into(),
                character_id: b,
                variant_id: None
            }]
        )
        .is_err());
    let prefs =
        json!({"characters":[a],"variant":variant,"workflow":wf,"job":solo.job_id,"favorite":true});
    db.set_preference("galleryFilters", prefs.clone()).unwrap();
    assert_eq!(db.preferences().unwrap()["galleryFilters"], prefs);
}

#[test]
fn character_job_history_filters_before_paging_and_keeps_snapshot() {
    let mut db = Database::in_memory().unwrap();
    let workflow = db
        .import_workflow(
            "history",
            &json!({"a":{"class_type":"Text","inputs":{"text":"one"}}}).to_string(),
        )
        .unwrap();
    let edits = vec![studio_core::workflow::Edit {
        node_id: "a".into(),
        input_name: "text".into(),
        value: "one".into(),
        section: Some("P1".into()),
    }];
    db.save_bindings(&workflow, &edits).unwrap();
    let a = db
        .save_character(None, "A".into(), json!({"studioTraits":"permanent"}))
        .unwrap();
    let b = db.save_character(None, "B".into(), json!({})).unwrap();
    let schema = json!({"Text":{"input":{"required":{"text":["STRING"]}},"output":["STRING"]}});
    let mut expected = Vec::new();
    for i in 0..205 {
        let character = if i < 105 { &a } else { &b };
        let job = db
            .prepare_job_with_characters(
                &workflow,
                &edits,
                &schema,
                &[studio_core::workflow::JobCharacter {
                    role: "P1".into(),
                    character_id: character.clone(),
                    variant_id: None,
                }],
            )
            .unwrap();
        if i < 105 {
            expected.push(job.job_id);
        }
    }
    expected.reverse();
    db.save_character(Some(a.clone()), "renamed".into(), json!({}))
        .unwrap();
    let first = db.character_jobs(&a, None).unwrap();
    assert_eq!(first.len(), 100);
    assert_eq!(first[0]["characters"][0]["name"], "A");
    assert_eq!(
        first[0]["characters"][0]["identity"]["studioTraits"],
        "permanent"
    );
    let last = first.last().unwrap()["id"].as_str().unwrap();
    let next = db.character_jobs(&a, Some(last)).unwrap();
    assert_eq!(next.len(), 5);
    let ids: Vec<_> = first
        .iter()
        .chain(next.iter())
        .map(|v| v["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids, expected);
    assert!(db.character_jobs("unknown", None).unwrap().is_empty());
    assert!(db.character_jobs(&a, Some("unknown")).unwrap().is_empty());
}
