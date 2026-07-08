//! Restore engine — reconstructs a saved snapshot.
//!
//! Order of operations (per the restore-engine spec):
//!   1. Reuse-if-running: don't relaunch apps that are already open.
//!   2. Launch the rest in priority order: background → terminals → IDEs → browsers → foreground.
//!   3. Bounded reposition pass: as windows appear, move them to their saved geometry/state.
//!   4. Macros: freshly-launched browsers get Ctrl+Shift+T (reopen tabs); the foreground
//!      app is focused last so it ends on top — exactly where it was at capture.
//!
//! Everything is best-effort and bounded in time. Launch failures are reported honestly in
//! `failed_items`; repositioning misses become warnings, never failures. The engine never panics.

use crate::classify::{self, Category};
use crate::{RestoreResult, Snapshot, WindowInfo};

#[cfg(windows)]
pub fn restore_desktop(snapshot: &Snapshot, close_others: bool, ignore_list: &[String]) -> RestoreResult {
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    let mut failed_items: Vec<String> = vec![];

    // ── 1. Reuse-if-running ───────────────────────────────────────────────────────────
    // Count how many instances of each exe are currently running. A process is only
    // "covered" if a running instance exists for it — but if the snapshot has 2
    // PowerShell windows and only 1 is running, we still need to launch 1 more.
    let running = running_exe_paths_counted();

    // ── 2. Ordered launch of everything not already open ──────────────────────────────
    // For each exe_path, subtract running-count from snapshot-count to find how many
    // new instances we need to launch. Uses a mutable counter per exe stem.
    // Build launch list: for each exe, skip the first `running_count` entries (already
    // covered), launch the rest.
    // Snapshot window count per exe — used to detect the "1 process, N windows" case
    // (Windows Terminal runs one process but can host multiple independent windows).
    let snap_win_counts: std::collections::HashMap<String, usize> = {
        let mut m = std::collections::HashMap::new();
        for w in &snapshot.windows {
            if !w.exe_path.is_empty() {
                *m.entry(w.exe_path.to_ascii_lowercase()).or_insert(0) += 1;
            }
        }
        m
    };

    // Live window counts BEFORE we launch anything — used later to avoid over-launching
    // in extra_windows and the window-deficit pass.
    let pre_launch_wins = live_windows();
    let pre_launch_win_counts: std::collections::HashMap<String, usize> = {
        let mut m = std::collections::HashMap::new();
        for w in &pre_launch_wins {
            if !w.exe.is_empty() {
                *m.entry(w.exe.to_ascii_lowercase()).or_insert(0) += 1;
            }
        }
        m
    };

    let mut covered: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut launch_list: Vec<&crate::ProcessInfo> = snapshot
        .processes
        .iter()
        .filter(|p| {
            if p.exe_path.is_empty() {
                return true; // let it through so the error path can report it
            }
            if crate::config::is_ignored(&exe_stem(&p.exe_path), ignore_list) {
                return false;
            }
            let key = p.exe_path.to_ascii_lowercase();
            let running_count = running.get(&key).copied().unwrap_or(0);
            let used = covered.entry(key).or_insert(0);
            if *used < running_count {
                *used += 1;
                false // this instance is already covered by a running process
            } else {
                true // need to launch
            }
        })
        .collect();
    launch_list.sort_by_key(|p| Category::from_str(&p.classification).launch_rank());

    let mut terminal_index: usize = 0;

    for proc_ in &launch_list {
        if proc_.exe_path.is_empty() {
            failed_items.push(format!("{}: no executable path recorded", display_name(proc_)));
            continue;
        }

        let cat = Category::from_str(&proc_.classification);
        // Detect browsers by exe too — a *foreground* browser is classified "foreground".
        let is_browser_proc = cat.is_browser() || classify::classify(&proc_.exe_path, true).is_browser();

        // Browsers: reopen the exact tabs captured at snapshot time (the active tab of
        // each window, via `browser_tab` hints) rather than the session-dependent
        // Ctrl+Shift+T trick. All captured URLs open as tabs in one launch. With no
        // captured URLs we launch plainly so the browser restores its own session.
        let is_terminal_proc = cat == Category::Terminal
            || classify::classify(&proc_.exe_path, true) == Category::Terminal;

        let launch_cmd = if is_browser_proc {
            let urls = browser_urls_for(snapshot, &exe_stem(&proc_.exe_path));
            if urls.is_empty() {
                String::new()
            } else {
                let quoted: Vec<String> = urls.iter().map(|u| format!("\"{u}\"")).collect();
                format!("\"{}\" {}", proc_.exe_path, quoted.join(" "))
            }
        } else if is_terminal_proc {
            terminal_restore_cmd(snapshot, proc_, &mut terminal_index, None)
                .unwrap_or_else(|| proc_.cmd_line.clone())
        } else {
            proc_.cmd_line.clone()
        };

        // Communication apps (Teams, Slack, …) are tray-resident: the user may have
        // "closed" them but the process is still running hidden. Skip launching but
        // make sure we bring their window to the foreground in the reposition pass.
        // (is_running was true → they're already in `running`, not in launch_list,
        //  but if somehow they are here, just launch normally.)

        match launch(&proc_.exe_path, &launch_cmd, &proc_.classification) {
            Ok(()) => {
                // Multi-window restore: if this process had more than one window in
                // the snapshot, launch it again for each additional window.
                // The first launch uses cmd_line (e.g. the specific file).
                // Subsequent launches use office_extra_file hints from context
                // extraction (full paths from the Office MRU registry) so each
                // extra window opens the correct file rather than a blank document.
                //
                // Skip browsers (tabs are restored via captured URLs above) and
                // communication apps (single-instance with their own multi-window model).
                if !is_browser_proc && !cat.is_communication() {
                    let snap_count = snapshot
                        .windows
                        .iter()
                        .filter(|w| {
                            !w.exe_path.is_empty()
                                && w.exe_path.eq_ignore_ascii_case(&proc_.exe_path)
                        })
                        .count();
                    // Subtract windows already open before this restore started (pre-launch)
                    // plus 1 for the window this launch just opened. This prevents over-launching
                    // when some windows were already open (e.g. WT with 1 of 2 windows still open).
                    let pre_live = pre_launch_win_counts
                        .get(&proc_.exe_path.to_ascii_lowercase())
                        .copied()
                        .unwrap_or(0);
                    let extra_windows = snap_count.saturating_sub(pre_live + 1);

                    if extra_windows > 0 {
                        // Collect office_extra_file hints for this exe stem.
                        let hint_prefix = format!("office_extra_file:{}:", exe_stem(&proc_.exe_path));
                        let extra_files: Vec<String> = snapshot.restore_hints.iter()
                            .filter_map(|h| h.strip_prefix(&hint_prefix).map(|s| s.to_string()))
                            .collect();

                        for idx in 0..extra_windows {
                            // Brief pause so the first instance initialises before
                            // the next one starts (avoids single-instance races on Office).
                            std::thread::sleep(Duration::from_millis(900));

                            // Build cmd_line: include exe as argv[0] so tokenize strips it,
                            // leaving just the file path as the argument.
                            let cmd = if let Some(path) = extra_files.get(idx) {
                                // Verified full path from Office MRU — open the exact file.
                                format!("\"{}\" \"{}\"", proc_.exe_path, path)
                            } else {
                                // No path hint — open a blank document.
                                String::new()
                            };
                            let _ = launch(&proc_.exe_path, &cmd, &proc_.classification);
                        }
                    }
                }
            }
            Err(e) => failed_items.push(format!("{}: failed to launch ({e})", display_name(proc_))),
        }
    }

    // Exes the main launch loop already handled — both the deficit pass and the
    // terminal reconciliation below must skip these to avoid double-launching.
    let launched_exes: std::collections::HashSet<String> = launch_list
        .iter()
        .map(|p| p.exe_path.to_ascii_lowercase())
        .collect();

    // ── Window deficit pass (non-terminal apps) ───────────────────────────────────────
    // Handles apps that host multiple windows in a single process — e.g. a single-process
    // app that opens N document windows. Terminals are handled separately below with
    // smarter reconciliation (close wrong ones, open missing ones).
    //
    // Only runs for exes NOT in the launch_list (process count was already covered).
    {
        for (exe_key, &snap_count) in &snap_win_counts {
            if launched_exes.contains(exe_key) {
                continue; // handled by main loop + extra_windows
            }
            let pre_live = pre_launch_win_counts.get(exe_key).copied().unwrap_or(0);
            if pre_live >= snap_count {
                continue;
            }

            if let Some(proc_) = snapshot
                .processes
                .iter()
                .find(|p| p.exe_path.to_ascii_lowercase() == *exe_key)
            {
                if crate::config::is_ignored(&exe_stem(&proc_.exe_path), ignore_list) {
                    continue;
                }
                let is_browser = classify::classify(&proc_.exe_path, true).is_browser();
                let is_terminal = Category::from_str(&proc_.classification) == Category::Terminal
                    || classify::classify(&proc_.exe_path, true) == Category::Terminal;
                // Skip browsers (handled separately) and terminals (handled below).
                if is_browser || Category::from_str(&proc_.classification).is_communication() || is_terminal {
                    continue;
                }

                let deficit = snap_count - pre_live;
                for i in 0..deficit {
                    if let Err(e) = launch(&proc_.exe_path, &proc_.cmd_line, &proc_.classification) {
                        failed_items.push(format!(
                            "{}: extra window {} failed ({e})",
                            display_name(proc_),
                            i + 1
                        ));
                    }
                    if i + 1 < deficit {
                        std::thread::sleep(Duration::from_millis(600));
                    }
                }
            }
        }
    }

    // ── Terminal reconciliation ───────────────────────────────────────────────────────
    // Terminals need smarter handling than a simple count-based deficit:
    //
    //   • On a clean restore (close_others), a terminal that is open but has a
    //     different title (wrong CWD / content) than any snapshot terminal is CLOSED
    //     and replaced with the correct one. A plain restore never closes anything.
    //   • A snapshot terminal that has no matching live terminal should be LAUNCHED,
    //     even if the total live count equals the snapshot count.
    //
    // This only runs for terminal exes where the process is ALREADY running (i.e. NOT
    // in launched_exes). When the process wasn't running at all, the main launch loop
    // already handled it via terminal_restore_cmd + extra_windows — reconciling those
    // against the pre-launch window list would double-launch them.
    //
    // Title matching: exact first, then substring (≥4 chars), same as the reposition pass.
    let mut term_closed: Vec<String> = vec![];
    {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

        // Collect snapshot terminal windows per exe, skipping exes the main
        // launch loop already launched this restore.
        let snap_terminal_windows: Vec<&WindowInfo> = snapshot
            .windows
            .iter()
            .filter(|w| {
                !w.exe_path.is_empty()
                    && !launched_exes.contains(&w.exe_path.to_ascii_lowercase())
                    && (Category::from_str(
                        &snapshot
                            .processes
                            .iter()
                            .find(|p| p.exe_path.eq_ignore_ascii_case(&w.exe_path))
                            .map(|p| p.classification.as_str())
                            .unwrap_or(""),
                    ) == Category::Terminal
                        || classify::classify(&w.exe_path, true) == Category::Terminal)
            })
            .collect();

        if !snap_terminal_windows.is_empty() {
            // Exes owning the snapshot's terminal windows. Reconciliation only
            // touches live windows of THESE exes — terminals of apps not in the
            // snapshot are the clean-restore pass's job; handling them here too
            // would close and report the same window twice.
            let snap_term_exes: std::collections::HashSet<String> = snap_terminal_windows
                .iter()
                .map(|w| w.exe_path.to_ascii_lowercase())
                .collect();

            // Collect pre-launch live terminal windows (again skipping exes the
            // main loop launched — their new windows aren't in the pre-launch list).
            let live_term_wins: Vec<&LiveWindow> = pre_launch_wins
                .iter()
                .filter(|w| {
                    !w.exe.is_empty()
                        && !launched_exes.contains(&w.exe.to_ascii_lowercase())
                        && snap_term_exes.contains(&w.exe.to_ascii_lowercase())
                        && classify::classify(&w.exe, true) == Category::Terminal
                })
                .collect();

            // One-to-one title match: for each snapshot terminal window find the best
            // live terminal window. Each live window can only be claimed once.
            let mut claimed_live: std::collections::HashSet<isize> = std::collections::HashSet::new();
            let mut matched_snap: std::collections::HashSet<usize> = std::collections::HashSet::new();

            for (si, sw) in snap_terminal_windows.iter().enumerate() {
                // Exact match first.
                if let Some(lw) = live_term_wins.iter().find(|lw| {
                    !claimed_live.contains(&lw.hwnd) && lw.title == sw.title
                }) {
                    claimed_live.insert(lw.hwnd);
                    matched_snap.insert(si);
                    continue;
                }
                // Substring match (min 4 chars so short generic titles don't over-match).
                if let Some(lw) = live_term_wins.iter().find(|lw| {
                    !claimed_live.contains(&lw.hwnd)
                        && lw.title.len() >= 4
                        && sw.title.len() >= 4
                        && (lw.title.contains(sw.title.as_str())
                            || sw.title.contains(lw.title.as_str()))
                }) {
                    claimed_live.insert(lw.hwnd);
                    matched_snap.insert(si);
                }
            }

            // Close live terminal windows that matched no snapshot terminal —
            // but only on a clean restore. Closing user windows on a plain
            // restore would be destructive and unreported (`closed_items` is
            // documented as clean-restore-only).
            if close_others {
                let mut any_closed = false;
                for lw in &live_term_wins {
                    if !claimed_live.contains(&lw.hwnd) {
                        let hwnd = HWND(lw.hwnd as *mut core::ffi::c_void);
                        let posted = unsafe { PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) };
                        if posted.is_ok() {
                            any_closed = true;
                            term_closed.push(format!("'{}' ({})", lw.title, exe_stem(&lw.exe)));
                        }
                    }
                }
                // Brief grace period for WM_CLOSE to process before we launch new windows.
                if any_closed {
                    std::thread::sleep(Duration::from_millis(600));
                }
            }

            // Launch snapshot terminals that have no live match.
            for (si, sw) in snap_terminal_windows.iter().enumerate() {
                if matched_snap.contains(&si) {
                    continue;
                }
                if let Some(proc_) = snapshot
                    .processes
                    .iter()
                    .find(|p| p.exe_path.eq_ignore_ascii_case(&sw.exe_path))
                {
                    if crate::config::is_ignored(&exe_stem(&proc_.exe_path), ignore_list) {
                        continue;
                    }
                    let cmd = terminal_restore_cmd(snapshot, proc_, &mut terminal_index, Some(&sw.title))
                        .unwrap_or_else(|| proc_.cmd_line.clone());
                    if let Err(e) = launch(&proc_.exe_path, &cmd, &proc_.classification) {
                        failed_items.push(format!(
                            "{}: terminal window {} failed to launch ({e})",
                            display_name(proc_),
                            si + 1
                        ));
                    }
                }
            }
        }
    }

    // Communication apps that are tray-resident (is_running == true, so they were
    // skipped from launch_list): find their saved windows and restore them from tray.
    for proc_ in snapshot.processes.iter() {
        if !Category::from_str(&proc_.classification).is_communication() {
            continue;
        }
        if running.get(&proc_.exe_path.to_ascii_lowercase()).copied().unwrap_or(0) == 0 {
            continue; // not running → it will be handled by the normal launch_list above
        }
        // Already running in the tray — just bring its window forward via ShowWindow.
        // We don't relaunch; instead we let the reposition pass surface the window when
        // it matches by title. Nudge it by posting a "restore from tray" signal via
        // the window's taskbar button (best-effort, no crash if it fails).
        let live = live_windows();
        for win in snapshot.windows.iter().filter(|w| {
            !w.exe_path.is_empty() && w.exe_path.eq_ignore_ascii_case(&proc_.exe_path)
        }) {
            if let Some(hwnd) = match_window(&live, &win.title) {
                focus_window(hwnd);
            }
        }
    }

    // ── 3. Bounded reposition pass ────────────────────────────────────────────────────
    // Two-tier, best-effort matching, run repeatedly until every saved window is placed
    // or the deadline passes. Each live window can be claimed only once (`consumed`), so
    // two saved windows never fight over the same hwnd.
    //
    //   Tier 1 — title match: precise, handles the common case.
    //   Tier 2 — exe fallback: when the title has drifted since capture (unsaved-doc '*'
    //            markers, the active browser tab, dynamic chat titles), place the saved
    //            window onto any still-unclaimed window owned by the same executable.
    //
    // The deadline is generous (capture must be <3s, but restore has no such budget):
    // Electron/JVM apps like Teams and Opera routinely take several seconds to show a
    // window, and a too-short deadline was the main reason so many windows were skipped.
    let mut pending: Vec<&WindowInfo> = snapshot.windows.iter().collect();
    let mut consumed: HashSet<isize> = HashSet::new();
    let deadline = Instant::now() + Duration::from_millis(8000);
    while !pending.is_empty() && Instant::now() < deadline {
        let live = live_windows();

        // Tier 1: title match across all pending first, so precise matches win before
        // any exe fallback claims a window.
        pending.retain(|target| {
            if let Some(hwnd) = match_window_titled(&live, &target.title, &consumed) {
                apply_geometry(hwnd, target);
                consumed.insert(hwnd);
                false
            } else {
                true
            }
        });

        // Tier 2: exe fallback for whatever title-matching couldn't place this round.
        pending.retain(|target| {
            if let Some(hwnd) = match_window_by_exe(&live, &target.exe_path, &consumed) {
                apply_geometry(hwnd, target);
                consumed.insert(hwnd);
                false
            } else {
                true
            }
        });

        if pending.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(150));
    }

    // Soft warnings: windows we never managed to place, with a human-readable reason.
    let mut warnings: Vec<String> = pending.iter().map(|w| unplaced_reason(w)).collect();

    // ── 4. Optional clean-up: close windows that aren't part of this snapshot ──────────
    // Terminals closed during reconciliation (clean restore only) are reported too.
    let mut closed_items = term_closed;
    if close_others {
        let (closed, leftover) = close_windows_not_in_snapshot(snapshot, ignore_list);
        // Windows that wouldn't close are surfaced honestly as warnings.
        warnings.extend(leftover);
        closed_items.extend(closed);
    }

    finalize(snapshot, failed_items, warnings, closed_items)
}

