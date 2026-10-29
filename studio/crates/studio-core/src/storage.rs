use crate::contracts::{Bootstrap, Counts, API_VERSION};
use rusqlite::{Connection, TransactionBehavior};
use std::{error::Error, path::Path, time::Duration};

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
pub(crate) const VERSION: u32 = 31;
const MIGRATION: &str = include_str!("../../../migrations/0001_initial.sql");

pub struct Database {
    pub(crate) connection: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(15))?;
        let version: u32 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if (1..VERSION).contains(&version) {
            let identity: String = connection.query_row(
                "SELECT name FROM schema_migrations WHERE version=1",
                [],
                |r| r.get(0),
            )?;
            if identity != "saya-studio-initial" {
                return Err("Unknown database".into());
            }
            let folder = path
                .parent()
                .ok_or("Database has no parent")?
                .join("backups");
            std::fs::create_dir_all(&folder)?;
            crate::backup::ensure_backup_space(path, &folder)?;
            // Named after the database so rotation never touches another
            // database's backups when several share one folder.
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("studio");
            let backup = folder.join(format!(
                "migration-{stem}-v{version}-{}.sqlite3",
                uuid::Uuid::new_v4()
            ));
            connection.backup("main", &backup, None)?;
            let check =
                Connection::open_with_flags(&backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            let result: String = check.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
            if result != "ok" {
                return Err("Backup integrity check failed; migration refused".into());
            }
            drop(check);
            // Keep only the two most recent automatic migration backups. Older
            // `studio-v*` backups predate rotation and are left to the user.
            crate::backup::rotate_backups(&folder, &format!("migration-{stem}-v"), ".sqlite3", 2)?;
        }
        let db = Self::init(connection)?;
        db.connection.pragma_update(None, "journal_mode", "WAL")?;
        // G5 perf (measured on btrfs /home): in WAL, synchronous=FULL pays an
        // fsync per commit (~3.5 ms measured), NORMAL ~0.01 ms. NORMAL keeps
        // consistency (never a corrupted database); only a power cut can
        // lose the last commits — a risk class already accepted for
        // preferences and logs. Every write (preferences, annotations,
        // job events, captures) benefits from it.
        db.connection.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(db)
    }
    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(connection: Connection) -> Result<Self> {
        connection.busy_timeout(Duration::from_secs(15))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let mut database = Self { connection };
        database.migrate()?;
        Ok(database)
    }

    pub fn migrate(&mut self) -> Result<()> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: u32 = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > VERSION {
            return Err("Database is newer than this application; downgrade refused".into());
        }
        if version == 0 {
            // An unversioned user database must never be adopted implicitly.
            let tables: u32 = tx.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'", [], |r| r.get(0))?;
            if tables > 0 {
                return Err("Unrecognized database; migration refused".into());
            }
            tx.execute_batch(MIGRATION)?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        let identity: String = tx.query_row(
            "SELECT name FROM schema_migrations WHERE version=1",
            [],
            |r| r.get(0),
        )?;
        if identity != "saya-studio-initial" {
            return Err("Database identity mismatch".into());
        }
        if version < 2 {
            tx.execute_batch(include_str!("../../../migrations/0002_functional.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 3 {
            tx.execute_batch(include_str!("../../../migrations/0003_gallery.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 4 {
            tx.execute_batch(include_str!(
                "../../../migrations/0004_character_history.sql"
            ))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 5 {
            tx.execute_batch(include_str!(
                "../../../migrations/0005_prompt_proposals.sql"
            ))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 6 {
            tx.execute_batch(include_str!(
                "../../../migrations/0006_generation_provenance.sql"
            ))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 7 {
            tx.execute_batch(include_str!(
                "../../../migrations/0007_character_references.sql"
            ))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 8 {
            tx.execute_batch(include_str!(
                "../../../migrations/0008_reference_history.sql"
            ))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 9 {
            tx.execute_batch(include_str!("../../../migrations/0009_index_runs.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        // Version 10 belongs to another parallel worktree; this branch only adds 11.
        if version < 11 {
            tx.execute_batch(include_str!("../../../migrations/0011_recipes.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 12 {
            tx.execute_batch(include_str!("../../../migrations/0012_generation_tree.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 13 {
            tx.execute_batch(include_str!("../../../migrations/0013_visual_search.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 14 {
            tx.execute_batch(include_str!("../../../migrations/0014_ab_lab.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 15 {
            tx.execute_batch(include_str!("../../../migrations/0015_scene_manifests.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 16 {
            tx.execute_batch(include_str!(
                "../../../migrations/0016_workflow_modules.sql"
            ))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 17 {
            tx.execute_batch(include_str!("../../../migrations/0017_ab_cell_jobs.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 18 {
            tx.execute_batch(include_str!("../../../migrations/0018_job_origins.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 19 {
            tx.execute_batch(include_str!("../../../migrations/0019_pipeline_runs.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 20 {
            tx.execute_batch(include_str!("../../../migrations/0020_gallery_sources.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 21 {
            tx.execute_batch(include_str!("../../../migrations/0021_grok_scenes.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 22 {
            tx.execute_batch(include_str!("../../../migrations/0022_editor_sessions.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 23 {
            tx.execute_batch(include_str!(
                "../../../migrations/0023_image_features_spatial.sql"
            ))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 24 {
            tx.execute_batch(include_str!(
                "../../../migrations/0024_image_semantic_features.sql"
            ))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 25 {
            tx.execute_batch(include_str!(
                "../../../migrations/0025_image_blur_documents.sql"
            ))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 26 {
            tx.execute_batch(include_str!(
                "../../../migrations/0026_prepare_profiles.sql"
            ))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 27 {
            tx.execute_batch(include_str!("../../../migrations/0027_casting.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 28 {
            tx.execute_batch(include_str!(
                "../../../migrations/0028_gallery_captures.sql"
            ))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 29 {
            tx.execute_batch(include_str!("../../../migrations/0029_query_indexes.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 30 {
            tx.execute_batch(include_str!("../../../migrations/0030_capture_lineage.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        if version < 31 {
            tx.execute_batch(include_str!("../../../migrations/0031_gallery_cover_capture.sql"))?;
            tx.pragma_update(None, "user_version", VERSION)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn status(&self) -> Result<Bootstrap> {
        let count = |sql| self.connection.query_row(sql, [], |row| row.get(0));
        Ok(Bootstrap {
            api_version: API_VERSION,
            app_version: env!("CARGO_PKG_VERSION"),
            schema_version: VERSION,
            counts: Counts {
                characters: count("SELECT count(*) FROM characters WHERE archived=0")?,
                assets: count("SELECT count(*) FROM assets")?,
                jobs: count("SELECT count(*) FROM generation_jobs")?,
            },
            comfy_connected: false,
            implemented: vec![
                "gallery",
                "characters",
                "preferences",
                "comfy",
                "generation",
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_newer_or_unrecognized_database() {
        let connection = Connection::open_in_memory().unwrap();
        connection.pragma_update(None, "user_version", 99).unwrap();
        assert!(Database::init(connection).is_err());
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE user_data(value TEXT)")
            .unwrap();
        assert!(Database::init(connection).is_err());
    }

    #[test]
    fn schema_enforces_relations_states_and_json() {
        let db = Database::in_memory().unwrap();
        assert!(db.connection.execute("INSERT INTO character_variants(id,character_id,name,prompt_blocks_json) VALUES('v','missing','Variant','{}')", []).is_err());
        assert!(db
            .connection
            .execute(
                "INSERT INTO characters(id,name,identity_json) VALUES('c','Test','invalid')",
                []
            )
            .is_err());
        db.connection
            .execute(
                "INSERT INTO characters(id,name,identity_json) VALUES('c','Test','{}')",
                [],
            )
            .unwrap();
        db.connection.execute("INSERT INTO character_variants(id,character_id,name,prompt_blocks_json) VALUES('v','c','Variant','{}')", []).unwrap();
        assert!(db
            .connection
            .execute("DELETE FROM characters WHERE id='c'", [])
            .is_err());
        assert_eq!(db.status().unwrap().counts.characters, 1);
        let violation: Option<String> = db
            .connection
            .query_row("PRAGMA foreign_key_check", [], |r| r.get(0))
            .optional()
            .unwrap();
        assert!(violation.is_none());
    }

    use rusqlite::OptionalExtension;

    #[test]
    fn lora_groups_reject_checkpoints_and_kind_changes() {
        let db = Database::in_memory().unwrap();
        db.connection.execute_batch("INSERT INTO model_resources(id,kind,name) VALUES('l','lora','LoRA'),('c','checkpoint','Checkpoint'); INSERT INTO lora_groups(id,name) VALUES('g','Group');").unwrap();
        assert!(db
            .connection
            .execute("INSERT INTO lora_group_members VALUES('g','c')", [])
            .is_err());
        db.connection
            .execute("INSERT INTO lora_group_members VALUES('g','l')", [])
            .unwrap();
        assert!(db
            .connection
            .execute("UPDATE lora_group_members SET model_id='c'", [])
            .is_err());
        assert!(db
            .connection
            .execute(
                "UPDATE model_resources SET kind='checkpoint' WHERE id='l'",
                []
            )
            .is_err());
    }

    #[test]
    fn job_seed_and_states_are_checked_without_integer_coercion() {
        let db = Database::in_memory().unwrap();
        db.connection.execute("INSERT INTO workflow_revisions(id,name,source_sha256,format,document_json) VALUES('w','Test','hash','comfy_api','{}')", []).unwrap();
        let insert = "INSERT INTO generation_jobs(id,workflow_revision_id,seed,request_snapshot_json,state) VALUES(?1,'w',?1,'{}',?2)";
        for seed in ["0", "18446744073709551615"] {
            db.connection.execute(insert, [seed, "draft"]).unwrap();
        }
        for seed in [
            "",
            "-1",
            "01",
            "1.5",
            "18446744073709551616",
            "999999999999999999999",
        ] {
            assert!(
                db.connection.execute(insert, [seed, "draft"]).is_err(),
                "accepted {seed}"
            );
        }
        assert!(db
            .connection
            .execute(insert, ["42", "imaginary_state"])
            .is_err());
        let seed: String = db
            .connection
            .query_row(
                "SELECT seed FROM generation_jobs WHERE id='18446744073709551615'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(seed, "18446744073709551615");
    }
}
