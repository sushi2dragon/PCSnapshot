# Commercial Strategy — Decisions and Rationale

*Every section below is a decision, not a menu. Rejected alternatives are named with reasons. Assumes: solo founder in India, no employees, no funding, limited marketing budget. Grounded in `01-product-brief.md` (product reality) and `02-market-research.md` (evidence). Currency: USD with INR at an assumed ₹87/USD.*

---

## 1. Target customer

**Decision: launch to Windows developers and technical freelancers first; expand to creators second.**

Why: developers are the only segment for whom *every* differentiator fires at once — terminal CWD restore, VS Code workspace restore, dev-server detection, exact tab restore, multi-monitor. They congregate in reachable free channels (HN, Reddit, X, dev newsletters), they pay for tools (DisplayFusion's and BetterTouchTool's buyer base), and they evangelize local-first software (Obsidian, LocalSend evidence). Freelancers/consultants add the sharpest money story: "client-switch time is unbilled time."

Rejected: *general productivity consumers* (need paid acquisition we don't have; can't appreciate terminal/IDE features), *teams/enterprise v1* (no license administration, SSO, or deployment tooling exists in the product — selling there would be dishonest).

## 2. Positioning and category

**Decision: create the small category "workspace snapshot tool" — "save your desktop like a file."**

Positioning statement: *For Windows power users who juggle multiple projects, PC Snapshot captures your entire desktop — apps, windows, tabs, terminals — as a visual snapshot and brings it back in one click. Unlike window managers that only remember positions or tab managers that only save the browser, it restores your whole working context, 100% locally, with no account and no cloud.*

Why: research found the direct set thin and unpolished (SmartWindows: weak trust; DisplayFusion: one buried feature). Fighting inside "window manager" invites comparison on 100 features we don't have; owning "snapshot" makes every competitor look partial. The word "snapshot" is already the product's UI language.

Rejected: *"session manager"* (browser-extension connotations), *"window manager"* (DisplayFusion/PowerToys own it and win on breadth), *"backup tool"* (implies files/data, invites wrong expectations).

## 3. Product tiers

**Decision: two tiers at launch — Free and Pro. The Ultra tier from the long-term vision waits until profile-merging ships.**

- **Free (forever):** full-fidelity capture, browse, restore — apps, layout, monitors, exact tabs, terminals — limited to the **3 most recent snapshots**. Free is the complete "pick up where you left off" product.
- **Pro:** unlimited snapshot library, named workspace profiles, recapture, ignore-list management, advanced restore controls, priority support.

Why: this matches the repo's own stated plan (free capture/restore core, paid workflow layer) and the research pattern "paid tier adds capability for a heavier user, doesn't cripple the core" (Rectangle, PureRef, Obsidian). The 3-snapshot cap is chosen so the *resume* job is genuinely free while the *workspace library* job — the one freelancers and multi-project developers get paid value from — is the purchase. One legible line, no feature matrix confusion (Raycast lesson).

Rejected: *fully free core with paid adjacent services* (Obsidian's model needs a sync/publish service we don't operate), *trial-only with no free tier* (kills the word-of-mouth engine that this zero-budget plan depends on), *three tiers at launch* (nothing real to put in Ultra yet; shipping an empty tier invites the Fences trust failure).

## 4. Pricing

**Decision: Pro at $29 one-time, including 1 year of updates. Regional (PPP) pricing on, ~₹1,499 in India. Launch-week price $24.**

Why $29: the validated band for serious Windows/Mac utilities is $20-35 (DisplayFusion $34, Fences ~$20-30, Workspaces $19.99, BTT $24 lifetime). $29 sits at the premium-but-impulse edge; under $10 (Rectangle Pro) caps revenue and signals "toy." Under Paddle's 5%+$0.50, a $29 sale nets ~$26.10 (~₹2,270).

Why one-time + 1-year updates (the "Sublime/JetBrains-perpetual" hybrid): pure lifetime-forever constrained DisplayFusion's monetization; subscriptions are actively resented in this category for a local tool with no server costs (SmartWindows charges $39.99/yr and has thin adoption). "Own it, get a year of updates, keep what you licensed forever, renew at 40% off ($17) if you want newer versions" is honest, sustainable, and marketable as anti-subscription.

License terms: per-person, 3 device activations (covers desktop + laptop), offline-tolerant activation via Keygen with a grace period.

