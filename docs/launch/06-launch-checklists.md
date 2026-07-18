# Launch Checklists, Channel Playbook, and Sales Playbook

*Companion to `04-21-day-launch-plan.md`. Check every box before Day 21. The channel playbook deliberately excludes low-ROI channels — see "Channels we are consciously skipping."*

---

## 1. Commercial setup checklist

- [ ] Domain registered (Cloudflare Registrar); `www` + apex both resolve, HTTPS enforced
- [ ] `hello@` and `support@` mailboxes live and tested
- [ ] Business bank account opened; Paddle payout method verified
- [ ] CA engaged: GST registration filed, LUT for export obtained/pending, payout treatment confirmed
- [ ] Lawyer review of EULA / privacy policy / terms / refund policy returned and applied
- [ ] Paddle live mode approved; product + launch-week coupon ($24) configured; test purchase **and refund** completed with a real card
- [ ] Keygen.sh: policy set to 3 activations, offline grace period tested; deactivation flow tested
- [ ] Paddle webhook → license issuance → Resend delivery email: end-to-end in production
- [ ] SSL.com OV cert issued; signing integrated into CI; signed installer verified (`signtool verify`, and Properties → Digital Signatures on a clean VM)
- [ ] GitHub Actions release pipeline: tag → build → sign → GitHub Release with installer + SHA-256 + updater `latest.json`
- [ ] Tauri updater: an in-the-wild update applied successfully on a beta machine
- [ ] Sentry crash dialog: opt-in only, fires on a test crash, no silent sends (verify with a network monitor — this guards the no-telemetry claim)
- [ ] winget manifest merged (or PR open); `winget install` tested
- [ ] Chrome Web Store + Edge Add-ons companion listings approved (or review in flight with sideload instructions documented as fallback)
- [ ] Zoho Desk connected to `support@`; saved replies loaded (activation, refund, companion setup, limitations)
- [ ] MailerLite list + site signup form working; Resend transactional templates (license delivery, trial started, trial expiring) tested

## 2. Website & content checklist

- [ ] Home, Pricing, Download, Docs, Changelog, Legal pages live; no dead links (run a crawler)
- [ ] Every landing-page claim audited against shipped features (per `05` style note — no "AI", no "sync", no over-promise on in-app state)
- [ ] Pricing page states verbatim: price, 1-year updates, "the app never stops working," 3 activations, regional pricing, 14-day refund, 30-day trial
- [ ] Download page: signed installer link, SHA-256 checksum, file size, Windows 10/11 64-bit requirement, winget one-liner
- [ ] Hero video (15s) + full demo (90s) embedded and playing on mobile
- [ ] Docs: getting started, per-browser companion setup, ignore list, restore reports, **known limitations**, snapshot JSON format, license FAQ
- [ ] 3 SEO/comparison pages published; sitemap submitted to Search Console
- [ ] Testimonials (3-5 from beta, via Senja) live with names/roles used with permission
- [ ] Cloudflare Web Analytics receiving events; goals identified (download click, checkout click)
- [ ] favicon, OG images, and social cards render correctly (test with a link-preview tool)
- [ ] 404 page exists and points to Download/Docs

## 3. Product/release QA checklist (clean machines, not the dev box)

- [ ] Fresh Windows 11 VM: install → guided first snapshot → restore → uninstall leaves no orphans
- [ ] Fresh Windows 10 VM: same pass
- [ ] Multi-monitor: capture on 3 monitors, restore after unplugging one (degrades gracefully, restore report says so)
- [ ] Each browser (Chrome, Edge, Brave, Opera, Opera GX): companion registers, exact-tab restore verified
- [ ] Free-tier boundary: 4th snapshot triggers the roll-off message; old snapshots read-only, not deleted
- [ ] Trial: starts keyless, full Pro; expiry reverts to Free without data loss
- [ ] Activation: license activates, deactivates, re-activates on a second machine; offline grace works with network disabled
- [ ] Updater: previous version updates to current on a beta machine
- [ ] SmartScreen status on a never-seen VM documented honestly (expect "unrecognized" early — support macro ready explaining it)

## 4. Launch-day checklist (Day 21)

