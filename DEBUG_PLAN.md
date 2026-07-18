# PC Snapshot — Weeklong Debugging & Hardening Plan

> A 7-day, behavior-first campaign to find and fix bugs across the whole product
> before the production-ready target (2026-07-24). Grounded in the actual module
> surface as of 2026-07-12, not generic advice. Each day ends on a concrete exit
> criterion, and every claim of "fixed" is proven against running behavior, not
> exit codes (see *Operating rules* below).

---

## Operating rules (apply every day)

1. **Behavior over decoration.** A bug is fixed when the user-facing behavior is
   correct in the running app — a green `cargo test` or `exit 0` is not proof.
   Reproduce → fix → re-run the *same* reproduction and watch it pass.
2. **One bug ledger.** Every issue goes in `DEBUG_LEDGER.md` (create day 1) with:
   id, area, repro steps, severity (S1 crash/data-loss · S2 wrong result · S3
   cosmetic), root-cause note, fix commit, verification note. Nothing is "done"
   until its ledger row has a verification note.
3. **Fix the class, not the instance** (global rule 9). When a bug is found, ask
   "what makes this whole category possible?" and add a test or guard that seals
   the class. A one-off patch that leaves the class open is a half-fix.
4. **Refute, don't confirm** (global rule 5). When re-testing a fix, actively try
   to break it with the adversarial input, not just re-run the happy path.
5. **Transient vs structural** (global rule 11). On any failure, first classify:
   flaky timing/external condition (retry + add tolerance) vs code/config defect
   (halt, diagnose, fix). Don't paper a structural bug with a retry.
6. **Commit small and often** (global rule 12). One fix per commit, each with its
   ledger id in the message. The uncommitted UI/activity/vscode workstream is the
   riskiest thing in the tree right now — get it committed on day 1.

### Environment gotchas (bake into every command)
- **Run `cargo` from `src-tauri/`** so the OneDrive target-dir redirect applies;
  OneDrive dehydrates `target/`. A cargo run from repo root writes to the wrong
  place and can appear to "lose" build output.
- Frontend: `npm run dev` (Vite) or `npm run tauri dev` (full app). Type+bundle:
  `npm run build` (`tsc -b && vite build`). Lint: `npm run lint`.
- Companion unit tests: `npm run test:companion`.
- Rust tests: `cargo test` from `src-tauri/`.
- Only `npm run tauri dev` exercises real Win32 capture/restore — Vite alone
  can't. Every capture/restore/positioning check must run under the full app.

---

## Day 1 — Baseline green + static sweep + harness

**Goal:** stop the bleeding — get the tree building, linting, and testing clean,
commit the in-flight workstream, and stand up the bug ledger. You cannot find
regressions on a tree that doesn't compile.

**Find bugs:**
- Commit the uncommitted workstream (`MissionControl.tsx`, `SettingsPage.tsx`,
  `activity.rs`, `vscode.rs`, `activity.ts`, `useActivity.ts`) so all further work
  has a clean diff baseline. If it's not review-ready, commit to a WIP branch.
- Run and record clean/dirty state for: `npm run build`, `npm run lint`,
  `cargo test` (from `src-tauri/`), `cargo clippy --all-targets`,
  `npm run test:companion`. Every warning is a ledger candidate.
- `cargo clippy` with `-W clippy::all` — triage every lint; unhandled `Result`,
  `.clone()` in hot paths, and `#[allow]` suppressions are prime suspects.
- Grep the tree for `TODO`, `FIXME`, `HACK`, `unwrap()`, `expect(`, `unimplemented!`
  and log each non-test site into the ledger with a risk note.

**Fix / harden:**
- Get all five commands to green (or documented-skip). No red baseline goes into
  day 2.
- Delete or wire `src/mock/data.ts` if it's dead — stale mock data masks real
  data-flow bugs.

**Exit criteria:** clean `build` + `lint` + `cargo test` + `test:companion`;
`DEBUG_LEDGER.md` exists; workstream committed.

