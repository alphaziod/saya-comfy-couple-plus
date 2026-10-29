//! Backup rotation, free-space guard and migration 0029 (query indexes).
//! Everything lives in private temporary directories.
use rusqlite::Connection;
use std::{fs, path::PathBuf, time::Duration};
use studio_core::{backup, Database};

struct Tmp(PathBuf);
impl Tmp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("storage-maint-{}", uuid_like()));
        fs::create_dir_all(&p).unwrap();
        Tmp(p)
    }
    fn db(&self) -> PathBuf {
        self.0.join("studio.sqlite3")
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn uuid_like() -> String {
    format!(
        "{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

const NEW_INDEXES: [&str; 5] = [
    "job_events_job",
    "generation_jobs_created",
    "generation_jobs_comfy_prompt",
    "pipeline_run_steps_job",
    "assets_capture",
];

fn has_index(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='index' AND name=?",
        [name],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
        == 1
}
fn user_version(conn: &Connection) -> u32 {
    conn.pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap()
}

#[test]
fn rotate_backups_keeps_only_the_most_recent_n() {
    let tmp = Tmp::new();
    for i in 0..4 {
        fs::write(tmp.0.join(format!("studio-v28-{i}.sqlite3")), b"x").unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }
    fs::write(tmp.0.join("unrelated.sqlite3"), b"x").unwrap();
    let removed = backup::rotate_backups(&tmp.0, "studio-v", ".sqlite3", 2).unwrap();
    assert_eq!(removed, 2);
    assert!(!tmp.0.join("studio-v28-0.sqlite3").exists());
    assert!(!tmp.0.join("studio-v28-1.sqlite3").exists());
    assert!(tmp.0.join("studio-v28-2.sqlite3").exists());
    assert!(tmp.0.join("studio-v28-3.sqlite3").exists());
    assert!(tmp.0.join("unrelated.sqlite3").exists());
    // Fewer files than N: nothing is deleted.
    assert_eq!(backup::rotate_backups(&tmp.0, "studio-v", ".sqlite3", 5).unwrap(), 0);
}

#[test]
fn insufficient_space_errors_and_deletes_nothing() {
    let tmp = Tmp::new();
    let old = tmp.0.join("studio-v27-old.sqlite3");
    fs::write(&old, b"keep me").unwrap();
    // 1000 bytes DB needs 1200 free.
    let err = backup::ensure_free_space(1000, Some(1199)).unwrap_err();
    assert!(err.to_string().contains("Not enough free disk space"));
    assert!(old.exists());
    backup::ensure_free_space(1000, Some(1200)).unwrap();
    // Unknown free space never blocks.
    backup::ensure_free_space(1000, None).unwrap();
}

#[test]
fn migration_backups_are_rotated_to_two() {
    let tmp = Tmp::new();
    drop(Database::open(&tmp.db()).unwrap());
    // A backup written before rotation existed belongs to the user: never removed.
    let legacy = tmp.0.join("backups/studio-v26-legacy.sqlite3");
    fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    fs::write(&legacy, b"legacy").unwrap();
    // Downgrade to v28 so each open migrates and takes a backup.
    for round in 0..4 {
        let conn = Connection::open(tmp.db()).unwrap();
        conn.execute_batch("DELETE FROM schema_migrations WHERE version=29;")
            .unwrap();
        conn.pragma_update(None, "user_version", 28).unwrap();
        drop(conn);
        std::thread::sleep(Duration::from_millis(20));
        drop(Database::open(&tmp.db()).unwrap());
        let names: Vec<_> = fs::read_dir(tmp.0.join("backups"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let count = names.iter().filter(|n| n.starts_with("migration-") && n.ends_with(".sqlite3")).count();
        assert!(legacy.exists(), "legacy backup kept");
        assert_eq!(count, (round + 1).min(2), "round {round}");
        // Sidecars of rotated-out backups are removed with them.
        assert!(names.len() <= 3 * 2 + 1, "orphan sidecars: {names:?}");
    }
}

#[test]
fn migration_0029_applies_on_fresh_db_and_on_v28() {
    let fresh = Tmp::new();
    drop(Database::open(&fresh.db()).unwrap());
    let conn = Connection::open(fresh.db()).unwrap();
    assert_eq!(user_version(&conn), 31);
    for name in NEW_INDEXES {
        assert!(has_index(&conn, name), "missing {name}");
    }

    // Simulate a v28 database: drop the new indexes and the version row.
    for name in NEW_INDEXES {
        conn.execute_batch(&format!("DROP INDEX {name};")).unwrap();
    }
    conn.execute_batch("DELETE FROM schema_migrations WHERE version=29;")
        .unwrap();
    conn.pragma_update(None, "user_version", 28).unwrap();
    drop(conn);
    drop(Database::open(&fresh.db()).unwrap());
    let conn = Connection::open(fresh.db()).unwrap();
    assert_eq!(user_version(&conn), 31);
    for name in NEW_INDEXES {
        assert!(has_index(&conn, name), "missing {name} after v28 upgrade");
    }
    let rows: i64 = conn
        .query_row("SELECT count(*) FROM schema_migrations WHERE version=29", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1);
}

fn plan(conn: &Connection, sql: &str) -> String {
    let mut q = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
    q.query_map([], |r| r.get::<_, String>(3))
        .unwrap()
        .map(|r| r.unwrap())
        .collect::<Vec<_>>()
        .join(" | ")
}

#[test]
fn explain_query_plans_before_and_after_0029() {
    let tmp = Tmp::new();
    drop(Database::open(&tmp.db()).unwrap());
    let conn = Connection::open(tmp.db()).unwrap();
    let queries = [
        ("job page", "SELECT j.id FROM generation_jobs j ORDER BY j.created_at DESC,j.rowid DESC LIMIT 100"),
        ("job events", "SELECT event_type,payload_json,observed_at FROM job_events WHERE job_id='x' ORDER BY id DESC LIMIT 100"),
        ("comfy prompt", "SELECT id FROM generation_jobs WHERE comfy_prompt_id='p'"),
        ("run steps by job", "SELECT * FROM pipeline_run_steps WHERE job_id='x'"),
    ];
    let after: Vec<String> = queries.iter().map(|(_, q)| plan(&conn, q)).collect();
    for name in NEW_INDEXES {
        conn.execute_batch(&format!("DROP INDEX {name};")).unwrap();
    }
    let before: Vec<String> = queries.iter().map(|(_, q)| plan(&conn, q)).collect();
    for (i, (label, _)) in queries.iter().enumerate() {
        println!("[{label}]\n  before: {}\n  after:  {}", before[i], after[i]);
    }
    assert!(after[0].contains("generation_jobs_created"), "{}", after[0]);
    assert!(after[1].contains("job_events_job"), "{}", after[1]);
    assert!(after[2].contains("generation_jobs_comfy_prompt"), "{}", after[2]);
    assert!(after[3].contains("pipeline_run_steps_job"), "{}", after[3]);
}
