# Clipboard capture & restore (opt-in)

> **Status — sensitive-exclusion DEFERRED to v1.1 (2026-07-28).** The
> current-clipboard sensitive check was removed for the current release: all data
> is stored locally and the app has no network access, so capturing the live
> clipboard unconditionally is an accepted risk for now. The detector
> (`win32::current_is_sensitive` in `clipboard.rs`) is retained but unwired.
> Before re-enabling in v1.1: (1) re-wire it at the two former call sites
> (`capture_current`, `win::capture`), and (2) fix the
> `CanIncludeInClipboardHistory` branch to read the marker's DWORD value
> (`0` = exclude) instead of treating mere presence as sensitive — presence
> false-positives on ordinary Chrome/Edge/Office copies. Also consider extending
> the exclusion to captured Win+V *history* items, not just the current slot: a
> secret copied unmarked and pinned still reaches disk via the history path.

## Goal

Optionally capture the clipboard — the current clipboard plus the Windows Win+V
history — as part of a snapshot, and make those items recoverable two ways:
(1) re-copy any captured item from the snapshot's details panel or a global
"Clipboard Cache" settings panel, and (2) on restore, reseed the OS Win+V
history so it *appears* as it did at capture time. Supports text, image, and
file copies. Pinned Win+V items are always preserved untouched.

A **single master opt-in** governs the entire feature. Opted out: nothing about
the clipboard is ever read or written to disk, and the settings panel is greyed
out. This is the whole privacy boundary — one toggle, no exceptions.

## Why this is achievable (and where it isn't)

- **Current clipboard** — read via the Win32 clipboard API (`OpenClipboard` /
  `GetClipboardData`) already reachable through the `windows` crate:
  `CF_UNICODETEXT` (text), `CF_DIB`/`CF_BITMAP` (image), `CF_HDROP` (file copies
  — this yields file *paths*, not bytes).
- **Win+V history** — read via the documented WinRT API
  `Windows.ApplicationModel.DataTransfer.Clipboard.GetHistoryItemsAsync()`
  (Windows 10 1809+). Each `ClipboardHistoryItem` carries an id, timestamp, and a
  `DataPackageView` from which text/bitmap/files are extracted. The `windows`
  crate covers this namespace; the calls are async WinRT and must be awaited.
- **Reseed on restore** — `Clipboard.ClearHistory()` wipes all *unpinned*
  history in one call and **leaves pinned items intact** (this is exactly why
  pinned-preservation is free). Then replay the captured items **oldest → newest**
  via `Clipboard.SetContent`, so the newest lands on top of the stack matching
  the original order.
- **What cannot be reproduced (accepted):** original timestamps. Every replayed
  item reads as copied "now," so Win+V's time grouping won't match capture time.
  The user has accepted this.
- **What the OS already protects:** `GetHistoryItemsAsync()` omits items apps
  marked sensitive (`CanIncludeInClipboardHistory = false`, set by password
  managers etc.). The Win32 *current-clipboard* read does **not** get this for
  free — see Constraints for the mitigation.

## Flow

### A. Master opt-in

- One boolean setting, e.g. `capture_clipboard` in the app config, default
  **off**. It is the top control of the Clipboard Cache settings panel.
- When off: capture never reads the clipboard, restore never touches Win+V, no
  pre-restore grab occurs, no sidecars are written, and the panel body is greyed
  out. When on: all of the below is active.

### B. Capture (during Take Snapshot / Recapture, only if opted in)

1. Read the current clipboard (Win32) and the Win+V history
   (`GetHistoryItemsAsync`).
2. Dedupe the current item against the top history item (they're usually the
   same copy) so it isn't stored twice.
3. Persist into the snapshot as a `clipboard` block (schema v3, below): text
   stored inline; images and file-copy payloads stored as sidecar files next to
   the snapshot's `{id}.png`.
4. Capture must not break the < 3 s budget or the snapshot itself — a clipboard
   read failure degrades to a warning, exactly like every other partial-capture
   path. The snapshot always saves.

