use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

mod browser;
mod activity;
mod active_session;
pub mod browser_bridge;
mod capture;
mod classify;
mod clipboard;
mod companion;
pub(crate) mod config;
mod context;
mod explorer;
mod icons;
mod restore;
mod terminal;
mod terminal_hook;
mod vscode;

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

const SCHEMA_VERSION: u32 = 5;
const THUMBNAIL_WIDTH: u32 = 480;
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

/// A concrete File Explorer folder window. Kept separate from `WindowInfo`
/// because all folder windows share the protected Windows shell process.
#[derive(Serialize, Deserialize, Clone)]
pub struct ExplorerWindow {
    pub path: String,
    pub path_kind: String, // "filesystem" | "virtual"
    pub title: String,
    pub position: WindowPosition,
    pub size: WindowSize,
    pub state: String,
    pub monitor_index: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TerminalSession {
    pub shell: String,
    pub cwd: String,
    pub history: Vec<String>,
    pub window_title: String,
    /// For Windows Terminal windows, the actual shell running inside
    /// (cmd/powershell/pwsh) — the window's own exe is WindowsTerminal.exe, so this
    /// is what restore relaunches directly. Empty for non-WT or unresolved shells.
    #[serde(default)]
    pub inner_shell: String,
    /// The launchable executable for this shell (e.g. "cmd.exe", "powershell.exe",
    /// or a full git-bash.exe path). Restore relaunches this directly.
    #[serde(default)]
    pub exe: String,
}

/// Browser identity as reported by the companion extension. The profile ID is
/// generated and kept in extension-local storage; native browser IDs are not
/// durable and must never be persisted as a restore key.
#[derive(Serialize, Deserialize, Clone)]
pub struct BrowserIdentity {
    pub family: String,
    pub profile_instance_id: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BrowserCapabilities {
    pub tab_groups: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BrowserBounds {
    pub left: Option<i32>,
    pub top: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BrowserTab {
    pub url: String,
    pub title: String,
    pub index: i32,
    pub active: bool,
    pub pinned: bool,
    pub muted: bool,
    pub discarded: bool,
    pub group_key: Option<String>,
    pub restorable: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BrowserTabGroup {
    pub key: String,
    pub title: String,
    pub color: String,
    pub collapsed: bool,
    pub index: Option<i32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct BrowserWindow {
    pub ordinal: u32,
    pub bounds: BrowserBounds,
    pub state: String,
    pub focused: bool,
    pub tabs: Vec<BrowserTab>,
    pub groups: Vec<BrowserTabGroup>,
}

/// Structured, companion-derived browser state. This is intentionally separate
/// from loose restore hints because it preserves window, tab-order, and group
/// membership needed for a safe later reconciliation.
#[derive(Serialize, Deserialize, Clone)]
pub struct BrowserSession {
    pub protocol_version: u32,
    pub browser: BrowserIdentity,
    pub captured_at: String,
    pub capabilities: BrowserCapabilities,
    pub windows: Vec<BrowserWindow>,
}

fn deserialize_browser_sessions<'de, D>(deserializer: D) -> Result<Vec<BrowserSession>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Companion output is optional context. Preserve an otherwise valid
    // snapshot when a future/partial extension payload cannot be understood.
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
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
    #[serde(default)]
    pub explorer_windows: Vec<ExplorerWindow>,
    pub context_clues: Vec<ContextClue>,
    pub restore_hints: Vec<String>,
    pub warnings: Vec<String>,
    pub thumbnail_path: String,
    #[serde(default)]
    pub terminal_sessions: Vec<TerminalSession>,
    #[serde(default, deserialize_with = "deserialize_browser_sessions")]
    pub browser_sessions: Vec<BrowserSession>,
    /// Captured clipboard (current + Win+V history). Present only when the
    /// clipboard opt-in was on at capture time. Optional/tolerant so older
    /// snapshots load unchanged.
    #[serde(default)]
    pub clipboard: Option<clipboard::ClipboardBlock>,
}

/// Lightweight summary returned by list_snapshots — avoids loading full data.
#[derive(Serialize, Deserialize, Clone)]
pub struct SnapshotSummary {
    pub id: String,
    pub name: String,
    pub timestamp: String,
    pub thumbnail_path: String,
    pub warning_count: u32,
    /// Captured apps (processes, plus one for File Explorer if any folders were captured).
    pub app_count: u32,
    /// Distinct monitors spanned by the captured windows (at least 1).
    pub monitor_count: u32,
    /// Exe paths of the first few distinct apps (capture order, foreground first),
    /// for the tile's app-icon stack. Empty/dup-stem paths are skipped.
    pub top_apps: Vec<String>,
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

#[derive(Serialize, Deserialize)]
pub struct CloseResult { pub closed: Vec<String>, pub refused: Vec<String> }

// ── Storage helpers ──────────────────────────────────────────────────────────

/// Returns the snapshots directory, creating it if it does not exist.
fn snapshots_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {e}"))?;
    let dir = base.join("Snapshots");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create snapshots dir: {e}"))?;
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
    let mut monitors: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for w in &s.windows {
        monitors.insert(w.monitor_index);
    }
    for w in &s.explorer_windows {
        monitors.insert(w.monitor_index);
    }
    let app_count = s.processes.len() as u32 + if s.explorer_windows.is_empty() { 0 } else { 1 };
    // First few distinct apps (by exe stem), in capture order, for the tile icon stack.
    let mut seen = std::collections::HashSet::new();
    let mut top_apps = Vec::new();
    for p in &s.processes {
        if p.exe_path.is_empty() {
            continue;
        }
        let stem = std::path::Path::new(&p.exe_path)
            .file_stem()
            .and_then(|x| x.to_str())
            .unwrap_or("")
            .to_lowercase();
        if stem.is_empty() || !seen.insert(stem) {
            continue;
        }
        top_apps.push(p.exe_path.clone());
        if top_apps.len() >= 5 {
            break;
        }
    }
    SnapshotSummary {
        id: s.id.clone(),
        name: s.name.clone(),
        timestamp: s.timestamp.clone(),
        thumbnail_path: s.thumbnail_path.clone(),
        warning_count: s.warnings.len() as u32,
        app_count,
        monitor_count: (monitors.len() as u32).max(1),
        top_apps,
    }
}

fn with_snapshot_name(mut snapshot: Snapshot, name: &str) -> Result<Snapshot, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Snapshot name cannot be empty".to_string());
    }
    snapshot.name = trimmed.to_string();
    Ok(snapshot)
}

/// Next free "Snapshot NN" auto-name number, derived from existing snapshot
/// names (not the file count) so deletions never produce a duplicate name.
/// Errors fall back to 1 so naming never fails.
fn next_auto_number(dir: &PathBuf) -> usize {
    let max = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
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
/// Callers exclude their own window from the shot via `set_capture_exclusion`
/// before spawning this, so nothing here needs to know about window state.
fn capture_thumbnail(png_path: &PathBuf) -> Result<(), String> {
    use image::imageops::FilterType;

    let monitors =
        xcap::Monitor::all().map_err(|e| format!("Could not enumerate monitors: {e}"))?;

    let monitor = monitors
        .into_iter()
        .next()
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

    thumbnail
        .save(png_path)
        .map_err(|e| format!("Failed to save thumbnail PNG: {e}"))?;

    Ok(())
}

/// Toggle screen-capture exclusion for our own window (Windows 10 2004+).
///
/// With `exclude` true the window stays fully visible to the user but is omitted
/// from screen captures — BitBlt, PrintWindow, and the modern capture APIs — at
/// the DWM compositor level, so it never lands in the snapshot thumbnail (xcap
/// grabs the monitor via a desktop-DC `BitBlt`, which this suppresses). Unlike
/// hiding the window this doesn't flicker, steal focus, or depend on the UI
/// thread pumping a `ShowWindow` message before the shot fires. Any failure
/// (older Windows where the flag is unsupported, or an unavailable handle) is a
/// silent no-op — the thumbnail just includes the window as it did before.
#[cfg(windows)]
fn set_capture_exclusion(window: &tauri::WebviewWindow, exclude: bool) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::DwmFlush;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
    };

    let Ok(handle) = window.hwnd() else { return };
    // Reconstruct the HWND from the raw pointer so we don't depend on Tauri's
    // bundled `windows` crate being the same version as ours.
    let hwnd = HWND(handle.0 as isize as *mut core::ffi::c_void);
    let affinity = if exclude { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
    unsafe {
        let _ = SetWindowDisplayAffinity(hwnd, affinity);
        // Block until DWM composes a frame reflecting the new affinity, so a
        // capture kicked off right after this call already sees us excluded.
        if exclude {
            let _ = DwmFlush();
        }
    }
}

/// RAII guard that turns capture exclusion off when it drops, so the window is
/// never left permanently hidden from captures even if the surrounding command
/// bails out early via `?`.
#[cfg(windows)]
struct CaptureExclusion<'a> {
    window: &'a tauri::WebviewWindow,
}

#[cfg(windows)]
impl<'a> CaptureExclusion<'a> {
    fn new(window: &'a tauri::WebviewWindow) -> Self {
        set_capture_exclusion(window, true);
        Self { window }
    }
}

#[cfg(windows)]
impl Drop for CaptureExclusion<'_> {
    fn drop(&mut self) {
        set_capture_exclusion(self.window, false);
    }
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
async fn take_snapshot(
    app: tauri::AppHandle,
    name: String,
    browser_bridge: tauri::State<'_, browser_bridge::BrowserBridge>,
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

    // Exclude our own window from the screen capture so it never appears in the
    // thumbnail. This keeps the window visible on screen (no flicker, no focus
    // theft) but omits it from the shot at the compositor level. The guard clears
    // the exclusion when this command returns, including on early error paths.
    let main_window = app.get_webview_window("main");
    #[cfg(windows)]
    let _capture_exclusion = main_window.as_ref().map(CaptureExclusion::new);

    // Run the (slow) screenshot on a separate thread so it overlaps window/process
    // enumeration. Total capture time ≈ max(screenshot, enumeration), not the sum.
    let thumb_path = thumbnail_path_buf.clone();
    let thumb_handle = std::thread::spawn(move || capture_thumbnail(&thumb_path));

    // Browser capture must begin while normal window enumeration and the
    // screenshot run. It has its own short deadline and is never fatal.
    let bridge = browser_bridge.inner().clone();
    let browser_capture = tauri::async_runtime::spawn(async move {
        bridge.capture(std::time::Duration::from_millis(1200)).await
    });

    let cfg = config::load_config(&app);

    // Clipboard capture (opt-in) starts here so it overlaps enumeration instead
    // of adding to it — same reasoning as the screenshot thread above, and what
    // keeps the whole capture inside its time budget. It is internally
    // time-boxed, so a wedged clipboard service costs a warning, not the
    // snapshot. Sidecars for image items land next to the thumbnail.
    let clip_task = if cfg.capture_clipboard {
        let (clip_dir, clip_id) = (dir.clone(), id.clone());
        Some(tauri::async_runtime::spawn_blocking(move || {
            clipboard::capture(&clip_dir, &clip_id)
        }))
    } else {
        None
    };

    // Real capture engine: enumerate windows + processes on this thread.
    let captured = capture::capture_desktop(&cfg.ignore_list);
    let mut warnings: Vec<String> = captured.warnings;

    let browser_reply = browser_capture
        .await
        .map_err(|e| format!("Browser bridge task failed: {e}"))?;
    let has_browser = captured.processes.iter().any(|process| {
        !process.exe_path.is_empty() && classify::classify(&process.exe_path, true).is_browser()
    });
    if has_browser || !browser_reply.sessions.is_empty() {
        warnings.extend(browser_reply.warnings.clone());
    }

    match thumb_handle.join() {
        Ok(Ok(())) => {}
        Ok(Err(e)) => warnings.push(format!("Thumbnail capture failed: {e}")),
        Err(_) => warnings.push("Thumbnail capture thread panicked".to_string()),
    }

    let clipboard_block = match clip_task {
        Some(task) => {
            let (block, clip_warnings) = task
                .await
                .unwrap_or_else(|e| (None, vec![format!("Clipboard capture task failed: {e}")]));
            warnings.extend(clip_warnings);
            block
        }
        None => None,
    };

    let snapshot = Snapshot {
        schema_version: SCHEMA_VERSION,
        id: id.clone(),
        name: resolved_name,
        timestamp,
        processes: captured.processes,
        windows: captured.windows,
        explorer_windows: captured.explorer_windows,
        context_clues: captured.context_clues,
        restore_hints: captured.restore_hints,
        warnings: warnings.clone(),
        thumbnail_path: thumbnail_path_buf.to_string_lossy().into_owned(),
        terminal_sessions: captured.terminal_sessions,
        browser_sessions: browser_reply.sessions,
        clipboard: clipboard_block,
    };

    let json =
        serde_json::to_string_pretty(&snapshot).map_err(|e| format!("Serialise error: {e}"))?;
    std::fs::write(json_path(&dir, &id), json).map_err(|e| format!("Write error: {e}"))?;

    let summary = snapshot_to_summary(&snapshot);
    activity::append(&app, activity::event("capture", Some(snapshot.name.clone()),
        if warnings.is_empty() { "success" } else { "warning" },
        format!("Snapshot captured · {} apps", snapshot.processes.len()), warnings.clone()));
    Ok(CaptureResult {
        snapshot: summary,
        warnings,
    })
}

#[tauri::command]
async fn recapture_snapshot(
    app: tauri::AppHandle,
    id: String,
    browser_bridge: tauri::State<'_, browser_bridge::BrowserBridge>,
) -> Result<CaptureResult, String> {
    let dir = snapshots_dir(&app)?;
    let existing_path = json_path(&dir, &id);

    let old_snapshot = try_load_snapshot(&existing_path)
        .ok_or_else(|| format!("Snapshot {id} not found or unreadable"))?;

    let timestamp = chrono::Utc::now().to_rfc3339();
    let thumbnail_path_buf = png_path(&dir, &id);

    // Exclude our own window from the shot (see `take_snapshot` for details); the
    // guard clears the exclusion when this command returns.
    let main_window = app.get_webview_window("main");
    #[cfg(windows)]
    let _capture_exclusion = main_window.as_ref().map(CaptureExclusion::new);

    // Screenshot on a separate thread, overlapping window enumeration.
    let thumb_tmp = dir.join(format!("{id}_tmp.png"));
    let thumb_tmp2 = thumb_tmp.clone();
    let thumb_handle = std::thread::spawn(move || capture_thumbnail(&thumb_tmp2));

    let bridge = browser_bridge.inner().clone();
    let browser_capture = tauri::async_runtime::spawn(async move {
        bridge.capture(std::time::Duration::from_millis(1200)).await
    });

    let cfg = config::load_config(&app);
    let captured = capture::capture_desktop(&cfg.ignore_list);
    let mut warnings: Vec<String> = captured.warnings;

    let browser_reply = browser_capture
        .await
        .map_err(|e| format!("Browser bridge task failed: {e}"))?;
    let has_browser = captured.processes.iter().any(|process| {
        !process.exe_path.is_empty() && classify::classify(&process.exe_path, true).is_browser()
    });
    if has_browser || !browser_reply.sessions.is_empty() {
        warnings.extend(browser_reply.warnings.clone());
    }

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

    // Clipboard capture (opt-in) — overwrites the prior block for this id.
    let clipboard_block = if cfg.capture_clipboard {
        let (block, clip_warnings) = capture_clipboard_block(&dir, &id).await;
        warnings.extend(clip_warnings);
        block
    } else {
        None
    };

    let snapshot = Snapshot {
        schema_version: SCHEMA_VERSION,
        id: id.clone(),
        name: old_snapshot.name,
        timestamp,
        processes: captured.processes,
        windows: captured.windows,
        explorer_windows: captured.explorer_windows,
        context_clues: captured.context_clues,
        restore_hints: captured.restore_hints,
        warnings: warnings.clone(),
        thumbnail_path: thumbnail_path_buf.to_string_lossy().into_owned(),
        terminal_sessions: captured.terminal_sessions,
        browser_sessions: browser_reply.sessions,
        clipboard: clipboard_block,
    };

    // Write to temp file first, then rename — if capture fails the original is untouched.
    let tmp_json = dir.join(format!("{id}_tmp.json"));
    let json =
        serde_json::to_string_pretty(&snapshot).map_err(|e| format!("Serialise error: {e}"))?;
    std::fs::write(&tmp_json, json).map_err(|e| format!("Write error: {e}"))?;
    std::fs::rename(&tmp_json, &existing_path).map_err(|e| format!("Rename error: {e}"))?;

    // Move temp thumbnail over the original only when capture fully succeeded —
    // a partially-written PNG must never replace a good thumbnail. On failure,
    // clean up the stray temp file instead of leaking it.
    if thumb_ok && thumb_tmp.exists() {
        let _ = std::fs::rename(&thumb_tmp, &thumbnail_path_buf);
    } else if thumb_tmp.exists() {
        let _ = std::fs::remove_file(&thumb_tmp);
    }

    let summary = snapshot_to_summary(&snapshot);
    activity::append(&app, activity::event("recapture", Some(snapshot.name.clone()),
        if warnings.is_empty() { "success" } else { "warning" },
        format!("Snapshot updated · {} apps", snapshot.processes.len()), warnings.clone()));
    Ok(CaptureResult {
        snapshot: summary,
        warnings,
    })
}

#[tauri::command]
async fn list_snapshots(app: tauri::AppHandle) -> Result<Vec<SnapshotSummary>, String> {
    let dir = snapshots_dir(&app)?;

    let entries = std::fs::read_dir(&dir).map_err(|e| format!("Read dir error: {e}"))?;

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
async fn get_snapshot(app: tauri::AppHandle, id: String) -> Result<Snapshot, String> {
    let dir = snapshots_dir(&app)?;
    try_load_snapshot(&json_path(&dir, &id)).ok_or_else(|| format!("Snapshot {id} not found or unreadable"))
}

#[tauri::command]
async fn close_all_windows(app: tauri::AppHandle) -> Result<CloseResult, String> {
    let ignored = config::load_config(&app).ignore_list;
    let (closed, refused) = tauri::async_runtime::spawn_blocking(move || restore::close_all_windows(&ignored))
        .await.map_err(|e| format!("Close task failed: {e}"))?;
    let status = if refused.is_empty() { "success" } else { "warning" };
    activity::append(&app, activity::event("start_new", None, status,
        format!("Started fresh · {} windows closed", closed.len()), refused.clone()));
    active_session::clear(&app);
    Ok(CloseResult { closed, refused })
}

/// Re-run companion setup and report the result together with who is connected.
/// Safe to call at any time: registration is idempotent and per-user.
#[tauri::command]
fn refresh_companion(
    browser_bridge: tauri::State<'_, browser_bridge::BrowserBridge>,
) -> companion::CompanionReport {
    companion::register().into_report(browser_bridge.connected_families())
}

#[tauri::command]
fn companion_status(
    status: tauri::State<'_, companion::CompanionStatus>,
    browser_bridge: tauri::State<'_, browser_bridge::BrowserBridge>,
) -> companion::CompanionReport {
    status
        .inner()
        .clone()
        .into_report(browser_bridge.connected_families())
}

#[tauri::command]
async fn restore_snapshot(
    app: tauri::AppHandle,
    id: String,
    close_others: Option<bool>,
    browser_bridge: tauri::State<'_, browser_bridge::BrowserBridge>,
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
    let capture_clipboard = cfg.capture_clipboard;
    let ignore_list = cfg.ignore_list;
    let sessions = snapshot.browser_sessions.clone();
    let snapshot_name = snapshot.name.clone();
    let clipboard_block = snapshot.clipboard.clone();
    let has_browser_sessions = !sessions.is_empty();

    let mut result = tauri::async_runtime::spawn_blocking(move || {
        restore::restore_desktop(&snapshot, close_others, &ignore_list, has_browser_sessions)
    })
    .await
    .map_err(|e| format!("Restore task failed: {e}"))?;

    if has_browser_sessions {
        let reply = browser_bridge
            .inner()
            .clone()
            .restore(&sessions, close_others)
            .await;
        result.closed_items.extend(reply.closed_items);
        result.warnings.extend(reply.warnings);
    }

    // Clipboard reseed — only when opted in and this snapshot carries a clipboard
    // block (so restoring an older, clipboard-less snapshot never clears the live
    // Win+V). Safety invariant: back up + verify the current clipboard first, and
    // ClearHistory only runs when that backup is confirmed.
    if capture_clipboard {
        if let Some(block) = clipboard_block {
            if !block.is_empty() {
                let backup_ok = match backup_current_clipboard_async(&app, &snapshot_name).await {
                    Ok(ok) => ok,
                    Err(e) => {
                        result
                            .warnings
                            .push(format!("Clipboard pre-restore backup failed: {e}"));
                        false
                    }
                };
                let clip_dir = dir.clone();
                let reseed_warnings = tauri::async_runtime::spawn_blocking(move || {
                    clipboard::reseed_history(&clip_dir, &block, backup_ok)
                })
                .await
                .unwrap_or_else(|e| vec![format!("Clipboard reseed task failed: {e}")]);
                result.warnings.extend(reseed_warnings);
            }
        }
    }

    let mut details = result.failed_items.clone();
    details.extend(result.warnings.clone());
    activity::append(&app, activity::event("restore", Some(snapshot_name),
        if !result.failed_items.is_empty() { "failed" } else if !result.warnings.is_empty() { "warning" } else { "success" },
        result.message.clone(), details));
    active_session::set(&app, &id);
    Ok(result)
}

/// Build the smallest snapshot that can safely travel through the normal restore
/// engine for one application. The executable must come from the persisted
/// snapshot; callers cannot use this command as an arbitrary process launcher.
fn snapshot_for_app(snapshot: &Snapshot, exe_path: &str) -> Result<(Snapshot, String), String> {
    if exe_path.trim().is_empty() {
        return Err("This app has no executable path recorded and cannot be restored".to_string());
    }

    let selected = snapshot
        .processes
        .iter()
        .find(|process| process.exe_path.eq_ignore_ascii_case(exe_path))
        .ok_or_else(|| "The selected app is not part of this snapshot".to_string())?;
    let app_name = std::path::Path::new(&selected.name)
        .file_stem()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| selected.name.clone());
    let stem = restore::exe_stem_pub(&selected.exe_path);
    let is_vscode = matches!(stem.as_str(), "code" | "code-insiders" | "cursor");

    let mut app_snapshot = snapshot.clone();
    app_snapshot
        .processes
        .retain(|process| process.exe_path.eq_ignore_ascii_case(exe_path));
    app_snapshot
        .windows
        .retain(|window| window.exe_path.eq_ignore_ascii_case(exe_path));
    // Explorer is not represented by a selectable ProcessInfo. Never let a
    // selective app restore accidentally bring back unrelated folder windows.
    app_snapshot.explorer_windows.clear();
    app_snapshot
        .terminal_sessions
        .retain(|session| terminal::session_matches_executable(session, exe_path));
    app_snapshot.browser_sessions.retain(|session| {
        let candidates: std::collections::HashSet<String> = snapshot
            .processes
            .iter()
            .filter(|process| !process.exe_path.is_empty())
            .map(|process| restore::exe_stem_pub(&process.exe_path))
            .filter(|candidate| browser_family_matches_exe(&session.browser.family, candidate))
            .collect();
        candidates.len() == 1 && candidates.contains(&stem)
    });

    let browser_prefix = format!("browser_tab:{stem}:");
    let office_prefix = format!("office_extra_file:{stem}:");
    app_snapshot.restore_hints.retain(|hint| {
        hint.starts_with(&browser_prefix)
            || hint.starts_with(&office_prefix)
            || (is_vscode
                && (hint.starts_with("vscode_folder:") || hint.starts_with("vscode_workspace:")))
            || hint
                .strip_prefix("foreground:")
                .is_some_and(|value| restore::exe_stem_pub(value) == stem)
    });
    // Context clues are descriptive capture metadata and are not read by the
    // restore engine. Excluding unrelated clues keeps this transient slice honest.
    app_snapshot.context_clues.clear();

    Ok((app_snapshot, app_name))
}

/// Build a selective-restore slice containing only captured File Explorer
/// folder windows. Explorer never enters the ordinary process launch path.
fn snapshot_for_explorer(snapshot: &Snapshot) -> Result<Snapshot, String> {
    if snapshot.explorer_windows.is_empty() {
        return Err("This snapshot has no File Explorer windows to restore".to_string());
    }

    let mut explorer_snapshot = snapshot.clone();
    explorer_snapshot.processes.clear();
    explorer_snapshot.windows.clear();
    explorer_snapshot.context_clues.clear();
    explorer_snapshot.restore_hints.clear();
    explorer_snapshot.terminal_sessions.clear();
    explorer_snapshot.browser_sessions.clear();
    Ok(explorer_snapshot)
}

fn browser_family_matches_exe(family: &str, stem: &str) -> bool {
    let family = family.to_ascii_lowercase();
    match stem {
        "chrome" | "chromium" => matches!(family.as_str(), "chrome" | "chromium"),
        "msedge" => matches!(family.as_str(), "edge" | "msedge" | "microsoft edge"),
        "firefox" => family == "firefox",
        "brave" => family == "brave",
        "opera" | "opera_gx" => matches!(family.as_str(), "opera" | "opera gx"),
        "vivaldi" => family == "vivaldi",
        "arc" => family == "arc",
        _ => false,
    }
}

#[tauri::command]
async fn restore_app(
    app: tauri::AppHandle,
    id: String,
    exe_path: String,
    browser_bridge: tauri::State<'_, browser_bridge::BrowserBridge>,
) -> Result<RestoreResult, String> {
    let dir = snapshots_dir(&app)?;
    let path = json_path(&dir, &id);
    let snapshot = try_load_snapshot(&path)
        .ok_or_else(|| format!("Snapshot {id} is missing, corrupt, or unreadable"))?;
    let snapshot_name = snapshot.name.clone();
    let (app_snapshot, app_name) = snapshot_for_app(&snapshot, &exe_path)?;
    let sessions = app_snapshot.browser_sessions.clone();
    let has_browser_sessions = !sessions.is_empty();
    let ignore_list = config::load_config(&app).ignore_list;

    // The desktop portion of a selective restore stays additive: it never
    // closes unrelated apps or windows. Browser tabs are reconciled separately
    // below because "restore this browser" means restoring its captured state,
    // including removing tabs that are not in that state.
    let mut result = tauri::async_runtime::spawn_blocking(move || {
        restore::restore_desktop(&app_snapshot, false, &ignore_list, has_browser_sessions)
    })
    .await
    .map_err(|e| format!("Restore task failed: {e}"))?;

    if has_browser_sessions {
        let reply = browser_bridge
            .inner()
            .clone()
            .restore(&sessions, true)
            .await;
        result.closed_items.extend(reply.closed_items);
        result.warnings.extend(reply.warnings);
    }
    restore::focus_app(&exe_path);

    result.message = if !result.failed_items.is_empty() {
        format!("{app_name} could not be fully restored")
    } else if !result.warnings.is_empty() {
        format!(
            "{app_name} restored with {} warning(s)",
            result.warnings.len()
        )
    } else {
        format!("{app_name} restored")
    };

    let mut details = result.failed_items.clone();
    details.extend(result.warnings.clone());
    activity::append(
        &app,
        activity::event(
            "restore_app",
            Some(snapshot_name),
            if !result.failed_items.is_empty() {
                "failed"
            } else if !result.warnings.is_empty() {
                "warning"
            } else {
                "success"
            },
            result.message.clone(),
            details,
        ),
    );
    // A partial restore deliberately does not update active_session: the live
    // desktop is not equivalent to the complete snapshot.
    Ok(result)
}

#[tauri::command]
async fn restore_explorer_windows(
    app: tauri::AppHandle,
    id: String,
) -> Result<RestoreResult, String> {
    let dir = snapshots_dir(&app)?;
    let path = json_path(&dir, &id);
    let snapshot = try_load_snapshot(&path)
        .ok_or_else(|| format!("Snapshot {id} is missing, corrupt, or unreadable"))?;
    let snapshot_name = snapshot.name.clone();
    let explorer_snapshot = snapshot_for_explorer(&snapshot)?;
    let ignore_list = config::load_config(&app).ignore_list;

    // Selective Explorer restore is additive: exact saved folders are reopened
    // or repositioned, while unrelated currently-open folders remain untouched.
    let mut result = tauri::async_runtime::spawn_blocking(move || {
        restore::restore_desktop(&explorer_snapshot, false, &ignore_list, false)
    })
    .await
    .map_err(|e| format!("File Explorer restore task failed: {e}"))?;

    result.message = if !result.failed_items.is_empty() {
        "File Explorer could not be fully restored".to_string()
    } else if !result.warnings.is_empty() {
        format!(
            "File Explorer restored with {} warning(s)",
            result.warnings.len()
        )
    } else {
        "File Explorer restored".to_string()
    };

    let mut details = result.failed_items.clone();
    details.extend(result.warnings.clone());
    activity::append(
        &app,
        activity::event(
            "restore_app",
            Some(snapshot_name),
            if !result.failed_items.is_empty() {
                "failed"
            } else if !result.warnings.is_empty() {
                "warning"
            } else {
                "success"
            },
            result.message.clone(),
            details,
        ),
    );
    Ok(result)
}

/// How far the live desktop is from `snap_windows`, as a symmetric per-app window-count
/// difference: `sum over apps of |live_windows − snapshot_windows|`. Zero means the
/// snapshot holds exactly the apps/windows open now — no more, no fewer. Symmetric on
/// purpose: opening an app *and* closing one both count as drift, because the question
/// is "does my current desktop match a saved snapshot," not "would a restore lose
/// something." Pure core of `is_current_state_saved`.
fn window_multiset_diff(
    live: &std::collections::HashMap<String, usize>,
    snap_windows: &[WindowInfo],
) -> usize {
    let mut snap_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for w in snap_windows {
        if w.exe_path.is_empty() {
            continue;
        }
        *snap_counts
            .entry(restore::exe_stem_pub(&w.exe_path))
            .or_insert(0) += 1;
    }
    // Live surplus (apps/windows open that the snapshot lacks) …
    let live_surplus: usize = live
        .iter()
        .map(|(stem, &n)| n.saturating_sub(snap_counts.get(stem).copied().unwrap_or(0)))
        .sum();
    // … plus snapshot surplus (apps/windows the snapshot has that are now closed).
    let snap_surplus: usize = snap_counts
        .iter()
        .map(|(stem, &n)| n.saturating_sub(live.get(stem).copied().unwrap_or(0)))
        .sum();
    live_surplus + snap_surplus
}

/// Is the desktop the user is looking at right now already captured in some saved
/// snapshot? Drives the restore-confirm warning. The question is symmetric: does any
/// snapshot hold *exactly* the apps/windows open now — no extras, none missing? We take
/// a live per-app window multiset and, for each snapshot, compute the symmetric count
/// difference (`window_multiset_diff`). "Saved" iff some snapshot matches exactly
/// (diff == 0).
///
/// This trips when you open another app, open a second window of an app already open,
/// *or close an app that was in the snapshot*. Titles and geometry are excluded, so it
/// never flips from ordinary title churn or a nudged window — but that also means the
/// known blind spot is browser tabs: adding or switching a tab changes no window count,
/// so an instant check can't see it (that needs the slow capture path). Conservative:
/// an enumeration failure reads as "not matched" (warns).
#[tauri::command]
async fn is_current_state_saved(app: tauri::AppHandle) -> Result<bool, String> {
    let dir = snapshots_dir(&app)?;
    let ignore_list = config::load_config(&app).ignore_list;

    // Win32 window enumeration + per-snapshot disk reads are synchronous;
    // offload so we never block the async runtime (same pattern as restore).
    tauri::async_runtime::spawn_blocking(move || {
        // Same enumeration + ignore filter capture uses, so live windows are directly
        // comparable to each snapshot's stored `windows`.
        let live = capture::current_window_counts(&ignore_list);
        if live.is_empty() {
            // Nothing meaningful open — nothing a restore could lose, treat as "saved".
            return Ok(true);
        }

        let entries = std::fs::read_dir(&dir).map_err(|e| format!("Read dir error: {e}"))?;
        let mut best_diff = usize::MAX;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Some(snap) = try_load_snapshot(&path) {
                best_diff = best_diff.min(window_multiset_diff(&live, &snap.windows));
                if best_diff == 0 {
                    break; // exact match — no snapshot can do better
                }
            }
        }

        // Saved only when some snapshot's apps/windows match the live desktop exactly.
        Ok(best_diff == 0)
    })
    .await
    .map_err(|e| format!("State check failed: {e}"))?
}

#[tauri::command]
async fn rename_snapshot(
    app: tauri::AppHandle,
    id: String,
    name: String,
) -> Result<SnapshotSummary, String> {
    let dir = snapshots_dir(&app)?;
    let path = json_path(&dir, &id);
    let snapshot = try_load_snapshot(&path)
        .ok_or_else(|| format!("Snapshot {id} not found or unreadable"))?;
    if snapshot.id != id {
        return Err("Snapshot id does not match its stored file".to_string());
    }

    let old_name = snapshot.name.clone();
    let renamed = with_snapshot_name(snapshot, &name)?;
    let json = serde_json::to_string_pretty(&renamed)
        .map_err(|e| format!("Serialise error: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("Write error: {e}"))?;

    activity::append(
        &app,
        activity::event(
            "rename",
            Some(renamed.name.clone()),
            "success",
            "Snapshot renamed".to_string(),
            vec![format!("{old_name} → {}", renamed.name)],
        ),
    );
    Ok(snapshot_to_summary(&renamed))
}

#[tauri::command]
async fn delete_snapshot(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let dir = snapshots_dir(&app)?;
    let deleted_name = try_load_snapshot(&json_path(&dir, &id)).map(|s| s.name);

    let json = json_path(&dir, &id);
    if json.exists() {
        std::fs::remove_file(&json).map_err(|e| format!("Delete JSON error: {e}"))?;
    }

    let png = png_path(&dir, &id);
    if png.exists() {
        std::fs::remove_file(&png).map_err(|e| format!("Delete PNG error: {e}"))?;
    }

    activity::append(&app, activity::event("delete", deleted_name, "success", "Snapshot deleted".into(), vec![]));
    if active_session::current_id(&app).as_deref() == Some(id.as_str()) {
        active_session::clear(&app);
    }
    Ok(())
}

#[tauri::command]
async fn duplicate_snapshot(app: tauri::AppHandle, id: String) -> Result<SnapshotSummary, String> {
    let dir = snapshots_dir(&app)?;
    let mut snapshot = try_load_snapshot(&json_path(&dir, &id))
        .ok_or_else(|| format!("Snapshot {id} not found or unreadable"))?;

    let new_id = format!("snap_{}", chrono::Utc::now().timestamp_millis());
    let new_png = png_path(&dir, &new_id);

    // Copy the thumbnail so the duplicate has its own image; a missing source PNG
    // (older/partial snapshot) is not fatal — the tile falls back to the placeholder.
    let src_png = png_path(&dir, &id);
    if src_png.exists() {
        std::fs::copy(&src_png, &new_png).map_err(|e| format!("Copy thumbnail error: {e}"))?;
    }

    snapshot.id = new_id.clone();
    snapshot.timestamp = chrono::Utc::now().to_rfc3339();
    snapshot.name = format!("{} (copy)", snapshot.name);
    snapshot.thumbnail_path = new_png.to_string_lossy().to_string();

    let json = serde_json::to_string_pretty(&snapshot).map_err(|e| format!("Serialise error: {e}"))?;
    std::fs::write(json_path(&dir, &new_id), json).map_err(|e| format!("Write error: {e}"))?;

    activity::append(&app, activity::event(
        "duplicate", Some(snapshot.name.clone()), "success", "Snapshot duplicated".into(), vec![],
    ));
    Ok(snapshot_to_summary(&snapshot))
}

#[tauri::command]
async fn clear_all_snapshots(app: tauri::AppHandle) -> Result<(), String> {
    let dir = snapshots_dir(&app)?;

    let entries = std::fs::read_dir(&dir).map_err(|e| format!("Read dir error: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if ext == "json" || ext == "png" {
                let _ = std::fs::remove_file(&path); // best-effort
            }
        }
    }

    active_session::clear(&app);
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
        return Err(format!(
            "{stem} is a system-critical process and is always protected"
        ));
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
            if exe.is_empty() {
                return None;
            }
            let stem = config::normalize_exe_name(&exe);
            if stem.is_empty() {
                return None;
            }
            if config::is_ignored(&stem, &cfg.ignore_list) {
                return None;
            }
            Some(stem)
        })
        .collect();
    stems.sort();
    stems.dedup();
    Ok(stems)
}