/// Runs the macro layer + builds the result. Separated so the timeout path reuses it.
#[cfg(windows)]
fn finalize(
    snapshot: &Snapshot,
    failed_items: Vec<String>,
    warnings: Vec<String>,
    closed_items: Vec<String>,
) -> RestoreResult {
    // ── Macro: focus the foreground app last so it ends on top ─────────────────────────
    if let Some(fg) = snapshot
        .processes
        .iter()
        .find(|p| p.classification == "foreground")
    {
        let live = live_windows();
        if let Some(w) = live.iter().find(|w| exe_stem(&w.exe) == exe_stem(&fg.exe_path)) {
            focus_window(w.hwnd);
        }
    }

    let message = build_message(&failed_items, &warnings, &closed_items);

    RestoreResult {
        success: failed_items.is_empty(),
        message,
        failed_items,
        warnings,
        closed_items,
    }
}

/// One-line human summary that names every category that had activity.
#[cfg(windows)]
fn build_message(failed: &[String], warnings: &[String], closed: &[String]) -> String {
    let mut parts: Vec<String> = vec![];
    if !failed.is_empty() {
        parts.push(format!("{} app(s) could not be launched", failed.len()));
    }
    if !warnings.is_empty() {
        parts.push(format!("{} window(s) not repositioned", warnings.len()));
    }
    if !closed.is_empty() {
        parts.push(format!("{} extra window(s) closed", closed.len()));
    }

    if parts.is_empty() {
        return "Snapshot restored successfully".to_string();
    }

    let prefix = if failed.is_empty() {
        "Snapshot restored"
    } else {
        "Partial restore"
    };
    format!("{prefix} — {}", parts.join(", "))
}

