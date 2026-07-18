# PC Snapshot — UI Redesign Brief

> Handoff brief for a UI designer (Claude Design / Stitch). Describes the product,
> its features, and the functional requirements the new UI must support — **not**
> visual layouts. The design exploration is intentionally open-ended: produce
> multiple distinct directions.

---

## 1. What the product is

**PC Snapshot** is a local Windows desktop app that saves and restores your entire
working desktop state. It captures which apps are open, where each window sits,
which monitor it's on, and the context each app is working on (code workspace,
browser tabs, terminal working directory). It stores that as a named **snapshot**
with a screenshot preview, and can later **restore** the whole arrangement — relaunching
apps and moving their windows back into place.

Think **"save states for your workspace."** You're deep in one project, you save it,
you wipe to a clean desktop or switch to another task, and later you restore the
exact setup in one click.

**Principles that shape the product:**
- **Local-first & private** — no cloud, no accounts, no sync. Everything lives on the machine.
- **Three verbs only** — the whole product is *capture, browse, restore*. Everything else is in service of those.
- **Honest reporting** — restores are often partial (an app won't relaunch, a window won't reposition). The app always tells the truth about what worked and what didn't; it never fakes success.
- **Never destructive by surprise** — closing/wiping the desktop and clean-restores always offer a "save current first" safety step.

---

## 2. Core capabilities (features the UI must expose)

- **Capture / Take Snapshot** — save the current desktop as a named snapshot (windows, positions, processes, context, screenshot thumbnail).
- **Browse snapshots** — see all saved snapshots at a glance. Each has a preview image, a name, a timestamp, and a signal when the capture was partial (warnings).
- **Restore** — relaunch and reposition everything in a snapshot. Two modes: *additive* (leave current apps alone) and *clean restore* (close everything not in the snapshot). Always offers "save the current desktop first."
- **Recapture** — update an existing snapshot in place with the current desktop (keeps its name/slot).
- **Delete** a snapshot, and **Clear All**.
- **Start New Session** *(planned)* — close all non-essential apps to a clean desktop and drop the app to the system tray.
- **Rich context capture** — terminal sessions (shell + working directory + history) and browser sessions (windows, tabs, pinned/muted state, tab groups) via a companion browser extension.
- **Import** snapshots from a folder.
- **Configuration** — an ignore list (apps to exclude from capture), a terminal-capture toggle, help/keyboard shortcuts.
- **Roadmap** *(inform the design's ambition, need not be built now)* — workspace profiles, merging snapshots together, and tiered plans (Basic / Pro / Ultra).

---

## 3. Functional UI requirements (must be supported — visual treatment is open)

These are the specific behaviors requested. Described as capabilities, not layouts.

1. **Snapshot browser.** A way to view all saved snapshots, each showing its preview + identity, and revealing its actions.
2. **Per-snapshot quick actions on hover.** Surface **Restore** and **Recapture** directly on a snapshot when hovered.
3. **Per-snapshot overflow menu (three-dots).** Opens the full action set for that snapshot — e.g. rename, delete, duplicate, export, view details.
4. **Prominent global actions.** Top-level verbs presented as primary controls: **Capture**, **Restore**, **Close All / Start New Session**, and similar.
5. **Terminal-style output console.** A running, terminal-styled feed that streams the result of every operation — capture succeeded, restore completed / partial with warnings, app failed to launch, window repositioned, snapshot deleted. This is the home for the product's "honest reporting" — one truthful log instead of scattered popups.
6. **Full settings page with side-panel navigation.** The settings button opens a dedicated settings surface (not a small dropdown), with a left side-panel of categories. Suggested categories: *General · Ignore List · Capture (terminal & browser) · Storage / Import-Export · Plans & Account · About / Help.*

---

## 4. Screens & states to cover

- **First run / empty** — no snapshots yet; invites the first capture.
- **Main / snapshot browser** — the browser plus the output console.
- **Capture naming** — name a snapshot before saving.
- **Restore confirmation** — with the "close others (clean restore)" and "save current desktop first" choices.
- **Restore report** — outcome detail (launched / failed / repositioned / closed); may live inside the console.
- **Recapture confirmation.**
- **Ignore-list editor.**
- **Settings page** (side-panel nav).

---

## 5. Constraints for any direction

- It's a **native Windows desktop app** (Tauri), not a website — it should feel like a desktop tool, sized for a window, not a landing page.
- **No login walls or cloud iconography** — it's local and private (tiers are a future concern, not a gate).
- The **three core verbs stay reachable at all times.**
- **Screenshot thumbnails are each snapshot's primary identity** — the design should treat them as hero content.
- Must **scale gracefully** from zero snapshots to many.
- **Partial outcomes are shown, never hidden** — the console/report is core, not decoration.

---

## 6. Seed directions to explore (optional starting points — diverge freely)

The goal is **several distinct designs**, not one. A few moods to riff on:

- **A — Mission control.** Dense, dark, technical. The output console is a first-class panel; snapshots read like a status board.
- **B — Calm gallery.** Snapshots as a spacious visual gallery; chrome is minimal; the console is collapsible/secondary.
- **C — Dev tool / IDE.** Panels and sidebars, keyboard-driven, the console docked like an integrated terminal.
- **D — Card deck.** Snapshots as tactile cards you flip through; big, friendly primary buttons; approachable rather than technical.

Explore beyond these. The only fixed points are Sections 3–5.

---

## 7. Open questions for the designer to answer with their mockups

- Snapshot browser as a **grid vs. list vs. carousel** — and card density.
- Where the **console docks** (bottom bar, side panel, collapsible drawer) and whether it's always visible.
- Whether primary verbs live in a **top bar, a persistent action rail, or a floating cluster.**
- How **hover actions** coexist with the **three-dots menu** without clutter.
- How the **partial-restore report** relates to the console (inline vs. dedicated view).
