//! Terminal session capture & restore — reads PSReadLine history, maps it to
//! terminal windows, and generates restore scripts that `cd` to the saved CWD
//! and display recent command history for context.
//! Git Bash sessions use their own `.bash_history` source and launcher.
//!
//! CWD source: we locate the shell process that actually backs each terminal
//! window and read its real working directory straight from that process's PEB
//! (`cwd_for_pid`). The shell isn't always the window's own process — see
//! `resolve_shell_cwd` for the self / parent / descendant walk that handles
//! standalone consoles, classic conhost, and Windows Terminal ("let Windows
//! decide"). Title/`-d` parsing remains only as a last-resort fallback.
//!
//! Limitations:
//!   - PSReadLine and Git Bash history files are global, so we cannot attribute
//!     specific commands to specific windows. Each shell type shows its own
//!     recent-history block.
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
/// Capture the interactive shell *processes* the user opened, reading each one's
/// working directory straight from the process. This sidesteps the unsolvable
/// "which Windows Terminal window hosts which shell" mapping: a Win+R cmd adopted
/// into WT via the Win11 default-terminal handoff isn't reachable from the WT
/// window, but the shell process itself always is. Window position and tab grouping
/// are intentionally not restored (Windows exposes no reliable way to recover them).
///
/// PowerShell keeps its location in-runspace, so its process CWD reads as the launch
/// dir; the opt-in profile hook (terminal_hook.rs) is the way to capture a PS that
/// cd'd. `cmd` and `bash` update their real process CWD, so those are exact.
#[cfg(windows)]
pub fn capture_terminal_sessions(
    processes: &[ProcessInfo],
    windows: &[(u32, WindowInfo)],
) -> Vec<TerminalSession> {
    let tree = ProcTree::snapshot();
    let mut sessions = Vec::new();

    for pid in tree.interactive_shells() {
        let exe_path = tree.exe_of(pid);
        let (shell, launch_exe) = match tree.stem_of(pid).as_str() {
            "powershell" => ("powershell", "powershell.exe".to_string()),
            "pwsh" => ("pwsh", "pwsh.exe".to_string()),
            "cmd" => ("cmd", "cmd.exe".to_string()),
            "bash" => match git_bash_launcher_path(&exe_path) {
                Some(launcher) => ("git_bash", launcher.to_string_lossy().into_owned()),
                None => continue, // a bash we can't relaunch (e.g. WSL) — skip
            },
            _ => continue,
        };

        // cmd keeps no history file; its per-session buffer is read from the live console.
        // powershell/pwsh use the (global) PSReadLine file. git_bash handled below.
        let history = if shell == "cmd" {
            read_cmd_console_history(pid)
        } else {
            read_terminal_history(shell)
        };

        sessions.push(TerminalSession {
            shell: shell.to_string(),
            cwd: cwd_for_pid(pid).unwrap_or_default(),
            history,
            window_title: String::new(),
            inner_shell: String::new(),
            exe: launch_exe,
        });
    }

    // Git Bash can't be found via the process tree: mintty's bash child routinely has a
    // severed parent link (its recorded parent PID points at an already-exited process),
    // so `interactive_shells()` never sees it. Recover it from the mintty *window* title
    // instead, which encodes the MSYS cwd (e.g. "MINGW64:/c/Users/…"). The git-bash
    // launcher comes from the owning mintty's exe path. Dedup against any bash the process
    // walk did find so a single terminal isn't captured twice.
    let pid_to_exe: HashMap<u32, &str> =
        processes.iter().map(|p| (p.pid, p.exe_path.as_str())).collect();
    for (pid, w) in windows {
        let Some(cwd) = git_bash_cwd_from_title(&w.title) else {
            continue;
        };
        if sessions
            .iter()
            .any(|s| s.shell == "git_bash" && s.cwd.eq_ignore_ascii_case(&cwd))
        {
            continue; // already captured via the process walk
        }
        let owner_exe = pid_to_exe.get(pid).copied().unwrap_or("");
        let launch_exe = git_bash_launcher_path(owner_exe)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| owner_exe.to_string());
        if launch_exe.is_empty() {
            continue; // no way to relaunch it
        }
        sessions.push(TerminalSession {
            shell: "git_bash".to_string(),
            cwd,
            history: read_terminal_history("git_bash"),
            window_title: w.title.clone(),
            inner_shell: String::new(),
            exe: launch_exe,
        });
    }

    sessions
}

