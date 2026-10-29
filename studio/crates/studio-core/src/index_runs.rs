use crate::{gallery::IndexReport, Database, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::{
    fs::{File, OpenOptions},
    path::Path,
};

impl Database {
    /// Like `index_lock`, but waits up to `patience` for another indexing to finish:
    /// a delivery (casting tick, auto-advance) must not fail because the UI or another
    /// delivery is indexing at the same moment.
    pub(crate) fn index_lock_waiting(&self, patience: std::time::Duration) -> Result<Option<File>> {
        let deadline = std::time::Instant::now() + patience;
        loop {
            match self.index_lock() {
                Ok(lock) => return Ok(lock),
                Err(error) if std::time::Instant::now() >= deadline => return Err(error),
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
            }
        }
    }
    pub(crate) fn index_lock(&self) -> Result<Option<File>> {
        let Some(path) = self.connection.path().filter(|p| !p.is_empty()) else {
            return Ok(None);
        };
        let path = Path::new(path).canonicalize()?;
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(format!("{}.index.lock", path.display()))?;
        file.try_lock()
            .map_err(|_| "Une indexation est déjà en cours sur cette base")?;
        Ok(Some(file))
    }
    pub(crate) fn start_index_run(&self, root: &Path) -> Result<i64> {
        let tx = self.connection.unchecked_transaction()?;
        // Only called while holding the process-independent index lock.
        tx.execute("UPDATE index_runs SET state='interrupted',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE state IN ('discovering','indexing')",[])?;
        tx.execute(
            "INSERT INTO index_runs(root_path,state) VALUES(?,'discovering')",
            [root.to_str().ok_or("Chemin non UTF-8")?],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }
    pub(crate) fn index_error(&self, run: i64, path: &Path, error: &str) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO index_errors(run_id,path,message) VALUES(?1,?2,?3)",
            params![run, path.to_string_lossy(), error],
        )?;
        tx.execute(
            "UPDATE index_runs SET error_count=error_count+1 WHERE id=?",
            [run],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn index_checkpoint(
        &self,
        run: i64,
        done: u32,
        total: u32,
        path: Option<&str>,
        report: &IndexReport,
    ) -> Result<()> {
        self.connection.execute("UPDATE index_runs SET state='indexing',completed=?2,total=?3,last_path=?4,indexed=?5,unchanged=?6,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",params![run,done,total,path,report.indexed,report.unchanged])?;
        Ok(())
    }
    pub(crate) fn finish_index_run(&self, run: i64, state: &str) -> Result<()> {
        self.connection.execute("UPDATE index_runs SET state=?2,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?1",params![run,state])?;
        Ok(())
    }
    pub fn latest_index_run(&self) -> Result<Value> {
        Ok(self.connection.query_row("SELECT id,root_path,state,completed,total,indexed,unchanged,error_count,last_path,started_at,updated_at FROM index_runs ORDER BY id DESC LIMIT 1",[],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"root":r.get::<_,String>(1)?,"state":r.get::<_,String>(2)?,"completed":r.get::<_,u32>(3)?,"total":r.get::<_,u32>(4)?,"indexed":r.get::<_,u32>(5)?,"unchanged":r.get::<_,u32>(6)?,"errorCount":r.get::<_,u32>(7)?,"lastPath":r.get::<_,Option<String>>(8)?,"startedAt":r.get::<_,String>(9)?,"updatedAt":r.get::<_,String>(10)?}))).optional()?.unwrap_or(Value::Null))
    }
    pub fn index_errors(&self, run: i64, offset: u32) -> Result<Vec<Value>> {
        let mut q=self.connection.prepare("SELECT id,path,message FROM index_errors WHERE run_id=?1 ORDER BY id LIMIT 100 OFFSET ?2")?;
        let rows=q.query_map(params![run,offset],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"path":r.get::<_,String>(1)?,"message":r.get::<_,String>(2)?})))?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}
