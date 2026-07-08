use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;

mod browser;
mod capture;
mod classify;
pub(crate) mod config;
mod context;
mod restore;
mod terminal;

/// Split a shell-style command string into tokens, respecting double-quoted segments.
/// Used by capture (to build quoted cmd_lines) and restore (to parse them back).
pub(crate) fn tokenize(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for ch in cmd.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            c if c.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// ── Schema version ──────────────────────────────────────────────────────────

const SCHEMA_VERSION: u32 = 2;
const THUMBNAIL_WIDTH:  u32 = 480;
const THUMBNAIL_HEIGHT: u32 = 270;

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
pub struct ProcessInfo {
    pub name: String,
    pub pid: u32,
    pub exe_path: String,
    pub cmd_line: String,
    pub classification: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WindowPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct WindowInfo {
    pub title: String,
    pub position: WindowPosition,
    pub size: WindowSize,
    pub state: String, // "normal" | "minimized" | "maximized"
    pub monitor_index: u32,
    /// Full path of the executable that owns this window.
    /// Added in schema_version 2; defaults to empty string for older snapshots.
    #[serde(default)]
    pub exe_path: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TerminalSession {
    pub shell: String,
    pub cwd: String,
    pub history: Vec<String>,
    pub window_title: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ContextClue {
    #[serde(rename = "type")]
    pub clue_type: String,
    pub value: String,
    pub confidence: f32,
    pub source: String,
}

/// Full snapshot — what is persisted to disk.
#[derive(Serialize, Deserialize, Clone)]
pub struct Snapshot {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub timestamp: String,
    pub processes: Vec<ProcessInfo>,
    pub windows: Vec<WindowInfo>,
    pub context_clues: Vec<ContextClue>,
    pub restore_hints: Vec<String>,
    pub warnings: Vec<String>,
    pub thumbnail_path: String,
    #[serde(default)]
    pub terminal_sessions: Vec<TerminalSession>,
}

/// Lightweight summary returned by list_snapshots — avoids loading full data.
#[derive(Serialize, Deserialize, Clone)]
pub struct SnapshotSummary {
    pub id: String,
    pub name: String,
    pub timestamp: String,
    pub thumbnail_path: String,
    pub warning_count: u32,
}

#[derive(Serialize, Deserialize)]
pub struct CaptureResult {
    pub snapshot: SnapshotSummary,
    pub warnings: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct RestoreResult {
    pub success: bool,
    pub message: String,
    /// Hard failures: apps that could not be launched at all.
    pub failed_items: Vec<String>,
    /// Soft warnings: windows that launched but could not be repositioned,
    /// plus any extra windows that refused to close during a clean restore.
    pub warnings: Vec<String>,
    /// Windows closed because they were not part of the snapshot (clean restore only).
    pub closed_items: Vec<String>,
}

// ── Storage helpers ──────────────────────────────────────────────────────────

/// Returns the snapshots directory, creating it if it does not exist.
fn snapshots_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {e}"))?;
    let dir = base.join("Snapshots");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Cannot create snapshots dir: {e}"))?;
    Ok(dir)
}

fn json_path(dir: &PathBuf, id: &str) -> PathBuf {
    dir.join(format!("{id}.json"))
}

fn png_path(dir: &PathBuf, id: &str) -> PathBuf {
    dir.join(format!("{id}.png"))
}

/// Try to read and parse a snapshot JSON file, returning None on any error
/// (corrupt file, missing fields, schema mismatch) so listing is always tolerant.
/// Unknown fields from newer schema versions are ignored by serde; fields added
/// since v1 carry #[serde(default)] so older files still load.
fn try_load_snapshot(path: &PathBuf) -> Option<Snapshot> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn snapshot_to_summary(s: &Snapshot) -> SnapshotSummary {
    SnapshotSummary {
        id: s.id.clone(),
        name: s.name.clone(),
        timestamp: s.timestamp.clone(),
        thumbnail_path: s.thumbnail_path.clone(),
        warning_count: s.warnings.len() as u32,
    }
}

/// Next free "Snapshot NN" auto-name number, derived from existing snapshot
/// names (not the file count) so deletions never produce a duplicate name.
/// Errors fall back to 1 so naming never fails.
fn next_auto_number(dir: &PathBuf) -> usize {
    let max = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path().extension().and_then(|ext| ext.to_str()) == Some("json")
                })
                .filter_map(|e| try_load_snapshot(&e.path()))
                .filter_map(|s| {
                    s.name
                        .strip_prefix("Snapshot ")
                        .and_then(|n| n.trim().parse::<usize>().ok())
                })
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    max + 1
}

/// Captures the primary monitor, resizes to thumbnail dimensions, and saves as PNG.
/// Returns Err on any failure — caller must treat this as a non-fatal warning.
fn capture_thumbnail(png_path: &PathBuf) -> Result<(), String> {
    use image::imageops::FilterType;

    let monitors = xcap::Monitor::all()
        .map_err(|e| format!("Could not enumerate monitors: {e}"))?;

    let monitor = monitors.into_iter().next()
        .ok_or_else(|| "No monitors found".to_string())?;

    let rgba_image = monitor
        .capture_image()
        .map_err(|e| format!("Screenshot capture failed: {e}"))?;

    let thumbnail = image::imageops::resize(
        &rgba_image,
        THUMBNAIL_WIDTH,
        THUMBNAIL_HEIGHT,
        FilterType::Lanczos3,
    );

    thumbnail.save(png_path)
        .map_err(|e| format!("Failed to save thumbnail PNG: {e}"))?;

    Ok(())
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
async fn take_snapshot(
    app: tauri::AppHandle,
    name: String,
) -> Result<CaptureResult, String> {
    let dir = snapshots_dir(&app)?;

    // Auto-name when the user provided no name
    let resolved_name = if name.trim().is_empty() {
        format!("Snapshot {:02}", next_auto_number(&dir))
    } else {
        name.trim().to_string()
    };

    let id = format!("snap_{}", chrono::Utc::now().timestamp_millis());
    let timestamp = chrono::Utc::now().to_rfc3339();
    let thumbnail_path_buf = png_path(&dir, &id);

    // Run the (slow) screenshot on a separate thread so it overlaps window/process
    // enumeration. Total capture time ≈ max(screenshot, enumeration), not the sum.
    let thumb_path = thumbnail_path_buf.clone();
    let thumb_handle = std::thread::spawn(move || capture_thumbnail(&thumb_path));

    // Real capture engine: enumerate windows + processes on this thread.
    let cfg = config::load_config(&app);
    let captured = capture::capture_desktop(&cfg.ignore_list);
    let mut warnings: Vec<String> = captured.warnings;

    match thumb_handle.join() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => warnings.push(format!("Thumbnail capture failed: {e}")),
        Err(_) => warnings.push("Thumbnail capture thread panicked".to_string()),
    }

    let terminal_sessions =
        terminal::capture_terminal_sessions(&captured.processes, &captured.windows);

    let snapshot = Snapshot {
        schema_version: SCHEMA_VERSION,
        id: id.clone(),
        name: resolved_name,
        timestamp,
        processes: captured.processes,
        windows: captured.windows,
        context_clues: captured.context_clues,
        restore_hints: captured.restore_hints,
        warnings: warnings.clone(),
        thumbnail_path: thumbnail_path_buf.to_string_lossy().into_owned(),
        terminal_sessions,
    };

    let json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| format!("Serialise error: {e}"))?;
    std::fs::write(json_path(&dir, &id), json)
        .map_err(|e| format!("Write error: {e}"))?;

    let summary = snapshot_to_summary(&snapshot);
    Ok(CaptureResult {
        snapshot: summary,
        warnings,
    })
}

#[tauri::command]
async fn recapture_snapshot(
    app: tauri::AppHandle,
    id: String,
) -> Result<CaptureResult, String> {
    let dir = snapshots_dir(&app)?;
    let existing_path = json_path(&dir, &id);

    let old_snapshot = try_load_snapshot(&existing_path)
        .ok_or_else(|| format!("Snapshot {id} not found or unreadable"))?;

    let timestamp = chrono::Utc::now().to_rfc3339();
    let thumbnail_path_buf = png_path(&dir, &id);

    // Screenshot on a separate thread, overlapping window enumeration.
    let thumb_tmp = dir.join(format!("{id}_tmp.png"));
    let thumb_tmp2 = thumb_tmp.clone();
    let thumb_handle = std::thread::spawn(move || capture_thumbnail(&thumb_tmp2));

    let cfg = config::load_config(&app);
    let captured = capture::capture_desktop(&cfg.ignore_list);
    let mut warnings: Vec<String> = captured.warnings;

    let thumb_ok = match thumb_handle.join() {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            warnings.push(format!("Thumbnail capture failed: {e}"));
            false
        }
        Err(_) => {
            warnings.push("Thumbnail capture thread panicked".to_string());
            false
        }
    };

    let terminal_sessions =
        terminal::capture_terminal_sessions(&captured.processes, &captured.windows);

    let snapshot = Snapshot {
        schema_version: SCHEMA_VERSION,
        id: id.clone(),
        name: old_snapshot.name,
        timestamp,
        processes: captured.processes,
        windows: captured.windows,
        context_clues: captured.context_clues,
        restore_hints: captured.restore_hints,
        warnings: warnings.clone(),
        thumbnail_path: thumbnail_path_buf.to_string_lossy().into_owned(),
        terminal_sessions,
    };

    // Write to temp file first, then rename — if capture fails the original is untouched.
    let tmp_json = dir.join(format!("{id}_tmp.json"));
    let json = serde_json::to_string_pretty(&snapshot)
        .map_err(|e| format!("Serialise error: {e}"))?;
    std::fs::write(&tmp_json, json)
        .map_err(|e| format!("Write error: {e}"))?;
    std::fs::rename(&tmp_json, &existing_path)
        .map_err(|e| format!("Rename error: {e}"))?;

    // Move temp thumbnail over the original only when capture fully succeeded —
    // a partially-written PNG must never replace a good thumbnail. On failure,
    // clean up the stray temp file instead of leaking it.
    if thumb_ok && thumb_tmp.exists() {
        let _ = std::fs::rename(&thumb_tmp, &thumbnail_path_buf);
    } else if thumb_tmp.exists() {
        let _ = std::fs::remove_file(&thumb_tmp);
    }

    let summary = snapshot_to_summary(&snapshot);
    Ok(CaptureResult {
        snapshot: summary,
        warnings,
    })
}

