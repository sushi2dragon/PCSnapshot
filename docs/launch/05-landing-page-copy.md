# PC Snapshot — Landing Page Copy (First Draft)

*Publishable after light editing. Structure mirrors the website design in `03-commercial-strategy.md`. Pricing figures match the commercial strategy: Free core + PC Snapshot Pro at **$29 one-time** (regional pricing applies; ~₹1,499 in India via purchasing-power parity discount). All claims below are grounded in implemented features — nothing here promises what the product doesn't do.*

---

## Hero section

**Headline:**
# Your whole workspace. One click back.

**Subheadline:**
PC Snapshot captures everything on your Windows desktop — every app, window position, browser tab, and terminal directory — as a visual snapshot. When you're ready to work again, one click brings it all back. Exactly as you left it.

**Primary CTA button:** `Download for Windows — Free`
**Secondary CTA (text link):** `See how it works ↓` (scrolls to demo video)

**Under the CTA (trust microcopy):**
No account. No cloud. No telemetry. Your snapshots never leave your PC.

**Hero visual:** 15-second looping video/GIF: cluttered dev workspace → "Take Snapshot" click → clean desktop → tile click → everything reopens into place, tabs included.

---

## Problem section

**Header:** You know this feeling.

It's Monday morning. Or after a reboot. Or you're switching from Client A to Client B. Either way, the ritual is the same: reopen the project folder, restart the terminals, `cd` back into the repo, relaunch the IDE, hunt down the fifteen browser tabs you had open, and drag every window back to the right monitor.

Ten minutes, twice a day, every day. That's over **80 hours a year** spent rebuilding a workspace you already built.

Windows won't remember it for you. Your browser only remembers its own tabs. Window managers only remember positions — not the apps, not the tabs, not the terminal you had sitting in the right directory.

**PC Snapshot remembers all of it.**

---

## How it works (3 steps, icons + short demo clips)

**1. Capture**
Click *Take Snapshot*. In under three seconds, PC Snapshot records every open app, where each window sits on which monitor, your exact browser tabs, your terminal working directories, and your VS Code workspaces — and takes a screenshot so you'll recognize it later.

**2. Browse**
Your snapshots live in a visual grid of desktop screenshots. No naming schemes, no folders to organize. You spot last Tuesday's editing session the way you'd spot your own desk — by looking at it.

**3. Restore**
Click a snapshot. Apps that are already running are reused; the rest launch in the right order. Windows return to their positions and monitors. Your browser gets your exact tabs back — same windows, same order, same groups. Terminals reopen where they were working.

---

## Feature sections

### The only tool that restores your *exact* browser tabs
Other tools "reopen your browser." PC Snapshot's lightweight companion extension reconciles your live browser to the snapshot: it reuses tabs that are already open, opens the missing ones, restores the order and tab groups, and closes the strays. Works with Chrome, Edge, Brave, Opera, and Opera GX. The companion is headless and local — no network access, no history access, no cookies. *(Firefox support is on the roadmap.)*

### Built for people who live in the terminal
Terminals reopen in the right working directories. VS Code reopens the right workspace. Local dev servers are detected and noted in the snapshot. If your workspace is "three terminals, an IDE, and localhost:5173," PC Snapshot was built for you.

### Workspaces you set up once, reuse forever
A snapshot doesn't have to be "how I left things." Set up your ideal environment — the client project, the editing rig, the writing setup — capture it, and it becomes a one-click profile. Recapture any snapshot to update it in place. Mark the one you're currently working in. Keep an ignore list for apps that should never be touched.

### Honest by design
Some things can't come back — an admin-elevated window, an app that's been uninstalled. PC Snapshot never pretends. Every restore ends with a plain-language report of what came back and what didn't. You'll always know exactly where you stand.

### Native speed, tiny footprint
Built in Rust, not Electron. Capture completes in under three seconds; screenshots happen on a background thread so nothing stutters. The whole app is a fraction of the size and memory of a browser tab.

---

## Privacy / trust section

**Header:** Your workspace is a map of your work. It stays home.

- **100% local.** Snapshots are files on your disk (`%APPDATA%`), not records on a server. There is no server.
- **No account. No sign-up. No subscription required to use it.**
- **No telemetry.** The app phones nobody.
- **Auditable.** Snapshots are human-readable JSON — open one and see exactly what was captured.
- **The browser companion can't spy.** It has no network access, no content scripts, no cookie or history access — it only reads tab titles and URLs, and only talks to the app on your own machine.

---

## Comparison section

**Header:** Half-solutions everywhere. One whole one.