/// A currently-running interactive shell: its PID, restore shell type
/// (cmd/powershell/pwsh/git_bash), and working directory. Restore reconciles these
/// against the captured sessions to decide what to keep, relaunch, or close.
#[cfg(windows)]
pub struct RunningShell {
    pub pid: u32,
    pub shell: String,
    pub cwd: String,
}

/// Enumerate the interactive shells the user currently has open, each with the
/// working directory read straight from its process. Uses the same discovery and
/// CWD source as `capture_terminal_sessions`, so a live shell compares like-for-like
/// against a captured session (same shell type + same cwd ⇒ "unchanged").
#[cfg(windows)]
pub fn running_interactive_shells() -> Vec<RunningShell> {
    let tree = ProcTree::snapshot();
    let mut out = Vec::new();
    for pid in tree.interactive_shells() {
        let shell = match tree.stem_of(pid).as_str() {
            "powershell" => "powershell",
            "pwsh" => "pwsh",
            "cmd" => "cmd",
            "bash" if git_bash_launcher_path(&tree.exe_of(pid)).is_some() => "git_bash",
            _ => continue,
        };
        out.push(RunningShell {
            pid,
            shell: shell.to_string(),
            cwd: cwd_for_pid(pid).unwrap_or_default(),
        });
    }
    out
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
/// Returns `(cwd, inner_shell_stem)` for the shell backing the window. The stem
/// (cmd/powershell/pwsh/…) lets a Windows Terminal window record which shell runs
/// inside it, since the window's own exe is just WindowsTerminal.exe.
fn resolve_shell_cwd(
    owner_pid: u32,
    shell_kind: &str,
    tree: &ProcTree,
    claimed: &mut HashSet<u32>,
) -> Option<(String, String)> {
    // The window's own process is the shell.
    if matches!(shell_kind, "powershell" | "pwsh" | "cmd") {
        claimed.insert(owner_pid);
        return cwd_for_pid(owner_pid).map(|c| (c, shell_kind.to_string()));
    }
    // The shell is the parent (classic conhost-hosted console).
    if let Some(par) = tree.parent(owner_pid) {
        if tree.is_shell(par) && claimed.insert(par) {
            return cwd_for_pid(par).map(|c| (c, tree.stem_of(par)));
        }
    }
    // The shell is a descendant (Windows Terminal → OpenConsole → shell).
    if let Some(shell_pid) = tree.first_shell_descendant(owner_pid, claimed) {
        claimed.insert(shell_pid);
        return cwd_for_pid(shell_pid).map(|c| (c, tree.stem_of(shell_pid)));
    }
    // Windows 11 default-terminal handoff: a console started outside WT (Win+R "cmd",
    // Start-menu, double-click) is adopted into a Windows Terminal window, but the
    // shell process stays a child of explorer.exe — NOT a descendant of
    // WindowsTerminal.exe — so the descendant walk above can't see it. Best-effort:
    // claim the lowest-PID unclaimed shell launched by the shell (parent = explorer).
    if shell_kind == "windows_terminal" {
        if let Some(shell_pid) = tree.first_adopted_console_shell(claimed) {
            claimed.insert(shell_pid);
            return cwd_for_pid(shell_pid).map(|c| (c, tree.stem_of(shell_pid)));
        }
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
    cmdline: HashMap<u32, String>,
    exe: HashMap<u32, String>,
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
        let mut cmdline = HashMap::new();
        let mut exe = HashMap::new();
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
            let cl = proc_
                .cmd()
                .iter()
                .map(|a| a.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            cmdline.insert(pid_u, cl);
            if let Some(path) = proc_.exe() {
                exe.insert(pid_u, path.to_string_lossy().into_owned());
            }
            if let Some(par) = proc_.parent() {
                let par_u = par.as_u32();
                parent.insert(pid_u, par_u);
                children.entry(par_u).or_default().push(pid_u);
            }
        }
        ProcTree { parent, children, stem, cmdline, exe }
    }

    fn parent(&self, pid: u32) -> Option<u32> {
        self.parent.get(&pid).copied()
    }

    fn stem_of(&self, pid: u32) -> String {
        self.stem.get(&pid).cloned().unwrap_or_default()
    }

    fn exe_of(&self, pid: u32) -> String {
        self.exe.get(&pid).cloned().unwrap_or_default()
    }

    /// True when a shell was launched to run a command and exit (a build tool, script,
    /// or the harness) rather than an interactive session the user is typing into.
    fn is_run_and_exit(&self, pid: u32) -> bool {
        let cl = self.cmdline.get(&pid).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
        cl.contains("/c ")
            || cl.contains("/s /c")
            || cl.contains("-command")
            || cl.contains("-encodedcommand")
            || cl.contains("-file ")
            || cl.contains("--headless")
    }

    /// PIDs of the interactive shells the user actually opened: a cmd/powershell/pwsh/
    /// bash whose parent is a shell launcher (explorer / a terminal host) and whose
    /// command line isn't a run-and-exit command. Excludes build tools and harness
    /// shells (children of node/chrome/the app), which is verified against real
    /// process trees. Sorted for stable ordering.
    fn interactive_shells(&self) -> Vec<u32> {
        const LAUNCHERS: &[&str] =
            &["explorer", "windowsterminal", "openconsole", "conhost", "wt", "mintty"];
        let mut out: Vec<u32> = self
            .stem
            .iter()
            .filter(|(pid, s)| {
                matches!(s.as_str(), "cmd" | "powershell" | "pwsh" | "bash")
                    && self
                        .parent(**pid)
                        .map(|par| LAUNCHERS.contains(&self.stem_of(par).as_str()))
                        .unwrap_or(false)
                    && !self.is_run_and_exit(**pid)
            })
            .map(|(pid, _)| *pid)
            .collect();
        out.sort_unstable();
        out
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

    /// Lowest-PID unclaimed shell whose parent is explorer.exe — a console that
    /// Windows 11's default-terminal handoff adopted into Windows Terminal without
    /// reparenting it under WindowsTerminal.exe. Used only after the descendant walk
    /// fails for a WT window, so standalone consoles (matched via self/parent) are
    /// already claimed and won't be grabbed here.
    fn first_adopted_console_shell(&self, claimed: &HashSet<u32>) -> Option<u32> {
        self.stem
            .keys()
            .copied()
            .filter(|&pid| {
                !claimed.contains(&pid)
                    && self.is_shell(pid)
                    && self
                        .parent(pid)
                        .and_then(|par| self.stem.get(&par))
                        .map(|s| s == "explorer")
                        .unwrap_or(false)
            })
            .min()
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
        "mintty" if git_bash_launcher_path(exe_path).is_some() => "git_bash",
        _ => "unknown",
    }
}

fn cwd_from_title(title: &str) -> Option<String> {
    let t = title.trim();
    if let Some(cwd) = git_bash_cwd_from_title(t) {
        return Some(cwd);
    }
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

/// Git for Windows' mintty title is normally `<MSYSTEM>:<POSIX cwd>`, for
/// example `MINGW64:/c/Users/me/project`. Convert it back to a Windows path.
fn git_bash_cwd_from_title(title: &str) -> Option<String> {
    let (environment, path) = title.trim().split_once(':')?;
    if !matches!(
        environment.to_ascii_uppercase().as_str(),
        "MSYS" | "MINGW32" | "MINGW64" | "UCRT64" | "CLANG32" | "CLANG64" | "CLANGARM64"
    ) {
        return None;
    }

    let path = path.trim();
    if path == "~" || path.starts_with("~/") {
        let home = std::env::var("USERPROFILE").ok()?;
        let suffix = path.strip_prefix('~').unwrap_or_default();
        let separator = std::path::MAIN_SEPARATOR.to_string();
        return Some(format!("{home}{}", suffix.replace('/', &separator)));
    }

    let bytes = path.as_bytes();
    if path.starts_with("//") {
        let separator = std::path::MAIN_SEPARATOR.to_string();
        return Some(path.replace('/', &separator));
    }
    if bytes.len() >= 2
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && (bytes.len() == 2 || bytes.get(2) == Some(&b'/'))
    {
        let drive = (bytes[1] as char).to_ascii_uppercase();
        let separator = std::path::MAIN_SEPARATOR.to_string();
        let suffix = match path.get(2..).unwrap_or_default() {
            "" => separator,
            suffix => suffix.replace('/', &separator),
        };
        return Some(format!("{drive}:{suffix}"));
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

/// Read a running cmd.exe's in-memory command history — what `doskey /history` shows.
/// cmd keeps no history file, but the console server holds the buffer, so we briefly
/// attach to the target's console and call `GetConsoleCommandHistoryW` (undocumented but
/// name-exported from kernel32 and stable for decades — the very API doskey uses). Works
/// through Windows Terminal's ConPTY (verified on-device). Best-effort: returns empty on
/// any failure. `AttachConsole` is process-global, so we `FreeConsole` before and after.
#[cfg(windows)]
fn read_cmd_console_history(pid: u32) -> Vec<String> {
    use windows::core::{s, w};
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};

    type AttachFn = unsafe extern "system" fn(u32) -> i32;
    type FreeFn = unsafe extern "system" fn() -> i32;
    type LenFn = unsafe extern "system" fn(*const u16) -> u32;
    type HistFn = unsafe extern "system" fn(*mut u16, u32, *const u16) -> u32;

    unsafe {
        let Ok(k32) = GetModuleHandleW(w!("kernel32.dll")) else {
            return vec![];
        };
        let (Some(a), Some(f), Some(gl), Some(gh)) = (
            GetProcAddress(k32, s!("AttachConsole")),
            GetProcAddress(k32, s!("FreeConsole")),
            GetProcAddress(k32, s!("GetConsoleCommandHistoryLengthW")),
            GetProcAddress(k32, s!("GetConsoleCommandHistoryW")),
        ) else {
            return vec![];
        };
        let attach: AttachFn = std::mem::transmute(a);
        let free: FreeFn = std::mem::transmute(f);
        let get_len: LenFn = std::mem::transmute(gl);
        let get_hist: HistFn = std::mem::transmute(gh);

        let exe: Vec<u16> = "cmd.exe".encode_utf16().chain(std::iter::once(0)).collect();

        free(); // drop any console we hold so AttachConsole can succeed
        if attach(pid) == 0 {
            return vec![];
        }
        let bytes = get_len(exe.as_ptr());
        let out = if bytes == 0 {
            vec![]
        } else {
            let mut buf = vec![0u16; (bytes as usize) / 2];
            let got = get_hist(buf.as_mut_ptr(), bytes, exe.as_ptr());
            let n = ((got as usize) / 2).min(buf.len());
            buf[..n]
                .split(|&c| c == 0)
                .filter(|part| !part.is_empty())
                .map(String::from_utf16_lossy)
                .collect()
        };
        free();
        out
    }
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

#[cfg(windows)]
fn read_terminal_history(shell: &str) -> Vec<String> {
    if shell != "git_bash" {
        return read_psreadline_history();
    }

    let home = match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(path) => path,
        Err(_) => return vec![],
    };
    read_history_tail(&std::path::PathBuf::from(home).join(".bash_history"))
}

#[cfg(windows)]
fn read_history_tail(path: &std::path::Path) -> Vec<String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return vec![],
    };
    let content = String::from_utf8_lossy(&bytes);
    let lines: Vec<String> = content.lines().map(str::to_string).collect();
    let start = lines.len().saturating_sub(MAX_HISTORY_LINES);
    lines[start..].to_vec()
}

// ── Restore ─────────────────────────────────────────────────────────────────

/// Build the launch command for a terminal process using its saved session.
/// Returns `None` to fall back to the raw cmd_line.
#[cfg(windows)]
pub struct TerminalLaunch {
    pub exe_path: String,
    pub cmd_line: String,
}

#[cfg(windows)]
pub fn terminal_launch_cmd(
    exe_path: &str,
    session: &TerminalSession,
    temp_dir: &std::path::Path,
    index: usize,
) -> Option<TerminalLaunch> {
    let shell = effective_shell(session, exe_path);
    let cmd_line = match shell {
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
            if session.cwd.is_empty() && session.history.is_empty() {
                None
            } else {
                // A .cmd script (run via /K, which keeps the window open) displays the
                // captured history then cd's. History is dumped from a sidecar file with
                // `type`, so command text isn't reparsed by the batch interpreter.
                let script_path = temp_dir.join(format!("restore_terminal_{index}.cmd"));
                let mut script = String::from("@echo off\r\n");
                if !session.history.is_empty() {
                    let history_path =
                        temp_dir.join(format!("restore_terminal_{index}.history"));
                    let body = session
                        .history
                        .iter()
                        .map(|c| format!("  {c}"))
                        .collect::<Vec<_>>()
                        .join("\r\n");
                    std::fs::write(&history_path, body).ok()?;
                    script.push_str("echo(\r\n");
                    script.push_str(
                        "echo   --- Recent history (this cmd session, at capture) ---\r\n",
                    );
                    script.push_str(&format!("type \"{}\"\r\n", history_path.to_string_lossy()));
                    script.push_str("echo(\r\n");
                    script.push_str(
                        "echo   ---------------------------------------------------\r\n",
                    );
                    script.push_str("echo(\r\n");
                }
                if !session.cwd.is_empty() {
                    script.push_str(&format!("cd /d \"{}\"\r\n", session.cwd));
                }
                std::fs::write(&script_path, script).ok()?;
                Some(format!(
                    "\"{}\" /K \"{}\"",
                    exe_path,
                    script_path.to_string_lossy()
                ))
            }
        }
        "windows_terminal" => {
            // Relaunch the actual inner shell directly. On Windows 11 (default terminal =
            // Windows Terminal) a launched powershell/cmd is auto-hosted in a WT window,
            // and the shell's own startup restores cwd + history — WT itself can't be
            // reliably handed a shell+script on its command line, and a bare `wt -d` sent
            // to a running WT instance drops the directory. Falls back to `wt -w new -d`
            // when the inner shell wasn't resolved at capture.
            match session.inner_shell.as_str() {
                "powershell" | "pwsh" => {
                    let script_path = temp_dir.join(format!("restore_terminal_{index}.ps1"));
                    std::fs::write(&script_path, build_ps_restore_script(session)).ok()?;
                    return Some(TerminalLaunch {
                        exe_path: format!("{}.exe", session.inner_shell),
                        cmd_line: format!(
                            "\"{}.exe\" -NoExit -ExecutionPolicy Bypass -File \"{}\"",
                            session.inner_shell,
                            script_path.to_string_lossy()
                        ),
                    });
                }
                "cmd" if !session.cwd.is_empty() => {
                    return Some(TerminalLaunch {
                        exe_path: "cmd.exe".to_string(),
                        cmd_line: format!("\"cmd.exe\" /K \"cd /d {}\"", session.cwd),
                    });
                }
                _ if !session.cwd.is_empty() => {
                    Some(format!("\"{}\" -w new -d \"{}\"", exe_path, session.cwd))
                }
                _ => None,
            }
        }
        "git_bash" => {
            let launcher = git_bash_launcher_path(exe_path)?;
            let cwd = if session.cwd.is_empty() {
                git_bash_cwd_from_title(&session.window_title)
            } else {
                Some(session.cwd.clone())
            };
            let arg = cwd
                .map(|cwd| format!("--cd={cwd}"))
                .unwrap_or_else(|| "--cd-to-home".to_string());
            let history_args = if session.history.is_empty() {
                String::new()
            } else {
                let history_path = temp_dir.join(format!("restore_git_bash_{index}.history"));
                let script_path = temp_dir.join(format!("restore_git_bash_{index}.sh"));
                std::fs::write(&history_path, session.history.join("\n")).ok()?;
                let script = build_bash_restore_script(&history_path)?;
                std::fs::write(&script_path, script).ok()?;
                let script_path = windows_path_to_msys(&script_path)?;
                format!(r#" -c "source {}""#, shell_single_quote(&script_path))
            };
            return Some(TerminalLaunch {
                exe_path: launcher.to_string_lossy().into_owned(),
                cmd_line: format!(
                    r#""{}" "{arg}"{history_args}"#,
                    launcher.to_string_lossy()
                ),
            });
        }
        _ => None,
    }?;

    Some(TerminalLaunch {
        exe_path: exe_path.to_string(),
        cmd_line,
    })
}

/// True when a saved terminal session belongs to the executable the user selected
/// for a per-app restore. Sessions are keyed by inner shell (powershell/pwsh/cmd/
/// git_bash), but the selectable app is usually the *host* process that owns the
/// window — WindowsTerminal.exe (or conhost) for the console shells, mintty for Git
/// Bash — so a host selection must match all the shells it hosts. `unknown` is
/// accepted for old Git Bash snapshots whose mintty titles still carry the cwd.
pub fn session_matches_executable(session: &TerminalSession, exe_path: &str) -> bool {
    let shell = effective_shell(session, exe_path);
    match executable_stem(exe_path).as_str() {
        // A console host relaunches all of its console shells. `windows_terminal` is the
        // legacy shell tag from schema-3 snapshots (kept so old snapshots still restore).
        "windowsterminal" | "wt" | "conhost" | "openconsole" => {
            matches!(shell, "powershell" | "pwsh" | "cmd" | "windows_terminal")
        }
        "mintty" => shell == "git_bash",
        // A shell selected directly (the app list occasionally surfaces the shell exe).
        "powershell" => shell == "powershell",
        "pwsh" => shell == "pwsh",
        "cmd" => shell == "cmd",
        _ => false,
    }
}

fn effective_shell<'a>(session: &'a TerminalSession, exe_path: &str) -> &'a str {
    if session.shell == "unknown"
        && executable_stem(exe_path) == "mintty"
        && git_bash_cwd_from_title(&session.window_title).is_some()
    {
        "git_bash"
    } else {
        session.shell.as_str()
    }
}

