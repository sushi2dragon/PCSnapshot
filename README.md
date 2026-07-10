# PC Snapshot

A minimalist Windows desktop app that captures your current desktop state —
running apps, window layout, and working context — as a **snapshot**, stores it
locally, and restores it on demand. Three actions: **capture, browse, restore**.
Local-first, visual-first — no cloud sync, no account, no telemetry.

> **Status:** active development (pre-release). The core capture/restore engine
> and the cross-browser tab companion work end-to-end today; polish, packaging,
> and the workspace-profile layer are in progress. See
> [Current state](#current-state) below.

## What it is

Modern work sprawls across dozens of windows and browser tabs. Closing your
laptop for the day, or switching from one project to another, means either
leaving everything open forever or losing your place. PC Snapshot fixes that:
it takes a picture of your whole working setup — which apps are open, where their
windows are, which files/folders/terminals/tabs are in play — and brings it all
back with one click, exactly as you left it.

Two core stories drive the product:

1. **Pick up where you left off.** Capture your desktop before you stop, restore
   it when you come back. The apps relaunch, windows return to their positions
   and monitors, terminals reopen at the right working directories, and your
   browser tabs come back — the exact tabs, in the right windows and order, not
   just "reopen the browser."

2. **Custom workspaces / work profiles.** Set up a context once — open the
   windows you want, capture it — and reuse it as a named profile. Examples:
   a per-project dev setup (repo folders, terminals, IDE, agent UIs), an
   editing rig (editor, footage folders, reference tabs, socials), an art
   setup (reference boards, canvas app, inspiration tabs), or an office bundle.
   One click rebuilds the whole environment.

**On the roadmap:** merging multiple profiles to open them together, richer
workspace management, and a tiered model (a free capture/restore core, with
paid tiers for advanced workflow and multi-profile management) — which is why
this project ships under a noncommercial license (see [License](#license)).

## Who it's for

- People who juggle **multiple projects or contexts** and lose time rebuilding
  their setup on every switch.
- **Developers, editors, designers, researchers** — anyone whose work lives in a
  specific arrangement of apps, folders, terminals, and browser tabs.
- Windows users who want their workspace back **instantly and locally**, with no
  cloud account and no data leaving their machine.

## How it works

- **Capture** — enumerates visible windows (Win32), maps them to processes,
  extracts context (VS Code workspaces, browser tabs, terminal working
  directories, local dev servers), and grabs a screenshot thumbnail — in under
  three seconds.
- **Browse** — snapshots appear as thumbnail tiles in a grid.
- **Restore** — reuses already-running apps, launches missing ones in priority
  order (background → terminals → IDEs → browsers → foreground), repositions
  windows, reconciles browser tabs via the companion extension, and honestly
  reports anything it couldn't do.

Snapshots are human-readable JSON (`%APPDATA%/com.pc-snapshot/Snapshots/`), one
`.json` + `.png` thumbnail per snapshot.

### Browser Companion

Exact browser-tab restore is handled by a lightweight **companion WebExtension**
that talks to the desktop app over native messaging (the only reliable way to
enumerate every tab and to close/open individual tabs from outside the browser).
It is headless — no UI, no network access, no history or cookie access — and
lives in [`companion-extension/`](companion-extension/). Capture records every
window, tab, order, and tab-group; restore reconciles the live browser to the
snapshot (reuse matching tabs, open missing ones, reorder, and close extras).
Works across Chromium browsers: Chrome, Edge, Opera, Opera GX, and Brave.

## Current state

**Working end-to-end today:**
- Capture, browse, and restore of apps, window layout, and monitors.
- Terminal working-directory restore.
- Browser tab capture **and** exact restore via the companion extension,
  verified on Chrome, Edge, Opera, Opera GX, and Brave.

**In progress / known limitations:**
- The companion's MV3 service worker idles out after ~30s, which can drop its
  connection; a keepalive is planned.
- Firefox is not yet supported (different extension/native-messaging model).
- Packaging: the companion extension currently sideloads unpacked and its native
  host registers via a script; a bundled installer flow is planned.
- Workspace profiles, profile merging, and the tiered model are upcoming.

## Tech stack

- **Frontend:** React 19 + TypeScript + Tailwind CSS 4 (Vite)
- **Backend:** Rust via Tauri 2 (`windows` crate, `xcap`, `sysinfo`)
- **Companion:** MV3 WebExtension (vanilla JS) + a Rust native-messaging host

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

# Companion extension tests
npm run test:companion
```

Rust build artifacts live outside the repo (see `src-tauri/.cargo/config.toml`)
because OneDrive's Files On-Demand corrupts `target/` with read-only
placeholders. Run cargo commands from `src-tauri/` so that config applies.

### Trying the Browser Companion (dev)

1. Build the native host: from `src-tauri/`, `cargo build --bin pc_snapshot_native_host`.
2. Build the extension: `node companion-extension/scripts/build-chromium.mjs`.
3. Register the native host for your installed browsers:
   `powershell -ExecutionPolicy Bypass -File companion-extension/scripts/register-chromium-host.ps1 -HostExecutable "<path to pc_snapshot_native_host.exe>"`
   (launch each target browser once first — some create their registry key lazily).
4. Load `companion-extension/dist/chromium` as an unpacked extension in each
   Chromium browser (Developer mode → Load unpacked).

## Project layout

- `src/` — React UI (grid, modals, toasts)
- `src-tauri/src/` — capture, context extraction, restore, classification,
  browser bridge, native host
- `companion-extension/` — the browser companion (source, tests, build/register scripts)
- `features/pending/` and `features/completed/` — feature specs and status
- `CLAUDE.md` — architecture notes and constraints

## Contributing & testing

The project is source-available and contributions/bug reports are welcome — the
most useful help right now is **testing capture and restore across different
apps, browsers, and multi-monitor setups** and reporting what does or doesn't
come back correctly. To run the app, follow [Development](#development) above; to
exercise the browser companion, see the dev setup steps in the same section.

Please note the license below: contributions and testing are for noncommercial
purposes, and commercial use is reserved to the copyright holder.

## License

PC Snapshot is licensed under the **[PolyForm Noncommercial License 1.0.0](LICENSE)**.

- ✅ You may use, modify, share, and contribute to it for **noncommercial**
  purposes — personal use, study, research, testing, and noncommercial
  organizations.
- ❌ You may **not** use it commercially — you cannot sell it, host it as a paid
  service, or ship it inside a commercial product.
- Commercial rights are reserved to the copyright holder, who may offer separate
  commercial licenses and release commercial versions.

Copyright (c) 2026 Sarthak. All rights not expressly granted are reserved.
