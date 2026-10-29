//! Remove only byte-identical files; seeds never identify duplicates.
//! Identical bytes include the PNG metadata (prompt, seed, workflow), so two
//! renders that differ by any parameter can never be merged here. Only copies
//! that both live in Saya Studio storage (results, casting, capture root) are
//! touched: libraries owned by the user are never modified.
use crate::{Database, Result};
use serde_json::json;
use std::path::PathBuf;
impl Database {
    pub fn cleanup_exact_duplicates(&mut self) -> Result<serde_json::Value> {
        // Serialize cleanups, including two gallery windows. Indexing remains independent.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().map_err(|_| "Nettoyage déjà en cours")?;
        let rows = {
            let mut q=self.connection.prepare("SELECT a.id,a.absolute_path,a.content_sha256,a.library_id,a.relative_path FROM assets a JOIN libraries l ON l.id=a.library_id WHERE a.availability='present' AND l.enabled=1 AND a.content_sha256 IS NOT NULL AND EXISTS(SELECT 1 FROM assets b WHERE b.content_sha256=a.content_sha256 AND b.absolute_path!=a.absolute_path) ORDER BY CASE WHEN a.capture_role='final' THEN 0 ELSE 1 END,a.rowid")?;
            let result = q
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            result
        };
        let mut keep = std::collections::HashMap::new();
        let mut removed = 0;
        let mut bytes = 0u64;
        let mut journal = Vec::new();
        let mut backup = false;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let root = self.pass_state_root()?.join("duplicate-cleanup");
        let data = self
            .pass_state_root()?
            .parent()
            .ok_or("Base sans dossier")?
            .to_path_buf();
        let mut owned: Vec<PathBuf> = [data.join("results"), data.join("casting")]
            .into_iter()
            .filter_map(|p| p.canonicalize().ok())
            .collect();
        if let Ok(Some(capture)) = self.capture_root() {
            owned.push(capture);
        }
        let saya = |p: &PathBuf| {
            owned
                .iter()
                .any(|r| crate::fsutil::ensure_within(r, p).is_ok())
        };
        for (id, path, hash, library, relative) in rows {
            let file = PathBuf::from(&path);
            if !saya(&file)
                || !file
                    .symlink_metadata()
                    .is_ok_and(|m| m.file_type().is_file())
                || self.asset_path(&id).is_err()
            {
                continue;
            }
            let data = std::fs::read(&file)?;
            if crate::gallery::digest(&data) != hash {
                continue;
            }
            let Some((kept, kept_library, kept_relative)) = keep.get(&hash) else {
                keep.insert(hash, (path, library, relative));
                continue;
            };
            if &path == kept {
                continue;
            }
            let keeper = PathBuf::from(kept);
            if file.canonicalize().ok() == keeper.canonicalize().ok() {
                continue;
            }
            if !keeper
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_file())
                || std::fs::read(&keeper).ok().as_deref() != Some(data.as_slice())
            {
                continue;
            }
            if !backup {
                std::fs::create_dir_all(&root)?;
                // Refuse before touching anything when the copy would not fit.
                if let Some(db) = self.connection.path().filter(|p| !p.is_empty()) {
                    crate::backup::ensure_backup_space(std::path::Path::new(db), &root)?;
                }
                let destination = root.join(format!("before-{}.sqlite3", uuid::Uuid::new_v4()));
                self.connection
                    .execute("VACUUM INTO ?", [destination.to_string_lossy().as_ref()])?;
                backup = true;
                // Only the most recent pre-cleanup copy is kept; journals are never rotated.
                crate::backup::rotate_backups(&root, "before-", ".sqlite3", 1)?;
            }
            // Preserve every asset ID used by character portraits, jobs and manifests.
            // Publish the alias before unlink: a crash leaves an extra file, never a broken reference.
            journal.push(json!({"removed":path,"retained":kept,"sha256":hash,"bytes":data.len()}));
            let log = root.join(format!("journal-{stamp}.json"));
            crate::fsutil::write_atomic(&log, &serde_json::to_vec_pretty(&journal)?)?;
            let aliases = {
                let mut q = self
                    .connection
                    .prepare("SELECT id FROM assets WHERE absolute_path=?")?;
                let v = q
                    .query_map([&path], |r| r.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                v
            };
            let tx = self.connection.transaction()?;
            for alias in aliases {
                let mut relative = format!("./{kept_relative}");
                while tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM assets WHERE library_id=? AND relative_path=?)",
                    rusqlite::params![kept_library, relative],
                    |r| r.get::<_, bool>(0),
                )? {
                    relative = format!("./{relative}");
                }
                tx.execute(
                    "UPDATE assets SET absolute_path=?2,library_id=?3,relative_path=?4 WHERE id=?1",
                    rusqlite::params![alias, kept, kept_library, relative],
                )?;
                // The move trigger derives a lexical path; restore the canonical representative.
                tx.execute(
                    "UPDATE assets SET absolute_path=?2 WHERE id=?1",
                    rusqlite::params![alias, kept],
                )?;
            }
            tx.commit()?;
            // Recheck both files immediately before deleting, including the cached fingerprint.
            if std::fs::read(&file).ok().as_deref() == Some(data.as_slice())
                && std::fs::read(&keeper).ok().as_deref() == Some(data.as_slice())
            {
                std::fs::remove_file(&file)?;
                removed += 1;
                bytes += data.len() as u64;
            }
        }
        Ok(json!({"removed":removed,"bytes":bytes}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    /// Neutral 16x16 PNG with ComfyUI-like metadata (tEXt "parameters").
    fn png(rgb: [u8; 3], parameters: &str) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, 16, 16);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            e.add_text_chunk("parameters".into(), parameters.into())
                .unwrap();
            let mut w = e.write_header().unwrap();
            w.write_image_data(&rgb.repeat(256)).unwrap();
        }
        out
    }
    #[test]
    fn exact_copies_removed_but_different_renders_preserved() {
        let dir = std::env::temp_dir().join(format!("cleanup-{}", uuid::Uuid::new_v4()));
        let results = dir.join("results");
        let user = dir.join("user-library");
        std::fs::create_dir_all(&results).unwrap();
        std::fs::create_dir_all(&user).unwrap();
        let render = png([20, 30, 40], "seed: 7, cfg: 5");
        std::fs::write(results.join("job1-a-0.png"), &render).unwrap();
        std::fs::write(results.join("job2-a-0.png"), &render).unwrap();
        // Same seed and same pixels, different configuration: two legitimate results.
        std::fs::write(
            results.join("job3-a-0.png"),
            png([20, 30, 40], "seed: 7, cfg: 6"),
        )
        .unwrap();
        std::fs::write(
            results.join("job4-a-0.png"),
            png([21, 30, 40], "seed: 7, cfg: 5"),
        )
        .unwrap();
        // Identical copy in a user library: never modified.
        std::fs::write(user.join("mine.png"), &render).unwrap();
        std::fs::write(user.join("mine-copy.png"), &render).unwrap();
        let mut db = Database::open(&dir.join("studio.sqlite3")).unwrap();
        db.index_library(&results).unwrap();
        db.index_library(&user).unwrap();
        let report = db.cleanup_exact_duplicates().unwrap();
        assert_eq!(report["removed"], 1);
        assert_eq!(std::fs::read_dir(&results).unwrap().count(), 3);
        assert_eq!(std::fs::read_dir(&user).unwrap().count(), 2);
        let ids = {
            let mut q = db.connection.prepare("SELECT id FROM assets").unwrap();
            let v = q
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap();
            v
        };
        assert_eq!(ids.len(), 6);
        for id in ids {
            assert!(db.asset_path(&id).unwrap().is_file());
        }
        let shown = db.gallery(&Default::default()).unwrap().items.len();
        assert_eq!(shown,5,"cfg 5, cfg 6 et pixels différents restent distincts ; les fichiers de l'utilisateur restent visibles");
        assert_eq!(db.cleanup_exact_duplicates().unwrap()["removed"], 0);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