/// Whether the PowerShell profile hook (mirrors $PWD into the window title so we
/// can capture a terminal's live directory) is installed.
#[tauri::command]
async fn terminal_hook_status() -> Result<bool, String> {
    Ok(terminal_hook::is_installed())
}

/// Install or remove the PowerShell directory-capture hook.
#[tauri::command]
async fn set_terminal_hook(enabled: bool) -> Result<String, String> {
    if enabled {
        terminal_hook::install()
    } else {
        terminal_hook::uninstall()
    }
}

/// The captured app's own icon as a PNG `data:` URI, or `None` if it can't be
/// read (the details pane falls back to a monogram). Extracted lazily per row.
#[tauri::command]
fn get_app_icon(exe_path: String) -> Option<String> {
    icons::extract_icon_data_uri(&exe_path)
}

// ── Clipboard opt-in + cache ─────────────────────────────────────────────────

/// Auto-backups kept (rolling) — pre-restore snapshots of the live clipboard.
const MAX_AUTO_BACKUPS: usize = 5;

#[derive(Serialize, Deserialize, Clone)]
struct ClipboardCacheEntry {
    id: String,
    label: String,
    created_at: String,
    block: clipboard::ClipboardBlock,
}

#[derive(Serialize, Deserialize, Default)]
struct ClipboardCacheStore {
    #[serde(default)]
    entries: Vec<ClipboardCacheEntry>,
}