- [ ] Product Hunt live at 12:01 AM PT; founder comment posted within 5 minutes
- [ ] Show HN posted in the US-morning window; stay in-thread 4-6 hours
- [ ] Reddit posts staggered per subreddit rules (each checked for self-promo policy)
- [ ] X thread posted with hero video; launch email sent to list
- [ ] Support inbox and PH/HN/Reddit threads checked hourly; every question answered same-day
- [ ] Sales, activations, crash reports, and site analytics checked at midday and end-of-day
- [ ] Objections logged verbatim into a running doc (feeds FAQ and copy edits on Day 22)
- [ ] No live code changes except showstoppers; the freeze holds

---

## 5. Marketing channel playbook (prioritized by expected ROI)

### Tier 1 — the launch bets

**Hacker News (Show HN)** — *the single best audience-product fit.*
- **Why:** Windows developers who value local-first, Rust, and honest engineering — the exact buyer. LocalSend/Obsidian-style products historically over-perform here.
- **How:** "Show HN: PC Snapshot – snapshot and restore your whole Windows workspace (Rust/Tauri)". Lead with the technical story: Win32 enumeration, the native-messaging companion, the honest restore report. Answer every comment; concede limitations plainly (Firefox, in-app state) — HN rewards candor and punishes marketing-speak.
- **Expected outcome:** if it lands front-page: thousands of visits, hundreds of downloads, a durable backlink; if not, ~nothing. High variance, zero cost.
- **Effort:** 2h writing + a full day of presence.
- **Mistakes to avoid:** posting ad copy; arguing with critics; ignoring the thread after posting; resubmitting repeatedly (once, plus one retry weeks later, is the tolerated norm).

**Product Hunt** — *social proof and a permanent listing.*
- **Why:** productivity-tool audience, badge/listing that keeps converting for months, and press/newsletter writers scout it.
- **How:** scheduled listing (Day 15), strong gallery, founder-story first comment, genuine acquaintances trying it early. Launch-week $24 price noted transparently.
- **Expected outcome:** realistic solo-founder result is a top-10 day and a few hundred visits; occasionally much more. The listing's long-tail matters more than launch-day rank.
- **Effort:** 4h setup + launch-day presence.
- **Mistakes to avoid:** buying upvotes (detected and penalized); launching Friday-Sunday; treating rank as the goal instead of the listing.

**Reddit** — *the highest-intent Windows audience anywhere.*
- **Why:** r/windows, r/software, r/SideProject, r/productivity (+ r/webdev / r/programming only via the technical write-up). "I built a thing that fixes X" posts from actual builders do well; ads don't.
- **How:** per-subreddit tailored posts, value/story-first with honest limitations, founder flair where required; respond to every comment for 48h. Check each subreddit's self-promotion rules first (some require 9:1 participation ratios).
- **Expected outcome:** one good post = hundreds-to-thousands of visits and the most brutal, useful feedback you'll get.
- **Effort:** 4h + 2 days of replies.
- **Mistakes to avoid:** cross-posting identical text; posting to rule-banning subs (instant removal + domain shadowban risk); getting defensive in comments.

### Tier 2 — the compounding channels (start launch week, pay off over quarters)

