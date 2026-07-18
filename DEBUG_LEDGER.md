# PC Snapshot — Debug Ledger

Companion to `DEBUG_PLAN.md`. One row per issue. Nothing is "done" until its row
has a verification note (behavior re-checked, not just a green build).

Severity: **S1** crash/data-loss/kills-protected-process · **S2** wrong result /
dropped event / off-screen window · **S3** cosmetic / style / micro-efficiency.

---

## Baseline health — 2026-07-12 (Day 1 sweep, executed)

| Suite | Command | Result |
|---|---|---|
| Frontend lint | `npm run lint` | ✅ clean, 0 issues |
| Frontend build | `npm run build` (`tsc -b && vite build`) | ✅ built in 9.35s (only a cosmetic Vite plugin-timings note) |
| Companion tests | `npm run test:companion` | ✅ 3/3 pass |
| Rust tests | `cargo test` (from `src-tauri/`) | ✅ 18 pass, 0 fail, 1 ignored |
| Rust lint | `cargo clippy --all-targets` (from `src-tauri/`) | ⚠️ 0 errors, 21 style warnings (all S3) |

Notes:
- `cargo` target correctly redirects to `C:/cargo-build/` (OneDrive rule active).
- The 1 ignored Rust test is `browser::tests::real_browser_session` — needs a live
  browser; belongs to the Day 4 on-device pass, correctly `#[ignore]`d.
- **The tree is buildable and test-green.** The debugging week starts from a clean
  baseline, so anything found from here is a genuine regression/gap, not noise.

---

## Open items

| id | area | sev | issue | repro / note | fix | verified |
|---|---|---|---|---|---|---|
| L-001 | rust/style | S3 | 21 clippy warnings (`map_or`, `&PathBuf`→`&Path`, manual char comparison, boolean simplification, doc-list indentation) | `cargo clippy --all-targets` from `src-tauri/`; 13 auto-fixable via `cargo clippy --fix --lib -p pc-snapshot` | — | — |
| L-002 | tests/rust | S2-risk | `capture.rs` has **no unit tests** — untested core of capture pipeline | Day 2 | — | — |
| L-003 | tests/rust | S2-risk | `context.rs` has **no unit tests** — untested clue extraction | Day 2 | — | — |
| L-004 | tests/rust | S2-risk | `config.rs` / `activity.rs` have no unit tests | Day 5 / Day 6 | — | — |
| L-005 | tests/frontend | S2-risk | **no frontend test runner exists** (no `*.test.tsx`, no test script) | Day 6 — stand up Vitest | — | — |
| L-006 | rust/crash | S1-risk | densest `unwrap/expect/panic` cluster is in `browser_bridge.rs`, on the untrusted named-pipe input path | Day 4/7 audit | — | — |
| L-007 | process/git | — | large UI/activity/vscode workstream uncommitted — no tracking baseline for regressions | **needs user go-ahead to commit** | — | — |
| L-008 | frontend/boot | S2 | app stuck on "Loading PC Snapshot…" splash after system sleep. On resume, WebView2 trims/terminates the renderer and re-navigates; in dev the Vite module fetch can fail with no retry, so `main.tsx` never runs and the static `#boot` splash (index.html) stays up forever. Prod (local files) rarely trips it. | leave `npm run tauri dev` open, sleep the machine, resume | boot self-heal in `index.html`: capture-phase resource-error listener + 15s mount-timeout backstop → bounded (max 3, sessionStorage) `location.reload()` | ✅ happy path re-verified in browser (React mounts, splash replaced, no spurious reload); prod build preserves watchdog; guard predicate table all-correct (script/css→reload, favicon/img/runtime/mounted→no reload) |
| L-009 | browser/companion | S2 | "Browser Companion did not respond for N profile(s)" warning + 0 tabs captured even with a browser open. Native host (`pc_snapshot_native_host`) blocks on the *pipe*, not stdin, so when a browser closes it keeps running and holds its bridge pipe open. The bridge still counts it as a connected profile and sends capture requests it can never answer → 1.2s timeout → warning. Zombies accumulate across the app's uptime (explains "3 profiles" with one browser open). | open+close a couple browser windows, then capture with one browser still open | native host now `std::process::exit(0)` on stdin EOF / broken port, so the pipe drops and the bridge's existing disconnect path removes the session. **Not** added: blind prune-on-timeout — would drop a live-but-slow MV3 profile (service-worker cold-start >1.2s); #1 clears true zombies structurally so pruning is unneeded and risky. | ⏳ compiles clean; runtime proof needs rebuilt host re-registered + live browser (Day-4 on-device). Existing zombies clear on desktop-app restart (in-memory sessions reset). |
| L-010 | browser/companion | S2 | capture returns 0 tabs (same warning) even with the extension installed & active; only a manual "Reload" of the extension fixes it, temporarily. Root cause: MV3 service worker (`background.js`) is terminated after ~30s idle, dropping the native connection, and it only ever calls `connectNative()` on install/startup/SW-load — nothing reconnects. Manual reload restarts the SW, which re-connects; capture works only in that window. | install companion, leave browser idle >30–60s, take a snapshot | (a) `chrome.alarms` keepalive (`periodInMinutes: 0.4`) that wakes the SW and reconnects with no user action; (b) native host heartbeat every 20s to keep the SW warm between alarm ticks (immune to alarm-period clamping); (c) `alarms` permission added to both manifests. | ⏳ host compiles, `test:companion` 3/3, rebuilt dist carries `alarms` + keepalive wiring. Behavioral proof (SW survives >60s idle, capture succeeds without reload) needs on-device: rebuild+re-register host, load rebuilt extension, one final reload, then verify. |
| L-010b | browser/companion | S2 | L-010's keepalive was insufficient — user still had to manually reload before capture. Deeper cause: the 30s alarm is a *floor* (Chrome clamps sub-30s periods) and the host heartbeat only flows while the desktop app is running to send it. In the normal flow (browser open, desktop app closed until you want a snapshot), the SW dies every ~30s with nothing warming it; opening the app + capturing immediately finds a dead port and an empty session map, which `capture()` reported instantly with zero grace. | close desktop app, browse for a minute, open app, capture right away | (a) extension reconnects on ordinary browser activity — `tabs.onActivated` / `tabs.onUpdated` / `windows.onFocusChanged` all call the guarded `connectNative()`, so any interaction re-establishes the port within a keystroke instead of ≤30s; (b) desktop `capture()` now polls up to 1.5s for a profile to register before declaring "not connected", covering the ~1s host reattach after the app opens. | ⏳ lib compiles clean, bridge unit test green, `test:companion` 3/3, dist rebuilt. Behavioral proof (capture succeeds without reload after the app was closed) needs on-device. |
| L-011 | browser/companion | S2 | cold-browser restore doesn't reconcile tabs: restoring a browser-containing session when the browser was **not** running launches the browser but the companion isn't connected yet, so tabs aren't restored. Root cause: `restore_one`'s connect wait was 5s, far shorter than a real Chromium cold start + MV3 SW init + native handshake (routinely >5s), so it timed out and reported "the companion extension is not connected for this browser profile". | close the browser entirely, restore a snapshot that has browser tabs | extended `restore_one` connect deadline 5s → 15s; the full wait only elapses when the companion is genuinely absent, otherwise it proceeds as soon as the freshly-launched browser's companion attaches. | ⏳ lib compiles clean, bridge unit test green. Behavioral proof (cold restore reconciles tabs) needs on-device. |

Add rows as issues are found. Move nothing to "verified" without a behavior re-check.