/// One flattened row for the settings "Clipboard Cache" panel.
#[derive(Serialize, Clone)]
struct ClipboardCacheRow {
    row_id: String,
    source: String, // "snapshot" | "backup"
    container_id: String,
    label: String,
    created_at: String,
    kind: clipboard::ClipboardKind,
    order: u32,
    text: Option<String>,
    /// Absolute path to the image sidecar (frontend runs it through convertFileSrc).
    sidecar_path: Option<String>,
    item_id: String,
}

fn clipboard_cache_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Cannot resolve app data dir: {e}"))?;
    let dir = base.join("ClipboardCache");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create clipboard cache dir: {e}"))?;
    Ok(dir)
}

fn clipboard_cache_json(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(clipboard_cache_dir(app)?.join("cache.json"))
}

fn load_clipboard_cache(app: &tauri::AppHandle) -> ClipboardCacheStore {
    let path = match clipboard_cache_json(app) {
        Ok(p) => p,
        Err(_) => return ClipboardCacheStore::default(),
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => ClipboardCacheStore::default(),
    }
}

fn save_clipboard_cache(app: &tauri::AppHandle, store: &ClipboardCacheStore) -> Result<(), String> {
    let path = clipboard_cache_json(app)?;
    let tmp = path.with_extension("json.tmp");
    let json =
        serde_json::to_string_pretty(store).map_err(|e| format!("Serialize cache error: {e}"))?;
    std::fs::write(&tmp, json).map_err(|e| format!("Write cache error: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("Rename cache error: {e}"))
}

/// Run the (blocking, internally time-boxed) clipboard capture off the async
/// runtime. Failures come back as warnings — a snapshot is never lost to the
/// clipboard.
async fn capture_clipboard_block(
    dir: &std::path::Path,
    id: &str,
) -> (Option<clipboard::ClipboardBlock>, Vec<String>) {
    let dir = dir.to_path_buf();
    let id = id.to_string();
    tauri::async_runtime::spawn_blocking(move || clipboard::capture(&dir, &id))
        .await
        .unwrap_or_else(|e| (None, vec![format!("Clipboard capture task failed: {e}")]))
}

/// Same, for the pre-restore backup.
async fn backup_current_clipboard_async(
    app: &tauri::AppHandle,
    snapshot_name: &str,
) -> Result<bool, String> {
    let app = app.clone();
    let name = snapshot_name.to_string();
    tauri::async_runtime::spawn_blocking(move || backup_current_clipboard(&app, &name))
        .await
        .unwrap_or_else(|e| Err(format!("backup task failed: {e}")))
}

/// Capture the CURRENT clipboard, persist it atomically to the cache, and verify
/// it read back. Returns Ok(true) only when a backup is confirmed on disk — or
/// when there was nothing to back up (clearing is then harmless). Any Ok(false)
/// or Err means the caller MUST NOT clear the live Win+V history.
fn backup_current_clipboard(app: &tauri::AppHandle, snapshot_name: &str) -> Result<bool, String> {
    let dir = clipboard_cache_dir(app)?;
    let backup_id = format!("backup_{}", chrono::Utc::now().timestamp_millis());
    let (block, _warnings) = clipboard::capture(&dir, &backup_id);
    let block = match block {
        Some(b) if !b.is_empty() => b,
        _ => return Ok(true), // nothing on the clipboard to preserve
    };
    let expected = block.items.len();
    let entry = ClipboardCacheEntry {
        id: backup_id.clone(),
        label: format!("Before restoring {snapshot_name}"),
        created_at: chrono::Utc::now().to_rfc3339(),
        block,
    };
    let mut store = load_clipboard_cache(app);
    store.entries.push(entry);
    while store.entries.len() > MAX_AUTO_BACKUPS {
        let removed = store.entries.remove(0);
        for item in &removed.block.items {
            if let Some(name) = &item.sidecar {
                let _ = std::fs::remove_file(dir.join(name));
            }
        }
    }
    save_clipboard_cache(app, &store)?;
    // Verify the write landed before we allow a destructive clear.
    let verify = load_clipboard_cache(app);
    let ok = verify
        .entries
        .iter()
        .any(|e| e.id == backup_id && e.block.items.len() == expected);
    if !ok {
        return Err("backup verification failed".into());
    }
    Ok(true)
}

fn clipboard_row(
    source: &str,
    container_id: &str,
    label: &str,
    created_at: &str,
    dir: &std::path::Path,
    item: &clipboard::ClipboardItem,
) -> ClipboardCacheRow {
    let sidecar_path = item
        .sidecar
        .as_ref()
        .map(|f| dir.join(f).to_string_lossy().into_owned());
    let text = item.text.as_ref().map(|t| {
        let mut s: String = t.chars().take(280).collect();
        if t.chars().count() > 280 {
            s.push('…');
        }
        s
    });
    ClipboardCacheRow {
        row_id: format!("{source}:{container_id}:{}", item.id),
        source: source.to_string(),
        container_id: container_id.to_string(),
        label: label.to_string(),
        created_at: created_at.to_string(),
        kind: item.kind.clone(),
        order: item.order,
        text,
        sidecar_path,
        item_id: item.id.clone(),
    }
}

fn load_clipboard_container(
    app: &tauri::AppHandle,
    source: &str,
    container_id: &str,
) -> Result<(std::path::PathBuf, clipboard::ClipboardBlock), String> {
    match source {
        "snapshot" => {
            let dir = snapshots_dir(app)?;
            let snap = try_load_snapshot(&json_path(&dir, container_id))
                .ok_or_else(|| "snapshot not found".to_string())?;
            let block = snap
                .clipboard
                .ok_or_else(|| "snapshot has no clipboard".to_string())?;
            Ok((dir, block))
        }
        "backup" => {
            let dir = clipboard_cache_dir(app)?;
            let store = load_clipboard_cache(app);
            let entry = store
                .entries
                .into_iter()
                .find(|e| e.id == container_id)
                .ok_or_else(|| "backup not found".to_string())?;
            Ok((dir, entry.block))
        }
        _ => Err("unknown clipboard source".to_string()),
    }
}

#[tauri::command]
async fn get_capture_clipboard(app: tauri::AppHandle) -> Result<bool, String> {
    Ok(config::load_config(&app).capture_clipboard)
}

#[tauri::command]
async fn set_capture_clipboard(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut cfg = config::load_config(&app);
    cfg.capture_clipboard = enabled;
    config::save_config(&app, &cfg)
}

#[tauri::command]
async fn list_clipboard_cache(app: tauri::AppHandle) -> Result<Vec<ClipboardCacheRow>, String> {
    if !config::load_config(&app).capture_clipboard {
        return Ok(Vec::new());
    }
    let mut rows: Vec<ClipboardCacheRow> = Vec::new();

    let sdir = snapshots_dir(&app)?;
    if let Ok(entries) = std::fs::read_dir(&sdir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let snap = match try_load_snapshot(&path) {
                Some(s) => s,
                None => continue,
            };
            if let Some(block) = &snap.clipboard {
                for item in &block.items {
                    rows.push(clipboard_row(
                        "snapshot",
                        &snap.id,
                        &snap.name,
                        &block.captured_at,
                        &sdir,
                        item,
                    ));
                }
            }
        }
    }

    let cdir = clipboard_cache_dir(&app)?;
    let store = load_clipboard_cache(&app);
    for e in &store.entries {
        for item in &e.block.items {
            rows.push(clipboard_row("backup", &e.id, &e.label, &e.created_at, &cdir, item));
        }
    }

    rows.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then(b.order.cmp(&a.order))
    });
    Ok(rows)
}

