# Feature — Start New Session

## Goal

Let the user wipe all running apps and start a clean desktop, with PC Snapshot minimized to the system tray.

---

## Trigger

New button in the UI: **Start New Session** (alongside Take Snapshot).

---

## Flow

1. User clicks "Start New Session".
2. If no snapshot was saved since the last session change, show a confirmation dialog: "You haven't saved a snapshot. Start fresh anyway?" with **Save & Continue**, **Continue Without Saving**, and **Cancel**.
3. If a snapshot was just saved (or user confirms), proceed.
4. Close every visible window and terminate every non-essential process — **except** PC Snapshot itself and anything on the ignore list (see `ignorelist.md`).
5. Minimize PC Snapshot to the system tray.
6. User now has a clean desktop and can begin new work, optionally saving a snapshot later.

---

## Constraints

- Process termination must be graceful first (`WM_CLOSE`), force-kill only after timeout.
- Never kill system-critical processes (explorer.exe, csrss.exe, etc.) — maintain a hardcoded system-critical list separate from the user ignore list.
- The confirmation dialog is skipped if a snapshot was saved within the last 60 seconds.
- Respect the user-configured ignore list for apps that should never be closed.