---

## Day 2 — Capture pipeline (the untested heart)

**Why here:** `capture.rs` and `context.rs` have **zero unit tests** yet own the
entire capture correctness story. This is the highest-value coverage gap.

**Find bugs (drive `npm run tauri dev`):**
- **Enumeration correctness.** Capture with a known desktop (VS Code + Chrome +
  a terminal + Explorer + a minimized app + a maximized app on a second monitor).
  Read the saved `{id}.json` and verify every real window is present with correct
  title, position, size, state, `monitor_index`, and `exe_path`. Note anything
  missing, duplicated, or mis-sized.
- **Foreground-first invariant.** Confirm the foreground window is processed first
  and classified `Foreground`.
- **The <3s budget.** Time capture with 30+ windows open. If it exceeds 3s or the
  UI thread stalls, that's an S2. Verify the screenshot thread truly overlaps
  enumeration (add temporary timing logs, then remove).
- **Ghost/edge windows.** Tool windows, cloaked UWP windows, `WS_EX_TOOLWINDOW`,
  0-size and off-screen windows — confirm the filter excludes noise without
  dropping real windows.
- **Process mapping.** Elevated processes, processes with no accessible cmd line,
  and PID reuse — confirm each degrades to a warning (one of the 5 warning seams
  in `capture.rs`) rather than panicking or emitting a bogus row.
- **Context heuristics (`context.rs`).** For each clue type (VS Code workspace,
  browser session, terminal CWD, local dev server) verify the value is correct and
  the confidence score is sane. Feed a deliberately weird case (a repo path with
  spaces/unicode, a dev server on a nonstandard port) and check it doesn't
  misfire.

**Fix / harden:**
- Write the **first unit tests** for `capture.rs`/`context.rs`: factor pure logic
  (window-filter predicate, monitor-index math, clue extraction from a fixture
  string) out of the Win32 calls so it's testable without a live desktop, then
  cover the edge cases you found.
- Seal each bug with a fixture case.

**Exit criteria:** capture verified across the multi-window matrix; `capture.rs`
and `context.rs` have unit tests covering the found edge cases; every capture
failure path degrades to a warning.

---

## Day 3 — Restore pipeline + window positioning + multi-monitor

**Why here:** restore is the product's payoff and its most failure-prone surface —
9 degradation seams live in `restore.rs`. Positioning across monitors/DPI is where
"looks right" and "is right" diverge.

**Find bugs (drive `npm run tauri dev`):**
- **Round-trip fidelity.** Capture a known layout → move/close everything →
  restore → diff actual window rects against the snapshot. Log every window that
  lands on the wrong monitor, wrong position, or wrong state (maximized restored
  as normal, etc.).
- **Launch order.** Verify the Background → Terminal → IDE → Browser → Foreground
  ordering actually holds, and that already-running processes are *reused*, not
  relaunched (spawning a second VS Code is an S2).
- **Window-appearance wait.** Slow-launching apps: confirm the bounded wait is
  long enough (no premature "window not found") but doesn't hang. Test the app
  that never opens a window — it must degrade to a warning.
- **Multi-monitor matrix.** Run the positioning matrix (below) — this is the
  single most bug-dense area. Different DPI per monitor, monitor to the *left*
  of primary (negative X coords), a monitor unplugged since capture (window must
  land somewhere visible, not off-screen into the void).
- **Office MRU multi-window.** Restore a snapshot with two Word/Excel docs; verify
  the registry-MRU path opens the right documents.