Rejected: *subscription* (category resentment; no recurring cost to justify), *lifetime-all-future-versions* (DisplayFusion's trap), *$9.99* (Rectangle's ceiling), *$49+* (needs social proof we won't have on day one).

## 5. Trial model

**Decision: 30-day full-featured Pro trial, no card required, keyless (starts on first Pro-feature use). After expiry the app reverts to Free — nothing is lost, snapshots beyond 3 become read-only until upgrade.**

Why: 30-45-day no-card trials are the category norm among winners (BTT 45d; Fences/Workspaces 30d) because workflow tools need real workdays to prove value. Reverting to Free instead of locking the app preserves goodwill and keeps the word-of-mouth engine running. "Read-only, never deleted" is the trust-preserving detail.

Rejected: *14-day trial* (too short for a habit-formation product), *card-required trial* (kills top-of-funnel for an unknown brand).

## 6. Refunds and guarantees

**Decision: 14-day unconditional money-back guarantee, self-serve by replying to the receipt. Refund anyone who asks even slightly late.**

Why: with a 30-day free trial ahead of purchase, refund volume will be low; an unconditional policy removes the last purchase objection and generates its own word of mouth. Paddle processes refunds as MoR. A single public refund dispute costs more than fifty quiet refunds.

## 7. Distribution: direct-first

**Decision: sell direct (website + Paddle checkout) as the only paid channel at launch. Submit the free tier to `winget` in launch week. Microsoft Store and Steam are quarter-two experiments, not launch channels.**

Why direct: full margin (5% vs Store fees), full customer relationship (emails for updates/upsell), full pricing control, and Paddle solves the India-seller problem. Why winget immediately: it's free, it's how the target segment installs software, and every `winget install pc-snapshot` is an SmartScreen-reputation-building download. Why defer the Store: certification overhead and a second billing integration for an audience (developers) that doesn't shop there; revisit once direct sales are stable because the Store *does* remove SmartScreen friction for the later consumer segment. Steam worked for DisplayFusion/Fences but suits products with broad consumer appeal — revisit at the creator-segment expansion.

Rejected as launch channels: *Microsoft Store first* (wrong audience order), *AppSumo lifetime deals* (SmartWindows did this; it trains a deal-hunting audience and poisons pricing integrity).

## 8. Website structure

**Decision: a fast static site (Astro on Cloudflare Pages) with exactly these pages:**

1. **Home** — hero with 15-second capture→restore video, problem, 3-step how-it-works, differentiator features, privacy section, comparison table, pricing, FAQ (full copy in `05-landing-page-copy.md`).
2. **Pricing** — Free vs Pro table, trial explanation, refund promise, regional-pricing note, license terms in plain English.
3. **Download** — one button, signed installer, SHA-256 checksum, "what happens on first run", winget command for devs.
4. **Docs** (VitePress subsite) — getting started, companion-extension setup per browser, ignore list, restore reports explained, troubleshooting, snapshot JSON format (publishing the format is a trust move consistent with local-first).
5. **Changelog** — every release, honestly, including fixes. DisplayFusion's docs-as-SEO lesson: each feature doc page targets a search query ("restore window layout windows 11", "reopen all tabs and apps after restart").
6. **Legal** — privacy policy, EULA, terms, refund policy.

Why: this is the minimum set that makes an unknown $29 product feel trustworthy. No blog at launch — content marketing starts post-launch (see 07) when there's bandwidth.

## 9. Privacy messaging

**Decision: make local-first the second-loudest message on every surface (after the core value prop), with the five-point trust block: local files, no account, no telemetry, auditable JSON, no-network companion.**

Why: Obsidian and LocalSend prove this pillar builds evangelist communities; the codebase genuinely earns it (no telemetry, headless companion with no network permission). Constraint this imposes: **crash reporting and any diagnostics must be opt-in dialogs, never silent** — the Sentry integration ships as a "send this crash report?" prompt, and website analytics stay cookieless (Cloudflare). If we ever add an opt-in diagnostics toggle it ships off-by-default with plain-language disclosure. Breaking this promise even once forfeits the positioning.

## 10. Onboarding

**Decision: the first-run experience is a guided first snapshot, not a tour.** On first launch: one screen ("Let's capture your desktop as it is right now"), user clicks the real Take Snapshot button, sees the real thumbnail appear, is invited to restore it after moving a window. Then one non-blocking prompt offers companion-extension setup per detected browser (skippable; everything else works without it). Pro trial is mentioned only when the user hits the 4th snapshot.

Why: the product's magic is only believable when it happens to *your* desktop; a demo video inside the app is weaker than 20 seconds of real usage. The companion is the highest-friction step, so it's offered exactly when its value is visible (browser detected in the snapshot) — never as a gate.

## 11. Support model

**Decision: Zoho Desk free tier behind `support@` on the product domain; public GitHub issues for the free/companion side; 48-hour weekday response target stated on the site; a public "known limitations" doc.**

Why: solo-sustainable, and the public-issues channel doubles as the community trust surface (Rectangle/BTT pattern). The honest-restore-report product philosophy extends to support: publishing known limitations pre-empts the angriest ticket class.

## 12. Analytics

**Decision: measure the funnel with website analytics (Cloudflare, cookieless), Paddle sales data, and Keygen activation counts. No in-app product analytics at launch.**

Why: the three external sources answer the launch-phase questions (visits → downloads → trials → purchases) without touching the no-telemetry promise. What we give up — feature-usage data — we replace with a running "talk to 5 users a week" habit and opt-in crash reports.

## 13. Update policy

**Decision: ship updates via Tauri's built-in updater from GitHub Releases; free minor/patch updates for all; license-entitled updates for 1 year from purchase, then optional 40%-off renewal; the app never nags more than once per version and keeps working forever if never renewed.**

Why: matches the pricing promise exactly; the Fences lesson makes the *never stops working* clause non-negotiable and worth stating on the pricing page verbatim.

## 14. Upgrade strategy (Free → Pro)

**Decision: exactly three upgrade surfaces, all contextual:** (1) hitting the 4th snapshot ("your oldest snapshot will roll off — keep them all with Pro"), (2) the recapture/profile features shown but labeled Pro in the settings menu, (3) trial-expiry summary showing what the user did in 30 days ("you restored 23 times this month"). No modal ambushes, no timers, no dark patterns.

Why: dark-pattern upsells would contradict the honesty positioning that everything else depends on; the three surfaces fire precisely when the user is experiencing the limit.

## 15. Roadmap philosophy

**Decision: public roadmap (GitHub Discussions or a simple roadmap page) with three lanes — Now / Next / Someday — and honest status on the two known gaps (Firefox companion, Fresh Session). Ship visibly every 2-4 weeks post-launch.**

Why: community-as-distribution (the dominant pattern in Part C of the research) needs a surface; a public roadmap is the cheapest one. Steady visible shipping is the solo-founder trust substitute for a brand (PureRef/BTT/Rectangle tenure effect).

## 16. Legal and business essentials (India)

**Decision:** operate initially as a sole proprietorship with Paddle as merchant of record; register under GST and obtain an LUT for zero-rated export of services; separate business bank account from day one.

**Get professional advice — this document is not legal or tax counsel:**
- A **chartered accountant** for: GST registration & LUT filing, treatment of Paddle payouts as export income (FIRC/e-FIRA documentation for RBI purposes), advance-tax planning, and whether/when to incorporate (Pvt Ltd or LLP) — typically ₹15,000-40,000/yr (~$170-460).
- A **lawyer (one-time)** to review the EULA, privacy policy, and terms — templates from Paddle and standard generators are the starting draft, but a one-time review (~₹20,000-50,000 / $230-575) is cheap insurance for a product that reads window titles and URLs from a user's machine.
- Trademark search + application for "PC Snapshot" (or the final brand name) in India and, revenue-permitting, US/EU — the name is generic-adjacent, so get a professional read on registrability; consider a more distinctive brand before launch if counsel advises it.

---

## Consistency check (summary of locked variables used across all documents)

| Variable | Value |
|---|---|
| Price | $29 one-time (₹1,499 India PPP); launch-week $24 |
| Updates | 1 year included; 40%-off renewal; app never stops working |
| Trial | 30 days, full Pro, no card; reverts to Free (read-only beyond 3) |
| Free tier | Full fidelity, 3 most recent snapshots, forever |
| Refunds | 14 days, unconditional |
| Activations | 3 devices per person |
| Payments | Paddle (MoR) |
| Licensing | Keygen.sh |
| Signing | SSL.com OV certificate |
| Distribution | Direct + winget at launch; Store/Steam deferred |
| Telemetry | None; opt-in crash dialog only |
| Target | Windows developers & technical freelancers first |
