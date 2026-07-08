# Project Overview

## Core Idea

A minimalist Windows desktop app that lets users:

1. Capture the current desktop state (apps + context)
2. Save it as a “snapshot”
3. Visually browse snapshots using thumbnails
4. Restore any snapshot with one click

There are no modes, profiles, or complex configuration systems.

The entire product revolves around:

- taking snapshots
- visually identifying them
- restoring them instantly

---

## User Workflow

### Capture Flow

- User opens app
- If no snapshots exist:
  - Sees a centered "Take Snapshot" button
- If snapshots exist:
  - Sees grid of snapshots
  - "Take Snapshot" button appears top-right

User clicks "Take Snapshot":

1. System scans running processes (foreground-first)
2. Extracts context clues (browser, VS Code, terminal, servers)
3. Captures desktop screenshot
4. Prompts user for name (optional)
5. Saves snapshot to disk
6. Shows success + warnings (if any)

---

### Restore Flow

- User clicks a snapshot tile OR restore button

1. App reads snapshot file
2. Builds restore plan
3. Launches apps in sequence
4. Restores layout
5. Applies context hints + macros
6. Shows success or partial restore result

---

## Design Principles

- Minimal UI (no clutter)
- One-click actions
- Visual-first navigation (thumbnails)
- Honest reporting (no fake “perfect restore”)
- Local-first storage
- Fast execution

---

## Non-Goals

- No profiles / modes
- No deep per-app integrations initially
- No cloud sync
- No full memory-state restoration

---

## Success Criteria

- Snapshots are created in < 3 seconds
- Restore works reliably for common workflows
- User can visually recognize snapshots instantly
- Partial restore is still useful
