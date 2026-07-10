//! Terminal session capture & restore — reads PSReadLine history, maps it to
//! terminal windows, and generates restore scripts that `cd` to the saved CWD
//! and display recent command history for context.
//!
//! CWD source: we locate the shell process that actually backs each terminal
//! window and read its real working directory straight from that process's PEB
//! (`cwd_for_pid`). The shell isn't always the window's own process — see
//! `resolve_shell_cwd` for the self / parent / descendant walk that handles
//! standalone consoles, classic conhost, and Windows Terminal ("let Windows
//! decide"). Title/`-d` parsing remains only as a last-resort fallback.
//!
//! Limitations:
//!   - PSReadLine history is global (one file for all sessions), so we cannot
//!     attribute specific commands to specific windows. Each restored terminal
//!     shows the same recent-history block.
//!   - One Windows Terminal process hosts many tabs/windows with no exposed
//!     window→shell mapping; we assign its shells to its windows by ascending
//!     PID, which is exact for the common single-terminal case but best-effort
//!     when several WT windows/tabs are open at once.

use crate::classify::{self, Category};
use crate::{ProcessInfo, TerminalSession, WindowInfo};

#[cfg(windows)]
use std::collections::{HashMap, HashSet, VecDeque};

const MAX_HISTORY_LINES: usize = 50;

// ── Capture ─────────────────────────────────────────────────────────────────

/// Capture one `TerminalSession` per terminal window. `windows` carries the
/// owning PID per window so each window maps to *its* process — matching by
/// exe path instead would fan every powershell window across every powershell
/// process (an N×M cross-product with cross-assigned CWDs).
#[cfg(windows)]
pub fn capture_terminal_sessions(
    processes: &[ProcessInfo],
    windows: &[(u32, WindowInfo)],
) -> Vec<TerminalSession> {
    let history = read_psreadline_history();
    let mut sessions = Vec::new();

    // Built once, on the first terminal window: a process-tree snapshot used to
    // find the shell that actually backs each window (see resolve_shell_cwd).
    // `claimed` stops two windows from mapping to the same shell process.
    let mut tree: Option<ProcTree> = None;
    let mut claimed: HashSet<u32> = HashSet::new();

    for (pid, win) in windows {
        let Some(proc_) = processes.iter().find(|p| p.pid == *pid) else {
            continue;
        };
        let cat = if proc_.classification == "foreground" {
            classify::classify(&proc_.exe_path, true)
        } else {
            Category::from_str(&proc_.classification)
        };
        if cat != Category::Terminal {
            continue;
        }

        let shell = shell_type(&proc_.exe_path);

        // Resolution order, best → worst:
        //   1. Window title, when it holds an absolute path. This is the ONLY
        //      way to get a PowerShell's *live* directory: PS keeps its location
        //      in-runspace and never writes it to the process CWD, so the OS read
        //      below returns the launch dir, not where the user cd'd. The opt-in
        //      profile hook (see terminal_hook.rs) mirrors $PWD into the title so
        //      this branch becomes authoritative.
        //   2. The backing shell's process CWD from the OS — correct for cmd and
        //      for any shell that never cd'd after launch.
        //   3. Windows Terminal's `-d <path>` startup flag.
        let cwd = if let Some(c) = cwd_from_title(&win.title) {
            c
        } else {
            let tree_ref = tree.get_or_insert_with(ProcTree::snapshot);
            resolve_shell_cwd(*pid, shell, tree_ref, &mut claimed)
                .or_else(|| cwd_from_cmdline(&proc_.cmd_line))
                .unwrap_or_default()
        };

        sessions.push(TerminalSession {
            shell: shell.to_string(),
            cwd,
            history: history.clone(),
            window_title: win.title.clone(),
        });
    }
    sessions
}

/// Find the working directory of the shell backing a terminal window. The shell
/// is often NOT the window's own process:
///   - standalone console → the window process IS the shell (powershell/pwsh/cmd)
///   - classic conhost     → the shell is the window process's PARENT
///   - Windows Terminal    → WindowsTerminal.exe owns the window; the shell is a
///                           DESCENDANT (via OpenConsole.exe)
/// so we try self, then parent, then descendants. `claimed` prevents two windows
/// (e.g. two WT tabs under one host) from resolving to the same shell.
#[cfg(windows)]
fn resolve_shell_cwd(
    owner_pid: u32,
    shell_kind: &str,
    tree: &ProcTree,
    claimed: &mut HashSet<u32>,
) -> Option<String> {
    // The window's own process is the shell.
    if matches!(shell_kind, "powershell" | "pwsh" | "cmd") {
        claimed.insert(owner_pid);
        return cwd_for_pid(owner_pid);
    }
    // The shell is the parent (classic conhost-hosted console).
    if let Some(par) = tree.parent(owner_pid) {
        if tree.is_shell(par) && claimed.insert(par) {
            return cwd_for_pid(par);
        }
    }
    // The shell is a descendant (Windows Terminal → OpenConsole → shell).
    if let Some(shell_pid) = tree.first_shell_descendant(owner_pid, claimed) {
        claimed.insert(shell_pid);
        return cwd_for_pid(shell_pid);
    }
    None
}

