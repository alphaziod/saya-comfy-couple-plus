//! Migration tests v1..v9 -> current (v24). Every fixture lives in a private temporary directory;
//! the user database and user images are never touched.
use rusqlite::{params, types::ValueRef, Connection, OpenFlags};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use studio_core::Database;

/// Current schema version; bump together with storage::VERSION.
const CURRENT: u32 = 31;

const NEW_TABLES: [&str; 24] = [
    "grok_scenes",
    "pipeline_definitions",
    "pipeline_runs",
    "pipeline_run_steps",
    "pipeline_run_events",
    "job_origins",
    "ab_cell_jobs",
    "recipes",
    "recipe_versions",
    "generation_relations",
    "image_features",
    "ab_campaigns",
    "ab_parameters",
    "ab_parameter_values",
    "ab_cells",
    "ab_cell_images",
    "ab_cell_verdicts",
    "ab_presets",
    "scene_manifests",
    "workflow_modules",
    "workflow_module_versions",
    "editor_sessions",
    "image_semantic_features",
    "casting_choices",
];
const IDENTITY: &str =
    "{\"prompt\":\"1girl,  solo,\\n  masterpiece   ,\\t日本語 é ü 🌸\",\"note\":\"a  b\\n\\nc\"}";
const IDENTITY2: &str = "{\"prompt\":\"  leading and trailing  \"}";
const BLOCKS: &str =
    "{\"P1\":\"red  hair,\\n blue eyes ,  ñandú\",\"QUALITY\":\"  best   quality  \"}";
const SEED_MAX: &str = "18446744073709551615";

struct Tmp(PathBuf);
impl Tmp {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("saya-mig16-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        Tmp(dir)
    }
    fn db(&self) -> PathBuf {
        self.0.join("studio.sqlite3")
    }
    fn backups(&self) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = match fs::read_dir(self.0.join("backups")) {
            Ok(entries) => entries.map(|e| e.unwrap().path()).collect(),
            Err(_) => Vec::new(),
        };
        // A WAL-mode backup may leave -wal/-shm companions; only the database counts.
        found.retain(|p| p.extension().is_some_and(|e| e == "sqlite3"));
        found.sort();
        found
    }
}
impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn migration_sql(number: u32) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let prefix = format!("{number:04}_");
    let file = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&prefix)
        })
        .unwrap_or_else(|| panic!("migration {number} not found"));
    fs::read_to_string(file).unwrap()
}

/// Builds a genuine vN database by applying migrations 1..=N (10 never existed).
fn make_old(path: &Path, version: u32) -> Connection {
    let conn = Connection::open(path).unwrap();
    for n in (1..=version).filter(|n| *n != 10) {
        conn.execute_batch(&migration_sql(n)).unwrap();
    }
    conn.pragma_update(None, "user_version", version).unwrap();
    conn
}

fn user_version(conn: &Connection) -> u32 {
    conn.pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap()
}
fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [name],
        |r| r.get::<_, u32>(0),
    )
    .unwrap()
        == 1
}
fn versions(conn: &Connection) -> Vec<u32> {
    let mut stmt = conn
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .unwrap();
    let rows = stmt.query_map([], |r| r.get(0)).unwrap();
    rows.map(|r| r.unwrap()).collect()
}
fn journal_mode(conn: &Connection) -> String {
    conn.pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap()
}

/// Every user table -> sorted, fully typed row dump (byte exact for text and blobs).
fn dump(conn: &Connection) -> BTreeMap<String, Vec<String>> {
    let names: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            )
            .unwrap();
        let rows = stmt.query_map([], |r| r.get(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    let mut out = BTreeMap::new();
    for name in names {
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{name}\"")).unwrap();
        let columns = stmt.column_count();
        let mut rows = stmt.query([]).unwrap();
        let mut lines = Vec::new();
        while let Some(row) = rows.next().unwrap() {
            let mut line = String::new();
            for i in 0..columns {
                match row.get_ref(i).unwrap() {
                    ValueRef::Null => line.push_str("N|"),
                    ValueRef::Integer(v) => line.push_str(&format!("I{v}|")),
                    ValueRef::Real(v) => line.push_str(&format!("R{:x}|", v.to_bits())),
                    ValueRef::Text(t) => {
                        line.push_str(&format!("T{:?}|", String::from_utf8_lossy(t)))
                    }
                    ValueRef::Blob(b) => line.push_str(&format!("B{b:02x?}|")),
                }
            }
            lines.push(line);
        }
        lines.sort();
        out.insert(name, lines);
    }
    out
}

