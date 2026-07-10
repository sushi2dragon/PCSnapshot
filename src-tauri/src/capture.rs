//! Capture engine — enumerates visible top-level windows, maps them to processes,
//! collects metadata, and produces the structured payload that `take_snapshot`
//! persists.
//!
//! Speed is the primary metric. Window enumeration is synchronous Win32 (sub-millisecond);
//! the only non-trivial cost is one `sysinfo` refresh scoped to *just* the PIDs that own a
//! visible window (not the whole process table). The desktop screenshot — the genuinely slow
//! part — is run on a separate thread by the caller (`lib.rs`) so it overlaps this work.
//!
//! Nothing here ever fails the capture wholesale: every fallible step degrades to a warning
//! so a snapshot is always produced (per the error-handling spec).

use crate::{ContextClue, ProcessInfo, TerminalSession, WindowInfo, WindowPosition, WindowSize};
use crate::classify::{self, Category};

/// Structured result of inspecting the live desktop. The caller folds this into a `Snapshot`.
pub struct CapturedDesktop {
    pub processes: Vec<ProcessInfo>,
    pub windows: Vec<WindowInfo>,
    pub context_clues: Vec<ContextClue>,
    pub restore_hints: Vec<String>,
    pub warnings: Vec<String>,
    /// Per-terminal-window session (shell, real CWD, history). Computed here —
    /// not in `lib.rs` — because it needs each window's owning PID, which is
    /// dropped from `WindowInfo` before the caller sees it.
    pub terminal_sessions: Vec<TerminalSession>,
}

impl CapturedDesktop {
    fn empty_with_warning(w: String) -> Self {
        CapturedDesktop {
            processes: vec![],
            windows: vec![],
            context_clues: vec![],
            restore_hints: vec![],
            warnings: vec![w],
            terminal_sessions: vec![],
        }
    }
}

/// A raw window record collected during Win32 enumeration, before process metadata is joined.
struct RawWindow {
    hwnd: isize,         // kept for in-capture UI Automation (browser tab URLs); never persisted
    pid: u32,
    title: String,
    /// Restored (normal) position — what we persist and later restore to.
    pos: WindowPosition,
    size: WindowSize,
    state: &'static str, // "normal" | "minimized" | "maximized"
    monitor: isize,      // HMONITOR, resolved to an index after enumeration
}

// ── Windows implementation ──────────────────────────────────────────────────────────────

#[cfg(windows)]
pub fn capture_desktop(ignore_list: &[String]) -> CapturedDesktop {
    let raw = match enumerate_windows() {
        Ok(r) => r,
        Err(e) => return CapturedDesktop::empty_with_warning(format!("Window enumeration failed: {e}")),
    };

    let monitors = enumerate_monitors();
    let foreground_pid = foreground_pid();

    let mut warnings: Vec<String> = vec![];

    // Build the window list, foreground window first.
    let mut windows: Vec<(u32, WindowInfo)> = raw
        .iter()
        .map(|w| {
            let monitor_index = monitors
                .iter()
                .position(|m| *m == w.monitor)
                .unwrap_or(0) as u32;
            (
                w.pid,
                WindowInfo {
                    title: w.title.clone(),
                    position: w.pos.clone(),
                    size: w.size.clone(),
                    state: w.state.to_string(),
                    monitor_index,
                    exe_path: String::new(), // filled in the enrichment pass below
                },
            )
        })
        .collect();
    windows.sort_by_key(|(pid, _)| Some(*pid) != foreground_pid);

    // Unique PIDs that own a visible window — the only processes we look up.
    let mut pids: Vec<u32> = raw.iter().map(|w| w.pid).collect();
    pids.sort_unstable();
    pids.dedup();

    let meta = process_metadata(&pids);

    // Filter out ignored processes (user ignore list + system-critical).
    // Done before classification/context so ignored apps never appear anywhere.
    let ignored_pids: std::collections::HashSet<u32> = pids
        .iter()
        .copied()
        .filter(|pid| {
            meta.get(pid)
                .map(|m| crate::config::is_ignored(&crate::restore::exe_stem_pub(&m.exe_path), ignore_list))
                .unwrap_or(false)
        })
        .collect();
    pids.retain(|pid| !ignored_pids.contains(pid));
    windows.retain(|(pid, _)| !ignored_pids.contains(pid));

    let mut processes: Vec<ProcessInfo> = Vec::with_capacity(pids.len());
    for pid in &pids {
        let (name, exe_path, cmd_line) = match meta.get(pid) {
            Some(m) => (m.name.clone(), m.exe_path.clone(), m.cmd_line.clone()),
            None => {
                warnings.push(format!("Process {pid}: metadata unavailable (likely protected/elevated)"));
                (String::new(), String::new(), String::new())
            }
        };

        if exe_path.is_empty() {
            warnings.push(format!("Process {pid} ({name}): executable path unavailable"));
        } else if cmd_line.is_empty() {
            warnings.push(format!("Process {pid} ({name}): command line unavailable — exact session may not restore"));
        }

        // Classify. The foreground process is overridden so restore focuses it last.
        let category = if Some(*pid) == foreground_pid {
            Category::Foreground
        } else {
            classify::classify(&exe_path, true)
        };

        processes.push(ProcessInfo {
            name,
            pid: *pid,
            exe_path,
            cmd_line,
            classification: category.as_str().to_string(),
        });
    }

    // Foreground hint: which app was in front.
    let mut restore_hints: Vec<String> = vec![];
    if let Some(fg) = foreground_pid {
        if let Some(p) = processes.iter().find(|p| p.pid == fg) {
            let stem = p.exe_path.rsplit(|c| c == '\\' || c == '/').next().unwrap_or(&p.name);
            restore_hints.push(format!("foreground:{stem}"));
        }
    }

    // Terminal sessions need each window's owning PID, so compute them now —
    // before enrichment consumes `windows` and drops the PID.
    let terminal_sessions = crate::terminal::capture_terminal_sessions(&processes, &windows);

    // Enrich each window with its owning process's exe path.
    let enriched_windows: Vec<WindowInfo> = windows
        .into_iter()
        .map(|(pid, mut w)| {
            w.exe_path = meta
                .get(&pid)
                .map(|m| m.exe_path.clone())
                .unwrap_or_default();
            w
        })
        .collect();

    // Run context extraction rules over the captured data.
    let (mut context_clues, ctx_hints) = crate::context::extract_context(&processes, &enriched_windows);
    restore_hints.extend(ctx_hints);

    // Browser active-tab URLs (best-effort, via UI Automation). Stored as
    // `browser_tab:<exe_stem>:<url>` hints so restore can reopen the exact tabs
    // instead of relying on the browser's session-dependent recently-closed list.
    capture_browser_tabs(&raw, &processes, &mut restore_hints, &mut context_clues);

    CapturedDesktop {
        processes,
        windows: enriched_windows,
        context_clues,
        restore_hints,
        warnings,
        terminal_sessions,
    }
}