- **Honest reporting.** Force partial failures (rename an exe so it can't launch)
  and confirm `failed_items` / `warnings` / `closed_items` are populated correctly
  and surfaced in the restore report — never hidden, never faked (global rule:
  honest reporting).

**Fix / harden:**
- Add restore unit tests for the pure pieces (launch-order sort, monitor-fit /
  clamp-to-visible math, MRU parsing).
- Add a **clamp-to-visible-bounds** guard if any window can restore off-screen.

**Multi-monitor / DPI matrix (run every case, record pass/fail):**

| Case | Setup | Expected |
|---|---|---|
| Single monitor | 1 display | exact rect restore |
| Dual same-DPI | 2 @ 100% | correct monitor + rect each |
| Dual mixed-DPI | primary 150%, second 100% | rect scales correctly, no drift |
| Left monitor | second monitor left of primary (neg X) | negative coords honored |
| Monitor removed | capture on 2, restore on 1 | window clamps to visible, warned |
| Maximized | maximized on second monitor | restores maximized on same monitor |
| Minimized | minimized window | restores minimized, not shown |

**Exit criteria:** the full matrix has a recorded result; no window restores
off-screen; reuse-not-relaunch verified; restore report matches reality.

---

## Day 4 — Browser companion (never live-tested — the #1 risk)

**Why here:** reconcile has, per the project's own spec, **never run against a
live browser**, and `browser_bridge.rs` holds the codebase's densest cluster of
`unwrap/expect/panic` while parsing **untrusted external input** over a named pipe.
This is the most likely place for both a wrong result *and* an outright crash.

**Setup (from the browser spec's on-device steps):**
1. `cargo build --bin pc_snapshot_native_host` (from `src-tauri/`).
2. `node companion-extension/scripts/build-chromium.mjs` → `dist/chromium/`.
3. Load unpacked in Chrome/Edge; **verify the loaded extension ID equals the
   pinned id** in `register-chromium-host.ps1` — a mismatch silently breaks
   native messaging.
4. Run `register-chromium-host.ps1` with the host exe path.
5. `npm run tauri dev`.

**Find bugs — run the spec's end-to-end script and watch for:**
- **Reconcile correctness.** Capture 1 window / N tabs → open extra windows and
  tabs → restore with **Close others ON**: exact tabs/order/groups/active come
  back, matching URLs are *reused not duplicated*, extras close. Then **Close
  others OFF**: missing tabs open, order fixed, nothing closes.
- **Tab-index stability** while creating+moving in the same window (the spec's
  named risk #1).
- **The blank-tab leak.** `windows.create` spawns a blank tab tracked as
  `createdBlankTabIds`, only closed when `close_extras` is true — confirm it does
  not leak a stray blank tab when Close-others is OFF.
- **Create-before-close safety.** A mid-reconcile failure must never empty the
  browser — verify by killing the native host mid-restore.
- **Launch→connect→reconcile timing.** Close the browser entirely, restore: it
  must relaunch, wait for the companion to reconnect, then reconcile. Measure real
  reconnect latency; if the wait is too short it falsely reports "not connected."
- **Profile identity.** A different profile / cleared extension storage must
  degrade to a warning, not a crash or a wrong-profile write.
- **Crash-hardening the pipe.** Feed the bridge malformed frames (truncated JSON,
  oversized payload, wrong `protocol_version`, missing `request_id`). Under the
  "never crash" constraint, **every one must degrade to a warning** — this is where
  the `unwrap/expect` audit pays off.

**Fix / harden:**
- Audit **every non-test `unwrap/expect/panic` in `browser_bridge.rs`** and the
  native host; replace with `Result` propagation → warning on the untrusted-input
  path. This is the day's most important structural fix.
- Extend `background.js` tests: a `restore_request` routes to
  `reconcileBrowserSession` and replies `restore_result{report}`.

**Exit criteria:** full end-to-end script passes on ≥1 Chromium browser; no
malformed input can crash the bridge; blank-tab leak and empty-browser cases
proven safe. (Cross-browser registration + Firefox + store distribution stay out
of scope — those are Phase 2/3 productization, not bug-fixing.)

---

## Day 5 — Context sources + Start New Session safety

**Why here:** context capture (VS Code, terminal, dev servers) drives restore
fidelity, and **Start New Session closes/kills user windows** — the single most
destructive action in the app. A bug here loses user work.

**Find bugs:**
- **VS Code capture (`vscode.rs`).** *(User reports this working — verify, don't
  assume, per behavior-over-claim.)* Test single-folder workspace, a
  just-opened folder (may not be flushed to `storage.json` yet — confirm graceful
  absence), Cursor and Code-Insiders variants, a multi-root `.code-workspace`
  (spec says deferred — confirm it's cleanly skipped, not mis-captured), and a
  corrupt/missing `storage.json` (must return empty, not panic).
- **Terminal capture (`terminal.rs`/`terminal_hook.rs`).** CWD, shell, and history
  accuracy; a shell with no hook installed; a CWD with spaces/unicode; the toggle
  off path.
- **Local dev-server clue.** A running dev server on a standard and a nonstandard
  port; confirm no false positives.
- **Start New Session / `close_all_windows` (S1 territory).**
  - The **system-critical protection list** (explorer.exe, csrss.exe, …) is never
    killed — verify explicitly; a bug here can crash the desktop.
  - The **user ignore list** is respected.
  - **Graceful first:** `WM_CLOSE` before force-kill, force only after timeout.
  - **The save-first safety net:** the confirmation dialog appears unless a
    snapshot was saved in the last 60s, and "Save & Continue" actually saves
    before closing. Losing unsaved work because the guard misfired is the worst
    possible bug in this app.

**Fix / harden:**
- Unit-test the protected-process predicate and the ignore-list filter — seal the
  "never kill critical/ignored process" class with tests, not just a manual check.
- Add tests for `vscode.rs` parsing against fixture `storage.json` blobs
  (valid, empty, corrupt, each editor variant).

**Exit criteria:** VS Code/terminal/dev-server clues verified across the edge
cases; Start New Session proven to never kill a protected/ignored process and
never lose unsaved work silently; parsing is corruption-tolerant.

---

## Day 6 — Frontend, activity console & honest reporting end-to-end

**Why here:** the UI is the newest code (just committed day 1) with **zero test
coverage**, and the activity console is the product's honest-reporting home — if
it lies or drops events, the core principle breaks.

**Find bugs (drive the running app + browser preview tools where useful):**
- **State-flow bugs.** Capture → the tile appears without a manual refresh.
  Restore/recapture/delete/clear-all each update the grid and the activity feed
  correctly. Rapid double-clicks don't double-fire commands. An in-flight capture
  disables the button (no concurrent captures).
- **Activity feed (`activity.rs`/`useActivity.ts`).** Every operation (capture,
  restore success/partial/failed, delete, start-new) emits exactly one event with
  the right status; the feed streams live; a failed restore shows its
  `failed_items`/`warnings` in detail lines (matches the redesign brief's
  terminal-console requirement). Confirm no event is dropped and none is duplicated.
- **Settings page (`SettingsPage.tsx`).** *(User reports finished — verify.)* Every
  section renders; the ignore-list editor add/remove persists to `config` and is
  read back by capture; the terminal-capture toggle round-trips; navigation
  between side-panel sections doesn't lose state.
- **Empty & scale states.** First-run empty state invites capture; the grid stays
  performant and correct at 0, 1, and 100+ snapshots.
- **Modal correctness.** Restore-confirm (close-others + save-first), recapture-
  confirm, name-prompt — each returns the right choice and cancel truly cancels.
- **Error surfacing.** A backend command that returns `Err` shows a real message
  to the user (toast/console), never a silent no-op or an unhandled promise
  rejection (check the devtools console via the preview tools).

**Fix / harden:**
- Stand up a **frontend test runner** (Vitest fits the Vite stack) and write the
  first component/hook tests for the state flows above — closes the app's largest
  coverage gap.
- Fix any state desync (missing refresh, stale closure in a hook) at the hook
  level, not with a band-aid re-render.

**Exit criteria:** all state flows verified in the running app; activity feed
proven complete-and-truthful for every operation; a frontend test runner exists
with tests for the core flows; no unhandled console errors.

---

## Day 7 — Fault injection, never-crash hardening & regression seal

**Why here:** the app's hardest constraint is *"never crash; all fallible ops
degrade gracefully."* You prove that by attacking the 32 degradation seams, not by
hoping. Then you seal the week so nothing regresses.

**Find bugs — chaos pass across all 32 warning/degrade seams:**
- **Corrupt storage.** Hand-mangle a `{id}.json` (truncate, invalid UTF-8, wrong
  `schema_version`, missing required field) and confirm it loads-with-warning or
  skips — never crashes the list (schema must be "tolerant to partial corruption"
  per the constraints).
- **Disk faults.** Read-only `AppData/Snapshots`, disk full on save, missing
  thumbnail PNG with present JSON (and vice-versa) — each degrades.
- **Permission faults.** Capture/position a window owned by an elevated process
  from a non-elevated app — must warn, not fail hard.
- **Concurrency.** Two restores fired back-to-back; capture during a restore;
  browser bridge receiving a second request before the first replies.
- **External-input fuzz.** Re-hit the native-messaging and native-host paths with
  malformed frames (from day 4) plus giant payloads and rapid connect/disconnect.
- **Resource exhaustion.** 200+ windows; a 4K multi-monitor screenshot;
  confirm no OOM/timeout crash.

**Fix / harden:**
- Any remaining non-test `unwrap/expect/panic` reachable from external or on-disk
  input becomes a `Result` → warning. Goal: **no user-reachable panic anywhere.**
- Add a top-level catch so any command that does panic returns an error to the UI
  instead of taking down the app.

**Seal the week:**
- Every ledger S1/S2 has a fix + a regression test (Rust unit, companion test, or
  frontend test) that would fail without the fix. Prove it: revert the fix, watch
  the test go red, restore it.
- Full green run of all five command suites, recorded.
- Write `DEBUG_REPORT.md`: bugs found by severity, bugs fixed, tests added,
  coverage before/after, and the **known-remaining** list (anything deferred, e.g.
  cross-browser companion registration, Firefox, retry-only-failed-restore-items)
  stated honestly — never claim a clean bill you didn't earn.

**Exit criteria:** no user-reachable panic; every S1/S2 sealed by a failing-
without-the-fix test; all suites green; `DEBUG_REPORT.md` written with an honest
known-remaining list.

---

## Standing checklists

### The 5 verification suites (run at each day's end)
- [ ] `npm run build` (`tsc -b && vite build`)
- [ ] `npm run lint`
- [ ] `cargo test` (from `src-tauri/`)
- [ ] `cargo clippy --all-targets` (from `src-tauri/`)
- [ ] `npm run test:companion`

### Coverage gaps to close this week (grounded in current state)
- [ ] `capture.rs` — no tests (day 2)
- [ ] `context.rs` — no tests (day 2)
- [ ] `config.rs` — no tests (day 5, via ignore-list/protected-process predicates)
- [ ] `activity.rs` — no tests (day 6)
- [ ] frontend — **no test runner at all** (day 6)
- [ ] `browser_bridge.rs` — untrusted-input unwrap audit (day 4/7)

### Severity key
- **S1** — crash, data loss, or killing a protected process. Fix before anything.
- **S2** — wrong capture/restore result, dropped/duplicated event, off-screen
  window. Fix this week.
- **S3** — cosmetic / copy / minor layout. Batch at the end.

---

## Scope notes
- **In scope:** correctness, crash-safety, honest-reporting fidelity, restore
  accuracy across the monitor/DPI matrix, and closing the test-coverage gaps.
- **Out of scope (productization, not bugs):** cross-browser native-host
  registration, Firefox companion, extension-store distribution,
  retry-only-failed-restore-items. Track these separately; don't let them expand
  the debugging week.
- **User-asserted-done, still verified this week** (behavior over claim): the
  Settings page (day 6) and VS Code capture (day 5). Verification is a read, not a
  rebuild — if they pass, they pass fast.