fn assert_preserved(before: &BTreeMap<String, Vec<String>>, after: &BTreeMap<String, Vec<String>>) {
    for (table, rows) in before {
        // schema_migrations legitimately gains rows 11..16.
        if table == "schema_migrations" {
            for row in rows {
                assert!(after[table].contains(row), "schema_migrations lost {row}");
            }
            continue;
        }
        if table == "assets" || table == "libraries" {
            // v20 appends columns; every original typed byte and row must survive.
            assert_eq!(rows.len(), after[table].len(), "row count changed: {table}");
            for (old, new) in rows.iter().zip(&after[table]) {
                assert!(
                    new.starts_with(old),
                    "original columns changed: {table}: {old} -> {new}"
                );
            }
        } else {
            assert_eq!(rows, &after[table], "table {table} changed");
        }
    }
}

fn assert_healthy_v16(path: &Path) {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(user_version(&conn), CURRENT);
    for table in NEW_TABLES {
        assert!(table_exists(&conn, table), "missing table {table}");
    }
    let v = versions(&conn);
    for n in 11..=CURRENT {
        assert!(v.contains(&n), "schema_migrations misses {n}");
    }
    assert!(!v.contains(&10), "version 10 must stay absent");
    let expected: Vec<_> = (1..=9).chain(11..=CURRENT).collect();
    assert_eq!(v, expected);
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    let violations = conn
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query_map([], |_| Ok(()))
        .unwrap()
        .count();
    assert_eq!(violations, 0);
}

