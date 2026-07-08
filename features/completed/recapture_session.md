# Feature — Recapture Existing Session

## Goal

Let the user update an already-saved snapshot with the current desktop state, preserving the original name and identity.

---

## Trigger

When viewing the snapshot grid, each tile gets a new hover action: **Recapture** (or an update/refresh icon). Alternatively, a "Recapture" option in a right-click context menu on the tile.

---

## Flow

1. User clicks Recapture on an existing snapshot tile.
2. Confirmation prompt: "Overwrite [Snapshot Name] with current desktop state?"
3. On confirm: run the full capture pipeline (windows, processes, context, thumbnail) and overwrite the existing snapshot's JSON and PNG, keeping the same `id` and `name`. Update the `timestamp` to now.
4. Show a toast: "Snapshot updated" (success) or warnings if partial.

---

## Constraints

- The snapshot `id` and `name` are preserved; only the captured data and timestamp change.
- The old thumbnail PNG is overwritten in place (same filename).
- If the capture fails entirely, the original snapshot is left untouched (write to a temp file first, then rename).
- Respect the ignore list — excluded processes are not captured on recapture either.
