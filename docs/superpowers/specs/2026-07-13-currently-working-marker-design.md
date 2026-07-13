# Currently-Working Session Marker — Design

**Date:** 2026-07-13
**Status:** Approved design, pending spec review

## Goal

Show a visual "Currently working" indicator on the snapshot card of the
last-restored session, and persist that state across app restarts.

## Source of Truth

A single marker file stored beside the snapshots:

`AppData/Snapshots/active_session.json`

```json
{ "id": "snap_<timestamp_ms>", "timestamp": "<ISO 8601>" }
```

- Absent / empty file = no active session.
- Keyed by snapshot **`id`**, not name — renames and duplicate names never
  confuse the marker.
- Lives in the same directory the backend already owns for snapshots and
  `activity.jsonl`, so it survives restarts with no extra machinery.

Rationale for a dedicated file over reusing `activity.jsonl`: restore events
in the activity log store only `snapshot_name` (`activity.rs`), which is
ambiguous. A tiny id-keyed marker is the reliable single source of truth.

## Lifecycle

| Event | Marker action |
| --- | --- |
| Restore a snapshot | **Set** marker to that snapshot's id (moves the badge) |
| Start new / close-all to clean desktop | **Clear** marker |
| Delete the active snapshot | **Clear** marker (only if deleted id matches) |
| Delete a non-active snapshot | No change |
| App restart | Marker read from disk; state restored |

No time-based expiry. The marker persists until one of the clearing events
above.

A restore **sets** the marker whenever the restore command runs, including
partial restores with warnings — a warned restore still means you are now
working in that session.

## Backend (Rust)

Marker helpers (new module or in `lib.rs` alongside existing storage code):

- `set_active_session(app, id)` — write `active_session.json`.
- `clear_active_session(app)` — remove / empty the file.
- `get_active_session(app) -> Option<ActiveSession>` — Tauri command exposed to
  the frontend. `ActiveSession { id: String, timestamp: String }`.

Wiring into existing commands:

- **Restore handler** (`restore.rs` / its `lib.rs` command): on completion call
  `set_active_session(id)`.
- **Start-new / close-all command**: call `clear_active_session()`.
- **`delete_snapshot`**: call `clear_active_session()` when the deleted id equals
  the current marker id.

All marker writes are best-effort and degrade to no-ops on IO error, matching
the app's "never crash, degrade gracefully" constraint.

## Frontend (React / TypeScript)

- New command wrapper in `src/commands/`:
  `getActiveSession(): Promise<{ id: string; timestamp: string } | null>`.
- `App.tsx` holds `activeSessionId` state. Load on mount; re-read after
  restore, start-new, and delete (the same points that already call
  `refreshActivity()` / `refresh()`).
- Pass `activeSessionId` down to `MissionControl`, which forwards the
  comparison to each card.

Card status line — `MissionControl.tsx` (currently line ~88):

- **Active card** (`s.id === activeSessionId`): render `● Currently working`
  in accent teal, slightly emphasized ("popping").
- **Warning card**: unchanged — `N warnings`.
- **Other good card**: render the relative timestamp only (e.g. `13m ago`).
  The `· All good` suffix is removed.

## Styling

Add a `.working` (or equivalent) rule in `src/index.css` for the emphasized
accent status line. Uses the existing accent token `--color-accent` (`#4bbfc3`);
no new color introduced.

## Testing / Verification

No automated test framework is configured. Verify by behavior in `npm run tauri
dev`:

1. Restore a snapshot → its card shows `● Currently working`; others show
   timestamp only.
2. Restart the app → the same card still shows `● Currently working`.
3. Restore a different snapshot → badge moves.
4. Start-new → badge clears.
5. Delete the active snapshot → badge clears; deleting a different one leaves it.

## Out of Scope

- Multiple simultaneous "active" sessions (exactly one at a time).
- Surfacing the marker anywhere other than the snapshot card.
- Time-based auto-expiry.
