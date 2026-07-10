# Browser Companion — exact tab restore (all browsers)

## Goal

Restore reproduces a snapshot's browser state exactly — same windows, same tabs
in the same order, same groups/pins/active tab — no more, no less, across
repeated switching between snapshots. It does this through a **companion
WebExtension** that reports and reconciles tab state over native messaging,
because the WebExtension `tabs` API is the only interface that can create, move,
and **close individual tabs** from outside the browser. This supersedes the old
SNSS-session-file plan: reverse-engineering on-disk session files could *read*
all tabs but could never *close* one, which is the whole ask.

Restore semantics the user specified, restated as the contract:
> On restore (after the existing confirm dialog): for each captured browser
> profile, compare its currently-open tabs to the snapshot — reuse tabs whose
> URL already matches, open the missing ones, move everything into snapshot
> order, and (when "Close others" is on) close the extras.

## Today's milestone (dev stage — the only goal right now)

Prove the **capture → restore reconciliation pipeline works end to end** on one
dev machine, one Chromium browser (Chrome or Edge). Everything productization is
explicitly deferred.

In scope today:
- Wire restore on both ends (Phase 1 below) so the existing, tested reconcile
  logic actually runs.
- Extension has **zero user-facing UI** — no popup, no options page, no badge.
  It runs silently. Its only "surface" is a **debug/error trace** for the
  developer (service-worker console + errors relayed to the desktop app), so
  failures during testing are visible without any user-side chrome.
- Get it onto the device by hand: build the unpacked extension, build + register
  the native host, sideload, and run the end-to-end test.
- The desktop app may show a light "install the companion for detailed browser
  restore" nudge with a link to the extension — but a manual sideload is all
  that's needed to test today; the polished nudge can come after the pipeline is
  proven.

Explicitly NOT today: store publishing (Chrome Web Store / Edge / AMO / Opera),
multi-browser native-host registration beyond the test browser, Firefox,
uninstall handling, installer integration. Those are Phases 2–3.

## What already landed (verified in code, this session)

**Capture — DONE and wired end to end:**
- `companion-extension/` — MV3/WebExtension. `capture.js::captureBrowserSession`
  reads every normal (non-incognito) window's tabs: url, title, index,
  active/pinned/muted/discarded, tab-group membership, plus group
  title/color/collapsed and window bounds/state. Runtime IDs are converted to
  snapshot-local keys (`g:<win>:<idx>`) because browser IDs die on restart.
- `background.js` opens a persistent native-messaging port, sends `hello`, and
  answers `capture_request` with a `capture_result`.
- `src-tauri/src/browser_bridge.rs` — named-pipe broker
  (`\\.\pipe\pc-snapshot-browser-bridge-v1`). `BrowserBridge::capture()` fans a
  `capture_request` to every connected profile, collects `BrowserSession`s,
  degrades a slow/absent extension to a warning (1200 ms deadline).
- `src-tauri/src/bin/pc_snapshot_native_host.rs` — the browser-launched stdio
  host that relays framed JSON between the browser port and the pipe.
- `lib.rs` — `BrowserSession`/`BrowserWindow`/`BrowserTab`/`BrowserTabGroup`
  types; `Snapshot.browser_sessions` (`#[serde(default)]`, tolerant deserialize
  so old snapshots still load); `take_snapshot` and the recapture command both
  call `bridge.capture()` and persist the result; `BrowserBridge::start()` runs
  at app launch and is `.manage()`d. Unit tests cover multi-profile capture
  completion and v2-snapshot back-compat.

**Restore — built on both ends, wired on NEITHER (the gap):**
- `restore.js::reconcileBrowserSession` is complete and unit-tested: it does
  exactly the user's contract (reuse-by-exact-URL, create missing, move to
  index, group, set active, close extras only when asked, create-before-close
  so a failure never empties the browser). Its return shape
  `{reused,opened,closed,skipped,warnings}` already matches the Rust
  `BrowserRestoreReport`.
- `BrowserBridge::restore()` / `restore_one()` are complete: they send
  `restore_request{browser_session, close_extras}` per profile and await a
  `restore_result{report}`.
- **Neither is reachable.** `background.js` only imports `capture.js` and only
  handles `capture_request` — it never imports `restore.js` or listens for
  `restore_request`. And `restore_snapshot` (lib.rs:507) does **not** take the
  bridge State and never calls `bridge.restore()`; it runs
  `restore_desktop(…, companion_managed_browsers = false)` with an explicit
  comment that reconciliation is gated "until the companion's destructive
  restore behavior is live-tested" (lib.rs:526).
- The old restore path is therefore still the active one:
  `restore.rs::browser_urls_for` + the launch loop reopen only the captured
  *active* tabs as command-line URLs, and only when the browser wasn't already
  running. Already-running browsers get no tab reconciliation at all — this is
  the user's original complaint.

