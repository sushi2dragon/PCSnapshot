# Terminal Restore Diagnostic — Your Checklist

Run each case yourself, then send me your raw notes (even messy/verbatim is fine — I'll write the final `TERMINAL_OBSERVATIONS.md`). For every case I need three things at minimum: **host** (standalone vs Windows Terminal), **captured `cwd`** (copy-paste the exact JSON string), and **actual restored directory**.

## Setup

1. `cd` to `C:\Users\sarth\OneDrive\Desktop\projects\PC Snapshot`, run `npm run tauri dev`, wait for the app window.
2. Keep a text editor open on the newest file in `%APPDATA%\com.pc-snapshot\Snapshots\` — filenames are `snap_<timestamp_ms>.json`, so newest = highest number. Re-open it after each snapshot (don't rely on a stale open tab).
3. For each case below, "standalone" means the terminal's own process is the shell (open via Win+R → `powershell.exe` or `cmd.exe`, not from the Start menu, unless you've set Windows Terminal → Settings → Startup → Default terminal application = **Windows Console Host**). "Inside Windows Terminal" means opened the normal Win11 way.

For each case, log: host, the `terminal_sessions` entry's `shell` + `cwd` (exact JSON string), whether that's correct, then close the terminal(s), hit Restore in the app, and log what actually opened (shell / directory / history block present or not).

---

### T1 — Standalone PowerShell, non-default dir
1. Standalone PowerShell → `cd C:\Windows\System32\drivers`
2. Take snapshot named "T1"
3. Open JSON → note `shell` and `cwd` for this session
4. Close the window
5. Restore T1 → note: did a window open? which shell? run `pwd` in it — what does it show? does a "--- Restored session history ---" block appear above the prompt?

### T2 — PowerShell with a themed/no-path prompt
1. Standalone PowerShell → run `function prompt { "PS> " }` then `cd C:\Users`
2. Snapshot "T2"
3. Open JSON → note `cwd` (this is the key test — should be `C:\Users` even though the prompt shows nothing)
4. Close, restore T2 → note actual directory it lands in

### T3 — Same shell, inside Windows Terminal
1. Open PowerShell the normal Win11 way (inside Windows Terminal) → `cd C:\Temp` (create the folder first if it doesn't exist)
2. Snapshot "T3"
3. Open JSON → note `shell` (expect `windows_terminal`) and `cwd` (likely empty — that's expected, not a bug)
4. Close the WT window, restore T3 → note: did anything reopen at all? which shell? which directory (verbatim, even if wrong/blank)?

### T4 — cmd.exe, path with spaces
1. Standalone `cmd.exe` → `cd /d "C:\Program Files"`
2. Snapshot "T4"
3. Open JSON → note `shell` (expect `cmd`) and `cwd` (expect `C:\Program Files`)
4. Close, restore T4 → note actual directory (watch for it landing in `C:\` or erroring — that'd mean a quoting bug)

### T5 — Two standalone PowerShell windows, different dirs
1. Window A: standalone PowerShell → `cd C:\Windows`, then `$Host.UI.RawUI.WindowTitle = "WIN"`
2. Window B: standalone PowerShell → `cd C:\Users`, then `$Host.UI.RawUI.WindowTitle = "USR"`
3. Snapshot "T5"
4. Open JSON → count `terminal_sessions` entries (expect exactly 2, not 4) and note each one's `cwd` — confirm they're not both the same directory and each matches the right window
5. Close both, restore T5 → note how many windows reopened and each one's directory

### T6 — Reuse of an already-running terminal
1. Standalone PowerShell in `C:\Windows` → snapshot "T6"
2. Do **not** close it — instead `cd C:\Users` in that same window
3. Restore T6 with close-others OFF → note: does the existing window stay at `C:\Users` untouched (expected), or does a second window open at `C:\Windows`, or does something else happen?

### T7 — Non-default shell (only if you use one: pwsh, Git Bash, WezTerm, Alacritty, etc.)
1. Open it, `cd` somewhere distinctive, snapshot "T7"
2. Open JSON → note the `shell` value
3. Close, restore → note what happened (expected: `pwsh` gets CWD restore; `bash`/`wezterm`/`alacritty`/`mintty`/`tabby`/bare `conhost` show up as `unknown` and relaunch from raw command line only, no CWD)
4. Skip this case entirely if you don't use a non-default shell

### T8 — History block
1. Standalone PowerShell → run `echo hello` and `Get-Date`
2. Snapshot "T8", close, restore
3. Note: does the restored window show "--- Restored session history ---"? Do `echo hello` / `Get-Date` appear in it? (History is global PSReadLine — every restored PowerShell window will show the same recent block, not a per-window one. Just note if that reads as confusing.)

---

## When you're done

Send me whatever you captured per case — exact `cwd` strings from the JSON and exact restored directories matter most. I'll classify each failure as capture-side (wrong data in the JSON) vs restore-side (right data, wrong launch behavior) and write up `TERMINAL_OBSERVATIONS.md`.
