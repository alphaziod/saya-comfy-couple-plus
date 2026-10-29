//! Per-user default locations (no personal path is hard-coded; everything derives from `$HOME`).

use std::path::PathBuf;

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Home directory of the current user (`$HOME`); an error when it is unset or empty.
pub fn home_dir() -> Result<PathBuf, String> {
    env_path("HOME").ok_or_else(|| "Variable d'environnement HOME absente".to_string())
}

/// ComfyUI installation: `SAYA_COMFYUI_DIR`, otherwise `$HOME/AI/ComfyUI`.
pub fn comfyui_dir() -> Result<PathBuf, String> {
    match env_path("SAYA_COMFYUI_DIR") {
        Some(dir) => Ok(dir),
        None => home_dir().map(|home| home.join("AI/ComfyUI")),
    }
}

/// The user's own folder for everything personal the app works with (draw engine, workflows,
/// app settings): `SAYA_CONFIG_DIR`, otherwise `$HOME/Documents/saya studio config`.
/// Its layout is described by the `INDEX.md` it contains.
pub fn config_dir() -> PathBuf {
    env_path("SAYA_CONFIG_DIR").unwrap_or_else(|| {
        home_dir()
            .unwrap_or_default()
            .join("Documents/saya studio config")
    })
}

/// Draw engine directory: `SAYA_ENGINE_DIR`, otherwise `<config>/GROK BUILD/generez`, otherwise
/// the former location `$HOME/Documents/GROK BUILD/generez` (the caller reports "not found").
pub fn engine_dir() -> PathBuf {
    env_path("SAYA_ENGINE_DIR").unwrap_or_else(|| {
        let configured = config_dir().join("GROK BUILD/generez");
        if configured.is_dir() {
            configured
        } else {
            home_dir()
                .unwrap_or_default()
                .join("Documents/GROK BUILD/generez")
        }
    })
}

/// Packaged data (presets, helpers): `/usr/share/saya-studio` for system packages, otherwise
/// `<binary>/../share/saya-studio` (AppImage, Nix store, Flatpak `/app`, Snap).
pub fn share_dir() -> PathBuf {
    let system = PathBuf::from("/usr/share/saya-studio");
    if system.is_dir() {
        return system;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|bin| bin.join("../share/saya-studio")))
        .filter(|dir| dir.is_dir())
        .unwrap_or(system)
}

/// Base presets (six-pass workflows, validated characters): `SAYA_STUDIO_PRESETS`, otherwise
/// `<config>/workflows` when it holds a `preset.json`, otherwise the packaged copy.
pub fn presets_dir() -> PathBuf {
    env_path("SAYA_STUDIO_PRESETS").unwrap_or_else(|| {
        let configured = config_dir().join("workflows");
        if configured.join("preset.json").is_file() {
            configured
        } else {
            share_dir().join("presets")
        }
    })
}

/// Pictures folder: `$XDG_PICTURES_DIR`, otherwise `$HOME/Images` when it exists, else `$HOME/Pictures`.
pub fn pictures_dir() -> Result<PathBuf, String> {
    if let Some(dir) = env_path("XDG_PICTURES_DIR") {
        return Ok(dir);
    }
    let home = home_dir()?;
    let images = home.join("Images");
    Ok(if images.is_dir() { images } else { home.join("Pictures") })
}
