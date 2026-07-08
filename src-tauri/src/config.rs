//! App-level configuration — persisted to `config.json` in the app data directory.
//!
//! Houses the user-editable ignore list and the hardcoded system-critical process
//! list that capture, restore, and close-others all respect.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Processes that are always implicitly ignored — shell/system surfaces whose
/// closure would break the taskbar, Start menu, IME, or desktop itself.
/// Lowercased exe stems (no `.exe` suffix).
pub const SYSTEM_PROTECTED: &[&str] = &[
    "explorer",
    "csrss",
    "svchost",
    "dwm",
    "sihost",
    "ctfmon",
    "searchhost",
    "searchapp",
    "startmenuexperiencehost",
    "shellexperiencehost",
    "textinputhost",
    "applicationframehost",
    "lockapp",
    "systemsettings",
];

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct AppConfig {
    #[serde(default)]
    pub ignore_list: Vec<String>,
}

pub fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {e}"))?;
    std::fs::create_dir_all(&base).map_err(|e| format!("Cannot create app data dir: {e}"))?;
    Ok(base.join("config.json"))
}

pub fn load_config(app: &tauri::AppHandle) -> AppConfig {
    let path = match config_path(app) {
        Ok(p) => p,
        Err(_) => return AppConfig::default(),
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => return AppConfig::default(),
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save_config(app: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = config_path(app)?;
    let json = serde_json::to_string_pretty(config).map_err(|e| format!("Serialize error: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("Write config error: {e}"))
}

/// Returns true if the given exe stem (lowercased, no extension) should be
/// excluded — either because it's system-critical or on the user's ignore list.
pub fn is_ignored(exe_stem: &str, user_ignore_list: &[String]) -> bool {
    let stem = exe_stem.to_ascii_lowercase();
    SYSTEM_PROTECTED.contains(&stem.as_str())
        || user_ignore_list.iter().any(|e| *e == stem)
}

/// Normalize an exe name to a lowercase stem: strips path separators and `.exe`.
pub fn normalize_exe_name(raw: &str) -> String {
    let last = raw
        .rsplit(|c: char| c == '\\' || c == '/')
        .next()
        .unwrap_or(raw);
    let stem = last
        .strip_suffix(".exe")
        .or_else(|| last.strip_suffix(".EXE"))
        .unwrap_or(last);
    stem.to_ascii_lowercase()
}
