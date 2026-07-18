# PC Snapshot — Terminal Restore Diagnostic (Cowork input)

Goal: find out **exactly what restores and what doesn't** for terminals, and
split every failure into a **capture** problem (wrong data in the snapshot JSON)
vs a **restore** problem (right data, wrong behavior). Self-contained — no other
conversation context needed.

Log every result in `TERMINAL_OBSERVATIONS.md` (same folder). Fill one row per
case. Failing honestly beats passing optimistically — record what you actually
saw, including "didn't open at all".

---

## Setup

1. Project root: `C:\Users\sarth\OneDrive\Desktop\projects\PC Snapshot`
2. Launch: `npm run tauri dev` from the root (first Rust build is slow; if you
   run cargo directly, `cd src-tauri` first so the target-dir redirect applies).
3. Snapshot storage: `%APPDATA%\com.pc-snapshot\Snapshots\` — one `{id}.json`
   per snapshot, newest by timestamp in the filename (`snap_<ms>`).
4. Keep a text editor open on the newest `.json` between steps. The block that
   matters is `"terminal_sessions"`, an array of:
   ```json
   { "shell": "...", "cwd": "...", "history": [...], "window_title": "..." }
   ```
   `shell` is one of: `powershell`, `pwsh`, `cmd`, `windows_terminal`, `unknown`.

### CRITICAL: turn on Terminal Capture first, and know why

A hard fact discovered in testing: **PowerShell never exposes its current
directory to the OS.** When you `cd` in PowerShell, `Get-Location` changes but
the process's Win32 working directory does not — so *nothing* reading the
process from outside (window title by default, process memory, sysinfo) can see
where you are. `cmd` is different: `cd` there *does* update the process, so cmd
is always readable.

To make PowerShell capturable, PC Snapshot ships an **opt-in hook**:
**Settings → Terminal Capture: Off → click to turn On.** This adds a few lines
to your PowerShell `$PROFILE` that mirror `$PWD` into the window title on every
prompt; capture then reads the directory back from the title. **Turn it on, then
open a NEW PowerShell/Windows Terminal window** (existing windows won't have run
the new profile). Without it, PowerShell captures only the *launch* directory.

So for every case, record **two** things:
1. **Terminal Capture On or Off** when you captured.
2. **How you opened the terminal** — inside Windows Terminal ("let Windows
   decide", the Win11 default) or as a standalone console window.

Expected with Terminal Capture ON: PowerShell, pwsh, and Windows Terminal tabs
all capture the real current directory from the title. With it OFF: only `cmd`
and never-`cd`'d shells are correct.

---

## How to read each case

Every case has two checks. Do them in order and log both:

- **CAPTURE check** — after taking the snapshot, open the JSON and read the
  relevant `terminal_sessions` entry. Is `shell` right? Is `cwd` the actual
  directory the terminal was in (exact string)?
- **RESTORE check** — close the terminal(s), run the restore, and observe: did a
  terminal window reopen? Was it the right shell? Did it land in the right
  directory? Did history print?

If CAPTURE is already wrong, the restore can't be right — say so and move on.

---

## Cases

### T1 — Standalone PowerShell, non-default directory
1. Open a **standalone** PowerShell (see host note above), then `cd` to a folder
   with no spaces, e.g. `cd C:\Windows\System32\drivers`.
2. Take a snapshot ("T1").
3. **CAPTURE:** JSON has a session with `shell: "powershell"` and
   `cwd: "C:\\Windows\\System32\\drivers"` (exact). Record the actual cwd string.
4. Close the PowerShell window.
5. **RESTORE:** restore T1. Expect one PowerShell window that opens **already in**
   `C:\Windows\System32\drivers` (prompt shows it; run `pwd` to confirm), with a
   "--- Restored session history ---" block above the prompt.

### T2 — PowerShell with a themed/no-path prompt
The whole reason the OS-read exists. Make the prompt NOT show the path: run
`function prompt { "PS> " }` in a standalone PowerShell, then `cd C:\Users`.
1. Snapshot ("T2").
2. **CAPTURE:** `cwd` must still be `C:\Users` even though the title/prompt shows
   no path (this proves the PEB read, not title-scraping). Record what `cwd`
   actually says — empty or wrong here is the key finding.
3. Close it. **RESTORE:** expect it to reopen in `C:\Users`.

### T3 — Same shell, inside Windows Terminal
1. Open PowerShell **inside Windows Terminal** (normal Win11 open), `cd C:\Temp`
   (create it if needed).
2. Snapshot ("T3").
3. **CAPTURE:** expect `shell: "windows_terminal"`. Record `cwd` — it will be
   whatever the tab title / `-d` yields, often empty. This documents the WT gap.
4. Close the WT window. **RESTORE:** observe whether a terminal reopens at all,
   which shell, and the directory. Log verbatim.

### T4 — cmd.exe with a path containing spaces
1. Standalone `cmd.exe`, then `cd /d "C:\Program Files"`.
2. Snapshot ("T4").
3. **CAPTURE:** `shell: "cmd"`, `cwd: "C:\\Program Files"`.
4. Close it. **RESTORE:** expect a cmd window in `C:\Program Files` (the space is
   the thing being tested — a broken quote would land it in `C:\` or error).

### T5 — Two PowerShell windows, different directories
Tests per-window mapping (this used to cross-assign CWDs).
1. Open **two** standalone PowerShell windows: one in `C:\Windows`, one in
   `C:\Users`. Give them distinguishable titles if you can (e.g.
   `$Host.UI.RawUI.WindowTitle = "WIN"` and `"USR"`).
2. Snapshot ("T5").
3. **CAPTURE:** two sessions, each with the correct matching `cwd`. Confirm they
   are NOT both the same directory and there are exactly two (not four).
4. Close both. **RESTORE:** expect two windows, each in its own directory.

### T6 — Reuse of an already-running terminal
Documents that a running terminal is left untouched.
1. Snapshot with one standalone PowerShell in `C:\Windows` ("T6").
2. Do NOT close it — instead `cd C:\Users` in that same window.
3. **RESTORE T6** with close-others OFF. Observe: does the app leave the existing
   window at `C:\Users` (reused, not corrected), or open a second one in
   `C:\Windows`? Log which. (Expected: reused as-is, CWD not changed.)

### T7 — Non-default shell (pwsh / Git Bash / WezTerm / Alacritty), if you use one
Only if you actually use one of these.
1. Open it, `cd` somewhere distinctive, snapshot ("T7").
2. **CAPTURE:** note the `shell` value. `pwsh` is handled; `bash`, `wezterm`,
   `alacritty`, `mintty`, `tabby`, bare `conhost` capture as `unknown` and get
   **no** CWD restore (relaunched from raw command line only).
3. Close, **RESTORE**, log what happened.

### T8 — History block
1. In a standalone PowerShell, run a few obvious commands (`echo hello`,
   `Get-Date`). Snapshot ("T8"), close, restore.
2. Observe whether the restored window prints the "--- Restored session history
   ---" block and whether your recent commands appear. (History is global
   PSReadLine, so every restored PowerShell shows the *same* recent block — note
   if that's confusing.)

---

## What to hand back

Fill `TERMINAL_OBSERVATIONS.md` and send it to me. The rows I most need:
the **host** (standalone vs Windows Terminal), the **captured `cwd` from the
JSON**, and the **actual restored directory**. That trio tells me instantly
whether each failure is capture-side (likely Windows Terminal or an unknown
shell) or restore-side (right data, wrong launch).

## Known limitations (don't file as bugs, but DO log if hit)
- Windows Terminal tabs/panes: CWD is title/`-d`-inferred, not OS-read.
- `bash`/`wezterm`/`alacritty`/`mintty`/`tabby`/`conhost`: no CWD restore.
- History is global, not per-window — same block in every restored PowerShell.
- An already-running terminal is reused, not repositioned/re-homed.