#[tauri::command]
async fn list_snapshots(app: tauri::AppHandle) -> Result<Vec<SnapshotSummary>, String> {
    let dir = snapshots_dir(&app)?;

    let entries = std::fs::read_dir(&dir)
        .map_err(|e| format!("Read dir error: {e}"))?;

    let mut summaries: Vec<SnapshotSummary> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()?.to_str()? != "json" {
                return None;
            }
            let snapshot = try_load_snapshot(&path)?;
            Some(snapshot_to_summary(&snapshot))
        })
        .collect();

    // Newest first
    summaries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(summaries)
}

#[tauri::command]
async fn restore_snapshot(
    app: tauri::AppHandle,
    id: String,
    close_others: Option<bool>,
) -> Result<RestoreResult, String> {
    let dir = snapshots_dir(&app)?;
    let path = json_path(&dir, &id);

    if !path.exists() {
        return Err(format!("Snapshot {id} not found"));
    }

    let snapshot = try_load_snapshot(&path)
        .ok_or_else(|| format!("Snapshot {id} is corrupt or unreadable"))?;

    let close_others = close_others.unwrap_or(false);
    let cfg = config::load_config(&app);
    let ignore_list = cfg.ignore_list;

    // Restore runs synchronous Win32 work; offload so we never block the async runtime.
    let result =
        tauri::async_runtime::spawn_blocking(move || restore::restore_desktop(&snapshot, close_others, &ignore_list))
            .await
            .map_err(|e| format!("Restore task failed: {e}"))?;

    Ok(result)
}