#[tauri::command]
async fn copy_clipboard_item(
    app: tauri::AppHandle,
    source: String,
    container_id: String,
    item_id: String,
) -> Result<(), String> {
    let (dir, block) = load_clipboard_container(&app, &source, &container_id)?;
    let item = block
        .items
        .iter()
        .find(|i| i.id == item_id)
        .ok_or_else(|| "clipboard item not found".to_string())?
        .clone();
    tauri::async_runtime::spawn_blocking(move || clipboard::copy_item(&dir, &item))
        .await
        .map_err(|e| format!("copy task failed: {e}"))?
}

/// Reseed the live Win+V history from one snapshot's stored clipboard block —
/// the clipboard-only counterpart of a full restore, driven from the details
/// panel. Same safety invariant: back up and verify the current clipboard first,
/// and never clear when that backup could not be confirmed. Returns the
/// warnings so the caller can surface a partial result honestly.
#[tauri::command]
async fn restore_clipboard(app: tauri::AppHandle, id: String) -> Result<Vec<String>, String> {
    let dir = snapshots_dir(&app)?;
    let snapshot = try_load_snapshot(&json_path(&dir, &id))
        .ok_or_else(|| format!("Snapshot {id} not found"))?;
    let block = snapshot
        .clipboard
        .clone()
        .filter(|b| !b.is_empty())
        .ok_or_else(|| "This snapshot has no captured clipboard".to_string())?;

    let mut warnings = match backup_current_clipboard_async(&app, &snapshot.name).await {
        Ok(true) => Vec::new(),
        Ok(false) => vec!["Clipboard pre-restore backup could not be verified".to_string()],
        Err(e) => vec![format!("Clipboard pre-restore backup failed: {e}")],
    };
    let backup_ok = warnings.is_empty();
    let count = block.items.len();
    let clip_dir = dir.clone();
    warnings.extend(
        tauri::async_runtime::spawn_blocking(move || {
            clipboard::reseed_history(&clip_dir, &block, backup_ok)
        })
        .await
        .unwrap_or_else(|e| vec![format!("Clipboard reseed task failed: {e}")]),
    );

    activity::append(
        &app,
        activity::event(
            "restore",
            Some(snapshot.name.clone()),
            if warnings.is_empty() { "success" } else { "warning" },
            format!("Clipboard restored ({count} item{})", if count == 1 { "" } else { "s" }),
            warnings.clone(),
        ),
    );
    Ok(warnings)
}

