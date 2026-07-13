# Currently-Working Session Marker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mark the last-restored snapshot card as "Currently working", persisted across app restarts.

**Architecture:** A backend-owned marker file `AppData/Snapshots/active_session.json` (keyed by snapshot id) is set on restore and cleared on start-new / delete-of-active / clear-all. The frontend reads it on load and after those actions, and the matching card renders an emphasized accent status line instead of the default one.

**Tech Stack:** Rust / Tauri 2 (backend commands, `serde`, `chrono`), React 19 + TypeScript, plain CSS (`src/index.css`).

## Global Constraints

- Marker writes are best-effort: every IO/serde error degrades to a no-op, never a panic or a returned error (app's "never crash, degrade gracefully" rule).
- Marker is keyed by snapshot `id`, never `name`.
- Exactly one active session at a time.
- No time-based expiry — the marker persists until restore (moves it), start-new (clears), delete-of-active (clears), or clear-all (clears).
- Accent color is the existing token `--color-accent` (`#4bbfc3`); introduce no new color.
- Run `cargo` from `src-tauri/` so the OneDrive target-dir redirect applies.

---

### Task 1: Backend marker module + `get_active_session` command

**Files:**
- Create: `src-tauri/src/active_session.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod active_session;` near the other `mod` decls; register `active_session::get_active_session` in the `generate_handler!` list at `lib.rs:838`)

**Interfaces:**
- Produces (consumed by Task 2):
  - `active_session::set(app: &tauri::AppHandle, id: &str)`
  - `active_session::clear(app: &tauri::AppHandle)`
  - `active_session::current_id(app: &tauri::AppHandle) -> Option<String>`
- Produces (consumed by Task 3, over IPC): command `get_active_session() -> Option<ActiveSession>` where `ActiveSession { id: String, timestamp: String }`.

- [ ] **Step 1: Create the module**

Create `src-tauri/src/active_session.rs`. Mirrors the self-contained path pattern in `activity.rs`:

```rust
use serde::{Deserialize, Serialize};
use tauri::Manager;

/// Persisted marker for the snapshot the user is currently working in.
/// Stored beside the snapshots so it survives app restarts.
#[derive(Clone, Serialize, Deserialize)]
pub struct ActiveSession {
    pub id: String,
    pub timestamp: String,
}

/// `AppData/Snapshots/active_session.json`, creating the dir if needed.
fn marker_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let mut dir = app.path().app_data_dir().ok()?;
    dir.push("Snapshots");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("active_session.json"))
}

fn read(path: &std::path::Path) -> Option<ActiveSession> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Mark `id` as the active session. Best-effort: any error is a silent no-op.
pub fn set(app: &tauri::AppHandle, id: &str) {
    let Some(path) = marker_path(app) else { return };
    let marker = ActiveSession {
        id: id.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    if let Ok(json) = serde_json::to_string(&marker) {
        let _ = std::fs::write(path, json);
    }
}

/// Remove the marker. Best-effort; missing file is fine.
pub fn clear(app: &tauri::AppHandle) {
    if let Some(path) = marker_path(app) {
        let _ = std::fs::remove_file(path);
    }
}

/// The currently-active snapshot id, or None if no marker is set.
pub fn current_id(app: &tauri::AppHandle) -> Option<String> {
    read(&marker_path(app)?).map(|m| m.id)
}

#[tauri::command]
pub fn get_active_session(app: tauri::AppHandle) -> Option<ActiveSession> {
    read(&marker_path(&app)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_round_trips_through_json() {
        let marker = ActiveSession { id: "snap_123".into(), timestamp: "2026-07-13T00:00:00+00:00".into() };
        let json = serde_json::to_string(&marker).unwrap();
        let back: ActiveSession = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "snap_123");
        assert_eq!(back.timestamp, "2026-07-13T00:00:00+00:00");
    }

    #[test]
    fn read_missing_file_returns_none() {
        assert!(read(std::path::Path::new("this_path_does_not_exist_zzq.json")).is_none());
    }
}
```

- [ ] **Step 2: Declare and register the module in `lib.rs`**

Add `mod active_session;` alongside the other module declarations (near `mod activity;`). Then add `get_active_session` to the handler list at `lib.rs:838`:

```rust
        .invoke_handler(tauri::generate_handler![
            take_snapshot,
            recapture_snapshot,
            list_snapshots,
            get_snapshot,
            close_all_windows,
            activity::list_activity,
            restore_snapshot,
            delete_snapshot,
            clear_all_snapshots,
            is_current_state_saved,
            get_ignore_list,
            add_to_ignore_list,
            remove_from_ignore_list,
            get_running_processes,
            terminal_hook_status,
            set_terminal_hook,
            get_app_icon,
            active_session::get_active_session,
        ])
```

- [ ] **Step 3: Run the unit tests to verify they pass**