/// Heuristic: is the desktop the user is looking at right now already captured in some
/// saved snapshot? Compares the set of currently-open apps against each snapshot's app
/// set (Jaccard similarity). Used to warn before a clean restore would discard unsaved
/// state. Conservative: returns `false` (treat as unsaved) when uncertain.
#[tauri::command]
async fn is_current_state_saved(app: tauri::AppHandle) -> Result<bool, String> {
    let dir = snapshots_dir(&app)?;

    // Win32 window enumeration + per-snapshot disk reads are synchronous;
    // offload so we never block the async runtime (same pattern as restore).
    tauri::async_runtime::spawn_blocking(move || {
        let current = restore::current_app_set();
        if current.is_empty() {
            // Nothing meaningful open — nothing to lose, treat as "saved".
            return Ok(true);
        }

        let entries = std::fs::read_dir(&dir).map_err(|e| format!("Read dir error: {e}"))?;
        let mut best = 0.0_f32;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Some(snap) = try_load_snapshot(&path) {
                let snap_set: std::collections::HashSet<String> = snap
                    .processes
                    .iter()
                    .filter(|p| !p.exe_path.is_empty())
                    .map(|p| restore::exe_stem_pub(&p.exe_path))
                    .collect();
                let inter = current.intersection(&snap_set).count() as f32;
                let union = current.union(&snap_set).count() as f32;
                if union > 0.0 {
                    best = best.max(inter / union);
                }
            }
        }

        // ≥ 0.8 overlap → the current arrangement is essentially already captured.
        Ok(best >= 0.8)
    })
    .await
    .map_err(|e| format!("State check failed: {e}"))?
}

