//! Env-mutating tests for `paths` helpers (single test fn: the process env is global).

use std::path::PathBuf;
use studio_core::paths;

#[test]
fn helpers_follow_env_overrides() {
    let tmp = std::env::temp_dir().join(format!("saya-paths-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    for name in ["SAYA_COMFYUI_DIR", "SAYA_ENGINE_DIR", "XDG_PICTURES_DIR"] {
        std::env::remove_var(name);
    }
    std::env::set_var("HOME", &tmp);
    assert_eq!(paths::home_dir().unwrap(), tmp);
    assert_eq!(paths::comfyui_dir().unwrap(), tmp.join("AI/ComfyUI"));
    assert_eq!(paths::engine_dir(), tmp.join("Documents/GROK BUILD/generez"));
    assert_eq!(paths::pictures_dir().unwrap(), tmp.join("Pictures"));
    std::fs::create_dir_all(tmp.join("Images")).unwrap();
    assert_eq!(paths::pictures_dir().unwrap(), tmp.join("Images"));

    std::env::set_var("SAYA_COMFYUI_DIR", "/x/comfy");
    std::env::set_var("SAYA_ENGINE_DIR", "/x/engine");
    std::env::set_var("XDG_PICTURES_DIR", "/x/pics");
    assert_eq!(paths::comfyui_dir().unwrap(), PathBuf::from("/x/comfy"));
    assert_eq!(paths::engine_dir(), PathBuf::from("/x/engine"));
    assert_eq!(paths::pictures_dir().unwrap(), PathBuf::from("/x/pics"));

    std::env::remove_var("HOME");
    std::env::remove_var("SAYA_COMFYUI_DIR");
    assert!(paths::home_dir().is_err());
    assert!(paths::comfyui_dir().is_err());
    std::fs::remove_dir_all(&tmp).ok();
}
