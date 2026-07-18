# 21-Day Launch Plan — 2026-07-16 → 2026-08-05

> **⚠️ Superseded (2026-07-16):** this schedule assumed a finished product and full-time hours. The operative plan is [08-solo-6-month-timeline.md](08-solo-6-month-timeline.md) — product stabilizes by Jul 26, a GitHub beta month follows, and the paid launch is **Oct 7, 2026** at 8 hours/week. Use this document only as the reference for *how* to execute each task (the day entries below remain the detailed task specs); use 08 for *when*.

*Day 1 = Thursday 2026-07-16. Public launch = Day 21, Wednesday 2026-08-05 (Tue/Wed launches perform best on Product Hunt/HN; avoids weekends). The product itself is assumed finished — this plan covers commercialization only. Effort assumes one full-time founder; total is deliberately ≤6-7 focused hours/day with slack for the unexpected.*

**Three clock-driven items start on Day 1 because they have external lead times:**
1. **Code-signing certificate (SSL.com OV)** — identity validation takes 3-10 business days.
2. **SmartScreen reputation** — builds only from real signed downloads over 2-8 weeks; the soft launch (Day 11) exists largely to start this clock before the public launch.
3. **Paddle account approval** — website with legal pages must exist for domain verification; typically several business days.

---

## Week 1 — Foundations (Days 1-7)

### Day 1 (Thu Jul 16) — Business + identity groundwork
- **Objective:** start every long-lead-time process.
- **Tasks:** choose/confirm final brand name (see trademark note in 03 §16); register domain (Cloudflare Registrar); order SSL.com OV code-signing certificate and begin identity validation; set up professional email on the domain (Zoho Mail free / Google Workspace ₹160/user/mo); open separate business bank account (start the paperwork); book a chartered accountant consult for GST/LUT/export-income treatment.
- **Deliverables:** domain live, cert order submitted, `hello@` + `support@` addresses working.
- **Dependencies:** none. | **Effort:** 4-5h | **Priority: CRITICAL (longest lead times in the plan).**

### Day 2 (Fri Jul 17) — Legal pages, first draft
- **Objective:** legal skeleton good enough for Paddle verification and honest enough to ship.
- **Tasks:** draft privacy policy (easy to write honestly: "we collect nothing; here is exactly what the app stores locally and what the opt-in crash dialog sends"), EULA, terms of sale, refund policy (14-day unconditional, verbatim from 03 §6); send drafts to lawyer for the one-time review (03 §16); create Paddle account and start verification.
- **Deliverables:** four legal pages drafted; Paddle onboarding in progress.
- **Dependencies:** domain (Day 1). | **Effort:** 4h | **Priority: CRITICAL.**

### Day 3 (Sat Jul 18) — Website build I
- **Objective:** the static site skeleton, deployed.
- **Tasks:** scaffold Astro site on Cloudflare Pages; implement Home, Pricing, Download, Legal pages using the copy in `05-landing-page-copy.md`; enable Cloudflare Web Analytics; commit to a public-facing GitHub org/repo for issues + roadmap.
- **Deliverables:** site live at the real domain (unindexed/noindex until Day 10).
- **Dependencies:** Days 1-2. | **Effort:** 6h | **Priority: HIGH.**

### Day 4 (Sun Jul 19) — Screenshots + demo capture day
- **Objective:** every visual asset the site and stores need, from one staged session.
- **Tasks:** stage a beautiful multi-monitor dev workspace; record the 15-second hero loop (capture → clean desktop → one-click restore, tabs visibly reconciling); record a 90-second full demo (voiceover optional); capture stills: grid UI, restore report, companion in action, settings/ignore list; export at web + Product Hunt + winget/Store dimensions.
- **Deliverables:** asset library (`design/launch-assets/`); hero video embedded on the site.
- **Dependencies:** none (product is done). | **Effort:** 5h | **Priority: HIGH — every channel below reuses these.**

### Day 5 (Mon Jul 20) — Release engineering
- **Objective:** a reproducible signed-release pipeline.
- **Tasks:** finalize `tauri.conf.json` bundle settings for NSIS; wire Tauri's built-in updater to a GitHub Releases manifest; set up a GitHub Actions release workflow (build → sign → attach installer + `latest.json` + SHA-256); integrate the opt-in Sentry crash-report dialog; decide the installer's companion-extension step (bundle the native-host registration the repo already scripts).
- **Deliverables:** CI produces an unsigned installer end-to-end (signing slots in when the cert arrives).
- **Dependencies:** none. | **Effort:** 6h | **Priority: CRITICAL.**

### Day 6 (Tue Jul 21) — Licensing + tiers
- **Objective:** Free/Pro boundary and license activation working.
- **Tasks:** implement the Free tier limit (3 most recent snapshots; older become read-only, never deleted) and the 30-day keyless Pro trial exactly as specified in 03 §3/§5; integrate Keygen.sh activation (3 devices, offline grace); wire Paddle webhook → Keygen license issuance → Resend license-delivery email.
- **Deliverables:** end-to-end test purchase in Paddle sandbox delivers a working key.
- **Dependencies:** Paddle account (Day 2). | **Effort:** 7h | **Priority: CRITICAL.**
- *Note: this is the one place the plan touches product code; it's commercialization plumbing, not feature work.*