/// Human-readable reason a saved window could not be repositioned.
#[cfg(windows)]
fn unplaced_reason(w: &WindowInfo) -> String {
    let title = if w.title.is_empty() { "(untitled window)" } else { &w.title };
    let app = exe_stem(&w.exe_path);
    if app == "explorer" {
        // File Explorer windows are owned by the always-running shell; we never relaunch
        // it, so a closed folder window simply isn't there to move.
        format!("'{title}' (File Explorer) — folder window was not reopened by Windows")
    } else if w.exe_path.is_empty() {
        format!("'{title}' — no owning app was recorded, so its window could not be found")
    } else {
        format!(
            "'{title}' ({app}) — the app did not show a matching window in time (it may still be loading, or its title changed since capture)"
        )
    }
}

#[cfg(windows)]
fn display_name(p: &crate::ProcessInfo) -> String {
    if !p.name.is_empty() {
        p.name.clone()
    } else if !p.exe_path.is_empty() {
        exe_stem(&p.exe_path)
    } else {
        format!("pid {}", p.pid)
    }
}

fn exe_stem(exe_path: &str) -> String {
    let last = exe_path.rsplit(|c| c == '\\' || c == '/').next().unwrap_or(exe_path);
    last.strip_suffix(".exe")
        .or_else(|| last.strip_suffix(".EXE"))
        .unwrap_or(last)
        .to_ascii_lowercase()
}