### C. Details panel (per snapshot)

- The snapshot details view gains a clipboard section listing that snapshot's
  captured items (text / image thumbnail / file paths), each with a **Copy**
  button that sets it as the live clipboard *now* (Win32 `SetClipboardData`, or
  WinRT `SetContent` so it also enters Win+V).
- Greyed out / absent for snapshots with no `clipboard` block (captured before
  opt-in, or capture failed).

### D. Settings "Clipboard Cache" panel

- **Top:** the master opt-in toggle (§A).
- **Below:** a single list aggregating clipboard entries **across all
  snapshots** (snapshot-scoped — no independent/continuous monitoring), plus the
  pre-restore auto-backups (§F). Each row shows:
  - the entry content (text preview / image thumbnail / file path list),
  - the **corresponding snapshot name** on the side (auto-backups labelled e.g.
    "Before restoring <name> · <when>"),
  - a **Copy** button,
  - a **three-dots menu** (at least: Delete this entry; Copy; reveal which
    snapshot it came from — final items TBD in UI pass).
- When opted out, the whole list is greyed out.

### E. Restore reseed (every restore of a clipboard-bearing snapshot, if opted in)

- **Gate:** only when the snapshot actually has a `clipboard` block. Restoring an
  older, clipboard-less snapshot must **not** `ClearHistory()` — that would wipe
  the user's live clipboard and replay nothing. No clipboard block → skip §E and
  §F entirely.
- Sequence: fire the pre-restore grab (§F) → `ClearHistory()` (pinned survive) →
  replay captured items oldest→newest via `SetContent`, pacing between items (see
  Risks — timing must be measured, not assumed).
- File-copy items only re-copy meaningfully if the files still exist at their
  captured paths; missing files surface as a restore warning (consistent with
  "restore reports honestly").

### F. Pre-restore auto-backup (safety net)

- Fires **whenever §E will reseed** (i.e. opted in AND snapshot has a clipboard
  block) — because that is the only moment the live Win+V is about to be
  destroyed. It grabs the *current* clipboard + history and appends it to the
  Clipboard Cache as an auto-backup entry, so the user's pre-restore clipboard is
  always recoverable from the panel even after the reseed clears it.
- Bounded by keep-last-N (Constraints) so backups don't accumulate forever.

## Snapshot schema (v3)

Bump `schema_version` to 3. Add an optional block (absent on opt-out / older
snapshots; deserialize must be tolerant — `#[serde(default)]`, mirroring how
`browser_sessions` was added):

```json
"clipboard": {
  "captured_at": "<ISO 8601>",
  "items": [
    {
      "id": "clip_<n>",
      "kind": "text | image | files",
      "order": 0,                        // 0 = oldest; newest = highest
      "text": "…",                       // kind=text only, inline
      "sidecar_path": "…",               // kind=image (PNG) or files (manifest)
      "file_paths": ["…"],               // kind=files
      "source": "current | history",
      "byte_size": 12345
    }
  ]
}
```

- Images: stored as PNG sidecars `{id}_clip_{n}.png` beside `{id}.png`.
- Files: store the path list inline (`file_paths`); no bytes copied.
- Auto-backups (§F) live in the app-level cache store, not inside a snapshot
  JSON (they belong to no snapshot); same item shape.

## Constraints

- **Master opt-in is absolute.** Off ⇒ no clipboard read/write anywhere, panel
  greyed. This is the entire privacy contract; do not add side channels.
- **Honor the sensitive-exclusion everywhere clipboard is read.** The WinRT
  history read already omits excluded items; the Win32 current-clipboard read
  does not, so it must check for the exclude-from-history / monitor-processing
  format (`CanIncludeInClipboardHistory` / `ExcludeClipboardContentFromMonitorProcessing`)
  and skip storing such items. A password sitting on the clipboard must never
  reach disk.