### Day 7 (Wed Jul 22) — Docs site
- **Objective:** documentation that pre-empts the support queue.
- **Tasks:** VitePress docs: getting started, companion setup per browser (Chrome/Edge/Brave/Opera/Opera GX, with screenshots), ignore list, restore reports explained, known limitations (Firefox, elevated windows, in-app state — honest, per product philosophy), snapshot JSON format reference, license/activation FAQ; link from site nav.
- **Deliverables:** docs live; "known limitations" page published.
- **Dependencies:** Day 3. | **Effort:** 5h | **Priority: HIGH.**

## Week 2 — Integration, beta, and content (Days 8-14)

### Day 8 (Thu Jul 23) — Sign + assemble the real release
- **Objective:** the actual v1.0 installer, signed if the cert has arrived.
- **Tasks:** sign the installer (or chase SSL.com validation); full clean-machine install test (fresh Windows 11 VM + a Windows 10 VM): install → first-run onboarding → companion registration → capture → restore → update check → uninstall; fix installer papercuts only.
- **Deliverables:** signed v1.0.0 installer + checksums on GitHub Releases (private/draft).
- **Dependencies:** Days 1, 5. | **Effort:** 5h | **Priority: CRITICAL.**

### Day 9 (Fri Jul 24) — Private beta wave
- **Objective:** 15-30 real outside users before any public eyes.
- **Tasks:** recruit from personal network, X, and 2-3 Discords you already belong to; give beta testers the signed build + a feedback form (3 questions: what broke, what confused, would you pay $29); state the testimonial ask up front ("if it earns it, a sentence I can quote"); set up Senja to collect them.
- **Deliverables:** beta cohort installed and using it; feedback channel live.
- **Dependencies:** Day 8. | **Effort:** 4h | **Priority: CRITICAL — testimonials and SmartScreen downloads both start here.**

### Day 10 (Sat Jul 25) — Payments end-to-end + site polish
- **Objective:** money can actually flow.
- **Tasks:** Paddle live-mode approval checks; real $1 test purchase → key → activation → refund round-trip; remove site noindex; final copy edit of landing page against `05` (claims audit: every sentence maps to a shipped feature); pricing page shows the "never stops working" clause verbatim.
- **Deliverables:** live checkout; site publicly indexable.
- **Dependencies:** Days 2, 3, 6. | **Effort:** 4h | **Priority: CRITICAL.**

### Day 11 (Sun Jul 26) — Soft launch (quiet availability)
- **Objective:** start the SmartScreen clock and the download counter with zero fanfare.
- **Tasks:** publish the GitHub release; submit the free tier to **winget** (manifest PR) and to Chocolatey (optional); publish the companion extension to the Chrome Web Store + Edge Add-ons (review takes days — submit now; document the developer-account fees: Chrome $5 one-time, Edge free); soft-post in 1-2 small friendly communities only.
- **Deliverables:** anyone can download; winget + extension-store reviews in flight.
- **Dependencies:** Days 8, 10. | **Effort:** 4h | **Priority: CRITICAL.**

### Day 12 (Mon Jul 27) — Beta feedback triage
- **Objective:** convert beta findings into fixes and copy changes.
- **Tasks:** triage feedback; fix only installer/onboarding/activation issues (feature requests → public roadmap "Next/Someday"); update docs and FAQ with every confused-user question verbatim; collect the first 3-5 testimonials into the site.
- **Deliverables:** v1.0.1 if needed; testimonial section populated.
- **Dependencies:** Day 9. | **Effort:** 6h | **Priority: HIGH.**

### Day 13 (Tue Jul 28) — Launch content writing
- **Objective:** every launch post written and reviewed cold, days before it's needed.
- **Tasks:** write the Product Hunt listing (tagline, gallery, first comment telling the founder story: the problem, why local-first, why Windows); the Hacker News Show HN post (technical, honest, mentions Rust/Tauri, links the honest-restore-report philosophy); 3 tailored Reddit posts (r/windows, r/software + r/SideProject — value-first, not ad-copy); an X/Twitter thread with the hero video; the launch newsletter email.
- **Deliverables:** all posts drafted in `docs/launch/posts/` (per-channel rules reviewed — some subreddits ban self-promo; identify the compliant framing for each).
- **Dependencies:** Day 4 assets. | **Effort:** 6h | **Priority: HIGH.**

### Day 14 (Wed Jul 29) — Support + ops rehearsal
- **Objective:** the machine that runs launch week.
- **Tasks:** Zoho Desk connected to `support@` with saved replies (activation, refund, companion setup, known limitations); MailerLite list + signup form on site footer; Resend transactional templates (license, trial-started, trial-expiry); a one-page ops runbook: where to check sales, crashes, mentions; refund SOP.
- **Deliverables:** support stack live; runbook written.
- **Dependencies:** Day 10. | **Effort:** 4h | **Priority: MEDIUM.**

