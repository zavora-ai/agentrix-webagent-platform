# agentrix-messages

Every message the platform sends to owners and customers (W9): the catalogue, the outbox, the
Messages app in the owner console, notification settings, and delivery.

## Sending from another crate

```rust
let messages = agentrix_messages::from_context(ctx);
messages.send_to_site_owner(site_id, "owner.new_order", context, "Amina paid for K-1042", Some(key)).await?;
messages.send(agentrix_messages::Outgoing { message: "customer.order_received".into(), .. }).await?;
```

Each message's values are in its example beside the template: `templates/<audience>/<name>.json`.
A missing value is an error, never a blank. Every message is recorded in the outbox first.

## Node wiring

| Where | Line |
|---|---|
| `MIGRATIONS` | `agentrix_messages::migrate` (tables `message_outbox`, `message_preferences`) |
| `MODULES` | `agentrix_messages::mount` (`/console/messages/*`, the bounce webhook, the quiet-hours release task) |
| The shell | `agentrix_shell::mount_with(ctx, vec![agentrix_messages::console_app(ctx)], …)` |
| Identity | `agentrix_identity::mount_with_mailer(ctx, agentrix_messages::identity_mailer(ctx))`, and `.with_mailer(…)` on customer accounts |

## Switching email on

Until this is done, every message stays in the outbox as "Not sent" and nothing leaves.
James set up Resend for `agentrix.website` (DKIM, the `send` and `rsend` records and DMARC
`p=none` on Vultr DNS) on Sun 8 Nov.

1. Give node the send-only key as `RESEND_API_KEY` in its environment (the control-plane
   session's deploy env, never a committed file). Optional: `AGENTRIX_EMAIL_FROM` (default
   `hello@agentrix.website`), `AGENTRIX_EMAIL_REPLY_TO` (an email address) and
   `RESEND_WEBHOOK_SECRET`. A JSON file named by `AGENTRIX_EMAIL_FILE` works too (see
   `src/email.rs`).
2. In Resend, a webhook for `email.bounced` and `email.complained` pointing at
   `https://agentrix.website/messages/email/webhook`, with its secret in `RESEND_WEBHOOK_SECRET`.
3. Restart node. The Messages page stops saying "Email isn't switched on yet", and new messages
   show Sent with Resend's id. Addresses at `example.com` are never delivered.
4. Move DMARC to `p=quarantine` once Resend's reports look clean.

## Checking it

```sh
scripts/wcargo test -p agentrix-messages
scripts/wcargo run -p agentrix-messages --example dump -- <dir>      # every message as files
scripts/wcargo run -p agentrix-messages --example console -- 18902   # the app in the shell
scripts/copy-lint crates/agentrix-messages/templates
```