/// Fills a v9 database with representative user data.
fn populate_v9(conn: &Connection) {
    conn.execute(
        "INSERT INTO characters(id,name,identity_json,revision,archived) VALUES('c1','Saya  Ünï',?1,1,0),('c2','Archivée',?2,1,1)",
        params![IDENTITY, IDENTITY2],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO character_variants(id,character_id,name,prompt_blocks_json,constraints_json,revision) VALUES('v1','c1','Tenue  été',?1,'{\"a\":  1}',3)",
        [BLOCKS],
    )
    .unwrap();
    conn.execute_batch("UPDATE characters SET revision=2, name='Saya  Ünï v2' WHERE id='c1';")
        .unwrap();
    conn.execute_batch(
        "INSERT INTO libraries(id,label,root_path,platform) VALUES('l1','Fixture','/nonexistent/saya-fixture','linux');
         INSERT INTO assets(id,library_id,relative_path,content_sha256,byte_size,modified_at,availability,metadata_json,metadata_state,width,height,favorite,note,review)
           VALUES('a1','l1','x/one.png','aa11',123,'2026-01-01T00:00:00Z','present','{\"k\":\"v  w\"}','read',832,1216,1,'note  avec\nretour','accepted'),
                 ('a2','l1','x/two.png','bb22',456,NULL,'missing','{}','unknown',NULL,NULL,0,'','rejected');
         INSERT INTO asset_characters VALUES('a1','c1','reference','human'),('a2','c1','depicted','inferred');
         INSERT INTO character_references(character_id,variant_id,variant_name,asset_id,position,is_primary) VALUES('c1','v1','Tenue  été','a2',0,1);
         INSERT INTO character_reference_history(character_id,operation,changes_json) VALUES('c1','set','{\"before\":[],\"after\":[\"a2\"]}');
         INSERT INTO model_resources(id,kind,name,family,content_sha256) VALUES('m1','lora','LoRA  one','sdxl','cc33'),('m2','checkpoint','Ckpt','sdxl',NULL);
         INSERT INTO lora_groups VALUES('g1','Groupe');
         INSERT INTO lora_group_members VALUES('g1','m1');
         INSERT INTO asset_models VALUES('a1','m2','embedded');
         INSERT INTO preferences VALUES('ui','{\"theme\":  \"dark\",\"n\":1}'),('seed.last','\"18446744073709551615\"');
         INSERT INTO prompt_proposals(id,engine_root,request_json,response_json,decision) VALUES('p1','/nonexistent/engine','{\"q\":\"a  b\"}','{\"r\":\"c\\nd\"}','accept');
         INSERT INTO index_runs(id,root_path,state,completed,total,indexed,unchanged,error_count,last_path) VALUES(1,'/nonexistent/saya-fixture','completed',2,2,2,0,1,'x/two.png');
         INSERT INTO index_errors(id,run_id,path,message) VALUES(1,1,'x/two.png','unreadable  é');
         INSERT INTO import_records(id,source_path,source_sha256,source_key,character_id) VALUES('i1','/nonexistent/src.json','dd44','key','c1');",
    )
    .unwrap();
    let workflow = "{\"3\":{\"class_type\":\"KSampler\",\"inputs\":{\"seed\":18446744073709551615,\"text\":\"a  b\\nc\"}}}";
    conn.execute(
        "INSERT INTO workflow_revisions(id,name,source_path,source_sha256,format,document_json,adapter_id,adapter_version,capability_state) VALUES('w1','Flux  wf',NULL,'ee55','comfy_api',?1,'ad',2,'verified')",
        [workflow],
    )
    .unwrap();
    let snapshot = format!(
        "{{\"seed\":\"{SEED_MAX}\",\"prompts\":{{\"P1\":\"{}\",\"NEG\":\"bad  hands\\n\"}},\"characters\":[{{\"role\":\"P1\",\"characterId\":\"c1\",\"variant\":{{\"id\":\"v1\"}}}}]}}",
        "red  hair,\\n 🌸"
    );
    conn.execute(
        "INSERT INTO generation_jobs(id,workflow_revision_id,state,seed,request_snapshot_json,endpoint_url,comfy_prompt_id,outputs_json) VALUES('j1','w1','succeeded',?1,?2,'http://127.0.0.1:8188','pp1','[{\"assetId\":\"a1\",\"nodeId\":\"9\",\"outputIndex\":0,\"kind\":\"saved\"}]')",
        params![SEED_MAX, snapshot],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO generation_jobs(id,workflow_revision_id,parent_job_id,state,seed,request_snapshot_json,failure_json) VALUES('j2','w1','j1','failed','0','{}','{\"why\":\"x  y\"}')",
        [],
    )
    .unwrap();
    conn.execute_batch(
        "INSERT INTO workflow_bindings VALUES('w1','seed','[]','3','KSampler','seed','seed');
         INSERT INTO job_characters VALUES('j1','P1','c1','v1','{\"role\":\"P1\"}');
         INSERT INTO job_outputs VALUES('j1','a1','9',0,'saved');
         INSERT INTO pass_attempts(id,job_id,phase_key,attempt,state,effective_parameters_json) VALUES('pa1','j1','base',1,'succeeded','{\"cfg\":  4.5}');
         INSERT INTO pass_artifacts(id,pass_attempt_id,asset_id,kind,status,manifest_json) VALUES('art1','pa1','a1','image_boundary','validated','{}');
         INSERT INTO human_reviews(id,artifact_id,decision,note) VALUES('r1','art1','accept','bien  fait\nvraiment');
         INSERT INTO job_events(job_id,event_type,payload_json) VALUES('j1','done','{\"ok\":true}');",
    )
    .unwrap();
}

fn v9_populated(tmp: &Tmp) -> BTreeMap<String, Vec<String>> {
    let conn = make_old(&tmp.db(), 9);
    populate_v9(&conn);
    dump(&conn)
}

