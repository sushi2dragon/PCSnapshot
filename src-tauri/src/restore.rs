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
pub fn restore_desktop(
    snapshot: &Snapshot,
    close_others: bool,
    ignore_list: &[String],
    companion_managed_browsers: bool,
) -> RestoreResult {
    use std::collections::{HashMap, HashSet};
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
            if is_vscode_family(&p.exe_path) {
                return false;
            }
            // Terminals are restored from captured shell sessions (process-based, see
            // restore_terminal_sessions), not from window enumeration — skip them here.
            if Category::from_str(&p.classification) == Category::Terminal
                || classify::classify(&p.exe_path, true) == Category::Terminal
            {
                return false;
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

    let mut terminal_used: HashMap<String, Vec<usize>> = HashMap::new();
    let mut extra_window_exes_handled: HashSet<String> = HashSet::new();

    for proc_ in &launch_list {
        if proc_.exe_path.is_empty() {
            failed_items.push(format!(
                "{}: no executable path recorded",
                display_name(proc_)
            ));
            continue;
        }

        let cat = Category::from_str(&proc_.classification);
        // Detect browsers by exe too — a *foreground* browser is classified "foreground".
        let is_browser_proc =
            cat.is_browser() || classify::classify(&proc_.exe_path, true).is_browser();
        // Browsers: reopen the exact tabs captured at snapshot time (the active tab of
        // each window, via `browser_tab` hints) rather than the session-dependent
        // Ctrl+Shift+T trick. All captured URLs open as tabs in one launch. With no
        // captured URLs we launch plainly so the browser restores its own session.
        let is_terminal_proc = cat == Category::Terminal
            || classify::classify(&proc_.exe_path, true) == Category::Terminal;

        let (launch_exe, launch_cmd) = if is_browser_proc {
            let cmd = if companion_managed_browsers {
                String::new()
            } else {
                let urls = browser_urls_for(snapshot, &exe_stem(&proc_.exe_path));
                if urls.is_empty() {
                    String::new()
                } else {
                    let quoted: Vec<String> = urls.iter().map(|u| format!("\"{u}\"")).collect();
                    format!("\"{}\" {}", proc_.exe_path, quoted.join(" "))
                }
            };
            (proc_.exe_path.clone(), cmd)
        } else if is_terminal_proc {
            terminal_restore_cmd(snapshot, proc_, &mut terminal_used, None)
                .map(|command| (command.exe_path, command.cmd_line))
                .unwrap_or_else(|| (proc_.exe_path.clone(), proc_.cmd_line.clone()))
        } else {
            (proc_.exe_path.clone(), proc_.cmd_line.clone())
        };

        // Communication apps (Teams, Slack, …) are tray-resident: the user may have
        // "closed" them but the process is still running hidden. Skip launching but
        // make sure we bring their window to the foreground in the reposition pass.
        // (is_running was true → they're already in `running`, not in launch_list,
        //  but if somehow they are here, just launch normally.)

        match launch(&launch_exe, &launch_cmd, &proc_.classification) {
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
                let exe_key = proc_.exe_path.to_ascii_lowercase();
                if !is_browser_proc
                    && !cat.is_communication()
                    && extra_window_exes_handled.insert(exe_key)
                {
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
                    let planned_launches = launch_list
                        .iter()
                        .filter(|candidate| {
                            candidate.exe_path.eq_ignore_ascii_case(&proc_.exe_path)
                        })
                        .count();
                    let extra_windows =
                        extra_window_launch_count(snap_count, pre_live, planned_launches);

                    if extra_windows > 0 {
                        // Collect office_extra_file hints for this exe stem.
                        let hint_prefix =
                            format!("office_extra_file:{}:", exe_stem(&proc_.exe_path));
                        let extra_files: Vec<String> = snapshot
                            .restore_hints
                            .iter()
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

    // Terminals: launch each captured interactive shell directly at its saved cwd +
    // history. Windows 11 re-hosts the launched shell in Windows Terminal. Count-based
    // dedup avoids duplicating a terminal the user still has open. Window position and
    // tab grouping are intentionally not restored.
    let mut terminal_closed: Vec<String> = vec![];
    restore_terminal_sessions(
        &snapshot.terminal_sessions,
        close_others,
        &mut failed_items,
        &mut terminal_closed,
    );

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
                if is_browser
                    || Category::from_str(&proc_.classification).is_communication()
                    || is_terminal
                    || is_vscode_family(&proc_.exe_path)
                {
                    continue;
                }

                let deficit = snap_count - pre_live;
                for i in 0..deficit {
                    if let Err(e) = launch(&proc_.exe_path, &proc_.cmd_line, &proc_.classification)
                    {
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

        // Terminals are now restored from captured shell sessions (see
        // restore_terminal_sessions); the window-based reconciliation is retired
        // because Windows exposes no reliable terminal-window↔shell mapping.
        let snap_terminal_windows: Vec<&WindowInfo> = Vec::new();

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
            let mut claimed_live: std::collections::HashSet<isize> =
                std::collections::HashSet::new();
            let mut matched_snap: std::collections::HashSet<usize> =
                std::collections::HashSet::new();

            for (si, sw) in snap_terminal_windows.iter().enumerate() {
                // Exact match first.
                if let Some(lw) = live_term_wins
                    .iter()
                    .find(|lw| !claimed_live.contains(&lw.hwnd) && lw.title == sw.title)
                {
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
                let protected_pids = self_and_ancestor_pids();
                let mut requested: Vec<(isize, u32, String)> = vec![];
                for lw in &live_term_wins {
                    if !claimed_live.contains(&lw.hwnd) && !protected_pids.contains(&lw.pid) {
                        let hwnd = HWND(lw.hwnd as *mut core::ffi::c_void);
                        if unsafe { PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok() {
                            requested.push((
                                lw.hwnd,
                                lw.pid,
                                format!("'{}' ({})", lw.title, exe_stem(&lw.exe)),
                            ));
                        }
                    }
                }
                // Grace period for WM_CLOSE, then force-close any that stalled on a
                // save prompt — only report the ones that actually went away.
                if !requested.is_empty() {
                    let closing: std::collections::HashSet<isize> =
                        requested.iter().map(|(h, _, _)| *h).collect();
                    let requested_hp: Vec<(isize, u32)> =
                        requested.iter().map(|(h, p, _)| (*h, *p)).collect();
                    let still_open =
                        force_close_stragglers(&requested_hp, &closing, &protected_pids, 600);
                    for (hwnd, _pid, label) in requested {
                        if !still_open.contains(&hwnd) {
                            term_closed.push(label);
                        }
                    }
                }
            }

            // Launch snapshot terminals that have no live match — but never more per
            // exe than are actually missing (captured count − currently-live count).
            // Title-based matching can fail to recognize a running terminal whose title
            // drifted since capture (a terminal's title tracks its running command), which
            // would otherwise launch a duplicate of a window that's already open.
            let mut launched_per_exe: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for (si, sw) in snap_terminal_windows.iter().enumerate() {
                if matched_snap.contains(&si) {
                    continue;
                }
                let snap_count = snap_terminal_windows
                    .iter()
                    .filter(|w| w.exe_path.eq_ignore_ascii_case(&sw.exe_path))
                    .count();
                let live_count = live_term_wins
                    .iter()
                    .filter(|w| w.exe.eq_ignore_ascii_case(&sw.exe_path))
                    .count();
                let launched = launched_per_exe
                    .entry(sw.exe_path.to_ascii_lowercase())
                    .or_insert(0);
                if *launched >= snap_count.saturating_sub(live_count) {
                    continue; // every genuinely-missing window for this exe already launched
                }
                *launched += 1;
                if let Some(proc_) = snapshot
                    .processes
                    .iter()
                    .find(|p| p.exe_path.eq_ignore_ascii_case(&sw.exe_path))
                {
                    if crate::config::is_ignored(&exe_stem(&proc_.exe_path), ignore_list) {
                        continue;
                    }
                    let command =
                        terminal_restore_cmd(snapshot, proc_, &mut terminal_used, Some(&sw.title));
                    let (launch_exe, cmd) = command
                        .map(|command| (command.exe_path, command.cmd_line))
                        .unwrap_or_else(|| (proc_.exe_path.clone(), proc_.cmd_line.clone()));
                    if let Err(e) = launch(&launch_exe, &cmd, &proc_.classification) {
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

    let mut vscode_closed: Vec<String> = vec![];
    {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};
        let targets: Vec<&str> = snapshot
            .restore_hints
            .iter()
            .filter_map(|h| h.strip_prefix("vscode_folder:"))
            .collect();
        let live: Vec<&LiveWindow> = pre_launch_wins
            .iter()
            .filter(|w| is_vscode_family(&w.exe))
            .collect();
        let mut claimed = std::collections::HashSet::new();
        let mut matched = std::collections::HashSet::new();
        for (i, path) in targets.iter().enumerate() {
            let name = std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase());
            if let Some(w) = live.iter().find(|w| {
                !claimed.contains(&w.hwnd)
                    && crate::context::vscode_folder_from_title(&w.title)
                        .map(|n| Some(n.to_ascii_lowercase()) == name)
                        .unwrap_or(false)
            }) {
                claimed.insert(w.hwnd);
                matched.insert(i);
            }
        }
        if close_others {
            let protected = self_and_ancestor_pids();
            let mut requested: Vec<(isize, u32, String)> = vec![];
            for w in &live {
                if !claimed.contains(&w.hwnd) && !protected.contains(&w.pid) {
                    let hwnd = HWND(w.hwnd as *mut core::ffi::c_void);
                    if unsafe { PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok() {
                        requested.push((
                            w.hwnd,
                            w.pid,
                            format!("'{}' ({})", w.title, exe_stem(&w.exe)),
                        ));
                    }
                }
            }
            // A multi-window editor usually shares one process, so the collateral
            // guard in force_close_stragglers will decline to kill when a kept
            // window shares the PID — safe, and reported honestly as not-closed.
            if !requested.is_empty() {
                let closing: std::collections::HashSet<isize> =
                    requested.iter().map(|(h, _, _)| *h).collect();
                let requested_hp: Vec<(isize, u32)> =
                    requested.iter().map(|(h, p, _)| (*h, *p)).collect();
                let still_open =
                    force_close_stragglers(&requested_hp, &closing, &protected, 800);
                for (hwnd, _pid, label) in requested {
                    if !still_open.contains(&hwnd) {
                        vscode_closed.push(label);
                    }
                }
            }
        }
        if let Some(proc_) = snapshot
            .processes
            .iter()
            .find(|p| is_vscode_family(&p.exe_path))
        {
            for (i, path) in targets.iter().enumerate() {
                if !matched.contains(&i) {
                    let cmd = format!("\"{}\" \"{}\"", proc_.exe_path, path);
                    if let Err(e) = launch(&proc_.exe_path, &cmd, "ide") {
                        failed_items.push(format!(
                            "{}: folder '{}' failed to launch ({e})",
                            display_name(proc_),
                            path
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
        if running
            .get(&proc_.exe_path.to_ascii_lowercase())
            .copied()
            .unwrap_or(0)
            == 0
        {
            continue; // not running → it will be handled by the normal launch_list above
        }
        // Already running in the tray — just bring its window forward via ShowWindow.
        // We don't relaunch; instead we let the reposition pass surface the window when
        // it matches by title. Nudge it by posting a "restore from tray" signal via
        // the window's taskbar button (best-effort, no crash if it fails).
        let live = live_windows();
        for win in snapshot
            .windows
            .iter()
            .filter(|w| !w.exe_path.is_empty() && w.exe_path.eq_ignore_ascii_case(&proc_.exe_path))
        {
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
    // Explorer folders are restored through ShellWindows, never by launching or
    // touching the protected explorer.exe process. Waiting here overlaps with
    // the startup time of apps launched above.
    let explorer_outcome = crate::explorer::restore_windows(
        &snapshot.explorer_windows,
        close_others && snapshot.schema_version >= 4,
    );
    failed_items.extend(explorer_outcome.failed_items);

    let mut pending: Vec<&WindowInfo> = snapshot
        .windows
        .iter()
        .filter(|window| {
            (!companion_managed_browsers
                || !classify::classify(&window.exe_path, true).is_browser())
                // Terminals are session-restored, not repositioned — Windows exposes no
                // reliable way to map a terminal window back to its captured shell.
                && classify::classify(&window.exe_path, true) != Category::Terminal
        })
        .collect();
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
    let mut warnings = explorer_outcome.warnings;
    warnings.extend(pending.iter().map(|w| unplaced_reason(w)));

    // ── 4. Optional clean-up: close windows that aren't part of this snapshot ──────────
    // Terminals closed during reconciliation (clean restore only) are reported too.
    let mut closed_items = term_closed;
    closed_items.extend(terminal_closed);
    closed_items.extend(vscode_closed);
    closed_items.extend(explorer_outcome.closed_items);
    if close_others {
        let (closed, leftover) =
            close_windows_not_in_snapshot(snapshot, ignore_list, companion_managed_browsers);
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
        if let Some(w) = live
            .iter()
            .find(|w| exe_stem(&w.exe) == exe_stem(&fg.exe_path))
        {
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
    let title = if w.title.is_empty() {
        "(untitled window)"
    } else {
        &w.title
    };
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

fn is_vscode_family(exe_path: &str) -> bool {
    matches!(
        exe_stem(exe_path).as_str(),
        "code" | "code-insiders" | "cursor"
    )
}

fn exe_stem(exe_path: &str) -> String {
    let last = exe_path
        .rsplit(|c| c == '\\' || c == '/')
        .next()
        .unwrap_or(exe_path);
    last.strip_suffix(".exe")
        .or_else(|| last.strip_suffix(".EXE"))
        .unwrap_or(last)
        .to_ascii_lowercase()
}

/// Public wrapper so other modules can derive the same lowercase exe stem.
pub fn exe_stem_pub(exe_path: &str) -> String {
    exe_stem(exe_path)
}

/// Store/UWP apps live under a locked-down `%ProgramFiles%\WindowsApps` path that
/// CreateProcess cannot execute directly — spawning the raw exe returns "Access is
/// denied" (os error 5). Map the known ones to their App Execution Alias, which is
/// on PATH and launches correctly. Returns `None` for normally-launchable exes.
#[cfg(windows)]
fn store_app_launch_alias(exe_path: &str) -> Option<&'static str> {
    if exe_stem(exe_path).eq_ignore_ascii_case("windowsterminal") {
        Some("wt.exe")
    } else {
        None
    }
}

/// Lowercase exe stems of every app that currently owns a visible window.
/// Used to compare the live desktop against saved snapshots.
#[cfg(windows)]
pub fn current_app_set() -> std::collections::HashSet<String> {
    live_windows()
        .iter()
        .filter(|w| !w.exe.is_empty())
        .map(|w| exe_stem(&w.exe))
        .filter(|stem| !crate::config::SYSTEM_PROTECTED.contains(&stem.as_str()))
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

    // Store/UWP apps (e.g. Windows Terminal) can't be CreateProcess'd from their
    // locked-down %ProgramFiles%\WindowsApps path (Access denied / os error 5).
    // Launch via the app's execution alias, which is on PATH, instead.
    let effective_exe = store_app_launch_alias(exe_path).unwrap_or(exe_path);

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

    Command::new(effective_exe)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)
        .spawn()
        .map(|_child| ()) // detach: drop the handle, let it run independently
        .map_err(|e| e.to_string())
}

/// Launch an interactive shell so it opens a visible terminal window.
///
/// We go through `ShellExecuteW` ("open") rather than a raw `CREATE_NEW_CONSOLE`
/// `CreateProcess`. This app is a GUI process with no console of its own; when it
/// hand-rolls a new console for the shell, Win11's default-terminal handoff to Windows
/// Terminal drops that console mid-handoff and the shell tears down a second later
/// (the "window flashes then closes" bug). `ShellExecuteW` launches the shell exactly
/// as the OS does from Win+R — a real interactive console, correct DefTerm handoff — so
/// the shell stays open. It works for GUI launchers (git-bash.exe) too. `new_console`
/// is no longer needed (the OS decides hosting) but kept for signature stability.
#[cfg(windows)]
fn launch_terminal(exe_path: &str, cmd_line: &str, _new_console: bool) -> Result<(), String> {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    // Parameters = cmd_line minus its leading quoted argv[0] (the exe), keeping the
    // original quoting of the rest (paths with spaces, the .ps1 path, cd targets).
    let params = params_after_argv0(cmd_line);
    let file = HSTRING::from(exe_path);
    let params_w = HSTRING::from(params.as_str());

    let result = unsafe {
        ShellExecuteW(
            HWND(std::ptr::null_mut()),
            w!("open"),
            PCWSTR(file.as_ptr()),
            PCWSTR(params_w.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns an HINSTANCE; > 32 means success, ≤ 32 is an error code.
    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(format!("ShellExecuteW failed (code {code})"))
    }
}

/// The parameter string for `ShellExecuteW`: everything after the leading quoted `"exe"`
/// token in a restore cmd_line, with the remaining args' quoting preserved.
#[cfg(windows)]
fn params_after_argv0(cmd_line: &str) -> String {
    let s = cmd_line.trim_start();
    if let Some(rest) = s.strip_prefix('"') {
        if let Some(close) = rest.find('"') {
            return rest[close + 1..].trim_start().to_string();
        }
    }
    s.split_once(char::is_whitespace)
        .map(|(_, r)| r.to_string())
        .unwrap_or_default()
}

/// The reconcile decision for a terminal restore, computed by `plan_terminal_reconcile`
/// and executed by `restore_terminal_sessions`. Split out so the matching logic can be
/// unit-tested without spawning or killing real processes.
#[cfg(windows)]
struct TerminalPlan {
    /// Indices into `sessions` that have no matching live shell and must be relaunched.
    launch: Vec<usize>,
    /// PIDs of live shells that match no captured session — closed on a clean restore.
    close_pids: Vec<u32>,
}

/// Reconcile the captured terminal sessions against the shells currently open.
///
/// A captured session and a live shell are "the same terminal" when they share a shell
/// type *and* a working directory (folder compared case-insensitively, trailing
/// separators ignored). For each captured session:
///   • an identical live shell exists → keep it, don't relaunch (claims that shell);
///   • otherwise → relaunch the session fresh at its saved cwd + history.
/// On a clean restore (`close_others`), every live shell left unclaimed — i.e. one the
/// snapshot doesn't contain — is marked for closing, except protected PIDs (this app and
/// its launching terminal). A plain restore never closes anything.
///
/// Note: PowerShell's process CWD reads as its *launch* dir, not where it `cd`'d, so a
/// PS session captured with a real cwd usually won't match a live PS and will relaunch —
/// intended (a fresh window at the right dir beats keeping one at the wrong dir). cmd and
/// git-bash update their real process CWD, so those match exactly.
#[cfg(windows)]
fn plan_terminal_reconcile(
    sessions: &[crate::TerminalSession],
    live: &[crate::terminal::RunningShell],
    protected: &std::collections::HashSet<u32>,
    close_others: bool,
) -> TerminalPlan {
    use std::collections::HashSet;

    let norm = |s: &str| s.trim_end_matches('\\').to_ascii_lowercase();
    let mut claimed: HashSet<u32> = HashSet::new();
    let mut launch = Vec::new();

    for (i, s) in sessions.iter().enumerate() {
        let matched = live.iter().find(|r| {
            !claimed.contains(&r.pid)
                && !s.cwd.is_empty()
                && r.shell.eq_ignore_ascii_case(&s.shell)
                && norm(&r.cwd) == norm(&s.cwd)
        });
        match matched {
            Some(r) => {
                claimed.insert(r.pid); // unchanged terminal already open — keep it
            }
            None => launch.push(i),
        }
    }

    let close_pids = if close_others {
        live.iter()
            .filter(|r| !claimed.contains(&r.pid) && !protected.contains(&r.pid))
            .map(|r| r.pid)
            .collect()
    } else {
        Vec::new()
    };

    TerminalPlan { launch, close_pids }
}

/// Restore terminals by reconciling the captured interactive-shell sessions against the
/// shells currently open (see `plan_terminal_reconcile`). Unchanged terminals are left
/// alone; missing ones are relaunched directly at their saved cwd (and history, for
/// PowerShell/bash) — Win11 re-hosts each in Windows Terminal. On a clean restore, live
/// terminals the snapshot doesn't contain are closed so the desktop ends up with exactly
/// the captured set. Window position and tab layout are not restored (no reliable OS map).
#[cfg(windows)]
fn restore_terminal_sessions(
    sessions: &[crate::TerminalSession],
    close_others: bool,
    failed_items: &mut Vec<String>,
    closed_items: &mut Vec<String>,
) {
    if sessions.is_empty() && !close_others {
        return;
    }

    let live = crate::terminal::running_interactive_shells();
    let protected = self_and_ancestor_pids();
    let plan = plan_terminal_reconcile(sessions, &live, &protected, close_others);

    if !plan.launch.is_empty() {
        let temp_dir = std::env::temp_dir().join("pc_snapshot_restore");
        let _ = std::fs::create_dir_all(&temp_dir);
        for (n, &i) in plan.launch.iter().enumerate() {
            let s = &sessions[i];
            let Some(cmd) = crate::terminal::terminal_launch_cmd(&s.exe, s, &temp_dir, n + 1) else {
                continue; // e.g. a shell with no saved cwd — nothing to restore
            };
            let new_console = !s.shell.eq_ignore_ascii_case("git_bash");
            if let Err(e) = launch_terminal(&cmd.exe_path, &cmd.cmd_line, new_console) {
                failed_items.push(format!(
                    "Terminal ({}) could not be launched ({e})",
                    s.shell
                ));
            }
        }
    }

    // Clean restore: close the terminals that aren't part of this snapshot. Terminating
    // the shell process is the reliable close — a console window is owned by conhost/WT,
    // not the shell, so it has no window of its own to WM_CLOSE; when the shell exits its
    // host window closes with it. This discards any unsaved state in that terminal.
    for r in &live {
        if plan.close_pids.contains(&r.pid) && terminate_pid(r.pid) {
            closed_items.push(if r.cwd.is_empty() {
                format!("terminal ({})", r.shell)
            } else {
                format!("terminal ({} @ {})", r.shell, r.cwd)
            });
        }
    }
}

// ── Live window inspection ────────────────────────────────────────────────────────────

#[cfg(windows)]
#[derive(Clone)]
struct LiveWindow {
    hwnd: isize,
    title: String,
    exe: String,
    pid: u32,
}

#[cfg(windows)]
fn live_windows() -> Vec<LiveWindow> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsWindowVisible,
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
            pid,
        });
        TRUE
    }

    let mut out: Vec<LiveWindow> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut out as *mut Vec<LiveWindow> as isize));
    }
    out
}

/// PIDs of this process and every ancestor (parent shell, its terminal, …).
/// Any window owned by one of these must never be closed during a restore —
/// otherwise a "close others" restore that omits the launching terminal would
/// kill the app's own process tree. Best-effort: returns at least our own PID.
#[cfg(windows)]
fn self_and_ancestor_pids() -> std::collections::HashSet<u32> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    let mut pids = std::collections::HashSet::new();
    let me = std::process::id();
    pids.insert(me);

    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, ProcessRefreshKind::new());

    // Walk parent links up to the root. Guard against cycles/self-parenting.
    let mut current = Pid::from_u32(me);
    for _ in 0..64 {
        let Some(parent) = sys.process(current).and_then(|p| p.parent()) else {
            break;
        };
        let ppid = parent.as_u32();
        if !pids.insert(ppid) {
            break;
        }
        current = parent;
    }
    pids
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
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

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
            *map.entry(exe.to_string_lossy().to_ascii_lowercase())
                .or_insert(0) += 1;
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

/// Bring a visible window owned by `exe_path` to the foreground after a
/// selective restore. Selecting one app is an explicit request to surface it.
#[cfg(windows)]
pub fn focus_app(exe_path: &str) {
    if let Some(window) = live_windows()
        .into_iter()
        .find(|window| window.exe.eq_ignore_ascii_case(exe_path))
    {
        focus_window(window.hwnd);
    }
}

#[cfg(not(windows))]
pub fn focus_app(_exe_path: &str) {}

/// Close every visible window whose owning executable is NOT part of `snapshot`.
///
/// Two-stage close. First sends WM_CLOSE — exactly what clicking the title-bar X
/// does — giving each app a brief chance to shut down cleanly. After a grace
/// period, any *targeted* window still alive is blocked (a "Save changes?" /
/// "Close all tabs?" dialog, or an app ignoring WM_CLOSE) and would otherwise
/// stall Start Fresh / clean Restore, so its owning process is force-terminated —
/// discarding unsaved work in that app. To avoid collateral, a PID is only killed
/// when *every* one of its still-live windows was a close target; a process that
/// also owns a window we're keeping is left alone and reported as leftover.
/// Returns `(closed, leftover)` so both outcomes can be reported honestly.
#[cfg(windows)]
fn close_windows_not_in_snapshot(
    snapshot: &Snapshot,
    ignore_list: &[String],
    companion_managed_browsers: bool,
) -> (Vec<String>, Vec<String>) {
    use std::collections::{HashMap, HashSet};
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

    // Apps that belong to the snapshot — their windows stay open (subject to the
    // surplus rule below).
    let keep: HashSet<String> = snapshot
        .processes
        .iter()
        .filter(|p| !p.exe_path.is_empty())
        .map(|p| exe_stem(&p.exe_path))
        .collect();

    // How many windows each in-snapshot app had, with their titles — used to
    // close *surplus* windows of an app that is in the snapshot but now has more
    // windows open than were captured (e.g. a browser: 1 window captured, 3 open
    // now → close the 2 extras). Without this, closing was exe-level: any app in
    // the snapshot kept ALL its windows no matter how many were opened after.
    let mut snap_titles_by_exe: HashMap<String, Vec<String>> = HashMap::new();
    for w in &snapshot.windows {
        if w.exe_path.is_empty() {
            continue;
        }
        snap_titles_by_exe
            .entry(exe_stem(&w.exe_path))
            .or_default()
            .push(w.title.clone());
    }

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

    // Never close our own window tree (this app, its launching shell/terminal).
    let protected_pids = self_and_ancestor_pids();

    let live = live_windows();

    // Set A: windows whose exe is not part of the snapshot at all. Terminals are
    // excluded here and handled exclusively by the process-based terminal pass (which
    // reconciles by shell+cwd and terminates the shell); otherwise this window-level
    // pass could close the conhost/WT window hosting a terminal that pass just relaunched.
    let mut targets: Vec<LiveWindow> = live
        .iter()
        .filter(|w| {
            let stem = exe_stem(&w.exe);
            !w.exe.is_empty()
                && !keep.contains(&stem)
                && classify::classify(&w.exe, true) != Category::Terminal
                && !(companion_managed_browsers && classify::classify(&w.exe, true).is_browser())
                && !protected.contains(&stem)
                && !protected_pids.contains(&w.pid)
        })
        .cloned()
        .collect();

    // Set B: surplus windows of an app that IS in the snapshot. Keep the windows
    // that best match the saved titles; close the rest.
    for (stem, snap_titles) in &snap_titles_by_exe {
        if snap_titles.is_empty() || protected.contains(stem) {
            continue;
        }
        // Terminals are reconciled/closed by the dedicated terminal pass earlier
        // in the restore; leave them to it so the two passes can't double-close
        // or double-report the same window.
        if classify::classify(stem, true) == Category::Terminal || is_vscode_family(stem) {
            continue;
        }
        if companion_managed_browsers && classify::classify(stem, true).is_browser() {
            continue;
        }
        let live_of: Vec<&LiveWindow> = live
            .iter()
            .filter(|w| {
                !w.exe.is_empty() && exe_stem(&w.exe) == *stem && !protected_pids.contains(&w.pid)
            })
            .collect();
        let live_titles: Vec<String> = live_of.iter().map(|w| w.title.clone()).collect();
        for idx in surplus_close_indices(snap_titles, &live_titles) {
            targets.push(live_of[idx].clone());
        }
    }

    // A window could qualify for both sets in odd cases; close each at most once.
    targets.sort_by_key(|w| w.hwnd);
    targets.dedup_by_key(|w| w.hwnd);

    if targets.is_empty() {
        return (vec![], vec![]);
    }

    // hwnd, pid, display label
    let target_hwnds: HashSet<isize> = targets.iter().map(|w| w.hwnd).collect();
    let mut requested: Vec<(isize, u32, String)> = vec![];
    for w in &targets {
        let label = if w.title.is_empty() {
            format!("({})", exe_stem(&w.exe))
        } else {
            format!("'{}' ({})", w.title, exe_stem(&w.exe))
        };
        let hwnd = HWND(w.hwnd as *mut core::ffi::c_void);
        let posted = unsafe { PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)) };
        if posted.is_ok() {
            requested.push((w.hwnd, w.pid, label));
        }
    }

    // Wait for clean exits, then force-terminate whatever's still blocked on a
    // save/confirm dialog. `target_hwnds` is the full set we asked to close, so
    // the helper's collateral guard never kills a process that also owns a kept
    // window.
    let requested_hp: Vec<(isize, u32)> = requested.iter().map(|(h, p, _)| (*h, *p)).collect();
    let still_open = force_close_stragglers(&requested_hp, &target_hwnds, &protected_pids, 1500);

    let mut closed: Vec<String> = vec![];
    let mut leftover: Vec<String> = vec![];
    for (hwnd, _pid, label) in requested {
        if still_open.contains(&hwnd) {
            leftover.push(format!(
                "{label} — still open (refused to close, and could not be force-closed safely)"
            ));
        } else {
            closed.push(label);
        }
    }
    (closed, leftover)
}

/// After a batch of windows have been sent WM_CLOSE, wait `grace_ms` for clean
/// exits, then force-terminate the process behind any that are still up — so a
/// blocking "Save changes?" / "Close all tabs?" dialog can't stall Start Fresh
/// or a clean Restore. A PID is only killed when *every* one of its still-live
/// windows is in `closing`, so a process that also owns a window we're keeping
/// (e.g. a single-process editor with one kept and one surplus window) is left
/// intact. `protected` PIDs (our own process tree) are never touched. Returns
/// the subset of `requested` hwnds that remain open after the whole attempt.
#[cfg(windows)]
fn force_close_stragglers(
    requested: &[(isize, u32)],
    closing: &std::collections::HashSet<isize>,
    protected: &std::collections::HashSet<u32>,
    grace_ms: u64,
) -> std::collections::HashSet<isize> {
    use std::collections::{HashMap, HashSet};
    use std::time::Duration;

    std::thread::sleep(Duration::from_millis(grace_ms));
    let still = live_windows();

    let mut live_by_pid: HashMap<u32, Vec<isize>> = HashMap::new();
    for w in &still {
        live_by_pid.entry(w.pid).or_default().push(w.hwnd);
    }

    let mut handled: HashSet<u32> = HashSet::new();
    let mut killed_any = false;
    for (hwnd, pid) in requested {
        if handled.contains(pid) || protected.contains(pid) {
            continue;
        }
        if !still.iter().any(|w| w.hwnd == *hwnd) {
            continue; // closed cleanly on its own
        }
        let pid_windows = live_by_pid.get(pid).map(Vec::as_slice).unwrap_or(&[]);
        if pid_windows.iter().all(|h| closing.contains(h)) && terminate_pid(*pid) {
            handled.insert(*pid);
            killed_any = true;
        }
    }

    // Let terminated processes tear down before the honest recount.
    if killed_any {
        std::thread::sleep(Duration::from_millis(400));
    }
    let final_live = if killed_any { live_windows() } else { still };
    requested
        .iter()
        .map(|(h, _)| *h)
        .filter(|h| final_live.iter().any(|w| w.hwnd == *h))
        .collect()
}

/// Force-terminate a process by PID. Best-effort: returns whether the kill
/// request succeeded. Used only to clear a window blocking Start Fresh / clean
/// Restore after a graceful WM_CLOSE was ignored — this discards unsaved work.
#[cfg(windows)]
fn terminate_pid(pid: u32) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
    unsafe {
        let handle = match OpenProcess(PROCESS_TERMINATE, false, pid) {
            Ok(h) => h,
            Err(_) => return false,
        };
        let ok = TerminateProcess(handle, 1).is_ok();
        let _ = CloseHandle(handle);
        ok
    }
}

/// Gracefully close every non-protected user window for the Start new action.
pub fn close_all_windows(ignore_list: &[String]) -> (Vec<String>, Vec<String>) {
    let empty = Snapshot {
        schema_version: 2, id: String::new(), name: String::new(), timestamp: String::new(),
        processes: vec![], windows: vec![], explorer_windows: vec![], context_clues: vec![], restore_hints: vec![],
        warnings: vec![], thumbnail_path: String::new(), terminal_sessions: vec![], browser_sessions: vec![],
    };
    let (mut closed, mut leftover) = close_windows_not_in_snapshot(&empty, ignore_list, false);
    let (explorer_closed, explorer_leftover) = crate::explorer::close_all_folder_windows();
    closed.extend(explorer_closed);
    leftover.extend(explorer_leftover);
    (closed, leftover)
}

#[cfg(not(windows))]
pub fn close_all_windows(_ignore_list: &[String]) -> (Vec<String>, Vec<String>) { (vec![], vec![]) }

/// Which live windows of an in-snapshot app to close as surplus. Keeps
/// `snap_titles.len()` windows, preferring ones whose title matches a saved
/// title; returns indices (into `live_titles`) of the windows to close. Empty
/// when the app has no more windows open than were captured.
#[cfg(windows)]
fn surplus_close_indices(snap_titles: &[String], live_titles: &[String]) -> Vec<usize> {
    let snap_count = snap_titles.len();
    if snap_count == 0 || live_titles.len() <= snap_count {
        return vec![];
    }
    // Matched windows sort first (kept); unmatched last (closed). Stable sort
    // keeps the surplus deterministic.
    let mut order: Vec<usize> = (0..live_titles.len()).collect();
    order.sort_by_key(|&i| {
        if title_matches_any(&live_titles[i], snap_titles) {
            0
        } else {
            1
        }
    });
    order.into_iter().skip(snap_count).collect()
}

/// Whether a live window title matches any saved title — exact, or a substring
/// match of ≥4 chars (same rule the reposition/terminal passes use). Used to
/// decide which windows of an over-populated app to keep vs close.
#[cfg(windows)]
fn title_matches_any(title: &str, snap_titles: &[String]) -> bool {
    if title.is_empty() {
        return false;
    }
    snap_titles.iter().any(|s| {
        s == title
            || (s.len() >= 4
                && title.len() >= 4
                && (s.contains(title) || title.contains(s.as_str())))
    })
}

#[cfg(windows)]
fn extra_window_launch_count(
    snapshot_windows: usize,
    live_windows: usize,
    planned_process_launches: usize,
) -> usize {
    snapshot_windows.saturating_sub(live_windows + planned_process_launches)
}

/// Build a restore-aware launch command for a terminal process by matching it
/// to a saved `TerminalSession`. Writes a temp restore script so the terminal
/// opens at the right CWD and shows recent command history.
#[cfg(windows)]
/// Build a restore-aware launch command for a terminal.
/// `window_title` — the snapshot window title to use for best-match session selection.
///                  Pass `None` to fall back to sequential index order.
/// `index`        — bumped on each call so successive calls pick different sessions.
/// Session cursors are keyed by executable so mixed terminal types advance independently.
fn terminal_restore_cmd(
    snapshot: &Snapshot,
    proc_: &crate::ProcessInfo,
    used: &mut std::collections::HashMap<String, Vec<usize>>,
    window_title: Option<&str>,
) -> Option<crate::terminal::TerminalLaunch> {
    if snapshot.terminal_sessions.is_empty() {
        return None;
    }

    let sessions_for_exe: Vec<(usize, &crate::TerminalSession)> = snapshot
        .terminal_sessions
        .iter()
        .enumerate()
        .filter(|(_, s)| crate::terminal::session_matches_executable(s, &proc_.exe_path))
        .collect();

    if sessions_for_exe.is_empty() {
        return None;
    }

    // Pick an as-yet-unconsumed session for this window. Terminal windows frequently
    // share a generic title ("Windows PowerShell"), so a title match alone collapses
    // several windows onto the same session — and thus the same CWD. Prefer a title
    // match among *unconsumed* sessions, then fall back to the next unconsumed one in
    // order, so each restored window keeps its own captured directory.
    let consumed = used.entry(proc_.exe_path.to_ascii_lowercase()).or_default();
    let pos = window_title
        .filter(|t| !t.is_empty())
        .and_then(|title| {
            (0..sessions_for_exe.len())
                .find(|&i| !consumed.contains(&i) && sessions_for_exe[i].1.window_title == title)
                .or_else(|| {
                    (0..sessions_for_exe.len()).find(|&i| {
                        !consumed.contains(&i) && {
                            let wt = &sessions_for_exe[i].1.window_title;
                            wt.len() >= 4
                                && title.len() >= 4
                                && (wt.contains(title) || title.contains(wt.as_str()))
                        }
                    })
                })
        })
        .or_else(|| (0..sessions_for_exe.len()).find(|&i| !consumed.contains(&i)))?;
    consumed.push(pos);
    let unique_index = consumed.len();

    let session = sessions_for_exe[pos].1;
    let temp_dir = std::env::temp_dir().join("pc_snapshot_restore");
    let _ = std::fs::create_dir_all(&temp_dir);

    crate::terminal::terminal_launch_cmd(&proc_.exe_path, session, &temp_dir, unique_index)
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

#[cfg(all(windows, test))]
mod tests {
    use super::{
        extra_window_launch_count, plan_terminal_reconcile, store_app_launch_alias,
        surplus_close_indices, terminal_restore_cmd,
    };
    use crate::terminal::RunningShell;
    use crate::{ProcessInfo, Snapshot, TerminalSession};
    use std::collections::{HashMap, HashSet};

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn term(shell: &str, cwd: &str) -> TerminalSession {
        TerminalSession {
            shell: shell.to_string(),
            cwd: cwd.to_string(),
            history: vec![],
            window_title: String::new(),
            inner_shell: String::new(),
            exe: format!("{shell}.exe"),
        }
    }

    fn shell(pid: u32, shell: &str, cwd: &str) -> RunningShell {
        RunningShell { pid, shell: shell.to_string(), cwd: cwd.to_string() }
    }

    #[test]
    fn terminal_reconcile_keeps_unchanged_relaunches_missing_closes_extras() {
        // Captured: cmd@C:\a and powershell@C:\b.
        let sessions = vec![term("cmd", r"C:\a"), term("powershell", r"C:\b")];
        let live = vec![
            shell(10, "cmd", r"C:\a\"),         // same as session 0 (trailing sep normalized) → keep
            shell(20, "cmd", r"C:\other"),      // not in snapshot → close on clean restore
            shell(30, "powershell", r"C:\b"),   // PS live cwd == session 1's cwd → keep
            shell(40, "powershell", r"C:\zzz"), // not in snapshot → close
        ];
        let protected = HashSet::new();
        let plan = plan_terminal_reconcile(&sessions, &live, &protected, true);
        assert!(plan.launch.is_empty(), "both captured sessions had a live match");
        let mut closed = plan.close_pids.clone();
        closed.sort_unstable();
        assert_eq!(closed, vec![20, 40]);
    }

    #[test]
    fn terminal_reconcile_relaunches_when_no_live_match() {
        let sessions = vec![term("cmd", r"C:\proj")];
        let live = vec![shell(10, "cmd", r"C:\somewhere\else")];
        let plan = plan_terminal_reconcile(&sessions, &live, &HashSet::new(), true);
        assert_eq!(plan.launch, vec![0]); // no cwd match → relaunch the captured session
        assert_eq!(plan.close_pids, vec![10]); // the mismatched live cmd is closed
    }

    #[test]
    fn terminal_reconcile_plain_restore_never_closes_and_protects_own_tree() {
        let sessions = vec![term("cmd", r"C:\a")];
        let live = vec![shell(10, "cmd", r"C:\a"), shell(99, "powershell", r"C:\x")];
        // Plain restore (close_others = false): keep the match, close nothing.
        let plain = plan_terminal_reconcile(&sessions, &live, &HashSet::new(), false);
        assert!(plain.launch.is_empty());
        assert!(plain.close_pids.is_empty());
        // Clean restore, but pid 99 is our own launching terminal → never closed.
        let mut protected = HashSet::new();
        protected.insert(99u32);
        let clean = plan_terminal_reconcile(&sessions, &live, &protected, true);
        assert!(clean.close_pids.is_empty());
    }

    #[test]
    fn terminal_reconcile_one_live_shell_satisfies_only_one_session() {
        // Two identical captured sessions, one live shell: keep one, relaunch the other.
        let sessions = vec![term("cmd", r"C:\a"), term("cmd", r"C:\a")];
        let live = vec![shell(10, "cmd", r"C:\a")];
        let plan = plan_terminal_reconcile(&sessions, &live, &HashSet::new(), true);
        assert_eq!(plan.launch, vec![1]); // first claims pid 10; second has no match
        assert!(plan.close_pids.is_empty());
    }

    #[test]
    fn windows_terminal_maps_to_wt_alias_not_the_locked_windowsapps_path() {
        // The reported bug: WindowsTerminal.exe under WindowsApps can't be launched
        // by path (os error 5) — it must go through the wt.exe execution alias.
        assert_eq!(
            store_app_launch_alias(
                r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal_1.21.0_x64__8wekyb3d8bbwe\WindowsTerminal.exe"
            ),
            Some("wt.exe")
        );
        // Case-insensitive on the stem.
        assert_eq!(store_app_launch_alias(r"D:\x\windowsterminal.exe"), Some("wt.exe"));
        // Normally-launchable exes are left alone.
        assert_eq!(
            store_app_launch_alias(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"),
            None
        );
        assert_eq!(store_app_launch_alias(r"C:\Program Files\Git\usr\bin\mintty.exe"), None);
    }

    fn wt_snapshot(cwds: &[&str]) -> (Snapshot, ProcessInfo) {
        let terminal_sessions = cwds
            .iter()
            .map(|c| TerminalSession {
                shell: "windows_terminal".to_string(),
                cwd: c.to_string(),
                history: vec![],
                window_title: "Windows PowerShell".to_string(), // shared, generic title
                inner_shell: String::new(), // exercises the wt -d fallback path
                exe: String::new(),
            })
            .collect();
        let snap = Snapshot {
            schema_version: 3,
            id: "t".into(),
            name: "t".into(),
            timestamp: "t".into(),
            processes: vec![],
            windows: vec![],
            explorer_windows: vec![],
            context_clues: vec![],
            restore_hints: vec![],
            warnings: vec![],
            thumbnail_path: String::new(),
            terminal_sessions,
            browser_sessions: vec![],
        };
        let proc_ = ProcessInfo {
            name: "WindowsTerminal.exe".into(),
            pid: 1,
            exe_path: r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal_x\WindowsTerminal.exe"
                .into(),
            cmd_line: String::new(),
            classification: "terminal".into(),
        };
        (snap, proc_)
    }

    #[test]
    fn same_titled_terminal_windows_get_distinct_cwds() {
        // The reported bug: several WT windows all titled "Windows PowerShell" must NOT
        // collapse onto one session's CWD — each keeps its own captured directory.
        let (snap, proc_) =
            wt_snapshot(&[r"C:\a\ozonetel work", r"C:\a\PC Snapshot", r"C:\a\projects"]);
        let mut used = HashMap::new();
        let title = Some("Windows PowerShell");
        let c1 = terminal_restore_cmd(&snap, &proc_, &mut used, title).expect("cmd1");
        let c2 = terminal_restore_cmd(&snap, &proc_, &mut used, title).expect("cmd2");
        let c3 = terminal_restore_cmd(&snap, &proc_, &mut used, title).expect("cmd3");
        assert!(c1.cmd_line.contains("ozonetel work"), "c1: {}", c1.cmd_line);
        assert!(c2.cmd_line.contains(r"\PC Snapshot"), "c2: {}", c2.cmd_line);
        assert!(c3.cmd_line.contains(r"\projects"), "c3: {}", c3.cmd_line);
        // New-window invocation so the -d directory is actually honored.
        assert!(c1.cmd_line.contains("-w new -d"), "c1: {}", c1.cmd_line);
    }

    #[test]
    fn no_surplus_when_live_not_over_snapshot() {
        assert!(surplus_close_indices(&v(&["A"]), &v(&["A"])).is_empty());
        assert!(surplus_close_indices(&v(&["A", "B"]), &v(&["X"])).is_empty());
        // No saved windows for this app → nothing to close via the surplus rule.
        assert!(surplus_close_indices(&v(&[]), &v(&["A", "B"])).is_empty());
    }

    #[test]
    fn closes_unmatched_extras_keeps_the_match() {
        // The user's case: 1 window captured ("Gmail"), 3 open now. Keep the one
        // matching the saved title, close the two opened afterward.
        let snap = v(&["Gmail — Inbox"]);
        let live = v(&["Gmail — Inbox", "New Tab", "Docs"]);
        let mut closed = surplus_close_indices(&snap, &live);
        closed.sort();
        assert_eq!(closed, vec![1, 2]); // indices of "New Tab" and "Docs"
    }

    #[test]
    fn keeps_snap_count_even_when_none_match() {
        // Titles all changed since capture: still close down to snap_count,
        // keeping an arbitrary-but-bounded one.
        let closed = surplus_close_indices(&v(&["old"]), &v(&["a", "b", "c"]));
        assert_eq!(closed.len(), 2);
    }

    #[test]
    fn two_captured_windows_close_one_of_three() {
        let snap = v(&["Win A", "Win B"]);
        let live = v(&["Win A", "Win B", "Extra"]);
        assert_eq!(surplus_close_indices(&snap, &live), vec![2]);
    }

    #[test]
    fn process_per_window_terminals_do_not_get_extra_blank_launches() {
        assert_eq!(extra_window_launch_count(3, 0, 3), 0);
        assert_eq!(extra_window_launch_count(3, 1, 2), 0);
        assert_eq!(extra_window_launch_count(3, 0, 1), 2);
    }

    #[test]
    fn mixed_terminal_types_advance_independent_session_cursors() {
        let root = std::env::temp_dir()
            .join("pc_snapshot_mixed_terminal_test")
            .join(std::process::id().to_string());
        let mintty = root.join("usr").join("bin").join("mintty.exe");
        let git_bash = root.join("git-bash.exe");
        std::fs::create_dir_all(mintty.parent().unwrap()).unwrap();
        std::fs::write(&mintty, []).unwrap();
        std::fs::write(&git_bash, []).unwrap();

        let powershell = ProcessInfo {
            name: "PowerShell".to_string(),
            pid: 1,
            exe_path: r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe".to_string(),
            cmd_line: "powershell.exe".to_string(),
            classification: "terminal".to_string(),
        };
        let git = ProcessInfo {
            name: "Git Bash".to_string(),
            pid: 2,
            exe_path: mintty.to_string_lossy().into_owned(),
            cmd_line: "mintty.exe".to_string(),
            classification: "terminal".to_string(),
        };
        let snapshot = Snapshot {
            schema_version: 2,
            id: "mixed".to_string(),
            name: "mixed".to_string(),
            timestamp: String::new(),
            processes: vec![powershell.clone(), git.clone()],
            windows: vec![],
            explorer_windows: vec![],
            context_clues: vec![],
            restore_hints: vec![],
            warnings: vec![],
            thumbnail_path: String::new(),
            terminal_sessions: vec![
                TerminalSession {
                    shell: "powershell".to_string(),
                    cwd: r"C:\Windows".to_string(),
                    history: vec![],
                    window_title: "Windows PowerShell".to_string(),
                    inner_shell: String::new(),
                    exe: String::new(),
                },
                TerminalSession {
                    shell: "git_bash".to_string(),
                    cwd: r"C:\repo".to_string(),
                    history: vec![],
                    window_title: "MINGW64:/c/repo".to_string(),
                    inner_shell: String::new(),
                    exe: String::new(),
                },
            ],
            browser_sessions: vec![],
        };

        let mut cursors = HashMap::new();
        assert!(terminal_restore_cmd(&snapshot, &powershell, &mut cursors, None).is_some());
        let git_launch = terminal_restore_cmd(&snapshot, &git, &mut cursors, None).unwrap();
        assert_eq!(git_launch.exe_path, git_bash.to_string_lossy());

        let _ = std::fs::remove_dir_all(&root);
    }
}

// ── Non-Windows fallback ──────────────────────────────────────────────────────────────

#[cfg(not(windows))]
pub fn restore_desktop(
    _snapshot: &Snapshot,
    _close_others: bool,
    _ignore_list: &[String],
    _companion_managed_browsers: bool,
) -> RestoreResult {
    RestoreResult {
        success: false,
        message: "Restore engine is only implemented on Windows".to_string(),
        failed_items: vec![],
        warnings: vec![],
        closed_items: vec![],
    }
}
