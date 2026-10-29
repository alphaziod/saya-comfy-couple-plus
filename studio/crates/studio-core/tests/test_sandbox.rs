//! Test isolation: under `SAYA_STUDIO_SANDBOX`, a capture root outside
//! the sandbox is refused (no fallback to the real Images folders).
//! Dedicated file: the environment variable is global to the process.
use serde_json::json;
use studio_core::Database;

#[test]
fn capture_root_outside_sandbox_fails_instead_of_writing_user_folders() {
    let dir = std::env::temp_dir().join(format!("sandbox-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("SAYA_STUDIO_SANDBOX", &dir);
    let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
    let outside = std::env::temp_dir().join(format!("outside-{}", uuid::Uuid::new_v4()));
    db.set_preference("captureRoot", json!(outside.to_str().unwrap())).unwrap();
    assert!(db.capture_root().is_err());
    assert!(!outside.exists(), "aucun dossier créé hors du bac à sable");
    let inside = dir.join("captures");
    db.set_preference("captureRoot", json!(inside.to_str().unwrap())).unwrap();
    assert!(db.capture_root().unwrap().unwrap().starts_with(dir.canonicalize().unwrap()));
    std::fs::remove_dir_all(dir).unwrap();
}