## Week 3 — Ramp and launch (Days 15-21)

### Day 15 (Thu Jul 30) — Product Hunt setup + hunter warm-up
- **Objective:** PH launch scheduled properly.
- **Tasks:** create the PH product page (don't go live); schedule for Day 21 12:01 AM PT; line up 10-15 genuine acquaintances who'll be awake at launch to try it and comment (never solicit fake upvotes — PH penalizes it); finalize gallery with Day 4 assets.
- **Deliverables:** PH listing scheduled.
- **Dependencies:** Days 4, 13. | **Effort:** 3h | **Priority: HIGH.**

### Day 16 (Fri Jul 31) — Second beta wave / stress test
- **Objective:** scale confidence: more machines, more browsers, more monitors.
- **Tasks:** push the beta build to a wider circle (aim 50+ total installs — SmartScreen fuel); explicitly test the matrix: Win10/Win11 × 1-3 monitors × each supported browser; verify updater by shipping a trivial v1.0.x update to the cohort.
- **Deliverables:** matrix test log; one successful in-the-wild auto-update.
- **Dependencies:** Day 11. | **Effort:** 5h | **Priority: HIGH.**

### Day 17 (Sat Aug 1) — SEO + comparison content
- **Objective:** the pages that earn search traffic for years.
- **Tasks:** publish 3 docs/comparison pages targeting real queries: "restore all windows and apps after restart (Windows 11)", "PC Snapshot vs DisplayFusion window position profiles" (respectful, scope-based — per 05 style note), "save and restore browser tabs with your whole desktop"; submit sitemap to Google Search Console.
- **Deliverables:** 3 indexed pages; Search Console verified.
- **Dependencies:** Day 7. | **Effort:** 4h | **Priority: MEDIUM.**

### Day 18 (Sun Aug 2) — Buffer day
- **Objective:** absorb slippage — something above has slipped; this is its slot.
- **Tasks:** whatever is red; else: rest before launch week, final clean-VM install pass.
- **Priority: reserved.**

### Day 19 (Mon Aug 3) — Newsletter + influencer seeding
- **Objective:** give the launch a running start.
- **Tasks:** email the beta list + newsletter signups: "launching Wednesday, here's the launch-week price ($24 for 7 days, then $29 — stated on the site too, no fake urgency)"; send 10-15 personal (not template) pitches to Windows/dev YouTubers and newsletter writers who cover this space, offering a free license and the 90-second demo; no expectations — 1-2 responses is success.
- **Deliverables:** pre-launch email sent; pitch tracker.
- **Dependencies:** Days 13, 14. | **Effort:** 4h | **Priority: MEDIUM.**

### Day 20 (Tue Aug 4) — Freeze + final checks
- **Objective:** nothing changes after today.
- **Tasks:** code/content freeze; run the full pre-launch checklist (`06-launch-checklists.md`); verify checkout, activation, refund, updater, download links, docs links on a clean machine + phone; confirm PH schedule; sleep.
- **Deliverables:** checklist signed off.
- **Dependencies:** everything. | **Effort:** 3h | **Priority: CRITICAL.**

### Day 21 (Wed Aug 5) — LAUNCH
- **Objective:** be everywhere you planned to be, respond to everything, sell honestly.
- **Timeline (IST):** PH goes live 12:31 PM IST (12:01 AM PT) — post the founder comment immediately; ~5:30-6:30 PM IST post Show HN (morning US-East) and be present in the thread for 4-6 hours answering technically and honestly (HN rewards candor about limitations — lead with the honest-restore-report philosophy); stagger the Reddit posts through the day per each subreddit's rules; X thread when PH momentum is visible; launch email to the list.
- **Tasks:** respond to every single comment/question all day; fix nothing live except true showstoppers; log every objection verbatim (it's tomorrow's FAQ/copy input).
- **Deliverables:** launched. | **Effort:** the whole day | **Priority: CRITICAL.**

---

## Where professional advice is explicitly recommended
- **Chartered accountant (Day 1 booking):** GST registration, LUT for zero-rated export, Paddle payout/FIRC treatment, advance tax, incorporation timing.
- **Lawyer (Day 2, one-time):** EULA/privacy/terms review; trademark registrability of the brand name.
- Budget: ₹35,000-90,000 (~$400-1,035) total for both at launch quality.

## Launch-budget summary (one-time + first month)
| Item | USD | INR |
|---|---|---|
| Domain | $10-15 | ₹870-1,300 |
| Code-signing cert (year 1) | $200-250 | ₹17,400-21,750 |
| CA + lawyer | $400-1,035 | ₹35,000-90,000 |
| Chrome Web Store dev fee | $5 | ₹435 |
| Everything else (free tiers) | $0 | ₹0 |
| **Total** | **~$615-1,305** | **~₹53,700-113,500** |
