# 04 · UI parity: the build against the concepts

Sun 8 Nov 2026, against `platform` at 0e33f28. Public pages were read on the local node
(127.0.0.1:18050, with the sample and `ops` hosts by Host header). Signed-in pages (console,
onboarding after sign-up, operator) were judged from their templates and handlers. Nothing was
signed in, created or posted.

Decisions already taken are not counted as gaps: PayPal is the only payment rail (facts.md),
chat channels never approve (13 §3), and Telegram is the live chat channel while WhatsApp waits
behind a setting (13 §2). Owner social sign-in is left out by decision (requirements R2.4).

**Status.** Matches: the screen and its main controls are built, small differences only.
Partial: built, but a section, control or state the concept draws is missing. Missing: nothing
built for it.

## 1. Concept screens

57 concept screens: **9 Matches, 43 Partial, 5 Missing.** Three of the five Missing are owner
surfaces (terminal and desktop tray), one is the zavora.ai page, and one is a concept map with
no product page.

| Concept | Built route · template | Status | What differs |
|---|---|---|---|
| `index.html` | `/` · agentrix-home `home.html`; promises and roster on `/how-it-works` · `how.html` | Partial | Different hero; a film and console shots replace the brief, sealed-room and decision collage. The seven promises and the WebAgent roster are only on How it works. Header "Sign in" opens the old `/auth/signin` |
| `assess.html` | `/` check (`home.html`, `check.js`), `/start`, `/assess/{id}` · onboarding `start.html`, `assess.html` | Partial | Paste, live reading, report with evidence, keep and share are built. No "your new website, alive" preview (old and new side by side, match by area, 7-day link), no teach card, no "Meet your WebAgents". The talk box is on the home check only. Share sends the report, not a preview |
| `move.html` | `/console/move` · onboarding `move.html`; `/console/quarantine` · console `quarantine.html` | Partial | Stages, sealed rooms, left behind, match facts, go-live card and undo are built. No old/new page comparison, no Keep/Release beside the evidence (moved to its own page), only a count of refusals, no "Not yet" on the card, no "31 old addresses redirected" |
| `console.html` | `/console/{site}` · shell `os/home.html`, `decisions.html`, `decision.html`, `webagents.html`, `website.html`, `trust.html` | Partial | No Customers app. Cards are Approve or Decline only: no Edit, no "Decline and teach", no "Allow, and tell me". Today has no visitor counts or "Next up". AI spend is shown in USD per day, not KSh per month |
| `site.html` | sample `/` · kilele `home.html`, `layout.html`; agent view at `/agents` · door `agents.html` | Partial | No Human/Agent toggle on the page (the agent view is `/agents` plus JSON). No EN/SW switch. Footer has no Privacy link, `/privacy` is 404, no "Release · signed" line. The cake form moved to `/cakes/studio` and `/order-a-cake` |
| `adk-surface/Main.dc.html` | web shell, Telegram bot, Flutter phone app, Talk (voice) | Partial | Terminal and desktop surfaces are missing. The shared rules hold: Esc never decides; chat and voice only allow once or deny |
| `adk-surface/MainDark.dc.html` | same, Night Time theme | Partial | Same gaps as Main |
| `adk-surface/Channel.dc.html` | agentrix-channels Telegram bot; message chat blocks | Partial | Morning message, Decline and STOP are built. "Allow once" in chat is off by decision; approving opens the console |
| `adk-surface/PhoneDecide.dc.html` | `apps/owner/lib/src/decisions.dart`, `/d/{card}` | Partial | Effect and fingerprint shown. A passkey approval opens the browser instead of signing in the app |
| `adk-surface/PhoneInbox.dc.html` | `apps/owner` `now.dart`, `decisions.dart`; push "A decision is waiting" | Partial | No question cards from WebAgents; no decided list showing where each was decided |
| `adk-surface/Terminal.dc.html` | none | Missing | No terminal UI for owners |
| `adk-surface/TerminalLight.dc.html` | none | Missing | Same |
| `adk-surface/Tray.dc.html` | none | Missing | No desktop tray |
| `adk-surface/Voice.dc.html` | Talk (agentrix-media `voice.js`, `/api/voice/{site}/decide`), `/console/{site}/walk` | Partial | Allow once, deny and stop by voice, with a 3-second cancel. No camera or screen sharing |
| `adk-surface/WebHome.dc.html` | `/console/{site}` · `os/home.html`, dock, lane | Partial | No thread with each WebAgent ("Message Finance"), no "Always allow for this person", no Edit, no "Coming up" |
| `adk-surface/WebToday.dc.html` | "While you were away" · `os/home.html` | Partial | Spend, quiet hours, waiting count and a tagged timeline are built. No action on each line (Retry, Do it, Not now), no "Routines today" |
| `adk-surface/WebTrust.dc.html` | `/console/{site}/memory` · `os/memory.html` | Partial | Every section exists, but memory and standing rules are empty states only: no Confirm, Fix or Forget, no Extend or Stop allowing, no Check again, no device Remove or Add |
| `console/index.html` | `/os` → `/console/{site}` · `os/base.html` | Partial | Overview of nine ideas. Dock, worlds, Night Time, Freeze, signed decisions and gestures are built. The command bar only finds pages; it doesn't plan or hand out jobs. No mission reports, desk whispers or editing by voice |
| `console/shell.html` | `/console/{site}` · `os/base.html`, `os/home.html` | Partial | Dock, worlds, lane, Night Time, Freeze and the approval sheet match. No plan preview with Go, no Proposals in the lane, no "what visitors triggered" card. Activity monitor shows spend only (no woke because, next, Pause). No site switcher |
| `console/mission.html` | `/console/{site}/missions`, `/missions/{id}` · `os/missions.html`, `os/mission.html` | Partial | Say the goal, read the contract, approve; count, pace, pause, end. No weekly report, mission budget, mission cards, "Ask about this mission" or night log (the page says so) |
| `console/live.html` | `/console/{site}/live`, `/live/{thread}`, `/walk` · `os/live.html`, `live-thread.html`, `walk.html` | Partial | Sit in, take over and hand back are built. No desk whispers, no "save this reply as an answer". The walk has no pointer, Editor draft or "Sign and publish" |
| `console/store.html` | `/console/{site}/store` · `os/store.html` | Partial | Listings, can/never, wakes, ladder and budget share; Add raises a card. Packs and Connections are empty. No filter tabs or "why suggested". Budget in %, not KSh |
| `console/trust.html` | `/console/{site}/trust` · `os/trust.html` (+ `/memory`, `/quarantine`) | Partial | Freeze, test-attack score, receipts and left behind are built. Missing on the page: protections summary, standing grants, platform notices, support access list (reachable only from the email), export and erasure, devices |
| `console/phone.html` | `apps/owner` (Now, Decisions, Live, Apps, Pair) and shell phone tabs under 900 px | Partial | Passkey approval leaves the app for the browser. Live shows WebAgent activity, not visitor conversations. Spend in USD. No mission tile on Now |
| `console/gestures.html` | `/console/{site}/gestures` · `os/gestures.html` | Matches | Six held gestures, camera opt-in, hold pad, palm countdown, keys. Only the demo cards differ |
| `control-plane/index.html` | ops host `/` → `/fleet`; scopes in operator `layout.html` | Partial | The operator and portfolio shells exist, with the scope chip. No overview page of the three scopes. Every operator WebAgent shows "not running yet" |
| `control-plane/fleet.html` | `/fleet` · operator `fleet.html` | Partial | Brief written from counts, KPIs, one cell, runner pool, Migration Factory, SLO table, model routes, ledger. Edge availability and Ask latency say "Not measured". One cell, not four. No operator WebAgents running, so no decisions from them |
| `control-plane/rollout.html` | `/rollouts` · `rollouts.html` | Partial | Rings sized from live sites and the gate list. No train has run: no waves, no canary against control, no two-person promotion; every gate but the gauntlet says "not recorded" |
| `control-plane/incident.html` | `/incidents`, `/incidents/{id}` · `incidents.html`, `incident.html` | Partial | Declare, timeline, evidence, drafts, status and postmortem are built. No latency chart (no probes). No SRE WebAgent, so no lever pulled within a grant and no "Undo the cap". Drafts are copied by hand; there is no status page |
| `control-plane/billing.html` | `/money` · `money.html` | Partial | KPIs, reconciliation table, PayPal rail, reminders, MRR by plan and the invoice-run card. Vultr usage isn't imported, so nothing reconciles and no variance is held |
| `control-plane/portfolio.html` | `/portfolio`, `/partners/{id}` · `portfolio.html` | Partial | Brief, clients with role and ring, invite, Moves, people, brand, authority. No bulk import, no client health, no margin. Dock "Sites" and "Rollouts" both go to `#import` |
| `control-plane/cli.html` | `/cli` · `cli.html` | Partial | Sessions from the ledger (none yet), pack rules and the can/never list. No session replay; no pack has shipped |
| `experience/index.html` | platform `/samples` · agentrix-home `samples.html` | Partial | Three samples and films exist. No archetype and tier picker. Kestrel Post became Tana Valley Times; zavora.ai is not built |
| `experience/nav-agents.html` | every sample page · kilele `layout.html`, `macros.html`, themes `desks.js` | Partial | Peeks with live facts, Instant starters, team sheet, ask bar, thread rail and person chip match. No "Remember me on this device" in the thread, no Privacy link, no "Release · signed" line |
| `experience/creator.html` | Kilele `/cakes/studio` · `kilele/studio.html` | Partial | Options, price lines, sketch, calendar, request and no-JS fallback match. "Save and share" only copies a link: no share sheet, no 30-day expiry, no "Play Amina's 7th" |
| `experience/artifact.html` | Classy `/gifts/plan` · `classy/plan.html` | Partial | The creator's plan works (budget bar, swap, add all to bag). No share sheet, invited view, owner view, `/a/{token}` or expiry; the plan lives only in the URL |
| `experience/search.html` | Classy `/search` · `classy/search.html`, `search_body.html` | Matches | Reading chips, refine, Answer/Plain toggle, compare and "What I used" are there |
| `experience/news.html` | Tana `/`, `/stories/{topic}`, `/digest`, `/corrections` · `tana/*.html` | Matches | Front page, catch me up, story so far with its correction, Friday five and house rules. A local weekly instead of a national paper, as planned |
| `experience/presentation.html` | none | Missing | No zavora.ai page, no guided tour, no `/book` |
| `identity/marks.html` | themes `marks.js`, agentrix-marks, `macros.html` `mark()`, console Appearance | Partial | Living marks with states render on the desks. SVG only: no PNG for chat avatars or the A2A icon |
| `journeys/index.html` | none | Missing | A map of the two journeys for the concept set. No product page needs it |
| `journeys/customer-browse.html` | Kilele `/` · `kilele/home.html`, `layout.html` | Matches | No pop-up, the AI chip is shown, visit counting with "Don't count my visits" and GPC |
| `journeys/customer-order.html` | `/bag`, `/checkout` · `shop/bag.html`, `checkout.html`; `/cakes/studio` | Matches | Source chips per line, live slot calendar, pinned rate, held cake request (checkout judged from the template; it needs sign-in) |
| `journeys/customer-pay.html` | `/orders/{id}`, `/orders/{id}/pay` · `shop/order.html`, `pay.html` | Matches | Amount, payee and rate first, then PayPal. The M-Pesa and Card buttons are left out by decision |
| `journeys/customer-after.html` | `/orders/{id}/receipt`, `/change`, `/help` · `shop/receipt.html`, `change.html`, `help.html` | Partial | Receipt, status steps, change with cut-off and refund under the rule match. Sample receipts are unsigned, so no "Verify this receipt"; no photo in "Something's wrong"; no order updates by chat |
| `journeys/customer-agent.html` | `/agents`, `/mcp`, `/.well-known/ai-catalog.json`, `/.well-known/awp` · `door/agents.html` | Partial | Tiers, terms, checkout link and held request match. Web Bot Auth isn't verified (a self-named agent counts as known). No signed A2A card, no `/a2a`, no "what the door sees" panel |
| `journeys/customer-account.html` | `/account/sign-in`, `/create`, `/keeps`, `/recovery-codes`, `/passkey-offer`, `/choices` · identity `visitor/*.html` | Partial | Passkey or email code only: no password, no social sign-in, no authenticator step. The "your bag is waiting" note shows only after a session expires |
| `journeys/customer-security.html` | `/account/security`, `/data`, `/erased` · `visitor/security.html`, `data.html`, `erased.html` | Partial | New sign-in alert, sign out everywhere, fresh sign-in gate, export and erasure match. No "Report a look-alike" on any sample |
| `journeys/owner-discover.html` | `/` check · `home.html`; `/assess/{id}` · onboarding `assess.html` | Partial | No account needed; paste, report and a talk box work. No preview with a match score, no "Want to teach it?", no first WebAgent suggestions, no "Share the preview" |
| `journeys/owner-account.html` | `/account/new` · onboarding `account.html`; `/setup` · `setup.html` | Matches | Passkey first, password plus a required authenticator, recovery codes, the Chief's "Karibu". Adds email-code sign-up. Social sign-in left out by decision |
| `journeys/owner-plan.html` | `/setup/plan` · `plan.html`; `/setup/pay` · `pay.html` | Partial | Plans, AI budget slider, a suggestion, PayPal on its own page. No "what it costs us" estimate. Monthly only, though `/` and `/prices` offer 12 months at 20% off |
| `journeys/owner-domain.html` | `/sites/{id}/claim` · onboarding `claim.html` | Matches | DNS or file tabs, Check now, found or not found in words. No registrar-aware help |
| `journeys/owner-move.html` | `/console/move` · onboarding `move.html` | Matches | Same facts as the concept, drawn as room cards instead of a timed log |
| `journeys/owner-golive.html` | `/console/move` · `move.html`; approval sheet on the decision page | Partial | Card, approve with passkey or code, live state, undo. The card doesn't list what approving does (address record, which WebAgents start, the first charge); no "Not yet"; no receipt line when live; two undo buttons |
| `journeys/owner-day1.html` | Night Time and brief card · `os/home.html`; `messages/owner/morning_brief.txt` | Partial | Night timeline and a brief built from receipts. No "Share: my website did this overnight" |
| `journeys/owner-order.html` | `/samples/{site}/custom`, `/orders/{id}` · sample `owner/custom.html`, `order.html`; `os/decision.html` | Partial | Held order with its exact effect, approve, decline, payments and refunds. No books or payout, no refund-above-the-rule card, and the real console has no Orders page (only the sample back office does) |
| `journeys/owner-billing.html` | `/billing` · onboarding `billing.html` | Partial | AI budget meter, plan list, cancel with passkey or code. No invoices, no plan change, no cancel timeline, no export bundle |

