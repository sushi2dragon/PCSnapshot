//! Context extraction — infers meaningful state from raw process + window data.
//!
//! Rules are heuristic. Each rule matches one or more processes/windows, extracts
//! data, and assigns a confidence score. Failures are always silent: a rule that
//! can't run produces no clue rather than an error, so capture is never blocked.
//!
//! Outputs:
//!   `ContextClue`  — informational metadata about what was running.
//!   restore_hints  — actionable strings consumed by the restore engine.
//!
//! Restore hint formats added here:
//!   `office_extra_file:<exe_stem>:<full_path>`  — extra Office file to open in a
//!       second/third/... launch of that app (solves the multi-Excel-window case).
//!   `vscode_workspace:<full_path>`              — workspace already in cmd_line,
//!       echoed here so the UI can surface it as a label.

use crate::classify::Category;
use crate::{ContextClue, ProcessInfo, WindowInfo};

// ── Entry point ──────────────────────────────────────────────────────────────

/// Run all heuristic rules over the captured processes and windows.
/// Returns (context_clues, restore_hints).
pub fn extract_context(
    processes: &[ProcessInfo],
    windows: &[WindowInfo],
) -> (Vec<ContextClue>, Vec<String>) {
    let mut clues: Vec<ContextClue> = Vec::new();
    let mut hints: Vec<String> = Vec::new();

    for proc_ in processes {
        let cat = Category::from_str(&proc_.classification);
        match cat {
            Category::Ide => ide_context(proc_, windows, &mut clues, &mut hints),
            Category::Browser => browser_context(proc_, windows, &mut clues),
            Category::Terminal => terminal_context(proc_, windows, &mut clues),
            _ => {}
        }
        // Dev-server detection applies to any process (node, python, etc.)
        devserver_context(proc_, &mut clues, &mut hints);
    }

    // Office multi-window: cross-process rule, needs the full window list.
    office_context(processes, windows, &mut clues, &mut hints);

    (clues, hints)
}

// ── IDE / VSCode ──────────────────────────────────────────────────────────────

fn ide_context(
    proc_: &ProcessInfo,
    windows: &[WindowInfo],
    clues: &mut Vec<ContextClue>,
    hints: &mut Vec<String>,
) {
    // ── Workspace from command line ──────────────────────────────────────────
    // The cmd_line is stored as a quoted shell string (argv[0] included).
    // First non-flag argument after argv[0] is the workspace / folder / file.
    let args = crate::tokenize(&proc_.cmd_line);
    let workspace: Option<String> = args.iter().skip(1)
        .find(|a| !a.starts_with('-') && a.len() > 2 && (a.contains('\\') || a.contains('/')))
        .cloned();

    if let Some(ref ws) = workspace {
        clues.push(ContextClue {
            clue_type: "vscode_workspace".to_string(),
            value: ws.clone(),
            confidence: 0.92,
            source: "cmd_line".to_string(),
        });
        hints.push(format!("vscode_workspace:{ws}"));
    }

    // ── Active folder / file from window title ────────────────────────────────
    // VSCode titles follow: "[● ]<filename> — <folder> — Visual Studio Code"
    // or just "<folder> — Visual Studio Code" / "<folder> — Cursor"
    for win in windows.iter().filter(|w| {
        !w.exe_path.is_empty() && w.exe_path.eq_ignore_ascii_case(&proc_.exe_path)
    }) {
        if let Some(folder) = vscode_folder_from_title(&win.title) {
            // Only emit if it differs from the workspace we already found.
            if workspace.as_deref().map_or(true, |ws| !ws.contains(&folder)) {
                clues.push(ContextClue {
                    clue_type: "vscode_active_folder".to_string(),
                    value: folder,
                    confidence: 0.72,
                    source: "window_title".to_string(),
                });
            }
        }
    }
}

