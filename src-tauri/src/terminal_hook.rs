//! Opt-in PowerShell profile hook.
//!
//! PowerShell keeps its current location inside its runspace and never writes it
//! to the process's Win32 current directory, so nothing outside the process can
//! read where the user `cd`'d to. This hook makes PowerShell cooperate: it
//! mirrors `$PWD` into the window title on every prompt, and the capture engine
//! reads the directory back from the title (`terminal::cwd_from_title`).
//!
//! The block is delimited by markers so it can be added/removed idempotently,
//! and it *wraps* any existing `prompt` function instead of clobbering it.

const MARK_BEGIN: &str = "# >>> PC Snapshot cwd hook >>>";
const MARK_END: &str = "# <<< PC Snapshot cwd hook <<<";

#[cfg(windows)]
const HOOK_BODY: &str = "# >>> PC Snapshot cwd hook >>>
# Mirrors the current directory into the window title so PC Snapshot can capture
# this terminal's working directory. PowerShell doesn't expose its location to
# the OS, so this cooperation is required. Safe to delete this whole block.
$__pcsnap_inner = $function:prompt
function prompt {
    try { $Host.UI.RawUI.WindowTitle = $PWD.Path } catch {}
    if ($__pcsnap_inner) { & $__pcsnap_inner } else { \"PS $($PWD.Path)> \" }
}
# <<< PC Snapshot cwd hook <<<
";

/// Per-user profile path for each installed PowerShell (Windows PowerShell 5.1
/// and PowerShell 7). Resolved by asking each shell for `$PROFILE` so OneDrive
/// Documents-folder redirection is handled correctly.
#[cfg(windows)]
fn profile_paths() -> Vec<std::path::PathBuf> {
    use std::path::PathBuf;
    let mut out: Vec<PathBuf> = Vec::new();
    for exe in ["powershell.exe", "pwsh.exe"] {
        if let Ok(o) = std::process::Command::new(exe)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$PROFILE.CurrentUserCurrentHost",
            ])
            .output()
        {
            if o.status.success() {
                let p = String::from_utf8_lossy(&o.stdout).trim().to_string();
                let pb = PathBuf::from(&p);
                if !p.is_empty() && !out.contains(&pb) {
                    out.push(pb);
                }
            }
        }
    }
    out
}

/// True if at least one discovered PowerShell profile contains the hook.
#[cfg(windows)]
pub fn is_installed() -> bool {
    profile_paths().iter().any(|p| {
        std::fs::read_to_string(p)
            .map(|c| c.contains(MARK_BEGIN))
            .unwrap_or(false)
    })
}

/// Append the hook to every PowerShell profile that lacks it. Idempotent.
#[cfg(windows)]
pub fn install() -> Result<String, String> {
    let profiles = profile_paths();
    if profiles.is_empty() {
        return Err("No PowerShell found on this system.".to_string());
    }
    let mut touched = 0;
    for path in &profiles {
        let existing = std::fs::read_to_string(path).unwrap_or_default();
        if existing.contains(MARK_BEGIN) {
            continue; // already installed
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Could not create {}: {e}", parent.display()))?;
        }
        // Separate from any prior content with a blank line.
        let sep = if existing.is_empty() || existing.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        let new_content = format!("{existing}{sep}\n{HOOK_BODY}");
        std::fs::write(path, new_content)
            .map_err(|e| format!("Could not write {}: {e}", path.display()))?;
        touched += 1;
    }
    Ok(format!(
        "Terminal directory capture enabled ({touched} profile{} updated). Open a new PowerShell window for it to take effect.",
        if touched == 1 { "" } else { "s" }
    ))
}

/// Remove the hook block from every profile that has it. Idempotent.
#[cfg(windows)]
pub fn uninstall() -> Result<String, String> {
    let mut touched = 0;
    for path in profile_paths() {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        if !content.contains(MARK_BEGIN) {
            continue;
        }
        let cleaned = strip_block(&content);
        std::fs::write(&path, cleaned)
            .map_err(|e| format!("Could not write {}: {e}", path.display()))?;
        touched += 1;
    }
    Ok(format!(
        "Terminal directory capture disabled ({touched} profile{} updated).",
        if touched == 1 { "" } else { "s" }
    ))
}

/// Remove the marked block (inclusive) and any blank line left immediately
/// before it. Leaves the rest of the profile untouched.
#[cfg(windows)]
fn strip_block(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let begin = lines.iter().position(|l| l.trim() == MARK_BEGIN);
    let end = lines.iter().position(|l| l.trim() == MARK_END);
    let (Some(mut b), Some(e)) = (begin, end) else {
        return content.to_string();
    };
    if e < b {
        return content.to_string();
    }
    // Also drop a single blank separator line just above the block.
    if b > 0 && lines[b - 1].trim().is_empty() {
        b -= 1;
    }
    let kept: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| *i < b || *i > e)
        .map(|(_, l)| *l)
        .collect();
    let mut out = kept.join("\n");
    if content.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out
}

// ── Non-Windows stubs ─────────────────────────────────────────────────────────

#[cfg(not(windows))]
pub fn is_installed() -> bool {
    false
}

#[cfg(not(windows))]
pub fn install() -> Result<String, String> {
    Err("Terminal directory capture is only available on Windows.".to_string())
}

#[cfg(not(windows))]
pub fn uninstall() -> Result<String, String> {
    Err("Terminal directory capture is only available on Windows.".to_string())
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;

    #[test]
    fn strip_block_removes_only_the_hook_and_preserves_the_rest() {
        let user = "Set-Alias ll Get-ChildItem\n$env:FOO = 'bar'\n";
        let with_hook = format!("{user}\n{HOOK_BODY}");
        // The hook (and its blank separator) must vanish, leaving the user's
        // content byte-for-byte.
        assert_eq!(strip_block(&with_hook), user);
        // Idempotent: stripping content that has no block is a no-op.
        assert_eq!(strip_block(user), user);
        // A profile that is *only* the hook strips to empty.
        assert!(strip_block(HOOK_BODY).trim().is_empty());
    }
}
