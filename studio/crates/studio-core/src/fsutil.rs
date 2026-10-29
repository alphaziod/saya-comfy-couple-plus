//! Small shared filesystem helpers: hashing, atomic writes, root containment.

use crate::Result;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

/// Lowercase hexadecimal SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Lowercase hexadecimal SHA-256 of a file, read in 1 MiB chunks.
pub fn file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Writes `bytes` to `path` atomically: a temporary file in the same directory
/// (`.<name>.<uuid>.tmp`), fsync, rename over `path`, then a best-effort fsync
/// of the directory. The temporary file is removed on failure.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| format!("Invalid file path: {}", path.display()))?
        .to_string_lossy();
    let temp = parent.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));
    let written = (|| -> std::io::Result<()> {
        let mut file = File::options().write(true).create_new(true).open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })()
    .and_then(|()| std::fs::rename(&temp, path));
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(error.into());
    }
    // Best effort: persist the rename itself.
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Canonicalizes `root` and `path` and returns the canonical `path` only when it
/// lies inside `root` (symlinks are resolved, so a link pointing outside is rejected).
pub fn ensure_within(root: &Path, path: &Path) -> Result<PathBuf> {
    let root = root.canonicalize()?;
    let file = path.canonicalize()?;
    if !file.starts_with(&root) {
        return Err(format!("Chemin hors de la racine autorisée : {}", path.display()).into());
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("saya-fsutil-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn hashes_match_known_vector() {
        let empty = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(sha256_hex(b""), empty);
        let dir = temp_dir();
        let file = dir.join("data.bin");
        let data: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&file, &data).unwrap();
        assert_eq!(file_sha256(&file).unwrap(), sha256_hex(&data));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn write_atomic_replaces_and_leaves_no_temp() {
        let dir = temp_dir();
        let file = dir.join("out.json");
        write_atomic(&file, b"one").unwrap();
        write_atomic(&file, b"two").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"two");
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("out.json")]);
        assert!(write_atomic(&dir.join("missing/out.json"), b"x").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ensure_within_rejects_escapes() {
        let dir = temp_dir();
        let root = dir.join("root");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("in.txt"), b"x").unwrap();
        std::fs::write(dir.join("out.txt"), b"x").unwrap();
        assert!(ensure_within(&root, &root.join("in.txt")).is_ok());
        assert!(ensure_within(&root, &root.join("../out.txt")).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.join("out.txt"), root.join("link.txt")).unwrap();
            assert!(ensure_within(&root, &root.join("link.txt")).is_err());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
