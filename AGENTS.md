# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Working Style

Optimize for minimal token usage. Be terse: skip preamble and recaps, don't restate the question, and keep explanations to what's needed to act. Read only the files relevant to the task rather than broad exploration, prefer targeted searches over reading whole files, and make edits without echoing large unchanged blocks back. Lead with the answer or the change; add detail only if asked.

## Project Summary

**PC Snapshot** is a minimalist Windows desktop app that captures the current desktop state (running apps, layout, context) as a "snapshot", stores it locally, and restores it on demand. The entire product revolves around three actions: capture, browse, restore.

No cloud sync, no profiles, no modes. Local-first, visual-first.

## Tech Stack

- **Frontend:** React 19 + TypeScript + Tailwind CSS 4, bundled by Vite
- **Backend:** Rust via Tauri 2 (Win32 API, `windows` crate, `xcap` for screenshots, `sysinfo` for process metadata)
- **IPC:** Tauri commands bridge React → Rust; defined in `src/commands/snapshots.ts` (frontend) and `src-tauri/src/lib.rs` (backend handlers)

## Commands

```bash
# Frontend dev server (Vite, port 5173)
npm run dev

# Full desktop app in dev mode (Tauri + Vite together)
npm run tauri dev

# Type-check + bundle frontend
npm run build

# Build desktop app for release
npm run tauri build

# Lint
npm run lint
```

No test framework is currently configured.

## Feature Status

Features are tracked as specs in `features/pending/` and `features/completed/`. Before implementing anything, check both directories to understand what is planned vs. done.

## Architecture

### Capture Pipeline (trigger: "Take Snapshot" click)

1. **Capture Engine** (`src-tauri/src/capture.rs`) — enumerates visible Win32 windows via `EnumWindows`, maps them to processes via `sysinfo`, collects PID/exe path/cmd line/window position+state+monitor. Foreground window is always processed first.
2. **Context Extraction** (`src-tauri/src/context.rs`) — applies heuristic rules to infer meaningful state (VS Code workspaces, browser sessions, terminal CWDs, local dev servers). Each clue carries a confidence score and contributes to `restore_hints`.
3. **Thumbnail System** — screenshot captured via `xcap` on a separate thread while window enumeration runs; resized to PNG for the grid UI.
4. **Session Storage** — `src-tauri/src/lib.rs` writes `{id}.json` + `{id}.png` to `AppData/Snapshots/`. Schema version is tracked (`schema_version: 2`).

### Restore Pipeline (trigger: snapshot tile click)

1. **Restore Engine** (`src-tauri/src/restore.rs`) — reads snapshot JSON, reuses already-running processes where possible, launches missing ones in priority order: Background → Terminal → IDE → Browser → Foreground. Waits for windows to appear, then repositions them. Handles multi-window Office docs via registry MRU.
2. **Process Classification** (`src-tauri/src/classify.rs`) — maps exe stems to categories that determine launch order.
3. **Macro Layer** — post-launch automation (e.g. `Ctrl+Shift+T` for browser tabs). Never the primary restore path; non-destructive with limited retries.
4. **Error Handling** — all failures degrade to warnings surfaced in the UI; the snapshot is always saved even if capture is partial.

### Frontend Data Flow

```
App.tsx
  └── NamePromptModal (rename before saving)
  └── useSnapshots hook  →  src/commands/snapshots.ts  →  Tauri IPC
        ├── takeSnapshot(name)    →  take_snapshot    (lib.rs)
        ├── listSnapshots()       →  list_snapshots   (lib.rs)
        ├── restoreSnapshot(id)   →  restore_snapshot (lib.rs)
        └── deleteSnapshot(id)    →  delete_snapshot  (lib.rs)
```

Key frontend files:
- `src/App.tsx` — orchestrates all flows; owns modal and toast state
- `src/hooks/useSnapshots.ts` — wraps Tauri commands; manages snapshots array + loading state
- `src/types/snapshot.ts` — canonical TypeScript interfaces (`SnapshotSummary`, `RestoreResult`, `ProcessInfo`, `WindowInfo`, `ContextClue`)

### UI Design Tokens

- Background: `#1c1c1f` (base), `#252528` (card), `#2a2a2d` (tile)
- Accent: `#4bbfc3` (teal) — the only accent color
- Tile size: 130×172 px thumbnails in a responsive grid

## Snapshot Schema (v2)

Stored at `AppData/Snapshots/{id}.json`:

```json
{
  "schema_version": 2,
  "id": "snap_<timestamp_ms>",
  "name": "...",
  "timestamp": "<ISO 8601>",
  "processes": [{ "name", "pid", "exe_path", "cmd_line", "classification" }],
  "windows": [{ "title", "position", "size", "state", "monitor_index", "exe_path" }],
  "context_clues": [{ "type", "value", "confidence", "source" }],
  "restore_hints": ["..."],
  "warnings": ["..."],
  "thumbnail_path": "..."
}
```

## Key Constraints

- Capture must complete in < 3 seconds; screenshot runs off the UI thread and overlaps with window enumeration.
- Snapshot JSON must be human-readable, versioned, and tolerant to partial corruption.
- Restore reports honestly — partial restores are surfaced as warnings, never hidden.
- Macro layer actions must be non-destructive with limited retries.
- The app must never crash; all fallible operations degrade gracefully.
