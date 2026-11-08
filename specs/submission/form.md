# The lablab form, field by field

Vultr Agent Rush, Challenge 1: Blast Radius Zero. Closes **Mon 9 Nov 2026, 02:00 EAT**.
Owner: W14 (the submission session). James pastes and submits.

Copy each field from between its markers. The character counts come from
`python3 specs/submission/count.py` and are checked against lablab's limits. `TODO(live)` marks a value
that waits on the live run on Vultr routes. Never submit one unfilled, and never fill one by
hand without the receipt behind it.

## Title (50 characters at most)

[field: title]
Agentrix: Upgrade your website into WebAgents
[end of field]

It's the home page's own headline. Alternatives, if James wants the containment in the title:
- `Agentrix: Move a hacked site, leave the hack behind` is 51 characters, one too many.
- `Agentrix: move hacked sites, leave the hack behind` is 50.

## Short description (50 to 255 characters)

[field: short]
Agentrix turns a WordPress, Shopify or React website into an autonomous AI business: WebAgents that work 24/7 for the owner, built and checked in secure sandboxes on Vultr, and live only on the owner's passkey.
[end of field]

James picked this one on Sun 8 Nov; it's his README's wording.

The earlier option, not used: "Agentrix moves a small business's WordPress site, hacked or not, into a new site run by AI WebAgents. Every untrusted step runs in a sealed room on Vultr, and nothing goes live until the owner approves with a passkey."

Tonight's live Move is of a WordPress site. No Shopify or React site has been moved live yet, so
judges who try one may not see the same result.

## Long description (600 to 2,000 characters)

[field: long]
About two in five websites run on WordPress. Many small-business sites just wait: questions go unanswered at night, plugins go unpatched, and some are quietly hacked. Moving one the usual way copies the hack across.

Agentrix is a web agent that does the Move. A control VM on Vultr plans it as a graph of jobs and sends each one, as a signed spec, to a sealed room on a separate Vultr sandbox VM. A real Firefox, driven by Playwright, reads the old site. The old WordPress is replayed from its own database with no route out. A Vultr model writes the new pages; each is rendered in a room with no network and checked against the old page by a Vultr vision model. Nothing goes live until the owner approves a go-live card with a passkey that signs that exact card.

Containment: every room is a gVisor container with a read-only root, no capabilities, caps on CPU, memory, processes and time, and an egress proxy that allows only the job's hosts and refuses Vultr's metadata address. The sandbox VM holds no secrets. Each room is destroyed after its job, and a signed receipt says so. Ten attacks planted in Zavora's infected test site: TODO(live) n/10 contained, scored from receipts.

On Vultr: Cloud Compute VMs for control, a VX1 sandbox and the test site, on a VPC with firewall groups; DNS; Object Storage for every receipt; and Serverless Inference for every model call, through one gateway that holds the only key.

After the Move, the site's WebAgents work 24/7 for the owner: they answer visitors with citations, take orders from customers' AI agents through an agent door, and ask the owner before anything that matters. Moving is free; plans start at $29 a month, paid with PayPal. Built in Rust on adk-rust. MIT licensed.
[end of field]

## Mode, challenge, categories and technologies

| Field | Value |
|---|---|
| Mode | Online |
| Track | **C1-Blast Radius Zero** (lablab's own label for Challenge 1) |
| Categories (pick from lablab's list) | **Security**, **Web Application**, **Cloud Application**, **Business**, **Customer Support** |
| Technologies (pick from lablab's list, then type the rest if it allows) | **VULTR**, **Qwen3**. Typed: Vultr Serverless Inference · Vultr Cloud Compute · Vultr Object Storage · Docker · gVisor · Playwright · Rust · adk-rust · WebAuthn passkeys · Ed25519 · MCP · PayPal |
| Vultr models (if the form asks) | `qwen3.8-27b` (build, Ask) · `glm-5.3` (build retries) · `deepseek-v4.1-flash-1017` (vision parity) · `nemotron-3.5-content-safety` (screening) · systemone `clef` (decisions) · `qwen3-embedding-4b` (retrieval) · `qwen3-tts-12hz-1.7b` (the video's narration) |
| Team size | 1 (James Karanja, Zavora Technologies), unless James adds people |

The pick-list values are the ones the 93 entries of 7 Nov used (lablab's public submissions):
the track was `C1-Blast Radius Zero` for 53 of them, and the commonest categories were Security
(53), Developer Tools (43), Cloud Application (23) and Web Application (16).

## Links and files

| Field | Value | State |
|---|---|---|
| Public GitHub repository | https://github.com/zavora-ai/agentrix-webagent-platform | Live since Sun 8 Nov, one commit, MIT. Later changes reach it through `scripts/publish-public.sh`, with James's go-ahead each time |
| Demo application platform | **Other**, if Vultr isn't in the list (84 of the 93 entries chose Other); say Vultr in the description | Ready |
| Demo application URL | https://agentrix.website | Control answers again (the old build); W0 deploys the platform on Vultr routes |
| Video | TODO: the upload link | [`video/agentrix-vultr-demo.mp4`](video/agentrix-vultr-demo.mp4), built by [`video/build.py`](video/build.py) |
| Slide presentation (PDF) | [`deck/agentrix-vultr-deck.pdf`](deck/agentrix-vultr-deck.pdf), 15 slides | Drafted; live values to fill |
| Cover image (16:9) | [`cover/cover.png`](cover/cover.png), 1920 × 1080 | Ready |

## Facts the texts rest on

Each number in the texts above, and where it comes from. Re-check any marked live before
submitting.

| Claim | Source |
|---|---|
| About two in five websites run on WordPress | W3Techs, 39.4% on 7 Nov 2026 |
| Each job is a signed spec; receipts are signed (Ed25519, RFC 8785 JSON) | README, *How the agent does real work*; `crates/agentrix-sandbox` |
| Firefox through Playwright in the fetch room | README Pattern B; `room-fetch-browser` image pinned in `deploy/env/node.env` |
| The replay has no route out | The first live Move's `job.replay` receipt (sha256:2d03d23a…): empty allowlist, the phone-home refused (board row S2-05) |
| The metadata address is refused | Live, Sat 7 Nov 21:17 EAT: the fixture's redirect to 169.254.169.254 refused by the job's egress proxy (board row S1-03) |
| n/10 contained | TODO(live): the `gauntlet.run` receipt on Vultr routes. It has not been recorded live yet |
| Every model call goes to Vultr Serverless Inference | TODO(live): the `routing.changed` receipt "on vultr" after the switch (S3-10). Until then, the platform routes to Anthropic, and this sentence is not true |
| Plans from $29 a month, PayPal, moving free | `crates/agentrix-billing/data/plans.json`, the site's Prices page |
