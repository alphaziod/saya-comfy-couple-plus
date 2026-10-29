//! Offline generation-tree relations: planned branches, path, cycle refusal,
//! orphan visibility. Temporary database only; no engine.
use studio_core::Database;

fn fixture() -> (Database, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("saya-tree-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("studio.sqlite3");
    let db = Database::open(&path).unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch(
        "INSERT INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES('wf','WF','hash','comfy_api','{}');
         INSERT INTO generation_jobs(id,workflow_revision_id,seed,state,request_snapshot_json) VALUES
           ('root','wf','1','succeeded','{}'),('child','wf','2','succeeded','{}'),('grand','wf','3','succeeded','{}');
         INSERT INTO generation_relations(id,parent_job_id,child_job_id,relation,label,state) VALUES
           ('r1','root','child','variant','child','linked'),
           ('r2','child','grand','detailer','grand','linked'),
           ('r3','root',NULL,'upscale','planned','planned');",
    )
    .unwrap();
    (db, dir)
}

#[test]
fn tree_path_branches_and_planned_children_are_consistent() {
    let (db, dir) = fixture();
    let tree = db.generation_tree("root").unwrap();
    assert_eq!(tree["root"]["jobId"], "root");
    let children = tree["root"]["children"].as_array().unwrap();
    assert_eq!(children.len(), 2, "{tree}");
    assert!(children.iter().any(|c| c["jobId"] == "child"));
    assert!(children.iter().any(|c| c["planned"] == true));
    let child = children.iter().find(|c| c["jobId"] == "child").unwrap();
    assert_eq!(child["children"][0]["jobId"], "grand");
    let path = db.generation_path("grand").unwrap();
    assert_eq!(
        path["path"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["jobId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["root", "child", "grand"]
    );
    let branches = db.generation_branches("root").unwrap();
    assert_eq!(branches["branches"].as_array().unwrap().len(), 2);
    let compare = db.generation_compare("child", "grand").unwrap();
    assert_eq!(compare["seeds"]["equal"], false);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cycle_and_second_parent_are_refused() {
    let (db, dir) = fixture();
    let error = db
        .record_generation_relation("grand", "root", "variant", "cycle", None)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Cycle") || error.contains("parent"),
        "{error}"
    );
    let error = db
        .record_generation_relation("grand", "child", "variant", "second", None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("parent"), "{error}");
    // A planned branch may be linked once, never twice.
    db.link_generation_child("r3", "grand").unwrap_err();
    std::fs::remove_dir_all(dir).unwrap();
}
