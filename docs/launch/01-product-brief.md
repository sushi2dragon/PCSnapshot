# PC Snapshot — Product Brief

*Internal foundation document. Everything in the launch plan derives from this. Grounded in the repository as of 2026-07-16 (README.md, project_overview.md, features/, src-tauri/, companion-extension/). Per the launch mandate, the Windows v1 is treated as finished, stable and production-ready.*

---

## 1. One-sentence value proposition

**PC Snapshot brings your entire Windows workspace back with one click — every app, window position, browser tab and terminal directory, exactly as you left it — stored locally, with no account and no cloud.**

## 2. The problem it solves

Modern work sprawls across dozens of windows and browser tabs. Every context switch — end of day, a reboot, a Windows update, jumping between client projects — forces a choice between leaving everything open forever or spending 10–20 minutes rebuilding the setup: reopening the repo, the terminals in the right directories, the IDE, the fifteen reference tabs, the folders, and dragging windows back to the right monitors.

Existing tools solve fragments of this: window managers restore *positions* but not *apps*; browser tab managers restore *tabs* but nothing else; OS "restart apps on sign-in" reopens a chaotic approximation. Nothing on Windows restores the **whole working context** as a single unit.

## 3. Ideal user workflow

1. **Capture** — one click on "Take Snapshot." In under 3 seconds the app enumerates every visible window, maps it to its process, extracts working context (VS Code workspaces, terminal CWDs, dev servers, exact browser tabs via the companion extension), and grabs a desktop screenshot as the thumbnail. Optional name prompt, then saved.
2. **Browse** — snapshots appear as a visual grid of 130×172 px screenshot tiles. You recognize your workspace by *sight*, not by filename.
3. **Restore** — click a tile. Apps that are already running are reused; missing ones launch in dependency order (background → terminals → IDEs → browsers → foreground); windows are repositioned onto the right monitors; browser tabs are reconciled exactly (reuse matching, open missing, reorder, close extras); terminals reopen at the right directories. A restore report honestly lists anything that couldn't come back.

Two usage patterns emerge from this loop:
- **Resume:** snapshot before you stop, restore when you return.
- **Workspace profiles:** set a context up once (per-client dev rig, editing rig, writing setup), capture it, and reuse it forever as a one-click environment.

## 4. Production feature set (implemented, per `features/completed/` and source)

| Feature | Evidence |
|---|---|
| Sub-3-second capture of all visible windows, processes, positions, monitors, window state | `src-tauri/src/capture.rs`, spec `capture_engine.md` |
| Context extraction: VS Code workspaces, browser sessions, terminal CWDs, local dev servers, with confidence-scored clues | `src-tauri/src/context.rs`, `vscode.rs`, `terminal.rs` |
| Screenshot thumbnails captured off the UI thread | `thumbnail_system.md`, `xcap` |
| Visual snapshot grid with tile thumbnails | `src/components/SnapshotGrid.tsx` |
| One-click restore with process reuse, priority-ordered launch, window repositioning, multi-monitor awareness | `src-tauri/src/restore.rs`, `classify.rs` |
| **Exact browser-tab restore** via companion WebExtension + native messaging: every window, tab, order, pin state, tab group — reconciled, not just reopened. Chrome, Edge, Opera, Opera GX, Brave | `companion-extension/`, README "Current state" |
| Terminal working-directory restore | README, `terminal.rs` |
| Ignore list (apps excluded from capture/restore) | `ignorelist.md` (completed), `IgnoreListModal.tsx` |
| Recapture (update an existing snapshot in place) | `recapture_session.md` (completed) |
| "Currently working" marker on the last-restored snapshot | commit `1dea5b5` |
| Restore report modal — honest partial-restore accounting | `RestoreReportModal.tsx` |
| Delete with confirmation; rename on capture; settings page; toasts | components |
| Multi-window Office document restore via registry MRU | CLAUDE.md architecture notes |
| Human-readable, versioned local JSON storage (`%APPDATA%/com.pc-snapshot/Snapshots/`, schema v2) tolerant of partial corruption | `session_storage.md` |
| Non-destructive macro layer (e.g. Ctrl+Shift+T) with limited retries — never the primary path | `macro_layer.md` |

