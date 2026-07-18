# Long-Term Growth Plan — 30 Days to Year One

*Continues from launch day (2026-08-05). Philosophy: a sustainable solo software business compounds through product trust, owned channels, and honest pricing — not through launch spikes. Every phase's plan assumes the previous phase's targets were only partially met; nothing here depends on a viral launch.*

---

## First 30 days after launch (Aug 5 → Sep 4)

**Theme: convert launch attention into a stable baseline; fix the funnel with real data.**

- **Week 1:** answer everything, everywhere; ship v1.0.x for launch-surfaced papercuts within 72h (visible responsiveness is marketing); send the launch-week-price-ends email honestly (price then returns to $29 and stays there); write the day-after retrospective — traffic sources, download→trial→purchase ratios, top 5 objections verbatim.
- **Weeks 2-3:** the objection sweep — turn the top objections into: FAQ edits, one landing-page copy revision, and the next 2 SEO pages. Post the technical write-up ("How PC Snapshot restores exact browser tabs from outside the browser — native messaging on Windows") to the blog/docs and HN; technical write-ups are this product's best repeatable content format.
- **Week 4:** first monthly changelog newsletter; first roadmap update with launch feedback incorporated; interviews with 5 purchasers and 5 free-tier users who didn't convert.
- **Support scaling check:** if tickets >10/day, the docs have failed somewhere — fix docs before considering tooling.
- **Targets (guesses to calibrate against, not promises):** 1,500-5,000 downloads, 40-150 Pro sales, support <5 tickets/day steady-state, SmartScreen warnings gone (reputation established).

## First 90 days (→ Nov 3)

**Theme: ship the two roadmap items that unlock the next audiences; establish the content engine.**

- **Product (in priority order):**
  1. **Firefox companion** (spec exists in `features/pending/browser_companion_restore.md`) — unlocks the privacy-conscious segment most aligned with the local-first message; announce as a mini-launch (HN, r/firefox, newsletter).
  2. **Fresh Session** (spec: `features/pending/fresh_session.md`) — completes the capture→clean-slate→restore loop; strong demo material.
  3. **Workspace profiles v1** (named profiles, pinning — the Pro-tier deepener from the product vision).
- **Marketing:** 2 SEO pages/month from support questions; monthly technical write-up; second wave of 15 creator pitches (now with real user numbers); PH "shoutouts"/updates for Firefox release.
- **Sales:** first trial-conversion analysis — if trial→paid <5%, the problem is almost always the *activation moment* (first restore) not the price; instrument via interviews, not telemetry.
- **Ops:** month-2 accounting rhythm with the CA (GST filings, advance tax); refund-rate check (>5% means a messaging-reality mismatch — fix copy, not policy).
- **Targets:** 8,000-20,000 cumulative downloads, 150-500 cumulative sales, 3 SEO pages ranking top-10 for a real query, newsletter ≥500.

## First 6 months (→ Feb 2027)

**Theme: the second engine — either the community surface or the second channel, chosen by evidence.**

- **Product:** profile merging (the flagship differentiator from the product vision — open multiple profiles together); evaluate **Ultra tier** timing: introduce only when merge + advanced profile management are genuinely worth a second paid tier, priced as an upgrade (~$19) for existing Pro owners with explicit grandfathering (the Fences anti-pattern is the checklist here).
- **Distribution experiments (pick by month-4 data, don't do all):**
  - **Microsoft Store** if consumer-segment (creator) interest is showing up in support/interviews — the Store removes install friction for non-developers.
  - **Steam** if the creator/enthusiast segment dominates instead (DisplayFusion/Fences precedent).
  - **Affiliates** (PromoteKit or manual codes) only if creators are already converting organically — affiliate programs amplify working channels, they don't create them.
- **Community:** open a Discord (or promote GitHub Discussions) once ≥500 engaged users exist; publish the snapshot JSON format as a stable, documented interface — the cheapest possible "ecosystem surface" (Raycast/Obsidian lesson scaled to solo reality): power users scripting their own snapshots become the plugin authors.
- **Pricing evolution:** hold $29. The correct month-6 lever is *tier structure* (Ultra), not price increases. If regional-pricing abuse via VPN appears, cap the discount rather than removing PPP.
- **Targets:** 1,000+ cumulative sales, one non-launch channel producing ≥30% of new downloads, support sustainable at <1h/day.

## First year (→ Aug 2027)

**Theme: durability — the version-2 moment and the first expansion decision.**

- **Roadmap philosophy holds:** ship visibly every 2-4 weeks; public roadmap; the honest-restore-report ethos extends to every announcement (say what doesn't work).
- **v2.0 decision point (~month 10-12):** bundle the year's accumulated depth (profiles, merge, Fresh Session, Firefox, polish) as PC Snapshot 2. Existing license holders within their update year get it free; lapsed holders get the 40%-off renewal ($17). Grandfathering terms published *before* the release, on the pricing page (the single most important trust rule from the research).
- **Expansion candidates, ranked:**
  1. **Team/small-studio licensing** (5-pack ~$99) — zero product work, pure packaging; agencies and studios already buy DisplayFusion site licenses this way.
  2. **Deeper app integrations** (JetBrains IDEs, more terminals, Office depth) — widens the moat in the existing market; driven by support-request frequency.
  3. **macOS port** — the biggest prize (Workspaces by Apptorium shows demand and weak incumbents) but a massive solo undertaking with a whole new OS API surface; only consider with 12+ months of Windows profitability banked, or explicitly park it.
  4. **Cloud sync** — *permanently out* unless done E2E-encrypted and optional; it contradicts the positioning that everything else is built on. Local export/import of snapshot bundles (e.g. sharable profile files) delivers most of the value with none of the betrayal.
- **Business metrics that matter (in order):** refund rate (<3%), trial→paid conversion (target 8-12% by year-end), support hours/week (<5), monthly sales trend excluding launch spikes, newsletter growth, % of sales from SEO+word-of-mouth (target >60% — proves the compounding engines work). Revenue targets are deliberately absent: at $29 one-time with these costs (~$25-40/mo fixed), the business is profitable from roughly the 3rd sale each month; the metric that decides everything is *sustainable weekly sales volume*.
- **Retention in a one-time-purchase business** means: update-year renewals (track renewal rate from month 13), upgrade-tier attach rate, and evangelism (each happy user's recommendation is the recurring revenue). The newsletter and changelog are the retention products.
- **Partnerships worth exploring in year one:** dev-tool newsletter sponsorship swaps (free license giveaways), bundling conversations (Setapp has no Windows equivalent at scale — watch this space), and hardware-adjacent communities (multi-monitor/ultrawide subreddits and Discords where the pain is acute).

## Support scaling path

Solo inbox (launch) → docs-first deflection (month 1) → saved-reply library + public issues (month 3) → if >2h/day by month 6, hire a part-time contractor for tier-1 triage (~$300-500/mo, ₹26,000-43,500) before it eats shipping time — support cost is the real COGS of this business.

## The two standing rules

1. **Never break the local-first promise** — no silent telemetry, no required accounts, no cloud dependency, ever. It's the moat that costs nothing to maintain and everything to rebuild.
2. **Never change the deal on existing customers** — every pricing/tier/entitlement change ships with explicit, pre-announced grandfathering. The research's clearest finding is that this single mistake (Fences) undoes years of goodwill in one forum thread.
