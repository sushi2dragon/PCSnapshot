# PC Snapshot — Cowork Test Plan

Input for a Claude Cowork session: drive the running app and verify each case
below. This file is self-contained — no other conversation context is needed.

## Setup

1. Project root: `C:\Users\sarth\OneDrive\Desktop\projects\PC Snapshot`
2. Launch the desktop app with `npm run tauri dev` from the project root
   (first Rust build takes several minutes; artifacts go to
   `C:/cargo-build/pc-snapshot` per `src-tauri/.cargo/config.toml`).
   If running cargo directly, always `cd src-tauri` first — from the repo root
   cargo misses the target-dir redirect and writes into an OneDrive-poisoned
   `src-tauri/target/`.
3. Snapshot storage to inspect between steps:
   `%APPDATA%\com.pc-snapshot\Snapshots\` (one `{id}.json` + `{id}.png` per
   snapshot; ids look like `snap_<timestamp_ms>`).
4. Report every deviation with what you did, what you expected, and what
   actually happened. Failing honestly beats passing optimistically.

## A. Startup & regression sentinels (recently changed — check first)

- **A1. App loads under the new CSP.** `tauri.conf.json` now sets a real
  Content-Security-Policy (was `null`). Verify the UI renders styled (dark
  #1c1c1f background, teal accent), thumbnails display on tiles, and every
  button works. If the window is unstyled, images are broken, or clicks do
  nothing, the CSP string in `src-tauri/tauri.conf.json` (`app.security.csp`)
  is too strict — report exactly what broke (check the webview devtools
  console for CSP violation lines).
- **A2. Empty state.** With no snapshots stored, the app shows a centered
  "Take Snapshot" button, nothing else.

## B. Capture

- **B1. Basic capture.** Open a few apps (Notepad, File Explorer, a browser
  with 2 tabs, a PowerShell window in a specific directory). Click Take
  Snapshot → the name prompt appears with a default like "Snapshot 01",
  pre-selected. Press Enter. Expect a success toast and a new tile with a real
  screenshot thumbnail within ~3 seconds.
- **B2. Snapshot JSON sanity.** Open the new `.json`: `schema_version: 2`,
  non-empty `processes` (each with name/pid/exe_path/cmd_line/classification),
  `windows` with plausible positions/sizes/monitor_index, `context_clues`, and
  `restore_hints`. The PowerShell window should yield a terminal session/CWD
  clue; the browser should yield `browser_tab:` hints.
- **B3. Default-name numbering after deletion.** Take snapshots so you have
  "Snapshot 01"–"Snapshot 03". Delete "Snapshot 02" (hover tile → trash icon →
  click again to confirm). Open the capture prompt: the default must be
  "Snapshot 04", NOT a duplicate "Snapshot 03". (Bug fixed this session — the
  number now derives from the max existing name, not the count.)
- **B4. Custom name.** Capture with a custom name including spaces and unicode
  (e.g. "Deep Work — résumé"). The tile shows it verbatim.
- **B5. Escape cancels.** Open the name prompt, press Esc → modal closes, no
  snapshot created.

## C. Browse / grid

- **C1. Search** filters tiles by name, case-insensitive; the "no snapshots
  match" message appears for garbage input; clearing the search restores all.
- **C2. Refresh button** spins once per click and ignores rapid re-clicks
  (it's disabled while in flight — fixed this session).
- **C3. Relative timestamps** read "Just now" / "Nm ago" / "Nh ago".

## D. Restore

- **D1. Plain restore (close-others OFF).** Take a snapshot with Notepad +
  PowerShell open. Close Notepad. Click the tile → confirmation modal appears,
  briefly shows "Checking your current desktop…". UNCHECK "Close apps that
  aren't part of this snapshot". Confirm. Expect Notepad relaunched and
  repositioned; nothing else closed. **Critically: any terminal windows you
  opened after the snapshot (with different titles/CWDs) must NOT be closed**
  — plain restore never closes anything (bug fixed this session).
- **D2. Clean restore (close-others ON).** Same snapshot, but open one extra
  app (e.g. Calculator) first and leave the checkbox ON. After restore, the
  extra app got a polite close (WM_CLOSE — unsaved-work prompts allowed), and
  the report modal lists it under "Closed (not in snapshot)". Terminals closed
  by reconciliation must also appear in that list.
- **D3. No duplicate terminals.** Take a snapshot with exactly one PowerShell
  window. Close it. Restore. Expect exactly ONE PowerShell window afterwards,
  not two (double-launch bug fixed this session — reconciliation now skips
  exes the launch loop already handled).
- **D4. Restore report colors.** Force a partial restore (snapshot an app,
  uninstall/rename nothing — instead edit the JSON's `exe_path` for one
  process to a nonexistent path, e.g. `C:\nope\fake.exe`). Restore → report
  modal opens; the failed item's name renders red (#f87171), warnings orange,
  closed items teal (color prop was dead until this session). Esc dismisses
  the report.
- **D5. Failure honesty.** Delete a snapshot's `.json` from disk while the app
  is open, then click its tile and confirm. Expect a warning toast
  ("Restore failed: …"), not silence and not a green success toast (both were
  possible before this session's fixes).
- **D6. Keyboard.** In the restore confirmation: Esc cancels, Enter restores
  (Help toast promises this; was previously broken in 3 of 5 modals).
- **D7. Save-current-first flow.** Arrange a desktop that matches no snapshot,
  click a tile → the modal should show the unsaved warning and a "Save current
  first" button. Click it → name prompt opens; save → you land back in the
  restore confirmation for the originally chosen snapshot.
- **D8. Stale-check race.** Click tile A, immediately Esc, click tile B. The
  "checking…" state must resolve to B's answer only (token-guarded now); no
  flicker of a wrong warning banner.

## E. Recapture

- **E1.** Hover a tile → circular-arrow button (top-left) → confirm modal
  (Esc/Enter work). Confirm → toast "Snapshot updated"; the tile keeps its id
  and name but the thumbnail and `timestamp` change; JSON `processes` reflect
  the *current* desktop.
- **E2. Original survives failure.** If capture fails mid-recapture the old
  JSON must remain intact (temp-file + rename). Also verify no stray
  `{id}_tmp.png` / `{id}_tmp.json` files linger in the Snapshots dir after
  any recapture (leak fixed this session).

## F. Ignore list

- **F1.** Settings → Ignore List. Modal opens focused on the input, shows a
  live "Running processes" list. Add one (e.g. `spotify.exe`), also add by
  clicking a running process. Entries persist across app restarts.
- **F2. Protected process.** Try adding `explorer.exe` → inline red error
  ("system-critical"), not an unhandled rejection.
- **F3. Effect on capture.** Add an open app (e.g. notepad) to the ignore
  list, take a snapshot → it appears in neither `processes` nor `windows` of
  the JSON, and a clean restore never closes it.
- **F4. Fresh state per open.** Type a filter, close the modal, reopen → the
  filter/input/error are cleared (modal remounts per open now).

## G. Toasts, Help, misc

- **G1.** Toasts self-dismiss after ~4s; a second action replaces the current
  toast and resets the timer.
- **G2.** Settings → Clear All requires a second confirming click; after it,
  the grid returns to the empty state and the Snapshots dir has no
  json/png files left.
- **G3.** Settings → Import shows a "not yet implemented" warning toast (known
  stub, not a bug).
- **G4.** Text selection: you can select/copy text inside the name input and
  ignore-list inputs (global user-select:none is overridden for inputs).
- **G5.** Corrupt-file tolerance: write a garbage `.json` (e.g. `not json`)
  into the Snapshots dir, refresh the grid → the app must not crash and the
  bad file is silently skipped.

## H. Performance

- **H1.** Capture with ~10 windows open completes in < 3 seconds (toast
  appears; thumbnail present).
- **H2.** The UI never freezes during restore (the engine runs off the async
  runtime; the window stays responsive while windows are being placed).

## Known limitations (do NOT report as bugs)

- Import is a stub (G3).
- File Explorer windows are not relaunched on restore; they surface as honest
  warnings in the report.
- Restore repositioning is best-effort: apps that rename their windows or are
  slow to start may land as "could not reposition" warnings.
- Office multi-window restore depends on the registry MRU; files absent from
  the MRU open as blank documents (a 0.55-confidence clue records the name).