#[test]
fn old_databases_migrate_to_v16() {
    for version in [1u32, 5, 8, 9] {
        let tmp = Tmp::new();
        let before = {
            let conn = make_old(&tmp.db(), version);
            assert_eq!(versions(&conn).len() as u32, version, "fixture v{version}");
            dump(&conn)
        };
        drop(Database::open(&tmp.db()).unwrap());
        assert_healthy_v16(&tmp.db());
        let conn = Connection::open(tmp.db()).unwrap();
        assert_eq!(journal_mode(&conn), "wal");
        assert_preserved(&before, &dump(&conn));
        let backups = tmp.backups();
        assert_eq!(backups.len(), 1, "one backup for v{version}");
        assert!(backups[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(&format!("-v{version}-")));
    }
}

#[test]
fn populated_v9_is_preserved_byte_for_byte() {
    let tmp = Tmp::new();
    let before = v9_populated(&tmp);
    assert!(before["character_history"].len() >= 2, "history fixture");
    assert!(before["character_references"].len() >= 2);
    drop(Database::open(&tmp.db()).unwrap());
    assert_healthy_v16(&tmp.db());
    let conn = Connection::open(tmp.db()).unwrap();
    let after = dump(&conn);
    assert_preserved(&before, &after);
    for table in NEW_TABLES {
        assert!(after[table].is_empty(), "{table} must start empty");
    }
    let identity: String = conn
        .query_row(
            "SELECT identity_json FROM characters WHERE id='c1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(identity, IDENTITY);
    let blocks: String = conn
        .query_row(
            "SELECT prompt_blocks_json FROM character_variants WHERE id='v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(blocks, BLOCKS);
    let (seed, snapshot): (String, String) = conn
        .query_row(
            "SELECT seed,request_snapshot_json FROM generation_jobs WHERE id='j1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(seed, SEED_MAX);
    assert!(snapshot.contains("red  hair,\\n 🌸") && snapshot.contains(SEED_MAX));
    let wf: String = conn
        .query_row(
            "SELECT document_json FROM workflow_revisions WHERE id='w1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(wf.contains("\"seed\":18446744073709551615"));
    let (fav, note, review): (i64, String, String) = conn
        .query_row(
            "SELECT favorite,note,review FROM assets WHERE id='a1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (fav, note.as_str(), review.as_str()),
        (1, "note  avec\nretour", "accepted")
    );
    let pref: String = conn
        .query_row(
            "SELECT value_json FROM preferences WHERE key='ui'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(pref, "{\"theme\":  \"dark\",\"n\":1}");
}

#[test]
fn wal_content_survives_migration() {
    let tmp = Tmp::new();
    // Holder keeps everything in the -wal file: no checkpoint ever happens.
    let holder = Connection::open(tmp.db()).unwrap();
    assert_eq!(journal_mode_set(&holder), "wal");
    holder.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    for n in (1..=9).filter(|n| *n != 10) {
        holder.execute_batch(&migration_sql(n)).unwrap();
    }
    holder.pragma_update(None, "user_version", 9).unwrap();
    populate_v9(&holder);
    let before = dump(&holder);
    let wal = tmp.0.join("studio.sqlite3-wal");
    assert!(
        fs::metadata(&wal).unwrap().len() > 0,
        "data must live in the WAL"
    );

    drop(Database::open(&tmp.db()).unwrap());
    let conn = Connection::open(tmp.db()).unwrap();
    assert_eq!(journal_mode(&conn), "wal");
    assert_eq!(user_version(&conn), CURRENT);
    assert_preserved(&before, &dump(&conn));
    drop(conn);
    drop(holder);
    assert_healthy_v16(&tmp.db());
    let backups = tmp.backups();
    assert_eq!(backups.len(), 1);
    let backup =
        Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(user_version(&backup), 9);
    assert_preserved(&before, &dump(&backup));
}

fn journal_mode_set(conn: &Connection) -> String {
    conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))
        .unwrap()
}

#[test]
fn backup_of_v9_is_valid_and_complete() {
    let tmp = Tmp::new();
    let before = v9_populated(&tmp);
    drop(Database::open(&tmp.db()).unwrap());
    let backups = tmp.backups();
    assert_eq!(backups.len(), 1);
    let name = backups[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(
        name.starts_with("migration-") && name.contains("-v9-") && name.ends_with(".sqlite3"),
        "{name}"
    );
    let backup =
        Connection::open_with_flags(&backups[0], OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(user_version(&backup), 9);
    let integrity: String = backup
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    assert_eq!(dump(&backup), before);
    assert!(!table_exists(&backup, "recipes"));
}

fn assert_migration_rolled_back(
    path: &Path,
    before: &BTreeMap<String, Vec<String>>,
    conflict: &str,
) {
    let conn = Connection::open(path).unwrap();
    assert_eq!(user_version(&conn), 9, "user_version must stay 9");
    assert_eq!(versions(&conn), vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    for table in NEW_TABLES.iter().filter(|t| **t != conflict) {
        assert!(
            !table_exists(&conn, table),
            "{table} leaked from the failed migration"
        );
    }
    assert_eq!(&dump(&conn), before, "data changed by a failed migration");
}

fn failure_then_recovery(conflict: &str, create: &str) {
    let tmp = Tmp::new();
    let _ = v9_populated(&tmp);
    {
        let conn = Connection::open(tmp.db()).unwrap();
        conn.execute_batch(create).unwrap();
    }
    let before = dump(&Connection::open(tmp.db()).unwrap());
    assert!(
        Database::open(&tmp.db()).is_err(),
        "conflict on {conflict} must fail"
    );
    assert_migration_rolled_back(&tmp.db(), &before, conflict);
    assert!(
        !tmp.backups().is_empty(),
        "the pre-migration backup is kept"
    );
    // Still refused while the conflict remains, still intact.
    assert!(Database::open(&tmp.db()).is_err());
    assert_migration_rolled_back(&tmp.db(), &before, conflict);
    // Recovery.
    Connection::open(tmp.db())
        .unwrap()
        .execute_batch(&format!("DROP TABLE {conflict}"))
        .unwrap();
    let mut expected = before.clone();
    expected.remove(conflict);
    drop(Database::open(&tmp.db()).unwrap());
    assert_healthy_v16(&tmp.db());
    let after = dump(&Connection::open(tmp.db()).unwrap());
    assert_preserved(&expected, &after);
}

#[test]
fn failure_on_first_migration_0011_is_atomic_and_recoverable() {
    failure_then_recovery(
        "recipes",
        "CREATE TABLE recipes(user_col TEXT); INSERT INTO recipes VALUES('mine');",
    );
}

#[test]
fn failure_on_middle_migration_0015_is_atomic_and_recoverable() {
    failure_then_recovery(
        "scene_manifests",
        "CREATE TABLE scene_manifests(user_col TEXT);",
    );
}

#[test]
fn failure_on_last_migration_0016_is_atomic_and_recoverable() {
    failure_then_recovery("workflow_modules", "CREATE TABLE workflow_modules(user_col TEXT); INSERT INTO workflow_modules VALUES('mine');");
}

fn file_bytes(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap()
}

#[test]
fn newer_database_is_refused_without_modification() {
    let tmp = Tmp::new();
    {
        let conn = make_old(&tmp.db(), 9);
        populate_v9(&conn);
        conn.pragma_update(None, "user_version", CURRENT + 1)
            .unwrap();
    }
    let bytes = file_bytes(&tmp.db());
    assert!(Database::open(&tmp.db()).is_err());
    assert_eq!(file_bytes(&tmp.db()), bytes, "file modified");
    assert!(tmp.backups().is_empty(), "no backup for a newer database");
    assert!(!tmp.0.join("studio.sqlite3-wal").exists());
    let conn = Connection::open(tmp.db()).unwrap();
    assert_eq!(user_version(&conn), CURRENT + 1);
}

#[test]
fn newer_database_after_real_migration_is_refused_without_modification() {
    let tmp = Tmp::new();
    drop(Database::open(&tmp.db()).unwrap());
    let before = {
        let conn = Connection::open(tmp.db()).unwrap();
        conn.pragma_update(None, "user_version", CURRENT + 1)
            .unwrap();
        dump(&conn)
    };
    assert!(Database::open(&tmp.db()).is_err());
    let conn = Connection::open(tmp.db()).unwrap();
    assert_eq!(user_version(&conn), CURRENT + 1);
    assert_eq!(dump(&conn), before);
    assert!(tmp.backups().is_empty());
}

#[test]
fn unversioned_database_with_tables_is_refused_without_modification() {
    let tmp = Tmp::new();
    {
        let conn = Connection::open(tmp.db()).unwrap();
        conn.execute_batch(
            "CREATE TABLE user_data(value TEXT); INSERT INTO user_data VALUES('precious');",
        )
        .unwrap();
    }
    let bytes = file_bytes(&tmp.db());
    assert!(Database::open(&tmp.db()).is_err());
    assert_eq!(file_bytes(&tmp.db()), bytes);
    let conn = Connection::open(tmp.db()).unwrap();
    assert_eq!(user_version(&conn), 0);
    assert!(!table_exists(&conn, "schema_migrations"));
    assert!(tmp.backups().is_empty());
}

#[test]
fn foreign_identity_database_is_refused_without_backup_or_change() {
    let tmp = Tmp::new();
    {
        let conn = make_old(&tmp.db(), 9);
        conn.execute(
            "UPDATE schema_migrations SET name='other-app' WHERE version=1",
            [],
        )
        .unwrap();
    }
    let bytes = file_bytes(&tmp.db());
    assert!(Database::open(&tmp.db()).is_err());
    assert_eq!(file_bytes(&tmp.db()), bytes);
    assert!(tmp.backups().is_empty());
}

#[test]
fn fresh_database_is_v16_without_backup() {
    let tmp = Tmp::new();
    drop(Database::open(&tmp.db()).unwrap());
    assert_healthy_v16(&tmp.db());
    assert!(tmp.backups().is_empty(), "nothing to back up on creation");
    drop(Database::in_memory().unwrap());
}

#[test]
fn reopening_a_v16_database_is_idempotent() {
    let tmp = Tmp::new();
    let _ = v9_populated(&tmp);
    drop(Database::open(&tmp.db()).unwrap());
    let backups = tmp.backups();
    assert_eq!(backups.len(), 1);
    let before = dump(&Connection::open(tmp.db()).unwrap());
    drop(Database::open(&tmp.db()).unwrap());
    drop(Database::open(&tmp.db()).unwrap());
    assert_eq!(tmp.backups(), backups, "no new backup at v16");
    let conn = Connection::open(tmp.db()).unwrap();
    assert_eq!(
        dump(&conn),
        before,
        "reopening changed data (incl. applied_at)"
    );
    assert_eq!(journal_mode(&conn), "wal");
    drop(conn);
    assert_healthy_v16(&tmp.db());
}

#[test]
fn failure_on_migration_0017_is_atomic_and_recoverable() {
    failure_then_recovery("ab_cell_jobs", "CREATE TABLE ab_cell_jobs(user_col TEXT);");
}

#[test]
fn failure_on_migration_0018_is_atomic_and_recoverable() {
    failure_then_recovery("job_origins", "CREATE TABLE job_origins(user_col TEXT);");
}

#[test]
fn failure_on_migration_0019_is_atomic_and_recoverable() {
    failure_then_recovery(
        "pipeline_run_events",
        "CREATE TABLE pipeline_run_events(user_col TEXT);",
    );
}

#[test]
fn populated_v19_upgrade_backs_up_and_initializes_source_paths() {
    let tmp = Tmp::new();
    let conn = make_old(&tmp.db(), 9);
    populate_v9(&conn);
    let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let mut paths = fs::read_dir(migrations)
        .unwrap()
        .map(|p| p.unwrap().path())
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let name = path.file_name().unwrap().to_str().unwrap();
        let version = name[..4].parse::<u32>().unwrap();
        if (11..=19).contains(&version) {
            conn.execute_batch(&fs::read_to_string(path).unwrap())
                .unwrap();
        }
    }
    conn.pragma_update(None, "user_version", 19).unwrap();
    let before = dump(&conn);
    drop(conn);
    drop(Database::open(&tmp.db()).unwrap());
    let conn = Connection::open(tmp.db()).unwrap();
    assert_preserved(&before, &dump(&conn));
    assert_eq!(tmp.backups().len(), 1);
    assert_eq!(
        conn.query_row(
            "SELECT absolute_path,catalog_removed FROM assets WHERE id='a1'",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        )
        .unwrap(),
        ("/nonexistent/saya-fixture/x/one.png".into(), 0)
    );
    assert_eq!(
        conn.query_row("SELECT scan_state FROM libraries WHERE id='l1'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "idle"
    );
    assert_healthy_v16(&tmp.db());
}