/// Public wrapper so other modules can derive the same lowercase exe stem.
pub fn exe_stem_pub(exe_path: &str) -> String {
    exe_stem(exe_path)
}

/// Lowercase exe stems of every app that currently owns a visible window.
/// Used to compare the live desktop against saved snapshots.
#[cfg(windows)]
pub fn current_app_set() -> std::collections::HashSet<String> {
    live_windows()
        .iter()
        .filter(|w| !w.exe.is_empty())
        .map(|w| exe_stem(&w.exe))
        .collect()
}

#[cfg(not(windows))]
pub fn current_app_set() -> std::collections::HashSet<String> {
    std::collections::HashSet::new()
}

// tokenize is now pub(crate) in lib.rs — use crate::tokenize everywhere below.

#[cfg(windows)]
fn launch(exe_path: &str, cmd_line: &str, classification: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    // Args = everything in the recorded command line after argv[0].
    let mut args = crate::tokenize(cmd_line);
    if !args.is_empty() {
        args.remove(0); // strip argv[0]
    }

    // IDEs (VSCode, JetBrains, etc.) are Electron/JVM apps that spawn many child
    // processes with internal flags like --type=renderer, --no-sandbox,
    // --renderer-client-id=N, etc.  If we captured a renderer/helper process instead
    // of the main process, blindly passing those flags back creates blank files and
    // broken windows.  Safe rule: for IDEs, keep only args that look like filesystem
    // paths (don't start with '-') — those are the workspace/folder/file args we want.
    let cat = Category::from_str(classification);
    if cat == Category::Ide {
        args.retain(|a| !a.starts_with('-'));
    }

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    Command::new(exe_path)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
        .spawn()
        .map(|_child| ()) // detach: drop the handle, let it run independently
        .map_err(|e| e.to_string())
}

