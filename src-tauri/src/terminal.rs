//! Terminal session capture & restore — reads PSReadLine history, maps it to
//! terminal windows, and generates restore scripts that `cd` to the saved CWD
//! and display recent command history for context.
//!
//! Limitations:
//!   - PSReadLine history is global (one file for all sessions), so we cannot
//!     attribute specific commands to specific windows. Each restored terminal
//!     shows the same recent-history block.
//!   - CWD is inferred from the window title or the process cmd_line `-d` flag.

use crate::classify::{self, Category};
use crate::{ProcessInfo, TerminalSession, WindowInfo};

const MAX_HISTORY_LINES: usize = 50;

// ── Capture ─────────────────────────────────────────────────────────────────

#[cfg(windows)]
pub fn capture_terminal_sessions(
    processes: &[ProcessInfo],
    windows: &[WindowInfo],
) -> Vec<TerminalSession> {
    let history = read_psreadline_history();
    let mut sessions = Vec::new();

    for proc_ in processes {
        let cat = if proc_.classification == "foreground" {
            classify::classify(&proc_.exe_path, true)
        } else {
            Category::from_str(&proc_.classification)
        };
        if cat != Category::Terminal {
            continue;
        }

        let shell = shell_type(&proc_.exe_path);
        // CWD from cmd_line as fallback for when the window title is generic.
        // Windows Terminal passes `-d <path>` as its starting directory.
        let cmdline_cwd = cwd_from_cmdline(&proc_.cmd_line);

        for win in windows
            .iter()
            .filter(|w| w.exe_path.eq_ignore_ascii_case(&proc_.exe_path))
        {
            let cwd = cwd_from_title(&win.title)
                .or_else(|| cmdline_cwd.clone())
                .unwrap_or_default();
            sessions.push(TerminalSession {
                shell: shell.to_string(),
                cwd,
                history: history.clone(),
                window_title: win.title.clone(),
            });
        }
    }
    sessions
}

#[cfg(not(windows))]
pub fn capture_terminal_sessions(
    _processes: &[ProcessInfo],
    _windows: &[WindowInfo],
) -> Vec<TerminalSession> {
    vec![]
}

fn shell_type(exe_path: &str) -> &'static str {
    let stem = exe_path
        .rsplit(|c| c == '\\' || c == '/')
        .next()
        .unwrap_or("");
    let stem = stem
        .strip_suffix(".exe")
        .or_else(|| stem.strip_suffix(".EXE"))
        .unwrap_or(stem);
    match stem.to_ascii_lowercase().as_str() {
        "powershell" => "powershell",
        "pwsh" => "pwsh",
        "cmd" => "cmd",
        "windowsterminal" | "wt" => "windows_terminal",
        _ => "unknown",
    }
}

fn cwd_from_title(title: &str) -> Option<String> {
    let t = title.trim();
    if matches!(
        t,
        "Windows PowerShell"
            | "Command Prompt"
            | "PowerShell"
            | "Administrator: Windows PowerShell"
    ) {
        return None;
    }
    // Absolute Windows path — but only if it looks like a directory, not an executable.
    if t.len() >= 3 && t.chars().next()?.is_alphabetic() && t[1..].starts_with(":\\") {
        let lower = t.to_ascii_lowercase();
        if lower.ends_with(".exe") || lower.ends_with(".com") || lower.ends_with(".bat") {
            return None; // WT tab title showing a shell executable, not a CWD
        }
        return Some(t.to_string());
    }
    // "PS C:\path>" prompt style.
    if let Some(rest) = t.strip_prefix("PS ") {
        let path = rest.trim_end_matches('>').trim();
        if path.len() >= 3 && path.chars().nth(1) == Some(':') {
            return Some(path.to_string());
        }
    }
    // ~ home shorthand.
    if t.starts_with('~') {
        if let Ok(home) = std::env::var("USERPROFILE") {
            return Some(t.replacen('~', &home, 1));
        }
    }
    None
}

/// Extract the starting directory from a terminal process cmd_line.
/// Handles Windows Terminal's `-d <path>` and `--startingDirectory <path>` flags.
fn cwd_from_cmdline(cmd_line: &str) -> Option<String> {
    let tokens = crate::tokenize(cmd_line);
    let mut iter = tokens.iter().skip(1); // skip argv[0]
    while let Some(tok) = iter.next() {
        if tok == "-d" || tok == "--startingDirectory" {
            if let Some(dir) = iter.next() {
                if !dir.is_empty() {
                    return Some(dir.clone());
                }
            }
        }
    }
    None
}

#[cfg(windows)]
fn read_psreadline_history() -> Vec<String> {
    let appdata = match std::env::var("APPDATA") {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    let path = std::path::PathBuf::from(&appdata)
        .join("Microsoft")
        .join("Windows")
        .join("PowerShell")
        .join("PSReadLine")
        .join("ConsoleHost_history.txt");

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let start = lines.len().saturating_sub(MAX_HISTORY_LINES);
    lines[start..].to_vec()
}

// ── Restore ─────────────────────────────────────────────────────────────────

/// Build the launch command for a terminal process using its saved session.
/// Returns `None` to fall back to the raw cmd_line.
#[cfg(windows)]
pub fn terminal_launch_cmd(
    exe_path: &str,
    session: &TerminalSession,
    temp_dir: &std::path::Path,
    index: usize,
) -> Option<String> {
    match session.shell.as_str() {
        "powershell" | "pwsh" => {
            let script_path = temp_dir.join(format!("restore_terminal_{index}.ps1"));
            let script = build_ps_restore_script(session);
            std::fs::write(&script_path, &script).ok()?;
            Some(format!(
                "\"{}\" -NoExit -ExecutionPolicy Bypass -File \"{}\"",
                exe_path,
                script_path.to_string_lossy()
            ))
        }
        "cmd" => {
            if session.cwd.is_empty() {
                None
            } else {
                Some(format!("\"{}\" /K \"cd /d {}\"", exe_path, session.cwd))
            }
        }
        "windows_terminal" => {
            // Windows Terminal's packaged UWP argument parsing doesn't reliably support
            // chaining a PowerShell command via `-d <cwd> powershell -NoExit -File <script>`.
            // Just restore the CWD via the `-d` flag; history display inside WT requires
            // a different mechanism (WT settings profile or post-launch keystrokes).
            if session.cwd.is_empty() {
                None // fall back to the captured cmd_line which may already have -d
            } else {
                Some(format!("\"{}\" -d \"{}\"", exe_path, session.cwd))
            }
        }
        _ => None,
    }
}

#[cfg(windows)]
fn build_ps_restore_script(session: &TerminalSession) -> String {
    let mut lines = Vec::new();

    if !session.cwd.is_empty() {
        let escaped = session.cwd.replace('\'', "''");
        lines.push(format!("Set-Location '{escaped}'"));
    }

    if !session.history.is_empty() {
        lines.push(String::new());
        lines.push("Write-Host ''".to_string());
        lines.push(
            "Write-Host '  --- Restored session history ---' -ForegroundColor DarkCyan"
                .to_string(),
        );

        let display_count = session.history.len().min(20);
        let display_start = session.history.len() - display_count;
        for cmd in &session.history[display_start..] {
            let escaped = cmd.replace('\'', "''");
            lines.push(format!(
                "Write-Host '  {escaped}' -ForegroundColor DarkGray"
            ));
        }
        lines.push(
            "Write-Host '  --------------------------------' -ForegroundColor DarkCyan"
                .to_string(),
        );
        lines.push("Write-Host ''".to_string());
    }

    lines.join("\r\n")
}
