# The Real Timeline — 6 Months at 8 Hours/Week

*Written 2026-07-16 after revising the original plan against actual constraints. This document supersedes the day-by-day schedule in `04-21-day-launch-plan.md` (which assumed a finished product and full-time hours); the task lists, checklists, and channel playbooks in 04/06 remain the reference for **how** to do each item — this document re-sequences **when**.*

## Revised operating assumptions (from founder)

- Product is still in debugging; **feature-complete/stable target: July 26, 2026**. Work before that date is product work and outside this plan's hour budget.
- **8 hours/week** available for six months after July 26 (≈ one focused day per week; launch week is the single exception).
- Independent developer, **no business registration up front** — sell as an individual through a merchant of record (Paddle accepts individuals/sole traders; Gumroad is the fallback if Paddle onboarding stalls). Payouts are personal income; GST registration and CA/lawyer engagement are deferred until revenue justifies them.
- **Month one is a public beta on GitHub.** The repo stays public under its existing PolyForm Noncommercial license. Two rules from this decision:
  1. Call it **"source-available," never "open source"** — PolyForm is not OSI-approved and the HN/Reddit crowd will derail any thread over the mislabel.
  2. State in the README **now** that a paid Pro tier is coming and the free core stays free — pre-announcing costs nothing; discovering it later reads as a rug-pull (the Fences lesson).
- Beta goal is **feedback, not downloads**: 20–30 engaged testers beat thousands of silent installs. The one-shot channels (Show HN, Product Hunt) are *not* spent on the beta — they're saved for the paid launch.
- GitHub is a destination, not a channel: users arrive via Reddit/Discord/DMs and land on the repo. LinkedIn is skipped entirely (wrong audience psychology for a $29 dev utility).

## Money at risk (worst case: zero installs)

| Committed when | Item | Cost |
|---|---|---|
| Week 3 | Domain + email (Zoho free) | ~$12 (₹1,050)/yr |
| Week 4 — only if beta shows signal | SSL.com OV code-signing cert | ~$200–250 (₹17,400–21,750)/yr |
| Week 9 | Chrome Web Store dev fee | $5 (₹435), one-time |
| — | Everything else (hosting, licensing, email, support, analytics) | $0 (free tiers) |

**Absolute floor if the beta shows no signal and you stop: ~$12. Full commitment: ~$215–270 (₹19,000–23,500)/yr.** The MoR takes 5% + $0.50 (Paddle) or 10% + $0.50 (Gumroad) per sale — zero sales, zero fees. The real stake is the ~200 hours of the six months.

---

## Phase 0 — Finish the product (now → Sun Jul 26)

Debug to stable. The only launch-prep items worth stealing time for before the 26th: none. Ship the product first.

## Phase 1 — Beta month (Weeks 1–4: Jul 27 → Aug 23)