**Frontend — sufficient, no change needed for MVP:**
- `RestoreConfirmModal.tsx` already shows the confirm dialog with a
  "Close others" toggle (default on); `is_current_state_saved` already backs a
  "save first" nudge. `snapshot.ts` already has `BrowserSession` types and
  `RestoreResult.closed`.

**Out of scope for this spec (separate uncommitted workstream):**
`terminal.rs`, `terminal_hook.rs`, `src/bin` terminal pieces, and the
`TERMINAL_*.md` / `COWORK_TERMINAL_TEST.md` notes are terminal-CWD work, not
browser. `AGENTS.md` is a verbatim copy of `CLAUDE.md`. None are touched here.

## Plan

### Phase 1 — Wire restore end to end (the core of the ask)

**1a. Extension: handle `restore_request` (`background.js`).**
Import `reconcileBrowserSession` from `./restore.js`. Extend the `onMessage`
listener: when `message.type === "restore_request"`, call
`reconcileBrowserSession(api, message.browser_session, message.close_extras)`,
then `port.postMessage({protocol_version:1, type:"restore_result",
request_id, report})`; on throw, post `restore_error{request_id, message}`.
Field names must match the bridge exactly (`browser_session`, `close_extras`,
and the reply wrapped under `report` — bridge reads `message.get("report")`).

**1b. Rust: call the bridge from `restore_snapshot` (`lib.rs`).**
Add `browser_bridge: tauri::State<'_, BrowserBridge>` to the command (mirror the
two capture commands). After `restore_desktop` returns, if
`snapshot.browser_sessions` is non-empty, `await bridge.restore(&sessions,
close_others)` and fold its `RestoreReply` into `RestoreResult` — `closed_items`
into the closed list, `warnings` into warnings. `restore_desktop` runs in
`spawn_blocking`; the bridge call is async and happens after it, on the async
runtime — no blocking-context problem.

**1c. Rust: stop the old path from double-restoring companion browsers
(`restore.rs` + `lib.rs`).**
Flip `companion_managed_browsers` on when the snapshot carries browser sessions,
but refine restore.rs:99 so it does **not** simply `continue` (skip launch):
- If the browser is a companion-managed family and **already running** → skip
  the command-line URL launch entirely; the extension reconciles it in place.
- If it is companion-managed but **not running** → still launch it, but with an
  **empty** URL list (plain launch), so a process exists for the extension to
  reconcile. Do not pass `browser_urls_for` URLs (that would open a second set
  of tabs the reconcile then has to clean up).
- Non-companion browsers (no matching session / extension absent) → keep the
  existing `browser_urls_for` command-line behavior as the fallback.

Precise gating unit: a browser family is "companion-managed" iff a
`BrowserSession` for it exists in the snapshot. Coarser than per-profile but
correct for the common single-profile case; per-family/per-profile refinement
is a follow-up (see Risks).

**1d. Rust: sequence launch → connect → reconcile.**
The extension can only reconcile a browser whose companion is connected to the
bridge. A freshly launched browser needs a moment for its host to reconnect and
send `hello`. After launching a companion-managed browser that wasn't running,
poll `BrowserBridge` for that profile's presence with a bounded wait (reuse the
existing window-appearance wait budget; e.g. up to ~5 s) before calling
`bridge.restore()`. If it never connects, degrade to a warning and fall back to
the plain launch (the browser at least reopened its own session).

**1e. Extension: quiet debug/error tracing, no UI.**
Keep the extension headless — no popup, options page, or badge. For dev
traceability add structured logging in `background.js`/`capture.js`/`restore.js`
via `console.debug/error` (viewable on demand at `chrome://extensions` →
Details → Inspect service worker) around each reconcile step (plan, per-tab
create/reuse/move, group, close). Errors already flow to the desktop app as
`capture_error`/`restore_error` → warnings; extend that so a reconcile failure
surfaces there too, so the tester sees problems without opening devtools. A
visible in-extension log page is deliberately avoided; if wanted later it's a
tiny options page reading a ring buffer from `storage.local`.

### On-device test setup (today — manual, one browser)

1. Build the native host: `cargo build --bin pc_snapshot_native_host` (from
   `src-tauri/`, per the OneDrive target-dir rule). Optionally add an explicit
   `[[bin]]` to `Cargo.toml` for clarity, though auto-discovery already works.
2. Build the unpacked extension: `node companion-extension/scripts/build-chromium.mjs`
   → `dist/chromium/`.
3. Load unpacked `dist/chromium/` in Chrome/Edge (`chrome://extensions`,
   Developer mode on). **Verify the loaded extension ID equals the pinned id in
   `register-chromium-host.ps1` (`chfbdgfhlkbocpeofdjkincopepifnlj`)** — the
   manifest `key` should force this, but a mismatch silently breaks native
   messaging, so confirm it, don't assume.
