use serde::{Deserialize, Serialize};
use std::io::Write;
use tauri::Manager;

#[derive(Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub id: String,
    pub timestamp: String,
    pub kind: String,
    pub snapshot_name: Option<String>,
    pub status: String,
    pub summary: String,
    pub detail_lines: Vec<String>,
}

pub fn event(kind: &str, name: Option<String>, status: &str, summary: String, detail_lines: Vec<String>) -> ActivityEvent {
    ActivityEvent {
        id: format!("event_{}", chrono::Utc::now().timestamp_micros()),
        timestamp: chrono::Utc::now().to_rfc3339(),
        kind: kind.to_string(), snapshot_name: name, status: status.to_string(), summary, detail_lines,
    }
}

pub fn append(app: &tauri::AppHandle, event: ActivityEvent) {
    let Ok(mut dir) = app.path().app_data_dir() else { return };
    dir.push("Snapshots");
    if std::fs::create_dir_all(&dir).is_err() { return; }
    let Ok(line) = serde_json::to_string(&event) else { return };
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("activity.jsonl")) {
        let _ = writeln!(file, "{line}");
    }
}

#[tauri::command]
pub fn list_activity(app: tauri::AppHandle, limit: Option<usize>) -> Result<Vec<ActivityEvent>, String> {
    let mut path = app.path().app_data_dir().map_err(|e| e.to_string())?;
    path.push("Snapshots/activity.jsonl");
    if !path.exists() { return Ok(vec![]); }
    let text = std::fs::read_to_string(path).map_err(|e| format!("Activity read error: {e}"))?;
    Ok(text.lines().rev().filter_map(|line| serde_json::from_str(line).ok()).take(limit.unwrap_or(50).min(200)).collect())
}