// ── Live window inspection ────────────────────────────────────────────────────────────

#[cfg(windows)]
struct LiveWindow {
    hwnd: isize,
    title: String,
    exe: String,
}

#[cfg(windows)]
fn live_windows() -> Vec<LiveWindow> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };

    unsafe extern "system" fn cb(hwnd: HWND, data: LPARAM) -> BOOL {
        let out = &mut *(data.0 as *mut Vec<LiveWindow>);
        if !IsWindowVisible(hwnd).as_bool() {
            return TRUE;
        }
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
        out.push(LiveWindow {
            hwnd: hwnd.0 as isize,
            title,
            exe: exe_path_for_pid(pid),
        });
        TRUE
    }

    let mut out: Vec<LiveWindow> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut out as *mut Vec<LiveWindow> as isize));
    }
    out
}

/// Best-effort title match: exact first, then either-contains.
#[cfg(windows)]
fn match_window(live: &[LiveWindow], saved_title: &str) -> Option<isize> {
    if saved_title.is_empty() {
        return None;
    }
    if let Some(w) = live.iter().find(|w| w.title == saved_title) {
        return Some(w.hwnd);
    }
    live.iter()
        .find(|w| w.title.contains(saved_title) || saved_title.contains(&w.title))
        .map(|w| w.hwnd)
}