#[tauri::command]
async fn delete_clipboard_entry(
    app: tauri::AppHandle,
    source: String,
    container_id: String,
    item_id: String,
) -> Result<(), String> {
    match source.as_str() {
        "snapshot" => {
            let dir = snapshots_dir(&app)?;
            let path = json_path(&dir, &container_id);
            let mut snap =
                try_load_snapshot(&path).ok_or_else(|| "snapshot not found".to_string())?;
            if let Some(block) = snap.clipboard.as_mut() {
                if let Some(pos) = block.items.iter().position(|i| i.id == item_id) {
                    let removed = block.items.remove(pos);
                    if let Some(name) = removed.sidecar {
                        let _ = std::fs::remove_file(dir.join(name));
                    }
                }
                if block.items.is_empty() {
                    snap.clipboard = None;
                }
            }
            let json = serde_json::to_string_pretty(&snap)
                .map_err(|e| format!("Serialize error: {e}"))?;
            let tmp = dir.join(format!("{container_id}_tmp.json"));
            std::fs::write(&tmp, json).map_err(|e| format!("Write error: {e}"))?;
            std::fs::rename(&tmp, &path).map_err(|e| format!("Rename error: {e}"))?;
            Ok(())
        }
        "backup" => {
            let dir = clipboard_cache_dir(&app)?;
            let mut store = load_clipboard_cache(&app);
            for e in store.entries.iter_mut() {
                if e.id == container_id {
                    if let Some(pos) = e.block.items.iter().position(|i| i.id == item_id) {
                        let removed = e.block.items.remove(pos);
                        if let Some(name) = removed.sidecar {
                            let _ = std::fs::remove_file(dir.join(name));
                        }
                    }
                }
            }
            store.entries.retain(|e| !e.block.items.is_empty());
            save_clipboard_cache(&app, &store)
        }
        _ => Err("unknown clipboard source".to_string()),
    }
}