4. Register the host: run `register-chromium-host.ps1 -HostExecutable
   <path to target/debug/pc_snapshot_native_host.exe>`.
5. Run the desktop app (`npm run tauri dev`), capture with the browser open,
   then restore and watch the pipeline.

### Phase 2 — Cross-browser native-host registration (LATER, not today)

Today only `register-chromium-host.ps1` exists (Chrome + Edge, hardcoded dev
extension id, run by hand; the `.template.json` still has `REPLACE_…`
placeholders). To actually reach "all browsers":

- **Chromium family** (Chrome, Edge, Brave, Vivaldi, Opera): register the host
  manifest under each browser's `HKCU\Software\<vendor>\NativeMessagingHosts\
  app.pcsnapshot.companion` key, `allowed_origins: ["chrome-extension://<id>/"]`.
  Verify each vendor's registry root during implementation (Brave/Vivaldi/Opera
  do not all read Chrome's key — confirm, don't assume).
- **Firefox**: different contract — `HKCU\Software\Mozilla\NativeMessagingHosts`,
  and the manifest uses `allowed_extensions: ["<amo-addon-id>"]`, not
  `allowed_origins`. `manifest.firefox.json` already exists; needs its own
  register script.
- **Installer integration**: the Tauri bundler must ship
  `pc_snapshot_native_host.exe` as a resource/sidecar, and the NSIS/WiX install
  step must run registration pointing the manifest `path` at the *actual*
  install location (not the hardcoded `C:\Program Files\PC Snapshot\…`), for
  every browser detected on the machine. Uninstall must remove the keys.

### Phase 3 — Extension distribution (LATER, not today)

The manifest ships a dev `key` → fixed dev id
`chfbdgfhlkbocpeofdjkincopepifnlj`. Production "all browsers" needs published
listings (Chrome Web Store, Edge Add-ons, Firefox AMO, Opera) or an
enterprise/sideload story, and the registered `allowed_origins`/
`allowed_extensions` must match each published id. For development and the
initial live test, sideloading the unpacked `dist/chromium` build against the
dev id is sufficient and already works.

## Risks / things to prove during implementation (not assume)

1. **Reconcile has never run against a live browser.** This is the reason codex
   gated it. Highest-priority verification (Phase 1). Watch specifically:
   tab-`index` stability while creating+moving in the same window; the blank tab
   `windows.create` spawns (tracked as `createdBlankTabIds`, only closed when
   `close_extras` is true — it leaks otherwise); and active-tab restoration.
2. **Profile identity is a per-extension-storage UUID.** Restore matches the
   captured `profile_instance_id` against a *connected* one. A different browser
   profile, a cleared extension storage, or a fresh install won't match and that
   browser silently won't reconcile (degrades to warning). Multi-profile users
   are ambiguous. Acceptable for MVP; document it.
3. **Timing (1d).** If the launch→connect wait is too short, reconcile reports
   "not connected" and falls back. Measure real reconnect latency.
4. **Old capture path still runs redundantly.** `capture.rs:217`
   (`read_open_tab_urls`/SNSS) still populates `browser_tab:` hints alongside the
   extension capture. Harmless (feeds the fallback) but redundant CPU; decide
   whether to keep as the no-extension fallback (recommended, one release) or
   remove once the companion is the default.
5. **Firefox** uses a different session/native-messaging contract throughout;
   treat it as its own verification target, not assumed-equal to Chromium.

## Verification (behavior, not exit code)

- **Unit (exists / extend):** `planTabReconciliation` and bridge completion are
  already covered. Add a `background.js` test that a `restore_request` routes to
  `reconcileBrowserSession` and replies `restore_result{report}`.
- **End-to-end, needs the user (their own workflow):**
  1. Capture with one browser window / N tabs.
  2. Open two more windows with different tab sets; add/remove a few tabs.
  3. Restore with **Close others ON** → the two extra windows' stray tabs close,
     the snapshot's exact tabs/order/groups/active come back, matching URLs are
     reused (not duplicated).
  4. Close the browser entirely, restore again → it relaunches and reconciles to
     exactly the captured windows/tabs.
  5. Restore with **Close others OFF** → missing tabs open, order fixed, nothing
     closed.
  6. Repeat across Chrome, Edge, Opera, Firefox.

## Critical files

- `companion-extension/src/background.js` — **wire `restore_request` → reconcile**
- `src-tauri/src/lib.rs` — `restore_snapshot` takes the bridge State, calls
  `bridge.restore()`, folds the report; sets `companion_managed_browsers`
- `src-tauri/src/restore.rs` — refine the `companion_managed_browsers` branch
  (launch plainly vs skip vs fallback) + launch→connect→reconcile sequencing
- `companion-extension/scripts/*` + Tauri installer — Phase 2 registration
- `companion-extension/manifest.*` + store listings — Phase 3 distribution
