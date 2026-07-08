# PC Snapshot

A minimalist Windows desktop app that captures the current desktop state
(running apps, window layout, working context) as a **snapshot**, stores it
locally, and restores it on demand. Three actions: **capture, browse, restore**.
Local-first, visual-first — no cloud sync, no profiles, no modes.

## How it works

- **Capture** — enumerates visible windows (Win32), maps them to processes,
  extracts context (VS Code workspaces, browser tabs, terminal CWDs, dev
  servers), and grabs a screenshot thumbnail — all in under 3 seconds.
- **Browse** — snapshots appear as thumbnail tiles in a grid.
- **Restore** — reuses already-running apps, launches missing ones in priority
  order (background → terminals → IDEs → browsers → foreground), repositions
  windows, and reports anything it couldn't do honestly.

Snapshots are human-readable JSON (`%APPDATA%/PC Snapshot/Snapshots/`), one
`.json` + `.png` thumbnail per snapshot.

## Tech stack

- **Frontend:** React 19 + TypeScript + Tailwind CSS 4 (Vite)
- **Backend:** Rust via Tauri 2 (`windows` crate, `xcap`, `sysinfo`)

## Development

```bash
npm install

# Full desktop app in dev mode (Tauri + Vite)
npm run tauri dev

# Frontend only (Vite dev server, port 5173)
npm run dev

# Type-check + bundle frontend
npm run build

# Lint
npm run lint

# Release build
npm run tauri build
```

Rust build artifacts live outside the repo (see `src-tauri/.cargo/config.toml`)
because OneDrive's Files On-Demand corrupts `target/` with read-only
placeholders. Run cargo commands from `src-tauri/` so that config applies.

## Project layout

- `src/` — React UI (grid, modals, toasts)
- `src-tauri/src/` — capture, context extraction, restore, classification
- `features/pending/` and `features/completed/` — feature specs and status
- `CLAUDE.md` — architecture notes and constraints