/// Read the active-tab URL of every browser window and append `browser_tab` hints
/// + clues. Best-effort: no-op when nothing is a browser or UIA can't read a URL.
#[cfg(windows)]
fn capture_browser_tabs(
    raw: &[RawWindow],
    processes: &[ProcessInfo],
    restore_hints: &mut Vec<String>,
    clues: &mut Vec<ContextClue>,
) {
    use std::collections::{HashMap, HashSet};

    // Map browser-owning PIDs → exe stem (detect by exe so a *foreground* browser,
    // whose classification is "foreground", is still included).
    let browser_pids: HashMap<u32, String> = processes
        .iter()
        .filter(|p| !p.exe_path.is_empty() && classify::classify(&p.exe_path, true).is_browser())
        .map(|p| (p.pid, crate::restore::exe_stem_pub(&p.exe_path)))
        .collect();

    if browser_pids.is_empty() {
        return;
    }

    let mut seen: HashSet<(String, String)> = HashSet::new();

    // Primary: parse each browser's on-disk session file to recover ALL open
    // tabs (including inactive ones, which UI Automation cannot see).
    let stems: HashSet<String> = browser_pids.values().cloned().collect();
    let mut covered: HashSet<String> = HashSet::new();
    for (stem, url) in crate::browser::read_open_tab_urls(&stems) {
        covered.insert(stem.clone());
        if seen.insert((stem.clone(), url.clone())) {
            restore_hints.push(format!("browser_tab:{stem}:{url}"));
            clues.push(ContextClue {
                clue_type: "browser_tab".to_string(),
                value: url,
                confidence: 0.95,
                source: "session_file".to_string(),
            });
        }
    }

    // Fallback: for any browser whose session file we couldn't read, grab at
    // least the active tab per window via UI Automation. One (stem, hwnd) target
    // per window, capped so a wall of browser windows can't blow the <3s budget.
    let targets: Vec<(String, isize)> = raw
        .iter()
        .filter_map(|w| browser_pids.get(&w.pid).map(|stem| (stem.clone(), w.hwnd)))
        .filter(|(stem, _)| !covered.contains(stem))
        .take(12)
        .collect();

    for (stem, url) in crate::browser::read_active_tab_urls(&targets) {
        if seen.insert((stem.clone(), url.clone())) {
            restore_hints.push(format!("browser_tab:{stem}:{url}"));
            clues.push(ContextClue {
                clue_type: "browser_tab".to_string(),
                value: url,
                confidence: 0.90,
                source: "ui_automation".to_string(),
            });
        }
    }
}

#[cfg(not(windows))]
fn capture_browser_tabs(
    _raw: &[RawWindow],
    _processes: &[ProcessInfo],
    _restore_hints: &mut Vec<String>,
    _clues: &mut Vec<ContextClue>,
) {
}

#[cfg(windows)]
struct ProcMeta {
    name: String,
    exe_path: String,
    cmd_line: String,
}