/// Title match that skips already-claimed windows. Exact title wins over a
/// substring match, and substring matches require a few characters so a tiny
/// live title (e.g. "1") doesn't greedily swallow unrelated saved windows.
#[cfg(windows)]
fn match_window_titled(
    live: &[LiveWindow],
    saved_title: &str,
    consumed: &std::collections::HashSet<isize>,
) -> Option<isize> {
    if saved_title.is_empty() {
        return None;
    }
    if let Some(w) = live
        .iter()
        .find(|w| !consumed.contains(&w.hwnd) && w.title == saved_title)
    {
        return Some(w.hwnd);
    }
    live.iter()
        .find(|w| {
            !consumed.contains(&w.hwnd)
                && w.title.len() >= 4
                && (w.title.contains(saved_title) || saved_title.contains(&w.title))
        })
        .map(|w| w.hwnd)
}

/// Fallback: claim any still-unclaimed live window owned by the same executable.
/// Used when the title has drifted since capture so a precise match is impossible.
#[cfg(windows)]
fn match_window_by_exe(
    live: &[LiveWindow],
    saved_exe: &str,
    consumed: &std::collections::HashSet<isize>,
) -> Option<isize> {
    if saved_exe.is_empty() {
        return None;
    }
    let stem = exe_stem(saved_exe);
    live.iter()
        .find(|w| !consumed.contains(&w.hwnd) && !w.exe.is_empty() && exe_stem(&w.exe) == stem)
        .map(|w| w.hwnd)
}