/// A lightweight process-tree snapshot: parent links, child links, and the
/// lowercased exe stem per PID. One `sysinfo` refresh over all processes (the
/// shells we want are children of a terminal host and own no window, so a
/// window-scoped refresh wouldn't see them).
#[cfg(windows)]
struct ProcTree {
    parent: HashMap<u32, u32>,
    children: HashMap<u32, Vec<u32>>,
    stem: HashMap<u32, String>,
}

#[cfg(windows)]
impl ProcTree {
    fn snapshot() -> Self {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, ProcessRefreshKind::new());

        let mut parent = HashMap::new();
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut stem = HashMap::new();
        for (pid, proc_) in sys.processes() {
            let pid_u = pid.as_u32();
            let name_cow = proc_.name().to_string_lossy();
            let name: &str = name_cow.as_ref();
            let s = name
                .strip_suffix(".exe")
                .or_else(|| name.strip_suffix(".EXE"))
                .unwrap_or(name)
                .to_ascii_lowercase();
            stem.insert(pid_u, s);
            if let Some(par) = proc_.parent() {
                let par_u = par.as_u32();
                parent.insert(pid_u, par_u);
                children.entry(par_u).or_default().push(pid_u);
            }
        }
        ProcTree { parent, children, stem }
    }

    fn parent(&self, pid: u32) -> Option<u32> {
        self.parent.get(&pid).copied()
    }

    fn is_shell(&self, pid: u32) -> bool {
        self.stem
            .get(&pid)
            .map(|s| matches!(s.as_str(), "powershell" | "pwsh" | "cmd" | "bash" | "wsl"))
            .unwrap_or(false)
    }

    /// Lowest-PID unclaimed shell reachable below `root` (BFS). Ascending PID
    /// gives a stable window→shell assignment across capture and restore.
    fn first_shell_descendant(&self, root: u32, claimed: &HashSet<u32>) -> Option<u32> {
        let mut queue: VecDeque<u32> = VecDeque::new();
        let mut seen: HashSet<u32> = HashSet::new();
        let mut best: Option<u32> = None;
        queue.push_back(root);
        seen.insert(root);
        while let Some(cur) = queue.pop_front() {
            if let Some(kids) = self.children.get(&cur) {
                for &k in kids {
                    if !seen.insert(k) {
                        continue;
                    }
                    if self.is_shell(k) && !claimed.contains(&k) {
                        best = Some(best.map_or(k, |b| b.min(k)));
                    }
                    queue.push_back(k);
                }
            }
        }
        best
    }
}

#[cfg(not(windows))]
pub fn capture_terminal_sessions(
    _processes: &[ProcessInfo],
    _windows: &[(u32, WindowInfo)],
) -> Vec<TerminalSession> {
    vec![]
}

/// Read a process's current working directory straight from its PEB.
///
/// Walks: `NtQueryInformationProcess` → PEB base, then `ReadProcessMemory` into
/// PEB → `ProcessParameters` → `CurrentDirectory.DosPath`. Offsets are the
/// stable x64 layout. Best-effort: returns `None` if the process is protected,
/// 32-bit under WOW64 mismatch, or exits mid-read. Never panics.
#[cfg(windows)]
fn cwd_for_pid(pid: u32) -> Option<String> {
    use std::ffi::c_void;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    if pid == 0 {
        return None;
    }

    // x64 field offsets (stable for many Windows releases).
    const PEB_PROCESS_PARAMETERS: usize = 0x20;
    const RTL_CURRENT_DIRECTORY_DOSPATH: usize = 0x38; // UNICODE_STRING
    const UNICODE_STRING_BUFFER: usize = 0x08; // offset of Buffer within UNICODE_STRING

    unsafe {
        let handle: HANDLE =
            OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid).ok()?;

        let read = |addr: usize, out: *mut c_void, len: usize| -> bool {
            ReadProcessMemory(handle, addr as *const c_void, out, len, None).is_ok()
        };

        let result = (|| {
            let peb_base = query_peb_base(handle)? as usize;

            // PEB.ProcessParameters
            let mut params_ptr: usize = 0;
            if !read(
                peb_base + PEB_PROCESS_PARAMETERS,
                &mut params_ptr as *mut _ as *mut c_void,
                std::mem::size_of::<usize>(),
            ) || params_ptr == 0
            {
                return None;
            }

            // CurrentDirectory.DosPath (UNICODE_STRING): Length (u16) + Buffer (ptr)
            let mut len_bytes: u16 = 0;
            if !read(
                params_ptr + RTL_CURRENT_DIRECTORY_DOSPATH,
                &mut len_bytes as *mut _ as *mut c_void,
                std::mem::size_of::<u16>(),
            ) || len_bytes == 0
            {
                return None;
            }
            let mut buf_ptr: usize = 0;
            if !read(
                params_ptr + RTL_CURRENT_DIRECTORY_DOSPATH + UNICODE_STRING_BUFFER,
                &mut buf_ptr as *mut _ as *mut c_void,
                std::mem::size_of::<usize>(),
            ) || buf_ptr == 0
            {
                return None;
            }

            let char_count = (len_bytes as usize) / 2;
            let mut wbuf = vec![0u16; char_count];
            if !read(
                buf_ptr,
                wbuf.as_mut_ptr() as *mut c_void,
                len_bytes as usize,
            ) {
                return None;
            }

            let s = String::from_utf16_lossy(&wbuf);
            let s = s.trim_end_matches('\0');
            // NT CWD carries a trailing separator; drop it unless it's a drive root ("C:\").
            let trimmed = if s.len() > 3 {
                s.trim_end_matches('\\')
            } else {
                s
            };
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })();

        let _ = CloseHandle(handle);
        result
    }
}