/// One scoped `sysinfo` refresh over exactly the PIDs that own visible windows.
/// WOW64-safe (sysinfo reads the PEB internally), so 32-bit and 64-bit targets both work.
#[cfg(windows)]
fn process_metadata(pids: &[u32]) -> std::collections::HashMap<u32, ProcMeta> {
    use std::collections::HashMap;
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

    let mut out = HashMap::new();
    if pids.is_empty() {
        return out;
    }

    let pid_list: Vec<Pid> = pids.iter().map(|p| Pid::from_u32(*p)).collect();
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pid_list),
        ProcessRefreshKind::new()
            .with_exe(UpdateKind::Always)
            .with_cmd(UpdateKind::Always),
    );

    for pid in pids {
        if let Some(proc_) = sys.process(Pid::from_u32(*pid)) {
            let name = proc_.name().to_string_lossy().into_owned();
            let exe_path = proc_
                .exe()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            // Quote each arg that contains spaces so the tokenizer in the restore
            // engine can reconstruct the original tokens without splitting on spaces
            // inside paths (e.g. "C:\Program Files\...").
            let cmd_line = proc_
                .cmd()
                .iter()
                .map(|s| {
                    let s = s.to_string_lossy();
                    if s.contains(' ') || s.is_empty() {
                        format!("\"{}\"", s)
                    } else {
                        s.into_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            out.insert(*pid, ProcMeta { name, exe_path, cmd_line });
        }
    }
    out
}

#[cfg(windows)]
fn foreground_pid() -> Option<u32> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            None
        } else {
            Some(pid)
        }
    }
}

/// Ordered list of monitor handles; a window's `monitor_index` is its position here.
#[cfg(windows)]
fn enumerate_monitors() -> Vec<isize> {
    use windows::Win32::Foundation::{BOOL, LPARAM, RECT, TRUE};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};

    unsafe extern "system" fn cb(h: HMONITOR, _hdc: HDC, _rc: *mut RECT, data: LPARAM) -> BOOL {
        let out = &mut *(data.0 as *mut Vec<isize>);
        out.push(h.0 as isize);
        TRUE
    }

    let mut out: Vec<isize> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(cb),
            LPARAM(&mut out as *mut Vec<isize> as isize),
        );
    }
    out
}

/// Enumerate real, user-facing top-level windows and collect raw geometry/title/pid.
#[cfg(windows)]
fn enumerate_windows() -> Result<Vec<RawWindow>, String> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowLongW, GetWindowPlacement, GetWindowTextLengthW,
        GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
        GWL_EXSTYLE, GW_OWNER, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED,
        WINDOWPLACEMENT, WS_EX_TOOLWINDOW,
    };

    unsafe extern "system" fn cb(hwnd: HWND, data: LPARAM) -> BOOL {
        let out = &mut *(data.0 as *mut Vec<RawWindow>);

        // Visible only.
        if !IsWindowVisible(hwnd).as_bool() {
            return TRUE;
        }
        // Skip tool windows (floating palettes, etc.).
        let exstyle = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if exstyle & WS_EX_TOOLWINDOW.0 != 0 {
            return TRUE;
        }
        // Skip owned windows (dialogs/popups belonging to a primary window).
        if let Ok(owner) = GetWindow(hwnd, GW_OWNER) {
            if !owner.0.is_null() {
                return TRUE;
            }
        }
        // Skip DWM-cloaked windows (UWP ghost windows on virtual desktops).
        let mut cloaked: u32 = 0;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return TRUE;
        }
        // Must have a non-empty title.
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return TRUE;
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let copied = GetWindowTextW(hwnd, &mut buf);
        if copied <= 0 {
            return TRUE;
        }
        let title = String::from_utf16_lossy(&buf[..copied as usize]);

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return TRUE;
        }

        // Placement gives the restored rect + show state in one call.
        let mut wp = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        let (pos, size, state) = if GetWindowPlacement(hwnd, &mut wp).is_ok() {
            let r = wp.rcNormalPosition;
            let state = match wp.showCmd {
                x if x == SW_SHOWMINIMIZED.0 as u32 => "minimized",
                x if x == SW_SHOWMAXIMIZED.0 as u32 => "maximized",
                _ => "normal",
            };
            (
                WindowPosition { x: r.left, y: r.top },
                WindowSize {
                    width: (r.right - r.left).max(0) as u32,
                    height: (r.bottom - r.top).max(0) as u32,
                },
                state,
            )
        } else {
            (WindowPosition { x: 0, y: 0 }, WindowSize { width: 0, height: 0 }, "normal")
        };

        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST).0 as isize;

        out.push(RawWindow {
            hwnd: hwnd.0 as isize,
            pid,
            title,
            pos,
            size,
            state,
            monitor,
        });
        TRUE
    }

    let mut out: Vec<RawWindow> = Vec::new();
    unsafe {
        EnumWindows(Some(cb), LPARAM(&mut out as *mut Vec<RawWindow> as isize))
            .map_err(|e| e.to_string())?;
    }
    Ok(out)
}

// ── Non-Windows fallback (keeps the crate cross-compilable) ───────────────────────────────

#[cfg(not(windows))]
pub fn capture_desktop(_ignore_list: &[String]) -> CapturedDesktop {
    CapturedDesktop::empty_with_warning(
        "Capture engine is only implemented on Windows".to_string(),
    )
}