#[cfg(windows)]
fn exe_path_for_pid(pid: u32) -> String {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::core::PWSTR;

    if pid == 0 {
        return String::new();
    }
    unsafe {
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(_) => return String::new(),
        };
        let mut buf = vec![0u16; 512];
        let mut size = buf.len() as u32;
        let res = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        if res.is_ok() {
            String::from_utf16_lossy(&buf[..size as usize])
        } else {
            String::new()
        }
    }
}

/// Count of running instances per lowercased full exe path.
/// Used to determine how many new instances we need to launch per app.
#[cfg(windows)]
fn running_exe_paths_counted() -> std::collections::HashMap<String, usize> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        ProcessRefreshKind::new().with_exe(UpdateKind::Always),
    );
    let mut map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for p in sys.processes().values() {
        if let Some(exe) = p.exe() {
            *map.entry(exe.to_string_lossy().to_ascii_lowercase()).or_insert(0) += 1;
        }
    }
    map
}

// ── Window manipulation ───────────────────────────────────────────────────────────────

#[cfg(windows)]
fn apply_geometry(hwnd_raw: isize, target: &WindowInfo) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_NOZORDER, SW_MAXIMIZE, SW_MINIMIZE,
        SW_RESTORE,
    };

    let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
    unsafe {
        // Restore first so SetWindowPos applies to the normal rect, then re-apply state.
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetWindowPos(
            hwnd,
            HWND_TOP,
            target.position.x,
            target.position.y,
            target.size.width as i32,
            target.size.height as i32,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        match target.state.as_str() {
            "maximized" => {
                let _ = ShowWindow(hwnd, SW_MAXIMIZE);
            }
            "minimized" => {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            }
            _ => {}
        }
    }
}

#[cfg(windows)]
fn focus_window(hwnd_raw: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Close every visible window whose owning executable is NOT part of `snapshot`.
///
/// Uses WM_CLOSE — exactly what clicking the title-bar X does — so apps with unsaved
/// work get the chance to raise their own "Save changes?" prompt. Nothing is ever
/// force-killed (consistent with the non-destructive macro-layer rule). Returns
/// `(closed, leftover)`: windows that actually closed, and those that ignored the
/// request within a short grace period so they can be reported honestly.
#[cfg(windows)]
fn close_windows_not_in_snapshot(snapshot: &Snapshot, ignore_list: &[String]) -> (Vec<String>, Vec<String>) {
    use std::collections::HashSet;
    use std::time::Duration;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

    // Apps that belong to the snapshot — these stay open.
    let keep: HashSet<String> = snapshot
        .processes
        .iter()
        .filter(|p| !p.exe_path.is_empty())
        .map(|p| exe_stem(&p.exe_path))
        .collect();

    // System-critical processes + user-ignored processes + this app's own window.
    let mut protected: HashSet<String> = crate::config::SYSTEM_PROTECTED
        .iter()
        .map(|s| s.to_string())
        .collect();
    for stem in ignore_list {
        protected.insert(stem.clone());
    }
    if let Ok(me) = std::env::current_exe() {
        protected.insert(exe_stem(&me.to_string_lossy()));
    }

    let targets: Vec<LiveWindow> = live_windows()
        .into_iter()
        .filter(|w| {
            let stem = exe_stem(&w.exe);
            !w.exe.is_empty() && !keep.contains(&stem) && !protected.contains(&stem)
        })
        .collect();

    if targets.is_empty() {
        return (vec![], vec![]);
    }

    // hwnd, display label
    let mut requested: Vec<(isize, String)> = vec![];
    for w in &targets {
        let label = if w.title.is_empty() {
            format!("({})", exe_stem(&w.exe))
        } else {
            format!("'{}' ({})", w.title, exe_stem(&w.exe))
        };
        let hwnd = HWND(w.hwnd as *mut core::ffi::c_void);
        let posted = unsafe { PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) };
        if posted.is_ok() {
            requested.push((w.hwnd, label));
        }
    }

    // Give apps a moment to close (or to raise a save prompt).
    std::thread::sleep(Duration::from_millis(1500));

    let still_live = live_windows();
    let mut closed: Vec<String> = vec![];
    let mut leftover: Vec<String> = vec![];
    for (hwnd, label) in requested {
        if still_live.iter().any(|w| w.hwnd == hwnd) {
            leftover.push(format!("{label} — still open (it may be asking to save, or refused to close)"));
        } else {
            closed.push(label);
        }
    }
    (closed, leftover)
}