Not screens, so not in the table: `styles.css`, the `.css`, `.js`, `.svg` and `.json` support
files, `adk-surface/README.md` and `experience/img/`.

## 2. Words

Every built screen's visible text was read against voice.md, not only the lint. Most copy
already follows the rules. The faults were: marketing lines on the home page ("autonomous AI
business", "transform", "We don't just remake your website"), internal names on owner screens
("Model Gateway", "deployment", "charter", "containment gauntlet", "Zavora's fixture"),
crypto and Linux terms in owner and visitor help, spec section numbers on operator screens, and
a few robotic or cute lines.

### 2.1 Fixed on this branch

89 lines in 49 files, one changed line each, committed on `w0/ui-parity`. Line numbers are
after the fix. `scripts/copy-lint` is clean on every changed file.

**Platform home, samples and prices** (`crates/agentrix-home`)

| File:line | Was | Now |
|---|---|---|
| `templates/home.html:24` | Turn your WordPress, Shopify or React website into an autonomous AI business with WebAgents… We don't just remake your website. We rebuild it from the ground up… | We move your WordPress, Shopify or React site to Agentrix and give it WebAgents. They answer customers from your own pages, take orders and bookings, and ask you before anything that costs money… |
| `templates/home.html:5`, `:10` | We rebuild your WordPress site on a secure platform hosted on Vultr, even if it has security issues and vulnerabilities. Your new Agentrix Website serves customers who come from… | We move your WordPress site to a secure platform on Vultr and leave any hack behind. The new site answers your customers, and the AI assistants and agents they send. |
| `templates/home.html:43` | See how a bakery transformed its business into an Agentrix Website. | See a bakery's Saturday, before and after its site moved to Agentrix. |
| `templates/home.html:61` | A busy shop, and an idle, passive website. | A busy shop, and a website nobody answers. |
| `templates/home.html:63` | The website is now an autonomous AI business that works for you, handling orders, talking with customers and supporting them. | The site answers customers and takes orders, and asks Wanjiru before anything that needs her. |
| `templates/home.html:70` | Agentrix WebAgent showcase websites | Samples |
| `templates/home.html:88` | See the samples and films for a shop, a paper and a salon | See all three samples, and films about a bakery, a perfume shop, a paper and a salon |
| `templates/yoursite.html:13` | Show me how I can transform my site | What your site becomes with WebAgents |
| `templates/yoursite.html:14` | Your website can become an autonomous AI business… | WebAgents answer your customers, take orders and look after the site. You approve anything that costs money or makes a promise. |
| `templates/prices.html:12` | Priced for Intelligence | Plans and prices |
| `templates/prices.html:13`, `:41` | Lead said "monthly"; the FAQ offered 12 months at 20% off | Both say monthly or for 12 months |
| `data/samples.json:9` | each menu item is a WebAgent: … a cake designed with you | each part of the menu has its own desk: today's bread, a cake you design with it, and orders Wanjiru confirms |
| `templates/how.html:59` | Dials, not blind trust | A dial for each job |
| `templates/how.html:124` | a gVisor room: read-only, not root, with no capabilities | its own gVisor container: read-only, with no admin rights |
| `templates/how.html:125` | The proxy refuses the cloud's metadata address. | …its proxy blocks the cloud's internal address for machine secrets. |
| `templates/how.html:176` | Its challenge commits to the card's fingerprint: the release's digest and the site's charter. | Your passkey signs a fingerprint of exactly this card: this release and your site's rules. |
| `templates/how.html:346` | PayPal through the Effects Gateway. Every payment is an approved effect with a receipt. | PayPal, through one gateway that holds the keys. Every payment is approved first and gets a receipt. |
| `templates/privacy.html:34` | …is on the home page | …is on the How it works page (it was the wrong page) |

**Sample sites** (`crates/agentrix-samples/templates`)

| File:line | Was | Now |
|---|---|---|
| `base.html:21` | The business, people and orders are fictional, and payments run in PayPal's sandbox. | The business and its people are invented, and payments run in PayPal's sandbox, so no real money moves. |
| `classy/layout.html:59` | It never guesses prices, dates or allergens. (a perfume shop) | It never guesses prices, stock or delivery dates. |
| `classy/layout.html:63`, `tana/layout.html:59` | Classy Perfumes's WebAgents; Tana Valley Times's desks | The WebAgents at Classy Perfumes; The desks at Tana Valley Times |
| `kilele/cakes.html:50` | The desks won't promise these, so you won't be surprised later. | The desks won't take an order for these. |
| `kilele/delivery.html:18` | Somewhere else? The site lists only these areas, so the desks won't promise others. | Delivery covers only these areas. Somewhere else? Ask Wanjiru. |
| `kilele/notfound.html:7`, `classy/not_found.html:6` | Nothing lives at /x. | There's no page at /x. |
| `classy/search_body.html:19` | static index, no AI | plain search, no AI |
| `classy/search_body.html:74` | Results re-rank from the catalogue, without a model. | Tap a part to drop it, and the list re-sorts from the catalogue without AI. |
| `classy/search_body.html:75` | No budget, family, size or product found in the words. Results match the words themselves. / Ask something and the parts of your question appear here. | No budget, family, size or product in your words, so the results match the words as typed. / Type a request, such as a budget or a scent family, and its parts show here. |
| `shop/orders.html:13` | For your privacy I've shown only the status. This order was placed with another account. | This page shows only the status, to keep the order private. This order belongs to another account. |
| `shop/checkout.html:54` | Only real times can be picked. | Only open times are listed. |
| `shop/receipt.html:25` | signed by the gateway that took it | signed by the payment service that took it |
| `shop/help.html:21` | The Cashier works behind the scenes. | You don't talk to the Cashier yourself. |
| `tana/macros.html:22` | Double opt-in | We email you a link to confirm first |
| `tana/subscribe.html:37` | Subscribers already: sign in and subscriber stories open. | Already a subscriber? Sign in and subscriber stories open. |

**Sign-up, sign-in, account and billing**

| File:line | Was | Now |
|---|---|---|
| `crates/agentrix-console/templates/auth/signin.html:35` | No magic links and no SMS codes: | We never sign you in from an email link or a text message: |
| `crates/agentrix-identity/templates/visitor/keeps.html:6` | your recovery codes as one-way hashes | your recovery codes, stored so nobody can read them back |
| `crates/agentrix-identity/templates/visitor/security.html:27` | Ends every session, this device included. | Signs you out on every device, this one too. |
| `crates/agentrix-identity/templates/visitor/erased.html:4` | Every store has answered. / A store couldn't finish | Every record below is cleared. / A record marked Open below isn't cleared yet |
| `crates/agentrix-onboarding/templates/onboarding/plan.html:19` | {host} · site in setup | Choose a plan for {host} |
| `crates/agentrix-onboarding/templates/onboarding/billing.html:33` | Counted by the Model Gateway on every call… it's a limit, not a meter. | Counted on every AI call… it's a hard limit. |
| `crates/agentrix-onboarding/templates/onboarding/billing.html:50` | Changing plan from here comes next. | You can't change plan here yet, so your plan stays as it is. |
| `crates/agentrix-onboarding/templates/onboarding/billing.html:75` | The one-file export… comes next. Today every receipt… | A one-file export… isn't ready yet. For now, every receipt… |
| `crates/agentrix-billing/data/plans.json:72` | Agent-to-agent commerce | Orders from customers' AI agents |

**Console: Today** (`crates/agentrix-shell/templates/os`, `crates/agentrix-console/templates/console`)

| File:line | Was | Now |
|---|---|---|
| `os/home.html:79` | Visitors passed these to you from Ask | Visitors asked these, and your pages don't say |
| `os/home.html:91` | planted attacks held against this deployment | test attacks were stopped on your site |
| `os/home.html:94` | The containment gauntlet hasn't run against this deployment yet | The test attacks haven't been run against your site yet |
| `os/home.html:103` | By job (button) | Spend by job |
| `os/home.html:128` | Decisions never expire into a yes | Nothing is approved without you |
| `os/home.html:146` | wants you (chip) | needs your OK |
| `os/home.html:161` | …once presence is switched on for your site | Live conversations will show here too, once that's switched on for your site. |
| `os/base.html:106` | from the Model Gateway's records | from the spending records |
| `os/base.html:114`, `console/today.html:51` | Every call goes through the Model Gateway, which stops at the limit before asking a model. | Every AI call is checked against your limit first, and refused once you reach it. |
| `console/today.html:49` | Model spend | AI spend |

**Console: Decisions and the approval sheet**

| File:line | Was | Now |
|---|---|---|
| `os/decision.html:28`, `console/decision.html:31` | These starting levels are written into your charter and signed with this card. Signing them unchanged is safe… | Approving this card also sets these starting levels… |
| `os/decision.html:42`, `console/decision.html:47` | The card and your approval come from the same arguments that make this fingerprint. Your passkey signs it, or a single-use authenticator code is bound to it, and it runs once. | This fingerprint stands for exactly what will run. The card you read and the approval you give are both made from it, so what you approve is what runs, once. |
| `os/decision.html:57`, `console/decision.html:61`, `os/base.html:131` | Single use, and bound to this exact card (effect). | It works once, and only for this card (approval). |

**Other owner screens and messages**

| File:line | Was | Now |
|---|---|---|
| `crates/agentrix-shell/templates/os/trust.html:22`, `crates/agentrix-console/templates/console/trust.html:20` | Ten attacks planted in Zavora's fixture, run against this deployment… The score is computed from receipts only. | Ten test attacks, hidden in a copy of an infected site we keep for testing, run against your site… The score counts only what the receipts show. |
| `os/trust.html:28`, `console/trust.html:25` | The gauntlet hasn't run against this deployment yet | The test attacks haven't been run against your site yet |
| `crates/agentrix-shell/templates/os/live-thread.html:35` | What you write here can become an answer for everyone… Teach your site (link to `#teach`, which doesn't exist) | What you write here reaches this visitor only. If the desk didn't know, add the answer to your site so it can answer the next person; open questions are listed on Website (link to `#teach-h2`) |
| `crates/agentrix-console/templates/console/webagents.html:12` | Read-only this weekend. | You can't change levels on this page. |
| `crates/agentrix-console/templates/console/receipt.html:26` | checks the Ed25519 signature here, without trusting this page | checks the signature in your browser, so you don't have to take this page's word for it |
| `crates/agentrix-samples/templates/owner/custom.html:4` | No model prices anything. | AI never sets a price. |
| `crates/agentrix-messages/templates/owner/weekly_review.txt:15` | Your WebAgents earned these. Each one is a suggestion; nothing changes until you sign it | Your WebAgents could do more without asking. Each is only a suggestion; nothing changes until you approve it in the console |
| `crates/agentrix-ui/words/en.json:69` | Unfreeze needs your passkey. | Unfreezing needs your passkey or an authenticator code. |

**Operator** (`crates/agentrix-operator/templates`)

| File:line | Was | Now |
|---|---|---|
| `app.html:7`, `:16`, `:19`, `:22`; `rollouts.html:25`; `customers.html:39` | Spec numbers on screen: "(08 §3.5)", "(08 §4.2)", "(08 §2.2, §4.4)", "(08 §2.4)", "Stages follow 08 §4.1" | Removed, or "Each stage comes from what the records show." |
| `incident.html:29`, `trust.html:30`, `cli.html:36`, `money.html:55` | "08 §3.7", "08 §2.4", "08 §6.4", "policy: 08 §4.2" as captions | set by each one's grant; the same for every operator; the same for every coding agent; nothing to chase |
| `fleet.html:49` | …come with the fleet profile (08 §3.2). | …come when the platform runs in more than one region. |
| `fleet.html:55` | They start with the operator tenant (08 §5). | They start when Zavora's operator tenant is set up. |
| `fleet.html:80`, `money.html:9` | targets are hypotheses…; still hypotheses until James sets them | targets are guesses…; drafts until James sets them |
| `signin.html:28` | Continue (button) | Sign in |

### 2.2 Still to fix

Not changed here: each needs Rust, JS or Dart, a test updated with it, a generator script, or
a fact check.

| File:line | As built | Rewrite | Why not here |
|---|---|---|---|
| `crates/agentrix-node/src/pages/map.rs:804-840`; `apps/owner/lib/src/now.dart:58` | $0.04 of your $1.00 daily limit; USD 0.04 | KSh 38 of KSh 3,870 this month (facts.md: a monthly cap in the site's currency) | Rust and Dart |
| `crates/agentrix-shell/src/routes.rs:2405`, `:2424`, `:2494` | Something went wrong on our side. Your site stays frozen; try again. | We couldn't finish that on our side. Your site stays frozen. Try again in a minute. | Rust; banned phrase the lint can't see |
| `crates/agentrix-home/templates/home.html:52` | How an Agentrix Website can transform your business | A bakery's Saturday, before and after | `tests/home.rs:798` checks it |
| `crates/agentrix-home/src/desks.rs:58`; `static/guide.js:247` | Show me how I can transform my site (starter) | What would my site become? | Rust and JS |
| `crates/agentrix-home/static/guide.js:39` | each menu item is a WebAgent | same as `samples.json:9` above | JS |
| `crates/agentrix-home/src/view.rs:579-581`, `:622` | Set checks · AI answers; Set tests | Rules · AI answers; Rules | Rust |
| `crates/agentrix-home/src/view.rs:553`, `:559` | No checks on this server yet. Its self-attack hasn't run on this server yet. | No sites checked yet. The Guardian hasn't attacked this site yet. | Rust; tests check it |
| `crates/agentrix-home/src/view.rs` (stats) | 1 signed receipts issued | 1 signed receipt issued | Rust |
| `crates/agentrix-onboarding/templates/onboarding/start.html:20`, `:36` | Show me (button) | Check my site (as on the home page) | onboarding tests check it |
| `crates/agentrix-onboarding/templates/onboarding/assess.html:109` | Continue with passkey | Keep this site: create your account (the next page also offers a password) | `tests/render.rs:115`, `:240` |
| `crates/agentrix-onboarding/templates/onboarding/plan.html:48` with `static/plan.js:16` | Nothing above it is ever billed. | Nothing above your limit is ever billed. | JS rewrites the line; change both |
| `crates/agentrix-identity/templates/visitor/sign-in.html:3`, `signed-out.html:3` | basket | bag (every sample says Bag) | `tests/visitor.rs:715` |
| `crates/agentrix-shell/templates/os/trust.html:20` | The containment gauntlet (owner heading) | Test attacks | `tests/pages.rs:158` |
| `crates/agentrix-samples/templates/classy/search_body.html:85` | Model calls | AI calls | `tests/desks_agent.rs:399`, `:427`, `:456` |
| `crates/agentrix-samples/src/owner.rs:1347` | PayPal (sandbox) | PayPal test mode | Rust |
| `crates/agentrix-samples/src/owner.rs:1717` | Today's bake, {} · left is the plan less today's orders and holds | Today's bake, {}. Left means what you planned, minus today's orders and holds. | Rust |
| `crates/agentrix-samples/src/news.rs:797` | Double opt-in, one-tap stop. | We email you a link to confirm first. One tap stops it. | Rust |
| `fixtures/samples/tools/tana.py:93` (writes `manifest.json:147`); `apps/customer/lib/src/flavour.dart:71` | Tana Valley Times's published stories | Tana Valley Times' published stories | generator script and Dart |
| `fixtures/samples/tools/kilele.py` (writes `kilele-bakery/manifest.json:170`) | Visit desk: Hours, directions, parking and where Kilele delivers. | I know the hours, directions, parking and where Kilele delivers. (the other desks speak as "I") | generator script |
| `crates/agentrix-messages/templates/owner/{weekly_review,budget_reached,new_device,signin_method_changed,role_changed,plan_started,plan_changed}.txt` | Buttons to `{console_url}/review`, `/webagents/budget`, `/trust/devices`, `/trust/people`, `/billing`, where `console_url` is `{origin}/os` | Point at routes that exist: `/console/{site}/…`, `/billing` | dead links; needs the site id in `src/identity.rs:134` |
| `crates/agentrix-messages/templates/owner/decision_waiting.txt:2`, `decision_expiring.txt:2` | push link `/console/decisions` | `/console/{site}/decisions` | lands on the old the first release list; needs the site id |
| `crates/agentrix-home/templates/how.html:245-253` | Phone mock and tab list of the old five-tab console | The console's current apps | needs a fact check |
| `crates/agentrix-onboarding/templates/onboarding/move.html:28` | {host} on WordPress | depends on fix 9 below | the home page promises Shopify and React |

## 3. Fix first

The ten gaps that most hurt a real owner or customer.

1. **No preview before sign-up.** Owners create an account without seeing their rebuilt site.
   `crates/agentrix-onboarding/templates/onboarding/assess.html`, with a preview route in
   `crates/agentrix-node/src/assess/mod.rs`.
2. **Two consoles.** The Move, receipts and decision push links open the first release pages with the old
   five-tab nav. `crates/agentrix-shell/src/routes.rs:529`, `crates/agentrix-shell/src/control.rs:780`,
   `crates/agentrix-console/templates/console/base.html`.
3. **Owner emails link to pages that don't exist** (`/os/webagents/budget`, `/os/trust/devices`,
   `/os/review`, `/os/billing`). `crates/agentrix-messages/src/identity.rs:134` and
   `crates/agentrix-messages/templates/owner/*.txt`.
4. **AI spend shown as a USD daily limit.** facts.md says a monthly cap in the site's
   currency. `crates/agentrix-node/src/pages/map.rs:804-840`, `apps/owner/lib/src/now.dart:58`.
5. **Home "Sign in" opens the old `/auth/signin`,** without email code or recovery; every other
   page uses `/account/signin`. `crates/agentrix-home/templates/base.html:38`.
6. **The go-live card doesn't say what approving does** (address record, which WebAgents start,
   the first charge) and has no "Not yet". `crates/agentrix-onboarding/templates/onboarding/move.html:163-181`,
   `crates/agentrix-shell/templates/os/decision.html`.
7. **Cards are Approve or Decline only.** No Edit, no decline reason that becomes a rule, no
   "Always allow for this person". `crates/agentrix-shell/templates/os/decision.html`,
   `crates/agentrix-shell/templates/os/macros.html`, `crates/agentrix-decisions/src/lib.rs`.
8. **No privacy page or look-alike report on the samples.** `/privacy` is 404 on all three and
   no footer links one. `crates/agentrix-samples/src/common.rs` and the footers in
   `crates/agentrix-samples/templates/{kilele,classy,tana}/layout.html`.
9. **Promises the build can't keep.** The home page offers Shopify and React moves; the Move takes
   a WordPress export only. `/` and `/prices` offer 12 months at 20% off; `/setup/plan` is monthly
   only. `crates/agentrix-home/templates/home.html:24`,
   `crates/agentrix-onboarding/templates/onboarding/plan.html`, `crates/agentrix-onboarding/src/billing.rs`.
10. **Real owners have no Orders or Customers pages;** only the sample back office has them. A
    new app in `crates/agentrix-shell`, modelled on
    `crates/agentrix-samples/templates/owner/{orders,order,customers}.html`.

Next after these: "Teach your site" lists questions with no Answer button
(`crates/agentrix-shell/templates/os/website.html`); gift plans and cake designs can't be shared
with permissions (`crates/agentrix-samples/templates/classy/plan.html`, `kilele/studio.html`);
billing has no invoices (`crates/agentrix-onboarding/templates/onboarding/billing.html`).