**SEO / comparison content** — **Why:** "restore windows after restart", "save desktop layout" queries have durable intent and weak incumbent content (DisplayFusion's docs win by default — beatable). **How:** the 3 launch pages, then 2/month from real support questions. **Expected:** negligible for 3 months, then a steady compounding trickle that becomes the top channel by month 6-12 (the DisplayFusion pattern). **Effort:** 4h/page. **Avoid:** thin AI-sounding listicles; write from real product knowledge.

**X/Twitter build-in-public** — **Why:** the indie-dev and dev-tools audience lives there; the hero video is inherently shareable. **How:** launch thread, then 2-3 posts/week: shipping updates, restore-report screenshots, honest metrics. **Expected:** slow follower compounding; occasional viral demo clip; inbound from newsletter writers. **Effort:** 2h/week. **Avoid:** engagement-bait; posting only promos.

**YouTube creators + newsletters (outbound)** — **Why:** one mid-size Windows-tips video or dev-newsletter mention outsells a week of social posting; borrowed trust. **How:** 10-15 *personal* pitches (Day 19) with a free license + the 90s demo; repeat monthly with fresh angles. **Expected:** 1-2 placements per 15 pitches is success; each worth hundreds of qualified visits. **Effort:** 3h/batch. **Avoid:** template blasts; paying for coverage without disclosure; expecting replies from mega-channels.

### Tier 3 — deliberate slow burns

**Newsletter (own list)** — start collecting at launch (footer form + trial signups); monthly changelog-plus-story email. It's the only owned channel; every future launch (Firefox support, profiles, v2) lands on it first. Effort: 1h/month.
**Discord/communities** — join 3-5 dev/productivity communities as a *member*; share only where invited. A dedicated PC Snapshot Discord waits until there are ~500+ users to avoid the empty-room effect; GitHub Discussions is the interim home (roadmap + support surface, Rectangle/BTT pattern).

### Channels we are consciously skipping (and why)

- **LinkedIn:** wrong buyer psychology for a $29 personal utility; revisit if a team/enterprise tier ships.
- **Paid ads (Google/Meta/Reddit):** $29 one-time price cannot sustain CAC at cold-traffic conversion rates; no budget for the learning phase. Revisit only with retargeting on proven organic traffic.
- **Affiliates:** per research, Paddle lacks native affiliates and third-party fit is unverified; low leverage before there's organic volume. Revisit ~month 4 (see 07).
- **Instagram/TikTok:** demo could work as short-form eventually, but the buyer intent isn't there for a Windows dev tool; opportunistic only.
- **Press releases/PR agencies:** no ROI at this scale; personal pitches beat wire services.

---

## 6. Sales playbook

**Pricing psychology.** $29 sits under the "ask my manager/spouse" threshold but above toy pricing — it signals a maintained product. Anchor it against time: the pricing page's one number to remember is *"rebuilding your workspace twice a day costs you ~80 hours a year; PC Snapshot costs one hour of freelance billing, once."* One-time pricing is itself the differentiator against SmartWindows' $39.99/yr — say "no subscription" out loud everywhere. The launch-week $24 is a real, dated, one-time discount — never fake-extend it (Fences checkout-price lesson: advertised price must equal checkout price, always).

**Conversion funnel.** Visitor → download (Free) → first successful restore (the activation moment — onboarding exists to force this in the first 5 minutes) → habit (3+ snapshots) → hits the Free boundary or discovers profiles → 30-day trial → purchase. Measure each stage with the privacy-safe stack (site analytics, download count, Paddle, Keygen); accept blindness in the middle stages rather than adding telemetry — user interviews fill the gap.

**Trial conversion.** The trial-expiry email (day 27 of trial) is the single highest-leverage sales asset: it should recount the user's own facts — how long they've had it, what they told us at signup — and restate the guarantee. In-app, the expiry screen shows the read-only library ("your snapshots are safe; unlock them any time") with one button. No countdown timers in the app beyond a single "7 days left" notice.

**Upgrade prompts.** Exactly the three contextual surfaces from 03 §14 — 4th-snapshot roll-off, Pro-labeled features in settings, trial expiry. Nothing else. Every prompt has a visible "not now" that is respected permanently for that session.

**Support workflow.** All channels funnel to Zoho Desk. Triage twice daily (morning/evening IST). SLA: first response <48h weekdays, stated publicly. Every resolved ticket that revealed confusion becomes a docs edit the same week — the support queue is the docs backlog. Bugs go to GitHub issues (public) with honest status.

**Refund workflow.** One saved reply: apologize, refund via Paddle immediately, ask (optionally) one question — "what did you expect it to do?" — and log the answer. Refund even slightly-late requests. Target: refund processed <24h. Never argue; a $29 dispute is never worth a public thread.

**Reviews and testimonials.** Ask at the moments of demonstrated success: after the 10th restore (in-app, once, dismissible) and in the post-purchase email ("a sentence about what it saved you"). Collect via Senja; publish with permission, name, and role. After launch, politely ask happy PH commenters to leave their comment as a PH review. Never incentivize reviews with licenses — it violates most platforms' rules and the product's honesty positioning.

**Customer success (solo edition).** Five user conversations a week, standing. A "first-week check-in" email to every purchaser (plain text, from the founder, reply-able). The question that matters: "what did you do right before you opened PC Snapshot?" — it reveals the real job-to-be-done and feeds the roadmap.
