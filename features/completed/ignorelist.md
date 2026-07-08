# Feature — Process Ignore List

## Goal

Let the user configure a list of apps/processes that PC Snapshot will never close or capture in any feature.

---

## Location

Accessible as a subtab under the Settings menu (the `···` menu).

---

## Behavior

- The ignore list is a set of exe names (e.g. `slack.exe`, `spotify.exe`).
- Ignored processes are:
  - **Never closed** by Start New Session.
  - **Never captured** in snapshots (excluded from process list, windows, and context clues).
  - **Never restored** (skipped during restore even if present in old snapshot data).
- The list persists locally (stored alongside app config, not inside snapshot JSON).
- UI: simple list with an Add (text input or pick from running processes) and Remove button per entry.

---

## Constraints

- System-critical processes (explorer.exe, csrss.exe, svchost.exe, etc.) are always implicitly ignored and cannot be removed from protection — this is a separate hardcoded list, not user-editable.
- Changes take effect immediately; no restart required.
- The ignore list is stored in `AppData/config.json` (or similar), not in individual snapshot files.