// ── App entry point ───────────────────────────────────────────────────────────

/// Use Windows' compositor-driven minimize transition, then remove the minimized
/// window from the taskbar once the animation has had time to finish.
#[cfg(target_os = "windows")]
fn minimize_main_window_to_tray<R: tauri::Runtime>(window: &tauri::Window<R>) {
    if window.minimize().is_err() {
        let _ = window.hide();
        return;
    }

    let minimized_window = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        // A fast tray click may already have restored the window. In that case,
        // do not let this delayed cleanup hide it again underneath the user.
        if minimized_window.is_minimized().unwrap_or(false) {
            let _ = minimized_window.hide();
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Re-register the native-messaging host on every launch. It is cheap, it is
    // idempotent, and it is what keeps the browser pointed at *this* build of the
    // relay instead of a path left behind by an older install or a dev build.
    let companion_setup = companion::register();
    let browser_bridge = browser_bridge::BrowserBridge::start();
    tauri::Builder::default()
        .manage(browser_bridge)
        .manage(companion_setup)
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                match window.label() {
                    "main" => {
                        api.prevent_close();
                        #[cfg(target_os = "windows")]
                        minimize_main_window_to_tray(window);
                        #[cfg(not(target_os = "windows"))]
                        let _ = window.hide();
                    }
                    "overlay" => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    _ => {}
                }
            }
        })
        .setup(|app| {
            #[cfg(target_os = "windows")]
            if let Some(overlay) = app.get_webview_window("overlay") {
                use tauri::window::{Color, Effect, EffectsBuilder};
                let _ = overlay.set_theme(Some(tauri::Theme::Dark));
                let _ = overlay.set_effects(
                    EffectsBuilder::new()
                        .effect(Effect::Acrylic)
                        .color(Color(18, 20, 24, 220))
                        .build(),
                );

                // Round the HWND itself so Acrylic cannot paint a square slab
                // outside the rounded report card.
                if let Ok(tauri_hwnd) = overlay.hwnd() {
                    use windows::Win32::{
                        Foundation::HWND,
                        Graphics::Dwm::{
                            DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE,
                            DWMWCP_ROUND,
                        },
                    };
                    let hwnd = HWND(tauri_hwnd.0 as *mut std::ffi::c_void);
                    let preference = DWMWCP_ROUND;
                    unsafe {
                        let _ = DwmSetWindowAttribute(
                            hwnd,
                            DWMWA_WINDOW_CORNER_PREFERENCE,
                            &preference as *const _ as *const std::ffi::c_void,
                            std::mem::size_of_val(&preference) as u32,
                        );
                    }
                }
            }

            let show_item =
                MenuItemBuilder::with_id("show", "Show PC Snapshot").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show_item, &quit_item])
                .build()?;

            let mut tray = TrayIconBuilder::with_id("main-tray")
                .menu(&menu)
                .tooltip("PC Snapshot")
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                });

            if let Some(icon) = app.default_window_icon().cloned() {
                tray = tray.icon(icon);
            }
            tray.build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            take_snapshot,
            recapture_snapshot,
            list_snapshots,
            get_snapshot,
            close_all_windows,
            activity::list_activity,
            activity::read_activity_log,
            restore_snapshot,
            restore_app,
            restore_explorer_windows,
            rename_snapshot,
            delete_snapshot,
            duplicate_snapshot,
            clear_all_snapshots,
            is_current_state_saved,
            get_ignore_list,
            add_to_ignore_list,
            remove_from_ignore_list,
            get_running_processes,
            terminal_hook_status,
            set_terminal_hook,
            get_app_icon,
            active_session::get_active_session,
            get_capture_clipboard,
            set_capture_clipboard,
            list_clipboard_cache,
            copy_clipboard_item,
            restore_clipboard,
            delete_clipboard_entry,
            companion_status,
            refresh_companion,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod snapshot_schema_tests {
    use super::{snapshot_for_app, snapshot_for_explorer, with_snapshot_name, Snapshot};

    #[test]
    fn rename_changes_only_the_trimmed_display_name() {
        let snapshot: Snapshot = serde_json::from_str(
            r#"{
            "schema_version": 2,
            "id": "snap_rename",
            "name": "Old name",
            "timestamp": "2026-01-01T00:00:00Z",
            "processes": [],
            "windows": [],
            "context_clues": [],
            "restore_hints": [],
            "warnings": ["kept"],
            "thumbnail_path": "C:/snapshot.png"
        }"#,
        )
        .unwrap();

        let renamed = with_snapshot_name(snapshot, "  New name  ").unwrap();
        assert_eq!(renamed.name, "New name");
        assert_eq!(renamed.id, "snap_rename");
        assert_eq!(renamed.timestamp, "2026-01-01T00:00:00Z");
        assert_eq!(renamed.warnings, vec!["kept"]);
    }

    #[test]
    fn rename_rejects_a_blank_name() {
        let snapshot: Snapshot = serde_json::from_str(
            r#"{
            "schema_version": 2,
            "id": "snap_rename",
            "name": "Old name",
            "timestamp": "2026-01-01T00:00:00Z",
            "processes": [], "windows": [], "context_clues": [],
            "restore_hints": [], "warnings": [], "thumbnail_path": ""
        }"#,
        )
        .unwrap();

        assert!(with_snapshot_name(snapshot, "  ").is_err());
    }

    #[test]
    fn v2_snapshot_without_browser_sessions_remains_readable() {
        let snapshot: Snapshot = serde_json::from_str(
            r#"{
            "schema_version": 2,
            "id": "snap_1",
            "name": "Old",
            "timestamp": "2026-01-01T00:00:00Z",
            "processes": [],
            "windows": [],
            "context_clues": [],
            "restore_hints": [],
            "warnings": [],
            "thumbnail_path": "C:/snapshot.png"
        }"#,
        )
        .expect("v2 snapshots must remain readable");

        assert!(snapshot.browser_sessions.is_empty());
        assert!(snapshot.explorer_windows.is_empty());
    }

    #[test]
    fn explorer_folder_windows_round_trip_with_geometry() {
        let snapshot: Snapshot = serde_json::from_str(
            r#"{
            "schema_version": 4,
            "id": "snap_explorer",
            "name": "Folders",
            "timestamp": "2026-01-01T00:00:00Z",
            "processes": [],
            "windows": [],
            "explorer_windows": [{
                "path": "C:\\Users\\sarth\\Downloads",
                "path_kind": "filesystem",
                "title": "Downloads",
                "position": {"x": 100, "y": 200},
                "size": {"width": 900, "height": 700},
                "state": "maximized",
                "monitor_index": 1
            }],
            "context_clues": [],
            "restore_hints": [],
            "warnings": [],
            "thumbnail_path": "C:/snapshot.png"
        }"#,
        )
        .expect("schema v4 Explorer window must deserialize");

        let folder = &snapshot.explorer_windows[0];
        assert_eq!(folder.path, r"C:\Users\sarth\Downloads");
        assert_eq!(folder.position.x, 100);
        assert_eq!(folder.size.height, 700);
        assert_eq!(folder.state, "maximized");
        assert_eq!(folder.monitor_index, 1);

        let encoded = serde_json::to_string(&snapshot).unwrap();
        let decoded: Snapshot = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.explorer_windows[0].path, folder.path);
    }

    #[test]
    fn selective_explorer_restore_slice_contains_only_folder_windows() {
        let snapshot: Snapshot = serde_json::from_str(
            r#"{
            "schema_version":4,"id":"snap_explorer","name":"Mixed","timestamp":"2026-01-01T00:00:00Z",
            "processes":[{"name":"notepad.exe","pid":1,"exe_path":"C:\\Windows\\notepad.exe","cmd_line":"notepad.exe","classification":"foreground"}],
            "windows":[{"title":"Notes","position":{"x":1,"y":2},"size":{"width":800,"height":600},"state":"normal","monitor_index":0,"exe_path":"C:\\Windows\\notepad.exe"}],
            "explorer_windows":[
                {"path":"C:\\Downloads","path_kind":"filesystem","title":"Downloads","position":{"x":3,"y":4},"size":{"width":900,"height":700},"state":"normal","monitor_index":0},
                {"path":"C:\\Scripts","path_kind":"filesystem","title":"Scripts","position":{"x":13,"y":14},"size":{"width":800,"height":600},"state":"normal","monitor_index":0},
                {"path":"C:\\Model Output","path_kind":"filesystem","title":"Model Output","position":{"x":23,"y":24},"size":{"width":700,"height":500},"state":"maximized","monitor_index":1}
            ],
            "context_clues":[{"type":"file","value":"notes.txt","confidence":1.0,"source":"test"}],
            "restore_hints":["foreground:notepad.exe"],"warnings":[],"thumbnail_path":"C:/snapshot.png",
            "terminal_sessions":[{"shell":"powershell","cwd":"C:\\repo","history":[],"window_title":"PowerShell"}],
            "browser_sessions":[]
        }"#,
        )
        .unwrap();

        let explorer = snapshot_for_explorer(&snapshot).unwrap();
        assert_eq!(explorer.explorer_windows.len(), 3);
        assert_eq!(explorer.explorer_windows[1].path, r"C:\Scripts");
        assert_eq!(explorer.explorer_windows[2].title, "Model Output");
        assert!(explorer.processes.is_empty());
        assert!(explorer.windows.is_empty());
        assert!(explorer.context_clues.is_empty());
        assert!(explorer.restore_hints.is_empty());
        assert!(explorer.terminal_sessions.is_empty());
        assert!(explorer.browser_sessions.is_empty());
        assert_eq!(snapshot.processes.len(), 1, "persisted snapshot is not mutated");
    }

    #[test]
    fn selective_explorer_restore_rejects_snapshots_without_folders() {
        let snapshot: Snapshot = serde_json::from_str(
            r#"{
            "schema_version":3,"id":"old","name":"Old","timestamp":"2026-01-01T00:00:00Z",
            "processes":[],"windows":[],"context_clues":[],"restore_hints":[],"warnings":[],
            "thumbnail_path":"C:/snapshot.png"
        }"#,
        )
        .unwrap();
        assert!(snapshot_for_explorer(&snapshot).is_err());
    }

    #[test]
    fn malformed_optional_browser_payload_does_not_corrupt_snapshot() {
        let snapshot: Snapshot = serde_json::from_str(
            r#"{
            "schema_version": 3,
            "id": "snap_2",
            "name": "Partial",
            "timestamp": "2026-01-01T00:00:00Z",
            "processes": [],
            "windows": [],
            "context_clues": [],
            "restore_hints": [],
            "warnings": [],
            "thumbnail_path": "C:/snapshot.png",
            "browser_sessions": "not-an-array"
        }"#,
        )
        .expect("browser context must not invalidate the desktop snapshot");

        assert!(snapshot.browser_sessions.is_empty());
    }

    #[test]
    fn selective_restore_slice_contains_only_the_requested_app() {
        let snapshot: Snapshot = serde_json::from_str(
            r#"{
            "schema_version": 3,
            "id": "snap_selective",
            "name": "Mixed workspace",
            "timestamp": "2026-01-01T00:00:00Z",
            "processes": [
                {"name":"opera.exe","pid":10,"exe_path":"C:\\Apps\\opera.exe","cmd_line":"opera.exe","classification":"browser"},
                {"name":"Code.exe","pid":20,"exe_path":"C:\\Apps\\Code.exe","cmd_line":"Code.exe C:\\repo","classification":"ide"}
            ],
            "windows": [
                {"title":"Opera","position":{"x":1,"y":2},"size":{"width":800,"height":600},"state":"normal","monitor_index":0,"exe_path":"C:\\Apps\\opera.exe"},
                {"title":"Repo - Code","position":{"x":3,"y":4},"size":{"width":900,"height":700},"state":"normal","monitor_index":0,"exe_path":"C:\\Apps\\Code.exe"}
            ],
            "explorer_windows": [{"path":"C:\\Downloads","path_kind":"filesystem","title":"Downloads","position":{"x":5,"y":6},"size":{"width":700,"height":500},"state":"normal","monitor_index":0}],
            "context_clues": [{"type":"browser_tab","value":"https://example.com","confidence":0.9,"source":"test"}],
            "restore_hints": [
                "browser_tab:opera:https://example.com",
                "vscode_folder:C:\\repo",
                "foreground:Code.exe"
            ],
            "warnings": [],
            "thumbnail_path": "C:/snapshot.png",
            "terminal_sessions": [{"shell":"powershell","cwd":"C:\\repo","history":[],"window_title":"PowerShell"}],
            "browser_sessions": [
                {"protocol_version":1,"browser":{"family":"opera","profile_instance_id":"opera-profile"},"captured_at":"2026-01-01T00:00:00Z","capabilities":{"tab_groups":true},"windows":[]},
                {"protocol_version":1,"browser":{"family":"edge","profile_instance_id":"edge-profile"},"captured_at":"2026-01-01T00:00:00Z","capabilities":{"tab_groups":true},"windows":[]}
            ]
        }"#,
        )
        .expect("test snapshot must deserialize");

        let (app, name) = snapshot_for_app(&snapshot, r"C:\Apps\opera.exe")
            .expect("captured app must be selectable");

        assert_eq!(name, "opera");
        assert_eq!(app.processes.len(), 1);
        assert_eq!(app.windows.len(), 1);
        assert_eq!(app.processes[0].exe_path, r"C:\Apps\opera.exe");
        assert_eq!(
            app.restore_hints,
            vec!["browser_tab:opera:https://example.com"]
        );
        assert!(app.context_clues.is_empty());
        assert!(app.terminal_sessions.is_empty());
        assert!(app.explorer_windows.is_empty());
        assert_eq!(app.browser_sessions.len(), 1);
        assert_eq!(app.browser_sessions[0].browser.family, "opera");
        assert_eq!(
            snapshot.processes.len(),
            2,
            "the persisted snapshot is not mutated"
        );
    }

    #[test]
    fn selective_restore_rejects_an_executable_not_in_the_snapshot() {
        let snapshot: Snapshot = serde_json::from_str(
            r#"{
            "schema_version":3,"id":"snap_1","name":"One","timestamp":"2026-01-01T00:00:00Z",
            "processes":[],"windows":[],"context_clues":[],"restore_hints":[],"warnings":[],
            "thumbnail_path":"C:/snapshot.png","terminal_sessions":[],"browser_sessions":[]
        }"#,
        )
        .unwrap();

        assert!(snapshot_for_app(&snapshot, r"C:\\malicious.exe").is_err());
        assert!(snapshot_for_app(&snapshot, "").is_err());
    }

    #[test]
    fn selective_restore_drops_ambiguous_browser_companion_sessions() {
        let snapshot: Snapshot = serde_json::from_str(
            r#"{
            "schema_version":3,"id":"snap_browsers","name":"Browsers","timestamp":"2026-01-01T00:00:00Z",
            "processes":[
                {"name":"chrome.exe","pid":1,"exe_path":"C:\\Chrome\\chrome.exe","cmd_line":"","classification":"browser"},
                {"name":"chromium.exe","pid":2,"exe_path":"C:\\Chromium\\chromium.exe","cmd_line":"","classification":"browser"}
            ],
            "windows":[],"context_clues":[],"restore_hints":["browser_tab:chrome:https://example.com"],"warnings":[],
            "thumbnail_path":"C:/snapshot.png","terminal_sessions":[],
            "browser_sessions":[{"protocol_version":1,"browser":{"family":"chromium","profile_instance_id":"ambiguous"},"captured_at":"2026-01-01T00:00:00Z","capabilities":{"tab_groups":true},"windows":[]}]
        }"#,
        )
        .unwrap();

        let (chrome, _) = snapshot_for_app(&snapshot, r"C:\Chrome\chrome.exe").unwrap();
        assert!(chrome.browser_sessions.is_empty());
        assert_eq!(
            chrome.restore_hints,
            vec!["browser_tab:chrome:https://example.com"]
        );
    }

    // ── window_multiset_diff: the drift signal behind the restore-confirm warning ──

    fn win(exe: &str) -> super::WindowInfo {
        super::WindowInfo {
            title: String::new(),
            position: super::WindowPosition { x: 0, y: 0 },
            size: super::WindowSize { width: 0, height: 0 },
            state: "normal".into(),
            monitor_index: 0,
            exe_path: exe.into(),
        }
    }

    fn live(pairs: &[(&str, usize)]) -> std::collections::HashMap<String, usize> {
        pairs.iter().map(|(s, n)| (s.to_string(), *n)).collect()
    }

    #[test]
    fn exact_match_is_zero_diff() {
        let snap = [win(r"C:\Chrome\chrome.exe"), win(r"C:\Apps\Code.exe")];
        let now = live(&[("chrome", 1), ("code", 1)]);
        assert_eq!(super::window_multiset_diff(&now, &snap), 0);
    }

    #[test]
    fn opening_another_app_is_drift() {
        // Snapshot has chrome only; user has since opened notepad.
        let snap = [win(r"C:\Chrome\chrome.exe")];
        let now = live(&[("chrome", 1), ("notepad", 1)]);
        assert_eq!(super::window_multiset_diff(&now, &snap), 1);
    }

    #[test]
    fn second_window_of_an_open_app_is_drift() {
        // Snapshot recorded one Chrome window; two are open now.
        let snap = [win(r"C:\Chrome\chrome.exe")];
        let now = live(&[("chrome", 2)]);
        assert_eq!(super::window_multiset_diff(&now, &snap), 1);
    }

    #[test]
    fn closing_an_app_in_the_snapshot_is_drift() {
        // Snapshot had Explorer + Chrome; Explorer since closed → symmetric diff catches it.
        let snap = [win(r"C:\Windows\explorer.exe"), win(r"C:\Chrome\chrome.exe")];
        let now = live(&[("chrome", 1)]);
        assert_eq!(super::window_multiset_diff(&now, &snap), 1);
    }

    #[test]
    fn snapshot_windows_without_an_exe_path_are_ignored() {
        let snap = [win(""), win(r"C:\Chrome\chrome.exe")];
        let now = live(&[("chrome", 1), ("notepad", 1)]);
        assert_eq!(super::window_multiset_diff(&now, &snap), 1);
    }
}
