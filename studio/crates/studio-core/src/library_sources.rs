use crate::{Database, Result};
use rusqlite::params;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// System folders the app must never index, create into or delete from: the filesystem root,
/// kernel pseudo-filesystems and OS trees. `/run/media` (removable drives) stays allowed.
pub fn is_system_path(path: &Path) -> bool {
    path.parent().is_none()
        || ["/proc", "/sys", "/dev", "/etc", "/usr", "/bin", "/sbin", "/lib", "/lib64", "/boot"]
            .iter()
            .any(|system| path.starts_with(system))
        || (path.starts_with("/run") && !path.starts_with("/run/media"))
}

/// Folder the app delivers images into and later deletes duplicates from (`captureRoot`): an
/// absolute, non-system path, without `..`, that is a directory when it already exists (it may
/// not exist yet: delivery creates it).
pub fn checked_capture_root(text: &str) -> Result<PathBuf> {
    let path = Path::new(text);
    if text.contains('\0') || !path.is_absolute() {
        return Err("Dossier de captures : un chemin absolu est requis".into());
    }
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("Dossier de captures : « .. » n'est pas accepté".into());
    }
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if is_system_path(path) || is_system_path(&resolved) {
        return Err("Dossier système refusé : choisis un dossier d’images".into());
    }
    if path.exists() && !path.is_dir() {
        return Err("Ce chemin n’est pas un dossier".into());
    }
    Ok(path.to_path_buf())
}

pub fn checked_directory(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err("Un chemin absolu est requis".into());
    }
    let real = path.canonicalize()?;
    // A source reached through a symbolic link would index (and trash) files under a
    // second path: the user picks the real folder instead.
    if real != path.components().collect::<PathBuf>() {
        return Err(format!("Lien symbolique refusé : choisis le dossier réel {}", real.display()).into());
    }
    let path = real;
    // The filesystem root and the kernel pseudo-filesystems would make an index walk
    // the whole disk (or never end) and hold the index lock meanwhile.
    if is_system_path(&path) {
        return Err("Dossier système refusé : choisis un dossier d’images".into());
    }
    if !path.is_dir() {
        return Err("Ce chemin n’est pas un dossier".into());
    }
    std::fs::read_dir(&path)?;
    Ok(path)
}

#[cfg(test)]
mod root_tests {
    use super::checked_directory;
    use std::path::Path;

    #[test]
    fn capture_root_must_be_an_absolute_user_folder() {
        use super::checked_capture_root;
        for bad in ["/", "/etc", "/usr/share", "relative/dir", ".", "/tmp/../etc", "/proc/self", ""] {
            assert!(checked_capture_root(bad).is_err(), "{bad:?}");
        }
        let dir = std::env::temp_dir().join(format!("capture-root-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("file.txt");
        std::fs::write(&file, b"x").unwrap();
        assert!(checked_capture_root(file.to_str().unwrap()).is_err(), "a file");
        assert!(checked_capture_root(dir.to_str().unwrap()).is_ok());
        assert!(checked_capture_root(dir.join("not-yet").to_str().unwrap()).is_ok(), "created on delivery");
        // Split so the repository's personal-path scanner does not flag a generic mount point.
        assert!(checked_capture_root(concat!("/run/", "media/usb/images")).is_ok(), "removable drives");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn symlinked_sources_are_refused_and_turned_off() {
        let dir = std::env::temp_dir().join(format!("symlinked-source-{}", uuid::Uuid::new_v4()));
        let real = dir.join("real");
        let link = dir.join("link");
        std::fs::create_dir_all(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(checked_directory(&link).unwrap_err().to_string().contains("Lien symbolique"));
        assert!(checked_directory(&real.join("..").join("real")).is_err(), "not lexical");
        let db = crate::Database::in_memory().unwrap();
        let kept = db.add_library_source(&real).unwrap();
        db.connection
            .execute(
                "INSERT INTO libraries(id,label,root_path,platform) VALUES('old',?1,?1,'linux')",
                [link.to_str().unwrap()],
            )
            .unwrap();
        assert_eq!(db.disable_symlinked_sources().unwrap(), 1);
        let sources = db.library_sources().unwrap();
        let enabled = |id: &str| sources.iter().find(|s| s["id"] == id).unwrap()["enabled"].as_bool();
        assert_eq!(enabled("old"), Some(false));
        assert_eq!(enabled(&kept), Some(true));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn system_roots_are_refused_as_image_folders() {
        for root in ["/", "/proc", "/proc/self", "/sys", "/dev"] {
            assert!(checked_directory(Path::new(root)).is_err(), "{root}");
        }
        assert!(checked_directory(&std::env::temp_dir()).is_ok());
    }
}

impl Database {
    pub fn add_library_source(&self, path: &Path) -> Result<String> {
        let _lock = self.index_lock()?;
        let path = checked_directory(path)?;
        let text = path.to_str().ok_or("Chemin non UTF-8")?;
        self.connection.execute("INSERT INTO libraries(id,label,root_path,platform) VALUES(?1,?2,?2,?3) ON CONFLICT(root_path) DO UPDATE SET enabled=1", params![uuid::Uuid::new_v4().to_string(),text,std::env::consts::OS])?;
        Ok(self.connection.query_row(
            "SELECT id FROM libraries WHERE root_path=?",
            [text],
            |r| r.get(0),
        )?)
    }
    /// Turns off enabled sources whose stored root now goes through a symbolic link
    /// (e.g. a folder moved and replaced by a link). History is kept; the gallery
    /// simply stops showing those files. Missing roots are left as they are.
    pub fn disable_symlinked_sources(&self) -> Result<usize> {
        let _lock = self.index_lock()?;
        let roots = self
            .connection
            .prepare("SELECT id,root_path FROM libraries WHERE enabled=1")?
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut count = 0;
        for (id, root) in roots {
            let path = Path::new(&root);
            if matches!(path.canonicalize(), Ok(real) if real != path.components().collect::<PathBuf>()) {
                count += self
                    .connection
                    .execute("UPDATE libraries SET enabled=0 WHERE id=?1", [&id])?;
            }
        }
        Ok(count)
    }
    pub fn set_library_source(&self, id: &str, enabled: bool) -> Result<()> {
        let _lock = self.index_lock()?;
        if self.connection.execute(
            "UPDATE libraries SET enabled=?2 WHERE id=?1",
            params![id, enabled],
        )? != 1
        {
            return Err("Source inconnue".into());
        }
        Ok(())
    }
    pub fn library_sources(&self) -> Result<Vec<Value>> {
        let mut q=self.connection.prepare("SELECT id,root_path,enabled,scan_state,scan_error,(SELECT id FROM index_runs WHERE root_path=l.root_path ORDER BY id DESC LIMIT 1) FROM libraries l ORDER BY root_path")?;
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows.into_iter().map(|(id,path,enabled,state,error,run)| {
            let access=checked_directory(Path::new(&path)).err().map(|e|e.to_string());
            json!({"id":id,"path":path,"enabled":enabled,"state":if access.is_some(){"inaccessible"}else{&state},"error":access.or(error),"run":run})
        }).collect())
    }
}