/// Extract the folder segment from a VSCode / Cursor window title.
/// e.g. "main.rs — pc-snapshot — Visual Studio Code" → "pc-snapshot"
fn vscode_folder_from_title(title: &str) -> Option<String> {
    // Strip editor names
    let base = title
        .strip_suffix(" - Visual Studio Code")
        .or_else(|| title.strip_suffix(" — Visual Studio Code"))
        .or_else(|| title.strip_suffix(" - Cursor"))
        .or_else(|| title.strip_suffix(" — Cursor"))
        .or_else(|| title.strip_suffix(" - Code - Insiders"))
        .unwrap_or(title);

    // Strip leading modification indicator (● or •)
    let base = base.trim_start_matches(['●', '•', ' ']);

    // The last segment separated by " - " or " — " is the folder.
    let sep = if base.contains(" — ") { " — " } else { " - " };
    let folder = base.rsplit(sep).next()?.trim().to_string();

    if folder.is_empty() { None } else { Some(folder) }
}

// ── Browser ───────────────────────────────────────────────────────────────────

fn browser_context(
    proc_: &ProcessInfo,
    windows: &[WindowInfo],
    clues: &mut Vec<ContextClue>,
) {
    let browser_name = exe_stem(&proc_.exe_path);

    clues.push(ContextClue {
        clue_type: "browser_session".to_string(),
        value: browser_name.clone(),
        confidence: 1.0,
        source: "process".to_string(),
    });

    // Count windows belonging to this browser as a rough tab proxy.
    let win_count = windows.iter()
        .filter(|w| !w.exe_path.is_empty() && w.exe_path.eq_ignore_ascii_case(&proc_.exe_path))
        .count();
    if win_count > 0 {
        clues.push(ContextClue {
            clue_type: "browser_windows".to_string(),
            value: win_count.to_string(),
            confidence: 1.0,
            source: "window_count".to_string(),
        });
    }

    // Detect localhost tabs — title contains "localhost" or "127.0.0.1".
    for win in windows.iter().filter(|w| {
        !w.exe_path.is_empty() && w.exe_path.eq_ignore_ascii_case(&proc_.exe_path)
    }) {
        let lower = win.title.to_ascii_lowercase();
        if lower.contains("localhost") || lower.contains("127.0.0.1") {
            // Try to extract the port from titles like "Vite App — localhost:5173"
            let port = extract_localhost_port(&win.title).unwrap_or_default();
            clues.push(ContextClue {
                clue_type: "localhost_tab".to_string(),
                value: if port.is_empty() {
                    "localhost".to_string()
                } else {
                    format!("localhost:{port}")
                },
                confidence: 0.85,
                source: "window_title".to_string(),
            });
            break; // one clue per browser is enough
        }
    }
}

fn extract_localhost_port(title: &str) -> Option<String> {
    // Match "localhost:NNNN" or "127.0.0.1:NNNN"
    let lower = title.to_ascii_lowercase();
    let marker = lower.find("localhost:")
        .map(|i| i + "localhost:".len())
        .or_else(|| lower.find("127.0.0.1:").map(|i| i + "127.0.0.1:".len()))?;
    let rest = &title[marker..];
    let port: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if port.is_empty() { None } else { Some(port) }
}

// ── Terminal ──────────────────────────────────────────────────────────────────

fn terminal_context(
    proc_: &ProcessInfo,
    windows: &[WindowInfo],
    clues: &mut Vec<ContextClue>,
) {
    let stem = exe_stem(&proc_.exe_path);

    // Emit shell type.
    let shell_type = match stem.as_str() {
        "powershell" | "pwsh" => "powershell",
        "cmd" => "cmd",
        "windowsterminal" | "wt" => "windows_terminal",
        "bash" => "bash",
        _ => "terminal",
    };
    clues.push(ContextClue {
        clue_type: "terminal_shell".to_string(),
        value: shell_type.to_string(),
        confidence: 1.0,
        source: "process".to_string(),
    });

    // Best-effort CWD from window title.
    // PowerShell titles: "Windows PowerShell" or the current path.
    // cmd.exe titles: often show the CWD directly.
    // Windows Terminal: tab title can be the CWD or app name.
    for win in windows.iter().filter(|w| {
        !w.exe_path.is_empty() && w.exe_path.eq_ignore_ascii_case(&proc_.exe_path)
    }) {
        if let Some(cwd) = cwd_from_terminal_title(&win.title) {
            clues.push(ContextClue {
                clue_type: "terminal_cwd".to_string(),
                value: cwd,
                confidence: 0.65,
                source: "window_title".to_string(),
            });
            break;
        }
    }
}