**Specced but not shipped (v1 roadmap, from `features/pending/` + memory):** Fresh Session (close everything, start clean), workspace-profile management and profile merging, Firefox companion support, tiered Basic/Pro/Ultra packaging.

## 5. Strongest differentiators

1. **Whole-context restore, not window-position restore.** The only Windows tool that treats "my workspace" — apps + layout + tabs + terminal directories — as one restorable object.
2. **Exact tab fidelity.** The companion extension reconciles the live browser to the snapshot: same tabs, same windows, same order, same groups. Competitors reopen the browser and hope.
3. **Visual-first.** You find a workspace by its screenshot, in one glance. No naming discipline required.
4. **Local-first, zero-trust-needed.** No account, no cloud, no telemetry, no network access in the companion. Snapshots are human-readable JSON on your own disk.
5. **Honest engineering.** Partial restores are reported, never faked — a trust feature that doubles as a review/word-of-mouth asset.
6. **Native performance.** Rust + Tauri: small binary, low memory, sub-3s capture — not an Electron shell.

## 6. Target customer segments (priority order)

1. **Developers on Windows** — the highest-fit segment: they live in a specific arrangement of IDE + terminals + localhost tabs + docs, switch contexts constantly, and pay for tools (see DisplayFusion, BetterTouchTool buyers). The terminal-CWD and VS Code-workspace restore features only matter to them — and matter a lot.
2. **Freelancers / consultants juggling clients** — per-client workspace profiles are a direct billing-hours saver.
3. **Creators: video editors, designers, artists, researchers** — reference boards, footage folders, canvas apps, inspiration tabs; PureRef's audience.
4. **Privacy-conscious power users** — the local-first story wins them; they are also the loudest reviewers on HN/Reddit.

## 7. Privacy and trust advantages

- All data stays on the machine; there is literally no server to leak from.
- The browser companion is headless by design: no network access, no content scripts, no cookies, no history — only tab URLs/titles/order, and only over local native messaging (`companion-extension/README.md`).
- No account, no sign-up, no telemetry in the app itself.
- Human-readable JSON storage: users can audit exactly what was captured.
- This is a *marketable* posture, not just an ethical one: "your workspace layout is a map of your life — it never leaves your PC."

## 8. Technical limitations that affect positioning

- **Windows-only** (Win32 APIs). Position as a strength ("built for Windows, not a port") — but it rules out Mac-heavy channels.
- **Apps relaunch; in-app state (unsaved documents, scroll positions) does not fully return** except where context extraction covers it. Message "picks up where you left off," never "hibernates your apps."
- **Firefox tabs not yet supported**; Chromium family only. Say so plainly on the site.
- **Companion extension is an extra install step** — onboarding must make it one-click and optional (everything else works without it).
- **Elevated/admin apps and exotic windows may not reposition** — the honest restore report covers this; the marketing must too.
- Snapshots capture what's *visible*; minimized-to-tray utilities are handled via the ignore list rather than restored.

## 9. Strongest selling points (in the order the landing page should make them)

1. One click rebuilds your whole workspace — apps, windows, monitors, tabs, terminals.
2. Your exact browser tabs come back — not "a browser opened."
3. Find any workspace by its screenshot in seconds.
4. 100% local. No account. No cloud. No telemetry.
5. Fast and native (Rust) — capture in under 3 seconds.
6. Honest restore reports — it tells you what it couldn't bring back.
7. One-time purchase (per the commercial strategy) — own it, no subscription.

## 10. Positioning for Windows v1

**Category:** create a small one — "workspace snapshot tool" / "session manager for your whole PC" — rather than fighting inside "window manager" (DisplayFusion, FancyZones own that) or "tab manager" (Workona owns that).

**Positioning statement:** *For Windows power users who juggle multiple projects, PC Snapshot is the workspace snapshot tool that captures your entire desktop — apps, windows, tabs, terminals — and restores it in one click. Unlike window managers that only remember positions, or tab managers that only save the browser, PC Snapshot brings back your whole working context, and it does it 100% locally with no account and no cloud.*

**Tagline candidates:** "Your whole workspace. One click back." / "Save your desktop like a file." / "Never rebuild your workspace again."

**v1 pricing posture (detailed in 03):** free capture/restore core + one-time-purchase Pro — matching both the repo's stated tier plan and the indie-Windows-utility market's norms.