Run (from `src-tauri/`): `cargo test active_session`
Expected: PASS — `marker_round_trips_through_json` and `read_missing_file_returns_none` both pass, crate compiles (confirms the module + handler wiring type-check).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/active_session.rs src-tauri/src/lib.rs
git commit -m "feat: active-session marker module + get_active_session command"
```

---

### Task 2: Wire set/clear into restore, start-new, delete, clear-all

**Files:**
- Modify: `src-tauri/src/lib.rs` — `restore_snapshot` (~`lib.rs:649`), `close_all_windows` (~`lib.rs:602`), `delete_snapshot` (~`lib.rs:716`), `clear_all_snapshots` (~`lib.rs:720`)

**Interfaces:**
- Consumes (from Task 1): `active_session::set`, `active_session::clear`, `active_session::current_id`.

- [ ] **Step 1: Set the marker on restore**

In `restore_snapshot`, after the `activity::append(...)` call and before `Ok(result)` (~`lib.rs:651`):

```rust
    active_session::set(&app, &id);
    Ok(result)
```

(A restore always sets the marker, including a warned/partial restore — the user is now working in that session.)

- [ ] **Step 2: Clear the marker on start-new**

In `close_all_windows`, after the `activity::append(...)` call and before `Ok(CloseResult { ... })` (~`lib.rs:604`):

```rust
    active_session::clear(&app);
    Ok(CloseResult { closed, refused })
```

(This covers both start-new paths — the direct one and the save-first-then-close path — because both invoke `close_all_windows`.)

- [ ] **Step 3: Clear the marker when the active snapshot is deleted**

In `delete_snapshot`, after the `activity::append(...)` call and before `Ok(())` (~`lib.rs:716`):

```rust
    if active_session::current_id(&app).as_deref() == Some(id.as_str()) {
        active_session::clear(&app);
    }
    Ok(())
```

- [ ] **Step 4: Clear the marker on clear-all**

In `clear_all_snapshots`, before its final `Ok(())`:

```rust
    active_session::clear(&app);
    Ok(())
```

- [ ] **Step 5: Verify it compiles**

Run (from `src-tauri/`): `cargo check`
Expected: compiles with no errors.

- [ ] **Step 6: Behavioral verification**

Run `npm run tauri dev`. With the app open:
1. Restore a snapshot → confirm `AppData/Snapshots/active_session.json` now exists and its `id` matches the restored snapshot.
2. Restore a different snapshot → the file's `id` changes to the new one.
3. Click Start new (confirm) → the file is deleted.
4. Restore again, then delete that snapshot → the file is deleted. Restore, then delete a *different* snapshot → the file remains.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: set/clear active-session marker on restore, start-new, delete, clear-all"
```

---

### Task 3: Frontend — read the marker and render the "Currently working" line

**Files:**
- Modify: `src/types/snapshot.ts` (add `ActiveSession` interface)
- Modify: `src/commands/snapshots.ts` (add `getActiveSession` wrapper)
- Create: `src/hooks/useActiveSession.ts`
- Modify: `src/App.tsx` (use the hook; refresh after restore/start-new/delete/clear-all; pass `activeSessionId` to `MissionControl`)
- Modify: `src/components/MissionControl.tsx` (accept `activeSessionId`; change the card status line at `MissionControl.tsx:88`)
- Modify: `src/index.css` (add the `.working` line style)

**Interfaces:**
- Consumes (from Task 1, over IPC): `get_active_session -> ActiveSession | null`.

- [ ] **Step 1: Add the `ActiveSession` type**

In `src/types/snapshot.ts`, after the `ActivityEvent` interface (~line 111):

```ts
export interface ActiveSession {
  id: string;
  timestamp: string;
}
```

- [ ] **Step 2: Add the command wrapper**

In `src/commands/snapshots.ts`, extend the type import on line 2 to include `ActiveSession`, then add:

```ts
export async function getActiveSession(): Promise<ActiveSession | null> {
  return invoke<ActiveSession | null>("get_active_session");
}
```

- [ ] **Step 3: Create the hook**

Create `src/hooks/useActiveSession.ts` (mirrors `useActivity.ts`):

```ts
import { useCallback, useEffect, useState } from "react";
import { getActiveSession } from "../commands/snapshots";

export function useActiveSession() {
  const [activeId, setActiveId] = useState<string | null>(null);
  const refresh = useCallback(
    () => getActiveSession().then((m) => setActiveId(m?.id ?? null)).catch(() => setActiveId(null)),
    []
  );
  useEffect(() => { refresh(); }, [refresh]);
  return { activeId, refresh };
}
```

- [ ] **Step 4: Wire the hook into `App.tsx`**

Add the import near the other hook imports (after line 12):

```ts
import { useActiveSession } from "./hooks/useActiveSession";
```