/// Heuristically extract a CWD from a terminal window title.
/// Works for cmd.exe (title = path) and some PowerShell configurations.
fn cwd_from_terminal_title(title: &str) -> Option<String> {
    let t = title.trim();
    // Skip generic titles.
    if matches!(t, "Windows PowerShell" | "Command Prompt" | "PowerShell" | "Administrator: Windows PowerShell") {
        return None;
    }
    // A title that looks like an absolute Windows path is the CWD.
    if t.len() >= 3 && t.chars().next()?.is_alphabetic() && t[1..].starts_with(":\\") {
        return Some(t.to_string());
    }
    // Some terminals prefix with "~" for the home directory.
    if t.starts_with('~') {
        return Some(t.to_string());
    }
    None
}

// ── Dev servers ───────────────────────────────────────────────────────────────

fn devserver_context(
    proc_: &ProcessInfo,
    clues: &mut Vec<ContextClue>,
    hints: &mut Vec<String>,
) {
    let stem = exe_stem(&proc_.exe_path);
    let cmd_lower = proc_.cmd_line.to_ascii_lowercase();

    // Node-based dev servers.
    if stem == "node" {
        let (server, confidence) = if cmd_lower.contains("vite") {
            ("vite", 0.93)
        } else if cmd_lower.contains("webpack-dev-server") || cmd_lower.contains("webpack serve") {
            ("webpack-dev-server", 0.92)
        } else if cmd_lower.contains("next") && cmd_lower.contains("dev") {
            ("next.js", 0.88)
        } else if cmd_lower.contains("react-scripts") && cmd_lower.contains("start") {
            ("create-react-app", 0.90)
        } else if cmd_lower.contains("nuxt") {
            ("nuxt", 0.88)
        } else if cmd_lower.contains("live-server") {
            ("live-server", 0.91)
        } else if cmd_lower.contains("ts-node") {
            ("ts-node", 0.85)
        } else if cmd_lower.contains("nodemon") {
            ("nodemon", 0.85)
        } else {
            return;
        };

        let port = extract_port(&proc_.cmd_line).unwrap_or_default();
        let value = if port.is_empty() {
            server.to_string()
        } else {
            format!("{server}:{port}")
        };

        clues.push(ContextClue {
            clue_type: "dev_server".to_string(),
            value: value.clone(),
            confidence,
            source: "cmd_line".to_string(),
        });
        hints.push(format!("dev_server_port:{port}:{server}"));
        return;
    }

    // Python dev servers.
    if stem == "python" || stem == "python3" {
        let (server, confidence) = if cmd_lower.contains("uvicorn") {
            ("uvicorn", 0.93)
        } else if cmd_lower.contains("flask") {
            ("flask", 0.88)
        } else if cmd_lower.contains("manage.py") && cmd_lower.contains("runserver") {
            ("django", 0.92)
        } else if cmd_lower.contains("-m http.server") || cmd_lower.contains("-m http") {
            ("python-http-server", 0.90)
        } else if cmd_lower.contains("fastapi") {
            ("fastapi", 0.88)
        } else {
            return;
        };

        let port = extract_port(&proc_.cmd_line).unwrap_or_default();
        let value = if port.is_empty() {
            server.to_string()
        } else {
            format!("{server}:{port}")
        };

        clues.push(ContextClue {
            clue_type: "dev_server".to_string(),
            value,
            confidence,
            source: "cmd_line".to_string(),
        });
        return;
    }

    // Special: Claude running in a terminal (common dev workflow).
    if (stem == "node" || stem == "cmd" || stem == "powershell" || stem == "pwsh")
        && cmd_lower.contains("claude")
    {
        clues.push(ContextClue {
            clue_type: "ai_assistant".to_string(),
            value: "claude".to_string(),
            confidence: 0.80,
            source: "cmd_line".to_string(),
        });
    }
}