- **Caps to bound storage:** a per-item byte cap (skip/flag oversized images or
  huge text), a total per-snapshot clipboard cap, a total cache cap, and
  keep-last-N on auto-backups. Text is cheap; images are the bloat risk.
- **Pinned items are never touched** — no code path pins, unpins, or relies on
  clearing them; `ClearHistory()` preserving them is the mechanism.
- **Windows history caps at ~25 items** — capture/replay cannot exceed it;
  don't design as if unlimited.
- **Never crash / always degrade.** Any clipboard failure (read, WinRT
  unavailable, history disabled at OS level, replay error) becomes a warning; the
  snapshot and the rest of restore proceed.
- **Reseed gates on a clipboard block** (§E) — no block, no `ClearHistory()`.

## Risks / things to prove during implementation (not assume)

1. **Replay cadence.** The clipboard-history service samples clipboard-changed
   events; firing `SetContent` in a tight loop may debounce so only the last item
   becomes a distinct history tile. A small inter-item delay (~50–150 ms, needs
   empirical tuning) is likely required. **Spike this first** against the real
   service before building the full replay — it looks fine in code and fails on
   the live service.
2. **WinRT from Rust/Tauri.** `GetHistoryItemsAsync` / `ClearHistory` /
   `SetContent` are async WinRT; confirm the `windows`-crate call pattern and
   that awaiting works cleanly off the Tauri async runtime (capture runs in
   `spawn_blocking` today — decide where the WinRT calls live).
3. **History may be disabled at the OS level** (`IsHistoryEnabled` false) or by
   group policy. Capture then yields only the current item; reseed's
   `ClearHistory`/replay may no-op. Detect and degrade to a warning; do not
   silently enable it behind the user's back.
4. **Image fidelity/size.** DIB → PNG round-trip and large images; enforce the
   per-item cap and confirm re-copied images paste correctly in a real target
   app (behavior, not just bytes written).
5. **File copies are paths only** — moved/deleted files make the re-copy dead;
   ensure the warning fires and the details/panel Copy handles a missing file
   gracefully.
6. **Sensitive-exclusion coverage.** Verify the Win32 current-clipboard read
   actually detects the exclusion format on a real password-manager copy — this
   is the load-bearing privacy check.

## Verification (behavior, not exit code)

- Opt out → confirm zero sidecars written, no `clipboard` block, no Win+V change
  on restore, panel greyed.
- Opt in → copy a mix of text/image/file, capture, confirm the block + sidecars
  and that the details panel Copy re-copies each correctly into a real app.
- Restore a clipboard-bearing snapshot → Win+V shows the captured items in
  captured order, pinned items still present, and the pre-restore clipboard is
  recoverable from the Clipboard Cache auto-backup entry.
- Restore a clipboard-less (older) snapshot → Win+V is **untouched** (no clear).
- Copy a password-manager entry, capture → confirm it is **not** stored.

## Critical files (anticipated)

- `src-tauri/src/clipboard.rs` (new) — Win32 current-clipboard read/write + WinRT
  history read / `ClearHistory` / reseed replay; sensitive-exclusion check.
- `src-tauri/src/capture.rs` — invoke clipboard capture when opted in; add to the
  snapshot build; keep within the time budget / degrade to warning.
- `src-tauri/src/restore.rs` + `src-tauri/src/lib.rs` — pre-restore grab + reseed
  gated on the clipboard block; new IPC commands (`copy_clipboard_item`,
  `list_clipboard_cache`, `delete_clipboard_entry`, toggle setting).
- `src-tauri/src/lib.rs` — schema v3 types (`ClipboardBlock`/`ClipboardItem`,
  `#[serde(default)]`), sidecar read/write, cache store, config `capture_clipboard`.
- `src/types/snapshot.ts` — TS interfaces for the clipboard block/items.
- `src/commands/snapshots.ts` — new command wrappers.
- `src/components/` — details-panel clipboard section + the Clipboard Cache
  settings panel (toggle, list rows: content / snapshot name / Copy / three-dots).