Add the hook next to `useActivity` (after line 34):

```ts
  const { activeId, refresh: refreshActive } = useActiveSession();
```

Call `refreshActive()` at each point the marker can change — immediately after the existing `refreshActivity()` / `refresh()` calls in:
- `handleConfirmRestore` (after `await refreshActivity();`, ~line 124)
- `handleConfirmDelete` (after `await refreshActivity();`, ~line 191)
- `handleStartNew` (after `await refreshActivity();`, ~line 205)
- `handleConfirmCapture` start-new branch (after `await refreshActivity();`, ~line 62)
- `handleClearAll` (after `await refresh();`, ~line 214)

Example (restore handler):

```ts
        const result = await restore(id, closeOthers);
        await refreshActivity();
        refreshActive();
```

Then pass the prop to `MissionControl` (line 259):

```tsx
      <MissionControl snapshots={snapshots} events={events} selectedId={selectedId} onSelect={setSelectedId}
        activeSessionId={activeId}
        onCapture={handleTakeSnapshot} onStartNew={() => setStartNewOpen(true)} onRestore={handleRestore}
```

- [ ] **Step 5: Accept the prop and render the line in `MissionControl.tsx`**

Add to the `Props` type (after `selectedId: string | null;` on line 10):

```ts
  activeSessionId: string | null;
```

Replace the card status `<span>` at `MissionControl.tsx:88`. Current:

```tsx
          <div className="card-copy"><strong>{s.name}</strong><span className={s.warning_count ? "warn" : "good"}>● <i>{s.warning_count ? `${s.warning_count} warnings` : `${relative(s.timestamp)} · All good`}</i></span></div>
```

New:

```tsx
          <div className="card-copy"><strong>{s.name}</strong>{p.activeSessionId === s.id
            ? <span className="working">● <i>Currently working</i></span>
            : s.warning_count
              ? <span className="warn">● <i>{s.warning_count} warnings</i></span>
              : <span className="good">● <i>{relative(s.timestamp)}</i></span>}</div>
```

(Active card → `● Currently working`; warning card → unchanged; other good card → relative time only, `· All good` dropped.)

- [ ] **Step 6: Add the `.working` style**

In `src/index.css`, append after the `.good,.success{...}.warn,.warning{...}` rules on line 38 (or at the end of the file):

```css
.card-copy .working{color:var(--color-accent)!important;font-weight:600;text-shadow:0 0 10px color-mix(in srgb,var(--color-accent) 55%,transparent)}
.card-copy .working i{color:var(--color-accent)}
```

- [ ] **Step 7: Type-check the frontend**

Run: `npm run build`
Expected: `tsc` passes (no missing-prop or type errors) and Vite bundles successfully.

- [ ] **Step 8: Behavioral verification**

Run `npm run tauri dev`:
1. Restore a snapshot → its card shows `● Currently working` (teal, emphasized); every other good card shows just its timestamp (e.g. `13m ago`), no "· All good"; warning cards still show `N warnings`.
2. Fully close and relaunch the app → the same card still shows `● Currently working` (persistence).
3. Restore a different snapshot → the badge moves to it.
4. Start new → no card shows the badge.
5. Delete the active snapshot → badge gone; deleting a different one leaves it.

- [ ] **Step 9: Commit**

```bash
git add src/types/snapshot.ts src/commands/snapshots.ts src/hooks/useActiveSession.ts src/App.tsx src/components/MissionControl.tsx src/index.css
git commit -m "feat: show persisted Currently-working line on the active snapshot card"
```

---

## Self-Review

**Spec coverage:**
- Marker file `active_session.json`, id-keyed, in Snapshots dir → Task 1. ✓
- Set on restore / clear on start-new / clear on delete-of-active → Task 2 (plus clear-all for correctness). ✓
- `get_active_session` command → Task 1. ✓
- Frontend reads on mount + after the mutating actions → Task 3 (hook + refresh calls). ✓
- Active card `● Currently working` emphasized; warnings unchanged; other good cards timestamp only → Task 3 Steps 5–6. ✓
- `.working` style uses `--color-accent`, no new color → Task 3 Step 6. ✓
- Best-effort / no-panic writes → Task 1 (all `let _ =` / `Option` early-returns). ✓
- Persists across restarts, no expiry → file-backed, no timer anywhere. ✓

**Placeholder scan:** No TBD/TODO; every code step shows complete code.

**Type consistency:** `ActiveSession { id, timestamp }` identical in Rust (Task 1) and TS (Task 3 Step 1). `get_active_session` name matches between handler registration (Task 1 Step 2), wrapper (Task 3 Step 2), and IPC string. `activeSessionId` prop name consistent between `App.tsx` (Step 4) and `MissionControl` Props (Step 5). `current_id` used in Task 2 Step 3 is defined in Task 1 Step 1.