/// Extract --port=NNNN or --port NNNN or :NNNN from a command line string.
fn extract_port(cmd: &str) -> Option<String> {
    // "--port=5173" or "--port 5173"
    let lower = cmd.to_ascii_lowercase();
    if let Some(pos) = lower.find("--port=").or_else(|| lower.find("--port ")) {
        let after = &cmd[pos + 7..];
        let port: String = after.chars().skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !port.is_empty() { return Some(port); }
    }
    // ":NNNN" at end — Vite prints "localhost:5173" in its output, not in cmd line,
    // but some tools pass the port at the end like "server :3000"
    None
}

// ── Office multi-window ───────────────────────────────────────────────────────

/// For each Office app (Excel, Word, PowerPoint) with multiple visible windows,
/// look up the file paths from the Office MRU registry and emit restore hints so
/// the restore engine can open the correct file in each extra window.
fn office_context(
    processes: &[ProcessInfo],
    windows: &[WindowInfo],
    clues: &mut Vec<ContextClue>,
    hints: &mut Vec<String>,
) {
    let office_apps: &[(&str, &str, &str)] = &[
        // (exe_stem, Office registry app name, title suffix to strip)
        ("excel",    "Excel",      " - Excel"),
        ("winword",  "Word",       " - Word"),
        ("powerpnt", "PowerPoint", " - PowerPoint"),
        ("onenote",  "OneNote",    " - OneNote"),
    ];

    for (exe_stem_str, reg_app, title_suffix) in office_apps {
        // Find the process for this Office app.
        let Some(proc_) = processes.iter().find(|p| {
            exe_stem(&p.exe_path) == *exe_stem_str
        }) else { continue };

        // Collect all windows belonging to this process.
        let app_windows: Vec<&WindowInfo> = windows.iter()
            .filter(|w| {
                !w.exe_path.is_empty()
                    && w.exe_path.eq_ignore_ascii_case(&proc_.exe_path)
            })
            .collect();

        if app_windows.is_empty() {
            continue;
        }

        // Extract filenames from window titles.
        let window_files: Vec<String> = app_windows.iter()
            .filter_map(|w| office_filename_from_title(&w.title, title_suffix))
            .collect();

        if window_files.is_empty() {
            continue;
        }

        // Look up full paths from the Office MRU registry.
        let mru_paths = office_mru_paths(reg_app);

        // Match each window filename to a full path.
        let mut matched_paths: Vec<String> = Vec::new();
        let mut unmatched: Vec<String> = Vec::new();

        for filename in &window_files {
            let fname_lower = filename.to_ascii_lowercase();
            if let Some(full_path) = mru_paths.iter().find(|p| {
                p.rsplit(|c| c == '\\' || c == '/').next()
                    .map(|f| f.to_ascii_lowercase() == fname_lower)
                    .unwrap_or(false)
            }) {
                matched_paths.push(full_path.clone());
            } else {
                unmatched.push(filename.clone());
            }
        }

        // Emit context clues for every discovered file.
        for path in &matched_paths {
            clues.push(ContextClue {
                clue_type: "office_file".to_string(),
                value: path.clone(),
                confidence: 0.88,
                source: "registry_mru".to_string(),
            });
        }
        for name in &unmatched {
            clues.push(ContextClue {
                clue_type: "office_file".to_string(),
                value: name.clone(),
                confidence: 0.55,
                source: "window_title".to_string(),
            });
        }

        // The FIRST window's file is handled by the cmd_line already.
        // Extra windows (index 1..) need restore hints so the engine can open the
        // correct file when it fires off the additional launches.
        //
        // Strategy: pair matched paths to window slots. We emit hints for windows
        // 2..N (the "extra" ones). The first window's path comes from cmd_line.
        //
        // We use ALL matched paths as hints, ordered by recency (MRU order).
        // The restore engine picks them in order, one per extra launch.
        if app_windows.len() > 1 {
            // Determine which file is already covered by cmd_line.
            let cmdline_file = cmdline_first_path(&proc_.cmd_line);
            let extra_paths: Vec<&String> = matched_paths.iter()
                .filter(|p| cmdline_file.as_deref().map_or(true, |cf| {
                    !p.to_ascii_lowercase().ends_with(&cf.to_ascii_lowercase())
                }))
                .collect();

            for path in extra_paths {
                hints.push(format!("office_extra_file:{}:{}", exe_stem_str, path));
            }
        }
    }
}

