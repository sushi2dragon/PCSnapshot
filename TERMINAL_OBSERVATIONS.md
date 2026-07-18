# PC Snapshot — Terminal Restore Observations

Log for the cases in `COWORK_TERMINAL_TEST.md`. Fill every column. Leave a cell
`?` only if you couldn't check it. Send this file back when done.

## Environment (fill once)

- Windows version: `______` (e.g. 11 Home 26200)
- Default terminal app (Settings → Privacy & security → For developers →
  Terminal, or Windows Terminal → Startup): `______`
  (Windows Terminal / Windows Console Host / Let Windows decide)
- Terminals you actually use day-to-day: `______`
- App run mode: `npm run tauri dev`  /  built release  (circle one)
- Date / build commit: `______`

## Results

Host = how you opened it: **STANDALONE** (own console window) or **WT** (inside
Windows Terminal). Captured cwd = the exact `cwd` string from the JSON
`terminal_sessions`. Restored dir = where the reopened terminal actually landed
(run `pwd` / `cd`).

| Case | Host (STANDALONE/WT) | JSON `shell` | Captured `cwd` (verbatim) | Window reopened? | Restored dir (actual) | CWD correct? | History shown? | Notes / anything weird |
|------|----------------------|--------------|---------------------------|------------------|-----------------------|--------------|----------------|------------------------|
| T1 Standalone PS, real path      |  |  |  |  |  |  |  |  |
| T2 Themed/no-path prompt         |  |  |  |  |  |  |  |  |
| T3 PS inside Windows Terminal    |  |  |  |  |  |  |  |  |
| T4 cmd.exe, path with spaces     |  |  |  |  |  |  |  |  |
| T5 Two PS windows, two dirs      |  |  |  |  |  |  |  |  |
| T6 Reuse running terminal        |  |  |  |  |  |  |  |  |
| T7 Non-default shell (___)       |  |  |  |  |  |  |  |  |
| T8 History block                 |  |  |  |  |  |  |  |  |

### T5 detail (two windows)
- Number of `terminal_sessions` entries in JSON: `___` (expect 2, not 4)
- Session 1: cwd `______` → restored dir `______`
- Session 2: cwd `______` → restored dir `______`
- Were the two dirs swapped or duplicated? `______`

## Free-form notes

Anything the table doesn't capture — error toasts, extra/duplicate windows,
windows opening then closing, wrong shell launched, restore report warnings,
timing (blank window for how long), etc.:

-
-
-

## Quick triage key (for my reference when you send it back)

- Captured cwd WRONG/EMPTY  → capture-side. If Host=WT, it's the known WT gap;
  if Host=STANDALONE, the PEB read regressed — flag it.
- Captured cwd RIGHT but restored dir WRONG → restore-side (launch/quote/match).
- Window didn't reopen at all → matching or launch failure; note the JSON `shell`.
- Second window appeared (T6) instead of reuse → reuse logic; expected reuse.