#[tauri::command]
async fn delete_snapshot(
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    let dir = snapshots_dir(&app)?;

    let json = json_path(&dir, &id);
    if json.exists() {
        std::fs::remove_file(&json)
            .map_err(|e| format!("Delete JSON error: {e}"))?;
    }

    let png = png_path(&dir, &id);
    if png.exists() {
        std::fs::remove_file(&png)
            .map_err(|e| format!("Delete PNG error: {e}"))?;
    }

    Ok(())
}

#[tauri::command]
async fn clear_all_snapshots(app: tauri::AppHandle) -> Result<(), String> {
    let dir = snapshots_dir(&app)?;

    let entries = std::fs::read_dir(&dir)
        .map_err(|e| format!("Read dir error: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext == "json" || ext == "png" {
                let _ = std::fs::remove_file(&path); // best-effort
            }
        }
    }

    Ok(())
}

// ── Ignore list commands ─────────────────────────────────────────────────────

#[tauri::command]
async fn get_ignore_list(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    Ok(config::load_config(&app).ignore_list)
}

#[tauri::command]
async fn add_to_ignore_list(app: tauri::AppHandle, exe_name: String) -> Result<(), String> {
    let stem = config::normalize_exe_name(&exe_name);
    if stem.is_empty() {
        return Err("Empty process name".to_string());
    }
    if config::SYSTEM_PROTECTED.contains(&stem.as_str()) {
        return Err(format!("{stem} is a system-critical process and is always protected"));
    }
    let mut cfg = config::load_config(&app);
    if !cfg.ignore_list.contains(&stem) {
        cfg.ignore_list.push(stem);
        cfg.ignore_list.sort();
        config::save_config(&app, &cfg)?;
    }
    Ok(())
}

#[tauri::command]
async fn remove_from_ignore_list(app: tauri::AppHandle, exe_name: String) -> Result<(), String> {
    let stem = config::normalize_exe_name(&exe_name);
    let mut cfg = config::load_config(&app);
    cfg.ignore_list.retain(|e| *e != stem);
    config::save_config(&app, &cfg)
}

#[tauri::command]
async fn get_running_processes(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

    let cfg = config::load_config(&app);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        ProcessRefreshKind::new().with_exe(UpdateKind::Always),
    );

    let mut stems: Vec<String> = sys
        .processes()
        .values()
        .filter_map(|p| {
            let exe = p.exe()?.to_string_lossy().to_string();
            if exe.is_empty() { return None; }
            let stem = config::normalize_exe_name(&exe);
            if stem.is_empty() { return None; }
            if config::is_ignored(&stem, &cfg.ignore_list) { return None; }
            Some(stem)
        })
        .collect();
    stems.sort();
    stems.dedup();
    Ok(stems)
}

// ── App entry point ───────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            take_snapshot,
            recapture_snapshot,
            list_snapshots,
            restore_snapshot,
            delete_snapshot,
            clear_all_snapshots,
            is_current_state_saved,
            get_ignore_list,
            add_to_ignore_list,
            remove_from_ignore_list,
            get_running_processes,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