/// Strip the app-name suffix from an Office window title to get the filename.
/// "Budget.xlsx - Excel" → "Budget.xlsx"
/// "Document1 - Microsoft Word" → "Document1"
fn office_filename_from_title(title: &str, suffix: &str) -> Option<String> {
    // Try exact suffix first, then "Microsoft ..." variant.
    let base = title.strip_suffix(suffix)
        .or_else(|| {
            let ms = format!(" - Microsoft{}", &suffix[2..]); // " - Microsoft Excel"
            title.strip_suffix(ms.as_str())
        })
        .or_else(|| {
            // Newer Office can append extra info: "Budget.xlsx - Excel  (Reading Mode)"
            if let Some(idx) = title.rfind(suffix) {
                Some(&title[..idx])
            } else { None }
        })?;

    let name = base.trim()
        .trim_start_matches('[') // "[Compatibility Mode]" prefix
        .trim()
        .to_string();

    // Placeholder titles for never-saved docs ("Document1", "Book1",
    // "Presentation1") carry no extension; only real filenames (which the MRU
    // lookup matches by extension-bearing name) are useful for restore. A
    // substring test like contains("New") would wrongly drop legitimate files
    // such as "Newsletter_Q3.xlsx".
    if name.is_empty() || !name.contains('.') { None } else { Some(name) }
}

/// Extract the first filesystem-path argument from a cmd_line (argv[0] stripped).
fn cmdline_first_path(cmd_line: &str) -> Option<String> {
    let args = crate::tokenize(cmd_line);
    args.into_iter().skip(1)
        .find(|a| !a.starts_with('-') && (a.contains('\\') || a.contains('/')) && a.len() > 3)
}

// ── Registry helpers (Windows only) ──────────────────────────────────────────

/// Return full file paths from Office's MRU registry list.
/// Tries Office 16 (2016/2019/365), 15 (2013), 14 (2010) in order.
#[cfg(windows)]
fn office_mru_paths(app_name: &str) -> Vec<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    for version in &["16.0", "15.0", "14.0"] {
        let key_path = format!(
            "Software\\Microsoft\\Office\\{}\\{}\\File MRU",
            version, app_name
        );
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(key) = hkcu.open_subkey(&key_path) else { continue };

        let mut paths: Vec<String> = Vec::new();
        for i in 1u32..=25 {
            let item_name = format!("Item {}", i);
            let Ok(value): Result<String, _> = key.get_value(&item_name) else { break };
            // Registry value format: "[F00000000][T01D9C5B6D0F6E000]*C:\path\to\file.xlsx"
            if let Some(path) = value.rsplit('*').next() {
                let path = path.trim().to_string();
                if !path.is_empty() && (path.contains('\\') || path.starts_with('/')) {
                    paths.push(path);
                }
            }
        }

        if !paths.is_empty() {
            return paths;
        }
    }

    vec![]
}

#[cfg(not(windows))]
fn office_mru_paths(_app_name: &str) -> Vec<String> {
    vec![]
}

// ── Shared helpers ────────────────────────────────────────────────────────────

fn exe_stem(exe_path: &str) -> String {
    let last = exe_path.rsplit(|c| c == '\\' || c == '/').next().unwrap_or(exe_path);
    let stem = last.strip_suffix(".exe").or_else(|| last.strip_suffix(".EXE")).unwrap_or(last);
    stem.to_ascii_lowercase()
}