/// Resolve a process's PEB base address via `ntdll!NtQueryInformationProcess`,
/// loaded dynamically so no Wdk feature/binding is required.
#[cfg(windows)]
unsafe fn query_peb_base(handle: windows::Win32::Foundation::HANDLE) -> Option<*mut std::ffi::c_void> {
    use std::ffi::c_void;
    use windows::core::{s, w};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

    // PROCESS_BASIC_INFORMATION (x64): ExitStatus, PebBaseAddress, AffinityMask,
    // BasePriority, UniqueProcessId, InheritedFromUniqueProcessId.
    #[repr(C)]
    struct ProcessBasicInformation {
        exit_status: i32,
        _pad: u32,
        peb_base_address: *mut c_void,
        affinity_mask: usize,
        base_priority: i32,
        _pad2: u32,
        unique_process_id: usize,
        inherited_from_unique_process_id: usize,
    }
    type NtQip =
        unsafe extern "system" fn(HANDLE, u32, *mut c_void, u32, *mut u32) -> i32;

    let ntdll = GetModuleHandleW(w!("ntdll.dll")).ok()?;
    let addr = GetProcAddress(ntdll, s!("NtQueryInformationProcess"))?;
    let func: NtQip = std::mem::transmute(addr);

    let mut pbi: ProcessBasicInformation = std::mem::zeroed();
    let mut ret_len: u32 = 0;
    // 0 = ProcessBasicInformation
    let status = func(
        handle,
        0,
        &mut pbi as *mut _ as *mut c_void,
        std::mem::size_of::<ProcessBasicInformation>() as u32,
        &mut ret_len,
    );
    if status < 0 || pbi.peb_base_address.is_null() {
        return None;
    }
    Some(pbi.peb_base_address)
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

#[cfg(all(windows, test))]
mod tests {
    use super::cwd_for_pid;

    /// End-to-end check of the PEB read: spawn a child with a known working
    /// directory and confirm `cwd_for_pid` recovers it. Exercises the x64
    /// offsets, `NtQueryInformationProcess`, and `ReadProcessMemory` for real.
    #[test]
    fn reads_child_process_cwd() {
        use std::process::{Command, Stdio};

        // A directory guaranteed to exist and differ from the test runner's CWD.
        let target = std::env::temp_dir().canonicalize().unwrap();

        // `ping` stays alive for the duration and never reads stdin (a shell
        // like `cmd /K` would hit EOF on a null stdin and exit immediately,
        // leaving nothing to read).
        let mut child = Command::new("ping.exe")
            .args(["-n", "30", "127.0.0.1"])
            .current_dir(&target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn ping.exe");

        // Give the child a moment to initialise.
        std::thread::sleep(std::time::Duration::from_millis(300));

        let got = cwd_for_pid(child.id());
        let _ = child.kill();
        let _ = child.wait();

        let got = got.expect("cwd_for_pid returned None");
        let want = target.to_string_lossy();
        // Strip the \\?\ verbatim prefix canonicalize adds, compare case-insensitively.
        let norm = |s: &str| s.trim_start_matches("\\\\?\\").trim_end_matches('\\').to_ascii_lowercase();
        assert_eq!(norm(&got), norm(&want), "got {got:?}, want {want:?}");
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