/// Build a restore-aware launch command for a terminal process by matching it
/// to a saved `TerminalSession`. Writes a temp restore script so the terminal
/// opens at the right CWD and shows recent command history.
#[cfg(windows)]
/// Build a restore-aware launch command for a terminal.
/// `window_title` — the snapshot window title to use for best-match session selection.
///                  Pass `None` to fall back to sequential index order.
/// `index`        — bumped on each call so successive calls pick different sessions.
fn terminal_restore_cmd(
    snapshot: &Snapshot,
    proc_: &crate::ProcessInfo,
    index: &mut usize,
    window_title: Option<&str>,
) -> Option<String> {
    if snapshot.terminal_sessions.is_empty() {
        return None;
    }

    let sessions_for_exe: Vec<(usize, &crate::TerminalSession)> = snapshot
        .terminal_sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            let shell_exe = match s.shell.as_str() {
                "powershell" => "powershell",
                "pwsh" => "pwsh",
                "cmd" => "cmd",
                "windows_terminal" => "windowsterminal",
                _ => "",
            };
            !shell_exe.is_empty() && exe_stem(&proc_.exe_path) == shell_exe
        })
        .collect();

    if sessions_for_exe.is_empty() {
        return None;
    }

    // Try to match by window title first (exact, then substring).
    let session = if let Some(title) = window_title.filter(|t| !t.is_empty()) {
        sessions_for_exe
            .iter()
            .find(|(_, s)| s.window_title == title)
            .or_else(|| {
                sessions_for_exe.iter().find(|(_, s)| {
                    s.window_title.len() >= 4
                        && title.len() >= 4
                        && (s.window_title.contains(title) || title.contains(s.window_title.as_str()))
                })
            })
            .map(|(_, s)| *s)
            // Fall back to sequential index if title didn't match any session.
            .or_else(|| sessions_for_exe.get(*index).map(|(_, s)| *s))
    } else {
        sessions_for_exe.get(*index).map(|(_, s)| *s)
    };

    *index += 1;

    let session = session?;
    let temp_dir = std::env::temp_dir().join("pc_snapshot_restore");
    let _ = std::fs::create_dir_all(&temp_dir);

    crate::terminal::terminal_launch_cmd(&proc_.exe_path, session, &temp_dir, *index)
}

/// Captured active-tab URLs for a browser, in snapshot order, de-duplicated.
/// Hint format: `browser_tab:<exe_stem>:<url>` (the stem has no ':' so the
/// remainder is the full URL, colons and all).
#[cfg(windows)]
fn browser_urls_for(snapshot: &Snapshot, stem: &str) -> Vec<String> {
    let prefix = format!("browser_tab:{stem}:");
    let mut urls: Vec<String> = vec![];
    for h in &snapshot.restore_hints {
        if let Some(url) = h.strip_prefix(&prefix) {
            let url = url.to_string();
            if !url.is_empty() && !urls.contains(&url) {
                urls.push(url);
            }
        }
    }
    urls
}

// ── Non-Windows fallback ──────────────────────────────────────────────────────────────

#[cfg(not(windows))]
pub fn restore_desktop(_snapshot: &Snapshot, _close_others: bool, _ignore_list: &[String]) -> RestoreResult {
    RestoreResult {
        success: false,
        message: "Restore engine is only implemented on Windows".to_string(),
        failed_items: vec![],
        warnings: vec![],
        closed_items: vec![],
    }
}