**Week 1 (Jul 27–Aug 2) — make the repo receptive.**
The highest-leverage 8 hours of the whole plan: record the 15-second capture→restore demo and put the **GIF at the top of the README** (the GIF *is* the pitch); build a v0.9.0 installer (Tauri NSIS) that also registers the companion's native-messaging host — if testers can't get exact-tab restore running in 5 minutes, the beta never tests the differentiator; write `BETA.md` (what works, what doesn't, how to report); add an issue template with three questions (what broke / what confused you / would you pay $29); enable GitHub Discussions; add the "Pro tier coming, free core stays free" paragraph.
**Deliverable: public v0.9.0 release with installer + demo GIF.**

**Week 2 (Aug 3–9) — hand-recruit testers.**
Direct outreach, not broadcasting: personal network; 2–3 Discords/Slacks where you already have standing (show-your-project channels, as a member); r/SideProject and r/coolgithubprojects (self-promo is the point of those subs); ten personal DMs to developers you respect ("I built this — would you break it for me?").
**Deliverable: 20–30 real installs, feedback arriving.**

**Week 3 (Aug 10–16) — fix visibly + buy the domain.**
Triage everything; ship v0.9.1 fast (responsiveness converts testers into advocates); answer every issue; log objections verbatim (they become launch-page FAQ). Register the domain, set up `hello@`/`support@`.
**Deliverable: v0.9.1 released; domain + email live.**

**Week 4 (Aug 17–23) — the wider beta post + the go/no-go gate.**
One honest beta-framed post to r/windows or r/software ("I built a thing that snapshots your whole desktop — it's beta, tell me what breaks"). Then the money gate: **if strangers are installing and the feedback says the problem resonates, order the SSL.com OV cert now** (validation takes 3–10 business days; every subsequent beta download builds SmartScreen reputation for free before the paid launch). If the signal is genuinely dead, stop and reassess having spent ~$12.
**Deliverable: 100–300 cumulative installs; cert ordered; go decision made.**

## Phase 2 — Commercialization (Weeks 5–10: Aug 24 → Oct 4)

One workstream per week; the beta keeps running in the background (~1h/week of replies).

- **Week 5 (Aug 24–30) — landing page.** Static site (Astro or plain HTML) on Cloudflare Pages using `05-landing-page-copy.md`; hero GIF; MailerLite email-capture form; Cloudflare Web Analytics. **Deliverable: site live at the domain.**
- **Week 6 (Aug 31–Sep 6) — the tier boundary.** Implement Free (3 most recent snapshots, older read-only, never deleted) + the 30-day keyless Pro trial per `03 §3/§5`. The one product-code week. **Deliverable: tiers working in a beta build.**
- **Week 7 (Sep 7–13) — money plumbing.** Paddle account + product (fallback: Gumroad with built-in keys, live in an afternoon at 10%); Keygen free tier; webhook → key → email → activation, end-to-end in sandbox including a refund. **Deliverable: a test purchase delivers a working license.**
- **Week 8 (Sep 14–20) — signed release + updater.** Signing in the release pipeline; Tauri updater against GitHub Releases; ship v1.0.0-rc to the beta cohort and verify one real auto-update in the wild. **Deliverable: signed RC updating itself on tester machines.**
- **Week 9 (Sep 21–27) — trust surface.** Collect testimonials from the best beta testers (ask directly for one quotable sentence + "would you have paid?"); minimal docs (getting started, per-browser companion setup, known limitations, license FAQ); honest privacy policy + EULA/terms/refund from templates (professional review deferred — flagged as accepted risk until revenue); winget manifest PR; Chrome/Edge store submissions for the companion. **Deliverable: site has testimonials, docs, legal pages; store reviews in flight.**
- **Week 10 (Sep 28–Oct 4) — launch prep + freeze.** Product Hunt listing scheduled; Show HN draft (technical, candid about limitations); 3 subreddit-tailored drafts; launch email; full clean-VM pass (install → onboard → capture → restore → buy → activate → update); freeze. **Deliverable: everything staged for launch day.**

## Phase 3 — Launch (Weeks 11–12: Oct 5 → Oct 18)

**Wednesday, October 7, 2026 — the paid launch.** The one day that needs more than the weekly budget — take the day off work. PH live 12:01 AM PT (12:31 PM IST) with founder comment; Show HN in the US-morning window with 4–6 hours of honest thread presence; Reddit posts staggered per each sub's rules; email the beta list + newsletter (launch-week $24, then $29 — dated and real, never extended). Answer everything; change nothing but showstoppers; log every objection.

**Week 12 (Oct 12–18) — convert the aftermath.** v1.0.1 for launch papercuts within 72h; retrospective (traffic sources, download→trial→purchase, top 5 objections verbatim → FAQ/copy edits); personal thank-you + check-in email to every buyer.

## Phase 4 — Growth rhythm (Weeks 13–26: Oct 19 → Jan 24, 2027)

The standing 8-hour week: **~2h support/community** (inbox, issues, Discussions) + **~3h shipping** + **~3h marketing**, with the marketing slot rotating on a four-week cycle: (1) one SEO/comparison page from real support questions, (2) 10 personal creator/newsletter pitches, (3) one technical write-up (first: "How PC Snapshot restores exact browser tabs from outside the browser" → HN), (4) monthly changelog newsletter.

Shipping sequence (from `07`, resized to 8h/wk): **Nov** — Fresh Session (spec exists, strong demo material) as a mini-launch; **Dec** — Firefox companion begins (unlocks the privacy-conscious segment; realistically spans several weeks); **Jan** — v1.1 bundling the quarter's work + a metrics review against the checkpoints below.

## Earnings checkpoints (estimates to calibrate against, not promises)

- **Launch month (Oct):** 30–100 sales ≈ **$900–2,900 gross (₹78k–2.5L)** if HN/PH/Reddit land moderately; a front-page outcome can multiply this, a quiet one can halve it.
- **Nov–Dec:** the post-spike trough — **$150–500/mo (₹13k–43k)** riding SEO, word of mouth, and the Fresh Session mini-launch.
- **End of month 6 (late Jan 2027):** the sustainable-side-income test — **$200–800/mo (₹17k–70k)** with >50% of downloads from search/word-of-mouth. Hitting the top of that band consistently is the trigger to engage the CA, register GST, and consider raising hours. Missing the bottom means the funnel has a specific broken stage (find it via the buyer/non-buyer interviews, not via more channels).
- Fixed costs ~$25/mo are covered by the first sale each month; everything past sale two is profit.

## What was consciously deferred from the original plan (and why it's safe)

- **CA + lawyer engagement** → deferred to first-revenue (MoR handles global tax as seller of record; personal income tax is a filing-time matter). Accepted risk: template legal pages until then.
- **GST/LUT registration** → deferred until revenue justifies paperwork.
- **Microsoft Store, Steam, affiliates, Discord server, Ultra tier** → all unchanged from `03`/`07`: post-launch experiments gated on evidence.
- **A second Show HN/PH shot for the beta** → deliberately not taken; those channels get one good impression and it's spent on the paid launch.
