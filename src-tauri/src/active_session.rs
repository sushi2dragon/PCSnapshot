use serde::{Deserialize, Serialize};
use tauri::Manager;

/// Persisted marker for the snapshot the user is currently working in.
/// Stored beside the snapshots so it survives app restarts.
#[derive(Clone, Serialize, Deserialize)]
pub struct ActiveSession {
    pub id: String,
    pub timestamp: String,
}

/// `AppData/Snapshots/active_session.json`, creating the dir if needed.
fn marker_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let mut dir = app.path().app_data_dir().ok()?;
    dir.push("Snapshots");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("active_session.json"))
}

fn read(path: &std::path::Path) -> Option<ActiveSession> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Mark `id` as the active session. Best-effort: any error is a silent no-op.
pub fn set(app: &tauri::AppHandle, id: &str) {
    let Some(path) = marker_path(app) else { return };
    let marker = ActiveSession {
        id: id.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    if let Ok(json) = serde_json::to_string(&marker) {
        let _ = std::fs::write(path, json);
    }
}

/// Remove the marker. Best-effort; missing file is fine.
pub fn clear(app: &tauri::AppHandle) {
    if let Some(path) = marker_path(app) {
        let _ = std::fs::remove_file(path);
    }
}

/// The currently-active snapshot id, or None if no marker is set.
pub fn current_id(app: &tauri::AppHandle) -> Option<String> {
    read(&marker_path(app)?).map(|m| m.id)
}

#[tauri::command]
pub fn get_active_session(app: tauri::AppHandle) -> Option<ActiveSession> {
    read(&marker_path(&app)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_round_trips_through_json() {
        let marker = ActiveSession { id: "snap_123".into(), timestamp: "2026-07-13T00:00:00+00:00".into() };
        let json = serde_json::to_string(&marker).unwrap();
        let back: ActiveSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "snap_123");
        assert_eq!(back.timestamp, "2026-07-13T00:00:00+00:00");
    }

    #[test]
    fn read_missing_file_returns_none() {
        assert!(read(std::path::Path::new("this_path_does_not_exist_zzq.json")).is_none());
    }
}