| | Window managers (DisplayFusion, FancyZones) | Tab managers (Workona, session extensions) | Windows "restart apps" setting | **PC Snapshot** |
|---|---|---|---|---|
| Restores window positions | ✅ | ❌ | Partially | ✅ |
| Relaunches your apps | ❌ | ❌ | Unreliably | ✅ |
| Restores exact browser tabs | ❌ | ✅ (browser only) | ❌ | ✅ |
| Restores terminal directories | ❌ | ❌ | ❌ | ✅ |
| Visual snapshot browsing | ❌ | ❌ | ❌ | ✅ |
| Multiple saved workspaces | Layout profiles only | Browser only | ❌ | ✅ |
| Works without cloud/account | ✅ | Mostly ❌ | ✅ | ✅ |

*(Fair-comparison note for editing: DisplayFusion is excellent at what it does — this table is about scope, not quality. Keep the tone respectful.)*

---

## Pricing section

**Header:** Own it. One payment, no subscription.

| **Free** | **Pro — $29, one-time** |
|---|---|
| For picking up where you left off | For running your work life on snapshots |
| Unlimited captures & restores | Everything in Free, plus: |
| Window layout & multi-monitor restore | Unlimited saved snapshots *(Free keeps your 3 most recent)* |
| Exact browser-tab restore (companion) | Named workspace profiles & recapture |
| Terminal & VS Code context restore | Ignore list & advanced restore controls |
| Honest restore reports | Priority email support |
| | 1 year of updates included — keep every version you've licensed, forever |

**Microcopy under pricing:**
- 14-day unconditional refund policy. Email us, get your money back, keep no hard feelings.
- Licenses are per-person, usable on all your own PCs (up to 3 activations).
- Regional pricing available — fair prices in 100+ countries at checkout.
- Prices exclude local taxes where applicable; checkout is handled by our merchant of record.

**CTA:** `Get Pro — $29` / `Download Free`

---

## Testimonials section

*(Placeholder at launch — populate from beta testers before going live; see checklist 06. Format: screenshot-style cards, name + role + one concrete sentence, e.g. "I bill by the hour and PC Snapshot gives me back the first 15 minutes of every client switch." — Freelance developer)*

---

## FAQ

**Does it restore unsaved work inside apps?**
No — and be wary of anything that claims to. PC Snapshot relaunches your apps, restores their windows, positions, tabs, terminal directories, and workspaces. Documents reopen via each app's own recovery/recent-files behavior. Save your work; PC Snapshot handles everything around it.

**Which browsers are supported for exact tab restore?**
Chrome, Microsoft Edge, Brave, Opera, and Opera GX via the companion extension. Firefox is on the roadmap. Without the companion, browsers still relaunch and reposition — you just don't get tab-level reconciliation.

**Is my data uploaded anywhere?**
No. Snapshots are JSON + PNG files in your own AppData folder. The app has no accounts, no sync, and no telemetry. The companion extension has no network permissions at all.

**What happens if an app can't be restored?**
You get an honest report. If an app was uninstalled or needs admin rights we can't request, the restore completes everything else and tells you exactly what it skipped.

**Does the free version expire?**
No. Free is free forever — full capture/restore fidelity, limited to your 3 most recent snapshots. Pro removes the limit and adds workspace-profile management.

**What does "1 year of updates" mean?**
Your license includes every update released for a year after purchase, and you keep everything you've licensed forever. After a year, the app keeps working; you can optionally renew at a discount to get newer versions.

**Windows version support?**
Windows 10 and 11, 64-bit.

**Can I move my license to a new PC?**
Yes — deactivate on the old machine (or email support if it's dead) and activate on the new one. 3 simultaneous activations per license.

**Is there a trial of Pro?**
Yes — 30 days, full-featured, no card required. When it ends, the app reverts to Free: nothing is deleted, and snapshots beyond your 3 most recent simply become read-only until you upgrade.

**I bought it and it's not for me.**
Reply to your receipt within 14 days and we'll refund you, no questions asked.

---

## Footer

Download · Pricing · Docs · Changelog · Support · Privacy Policy · Terms · EULA · Refund Policy
Made in India 🇮🇳 · © 2026 [Legal entity name] · PC Snapshot is not affiliated with Microsoft.

---

## Copy style notes (for editing pass)

- Every claim maps to a shipped feature; do not add "AI", "sync", or "cross-platform" language.
- Keep "exactly as you left it" as the emotional spine; keep "no account, no cloud" within one screen of every CTA.
- The word "snapshot" is doing category-creation work — never swap in "backup" (implies files) or "session" (implies browser-only).