fn executable_stem(exe_path: &str) -> String {
    std::path::Path::new(exe_path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn git_bash_launcher_path(mintty_path: &str) -> Option<std::path::PathBuf> {
    std::path::Path::new(mintty_path)
        .ancestors()
        .map(|ancestor| ancestor.join("git-bash.exe"))
        .find(|candidate| candidate.is_file())
}

fn windows_path_to_msys(path: &std::path::Path) -> Option<String> {
    let separator = std::path::MAIN_SEPARATOR;
    let windows = path.to_string_lossy().replace(separator, "/");
    let bytes = windows.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        let suffix = windows.get(2..).unwrap_or_default().trim_start_matches('/');
        return Some(if suffix.is_empty() {
            format!("/{drive}")
        } else {
            format!("/{drive}/{suffix}")
        });
    }
    None
}

fn shell_single_quote(value: &str) -> String {
    let quote = char::from(39u8);
    format!("'{}'", value.replace(quote, r#"'"'"'"#))
}

fn build_bash_restore_script(history_path: &std::path::Path) -> Option<String> {
    let history_path = shell_single_quote(&windows_path_to_msys(history_path)?);
    Some(format!(
        r#"#!/usr/bin/env bash
printf '\n  --- Recent history (shared across Git Bash sessions) ---\n'
tail -n 20 -- {history_path} | while IFS= read -r line; do
  printf '  %s\n' "$line"
done
printf '  --------------------------------\n\n'
exec /usr/bin/bash --login -i
"#
    ))
}

#[cfg(all(windows, test))]
mod tests {
    use super::{
        cwd_for_pid, git_bash_cwd_from_title, read_history_tail, session_matches_executable,
        shell_type, terminal_launch_cmd,
    };
    use crate::TerminalSession;

    fn sess(shell: &str) -> TerminalSession {
        TerminalSession {
            shell: shell.to_string(),
            cwd: r"C:\x".to_string(),
            history: vec![],
            window_title: String::new(),
            inner_shell: String::new(),
            exe: format!("{shell}.exe"),
        }
    }

    #[test]
    fn per_app_restore_maps_host_to_its_shells() {
        let wt = r"C:\Program Files\WindowsApps\...\WindowsTerminal.exe";
        // Selecting the Windows Terminal host restores its console shells...
        assert!(session_matches_executable(&sess("powershell"), wt));
        assert!(session_matches_executable(&sess("cmd"), wt));
        assert!(session_matches_executable(&sess("pwsh"), wt));
        // ...but not git-bash (that's mintty's).
        assert!(!session_matches_executable(&sess("git_bash"), wt));
        assert!(session_matches_executable(&sess("git_bash"), r"C:\Git\usr\bin\mintty.exe"));
        // A directly-selected shell exe still matches its own shell only.
        assert!(session_matches_executable(&sess("cmd"), r"C:\Windows\System32\cmd.exe"));
        assert!(!session_matches_executable(&sess("powershell"), r"C:\Windows\System32\cmd.exe"));
    }

    #[test]
    fn parses_git_bash_title_into_windows_cwd() {
        let title = format!(
            "MINGW64:/c/Users/Alice/project with spaces{}",
            char::from_u32(0xa0).unwrap()
        );
        assert_eq!(
            git_bash_cwd_from_title(&title).as_deref(),
            Some(r"C:\Users\Alice\project with spaces")
        );
        assert_eq!(
            git_bash_cwd_from_title("MSYS:/d/repo/src-tauri").as_deref(),
            Some(r"D:\repo\src-tauri")
        );
        assert_eq!(
            git_bash_cwd_from_title("MINGW64:/c").as_deref(),
            Some(r"C:\")
        );
        assert_eq!(
            git_bash_cwd_from_title("MINGW64://server/share/project").as_deref(),
            Some(r"\\server\share\project")
        );
    }

    #[test]
    fn legacy_mintty_snapshot_uses_git_bash_launcher_and_saved_cwd() {
        let root = std::env::temp_dir()
            .join("pc_snapshot_git_bash_launcher_test")
            .join(std::process::id().to_string());
        let mintty = root.join("usr").join("bin").join("mintty.exe");
        let git_bash = root.join("git-bash.exe");
        std::fs::create_dir_all(mintty.parent().unwrap()).unwrap();
        std::fs::write(&mintty, []).unwrap();
        std::fs::write(&git_bash, []).unwrap();

        let session = TerminalSession {
            shell: "unknown".to_string(),
            cwd: String::new(),
            history: vec!["git status".to_string()],
            window_title: "MINGW64:/c/Users/Alice/project with spaces".to_string(),
            inner_shell: String::new(),
            exe: String::new(),
        };
        assert_eq!(shell_type(mintty.to_str().unwrap()), "git_bash");
        assert!(session_matches_executable(
            &session,
            mintty.to_str().unwrap()
        ));

        let launch =
            terminal_launch_cmd(mintty.to_str().unwrap(), &session, &root, 1).unwrap();
        assert_eq!(launch.exe_path, git_bash.to_string_lossy());
        let args = crate::tokenize(&launch.cmd_line);
        assert_eq!(args[0], git_bash.to_string_lossy());
        assert_eq!(args[1], r"--cd=C:\Users\Alice\project with spaces");
        assert_eq!(args[2], "-c");
        assert!(args[3].starts_with("source '/"));

        let script = std::fs::read_to_string(root.join("restore_git_bash_1.sh")).unwrap();
        let history = std::fs::read_to_string(root.join("restore_git_bash_1.history")).unwrap();
        assert!(script.contains("--- Recent history (shared across Git Bash sessions) ---"));
        assert!(script.contains("exec /usr/bin/bash --login -i"));
        assert_eq!(history, "git status");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bash_history_capture_keeps_the_latest_fifty_commands() {
        let root = std::env::temp_dir()
            .join("pc_snapshot_git_bash_history_test")
            .join(std::process::id().to_string());
        std::fs::create_dir_all(&root).unwrap();
        let history_path = root.join(".bash_history");
        let contents = (0..55)
            .map(|index| format!("command-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&history_path, contents).unwrap();

        let history = read_history_tail(&history_path);
        assert_eq!(history.len(), 50);
        assert_eq!(history.first().map(String::as_str), Some("command-5"));
        assert_eq!(history.last().map(String::as_str), Some("command-54"));

        let _ = std::fs::remove_dir_all(&root);
    }

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
            "Write-Host '  --- Recent history (shared across PowerShell sessions) ---' -ForegroundColor DarkCyan"
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
