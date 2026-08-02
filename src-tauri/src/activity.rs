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

#[derive(Serialize)]
pub struct ActivityLog {
    pub path: String,
    pub text: String,
}

/// The raw log tail (oldest first), for the "Show logs" viewer. Returned
/// verbatim rather than parsed so a malformed line is still visible to the
/// user, where `list_activity` silently drops it.
#[tauri::command]
pub fn read_activity_log(app: tauri::AppHandle, lines: Option<usize>) -> Result<ActivityLog, String> {
    let mut path = app.path().app_data_dir().map_err(|e| e.to_string())?;
    path.push("Snapshots/activity.jsonl");
    let display = path.to_string_lossy().to_string();
    if !path.exists() { return Ok(ActivityLog { path: display, text: String::new() }); }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("Activity read error: {e}"))?;
    let all: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = all.len().saturating_sub(lines.unwrap_or(200).min(1000));
    Ok(ActivityLog { path: display, text: all[start..].join("\n") })
}

#[tauri::command]
pub fn list_activity(app: tauri::AppHandle, limit: Option<usize>) -> Result<Vec<ActivityEvent>, String> {
    let mut path = app.path().app_data_dir().map_err(|e| e.to_string())?;
    path.push("Snapshots/activity.jsonl");
    if !path.exists() { return Ok(vec![]); }
    let text = std::fs::read_to_string(path).map_err(|e| format!("Activity read error: {e}"))?;
    Ok(text.lines().rev().filter_map(|line| serde_json::from_str(line).ok()).take(limit.unwrap_or(50).min(200)).collect())
}
