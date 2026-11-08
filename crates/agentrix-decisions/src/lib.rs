//! Decision card v2 (contract C4, 02 §6.1): one card per decision on every surface.
//!
//! A WebAgent (or a workstream acting for one) raises a [`NewCard`] over one [`EffectRequest`];
//! the card carries who raised it, the risk with its reasons, when it expires and how it may be
//! approved. The owner approves it in the console's lane through the frozen approve contract
//! (design §4.1): a passkey assertion over the card's fingerprint, or a single-use authenticator
//! code bound to it, verified by node's [`Approvals`]. Approving writes a `decision.approved`
//! receipt, then runs the effect through the [`EffectRunner`] registered for the effect's tool.
//! Declining writes `decision.declined`. An unanswered card waits, and past its expiry it is
//! recorded as expired, never as declined and never as approved.
//!
//! The first release's cards (go-live, restore, held order) keep their own table and routes; this crate
//! never changes a signed type in place (02 §5.3): v1's `DecisionCard` stays as it is.
//!
//! Routes ([`router`]): `POST /decisions/v2/{id}/approve` (JSON `{method: "passkey",
//! credential}` or `{method: "totp", code, fingerprint}`, or a form with `method=totp`) and
//! `POST /decisions/v2/{id}/decline`. Replies are the contract's: 303 to a form, `{redirect}` or
//! `{message}` to JSON.
//!
//! Spoken and chat answers ([`Decisions::decide_spoken`]) follow [`surface`]'s one rule table:
//! they may allow once only an everyday card (low risk, [`ApprovalKind::OneTap`], no money, not
//! publishing) and decline any card; email decides nothing. They never count as a
//! reviewed approval (07 §5.3). No agent approves its own card: an approval needs a signed-in
//! owner.

pub mod surface;

use std::str::FromStr;
use std::sync::Arc;

use agentrix_host::{Approvals, NodeContext, OwnerSessions, Receipts};
use agentrix_types::{
    DecisionId, Digest, EffectRequest, ReceiptBody, ReceiptKind, SiteId, Subject,
};
use anyhow::Context;
use async_trait::async_trait;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::post;
use chrono::{DateTime, SecondsFormat, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;

/// Runs this crate's migrations (0550) with their own table, before node mounts anything.
pub fn migrate(pool: &SqlitePool) -> BoxFuture<'_, anyhow::Result<()>> {
    Box::pin(async move {
        let mut m = sqlx::migrate!("./migrations");
        m.set_ignore_missing(true);
        m.dangerous_set_table_name("_sqlx_migrations_agentrix_decisions");
        m.run(pool).await.context("agentrix-decisions migrations")?;
        Ok(())
    })
}

/// How risky a card is, with the reasons in the owner's words. Never a bare score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// Everyday: a returning customer, a usual amount, an undoable change.
    Low,
    /// Worth a look.
    Medium,
    /// Money, people outside the business, or something hard to undo.
    High,
}

/// The risk on a card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Risk {
    /// The level.
    pub level: RiskLevel,
    /// Why, in the owner's words: "Returning customer", "Usual amount".
    pub reasons: Vec<String>,
}

/// How a card may be approved (07 §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalKind {
    /// Only on a screen, with the card open: a passkey, or an authenticator code without one.
    Reviewed,
    /// Also "allow once" by voice or chat, when the risk is low. Never counts toward promotion.
    OneTap,
}

/// Who raised a card. Agents raise cards; only people approve them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaisedBy {
    /// The WebAgent's id in the roster: `cashier`, `editor`, `concierge`.
    pub webagent: String,
    /// The job, when one: `payments`, `freshness`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<String>,
}

/// A decision card, version 2.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionCardV2 {
    /// Always 2.
    pub v: u8,
    /// `dec2_…`.
    pub id: String,
    /// The site.
    pub site: SiteId,
    /// What kind of decision: `dial.raise`, `webagent.add`, `release.publish`, `order.confirm`.
    pub kind: String,
    /// Plain words: what will happen. "Approve cake order · KSh 4,500".
    pub title: String,
    /// One line for the lane.
    pub summary: String,
    /// "If you approve, exactly this happens", line by line, from the effect's arguments.
    pub effects: Vec<String>,
    /// The exact effect an approval allows.
    pub effect: EffectRequest,
    /// `effect.fingerprint()`: what the approval signs.
    pub fingerprint: Digest,
    /// Who raised it.
    pub raised_by: RaisedBy,
    /// How risky, and why.
    pub risk: Risk,
    /// How it may be approved.
    pub approval: ApprovalKind,
    /// When it was raised.
    pub created_at: DateTime<Utc>,
    /// Where to send the owner after they decide, if not back to where they were.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back: Option<String>,
}

impl DecisionCardV2 {
    /// After this the card can't be approved (the effect's expiry).
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.effect.expiry
    }
}

/// What a raiser hands [`Decisions::raise`].
#[derive(Debug, Clone)]
pub struct NewCard {
    /// The kind (see [`DecisionCardV2::kind`]).
    pub kind: String,
    /// Plain words: what will happen.
    pub title: String,
    /// One line for the lane.
    pub summary: String,
    /// "If you approve, exactly this happens".
    pub effects: Vec<String>,
    /// The exact effect. Its site is the card's site.
    pub effect: EffectRequest,
    /// Who raised it.
    pub raised_by: RaisedBy,
    /// How risky, and why.
    pub risk: Risk,
    /// How it may be approved.
    pub approval: ApprovalKind,
    /// Where to go after deciding.
    pub back: Option<String>,
    /// Raising twice with the same key returns the first card (an order held twice is one card).
    pub dedupe_key: Option<String>,
}

/// Where a card is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardState {
    /// Waiting for the owner.
    Waiting,
    /// Approved; the effect ran (or was handed to its runner).
    Approved,
    /// Declined; nothing changed.
    Declined,
    /// Expired unanswered; nothing was done.
    Expired,
}

impl CardState {
    fn as_str(self) -> &'static str {
        match self {
            CardState::Waiting => "waiting",
            CardState::Approved => "approved",
            CardState::Declined => "declined",
            CardState::Expired => "expired",
        }
    }
    fn parse(s: &str) -> anyhow::Result<Self> {
        Ok(match s {
            "waiting" => CardState::Waiting,
            "approved" => CardState::Approved,
            "declined" => CardState::Declined,
            "expired" => CardState::Expired,
            other => anyhow::bail!("unknown card state {other}"),
        })
    }
}

/// A card and what happened to it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CardRow {
    /// The card.
    pub card: DecisionCardV2,
    /// Its state; a waiting card past its expiry reads as expired.
    pub state: CardState,
    /// `passkey`, `totp`, `spoken` or `chat`, once decided.
    pub method: Option<String>,
    /// When it was decided.
    pub decided_at: Option<DateTime<Utc>>,
    /// The decision's receipt.
    pub receipt: Option<String>,
}

/// Runs an approved card's effect. The crate that raises cards for a tool registers one.
#[async_trait]
pub trait EffectRunner: Send + Sync {
    /// The tool it runs, e.g. `order.confirm`.
    fn tool(&self) -> &str;
    /// Runs the effect once, after the approval's receipt is written. Returns where to send the
    /// owner, or `None` to go back where they were. An error leaves the approval recorded and
    /// tells the owner the effect didn't finish.
    async fn run(&self, card: &DecisionCardV2, receipt: &str) -> anyhow::Result<Option<String>>;
    /// Called after a decline, for the raiser to tidy up (release a held order). Optional.
    async fn declined(&self, _card: &DecisionCardV2, _receipt: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

/// How the owner answered.
#[derive(Debug, Clone, PartialEq)]
pub enum Attempt {
    /// A webauthn-rs `PublicKeyCredential`, as JSON.
    Passkey(Value),
    /// Six digits from the authenticator app.
    Totp(String),
}

/// Why a decision was refused, with plain words for the owner and a status.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    /// The HTTP status.
    pub status: StatusCode,
    /// What to tell the owner.
    pub words: String,
}

fn refuse(status: StatusCode, words: impl Into<String>) -> Refusal {
    Refusal {
        status,
        words: words.into(),
    }
}

fn internal(e: anyhow::Error) -> Refusal {
    tracing::error!(error = %format!("{e:#}"), "decision failed");
    refuse(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Something went wrong on our side. Nothing was changed; try again.",
    )
}

fn stored(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn parse_time(s: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(s)?.with_timezone(&Utc))
}

type RowSql = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// A database whose migrations haven't run yet has no v2 cards rather than an error.
fn missing_table(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(d) if d.message().contains("no such table"))
}

/// Raises, lists and decides v2 cards.
#[derive(Clone)]
pub struct Decisions {
    pool: SqlitePool,
    receipts: Arc<dyn Receipts>,
    approvals: Arc<dyn Approvals>,
    owners: Arc<dyn OwnerSessions>,
    runners: Arc<Vec<Arc<dyn EffectRunner>>>,
}

impl Decisions {
    /// Over node's database, receipts, approvals and sessions.
    pub fn new(ctx: &NodeContext) -> Self {
        Self {
            pool: ctx.pool.clone(),
            receipts: ctx.receipts.clone(),
            approvals: ctx.approvals.clone(),
            owners: ctx.owners.clone(),
            runners: Arc::new(Vec::new()),
        }
    }

    /// Adds runners for the tools other crates raise cards for.
    pub fn with_runners(mut self, runners: Vec<Arc<dyn EffectRunner>>) -> Self {
        self.runners = Arc::new(runners);
        self
    }

    /// Raises a card. With a `dedupe_key` already used on the site, returns that card instead.
    pub async fn raise(&self, new: NewCard) -> anyhow::Result<DecisionCardV2> {
        let site = new.effect.site.clone();
        if let Some(key) = &new.dedupe_key {
            let existing: Option<(String,)> = sqlx::query_as(
                "SELECT card FROM decision_cards_v2 WHERE site_id = ? AND dedupe_key = ?",
            )
            .bind(site.as_str())
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
            if let Some((card,)) = existing {
                return Ok(serde_json::from_str(&card)?);
            }
        }
        let now = Utc::now();
        let card = DecisionCardV2 {
            v: 2,
            id: format!("dec2_{}", uuid::Uuid::now_v7().simple()),
            site: site.clone(),
            kind: new.kind,
            title: new.title,
            summary: new.summary,
            effects: new.effects,
            fingerprint: new.effect.fingerprint()?,
            effect: new.effect,
            raised_by: new.raised_by,
            risk: new.risk,
            approval: new.approval,
            created_at: now,
            back: new.back,
        };
        sqlx::query(
            "INSERT INTO decision_cards_v2 (id, site_id, kind, card, fingerprint, dedupe_key, state, created_at) VALUES (?, ?, ?, ?, ?, ?, 'waiting', ?)",
        )
        .bind(&card.id)
        .bind(site.as_str())
        .bind(&card.kind)
        .bind(serde_json::to_string(&card)?)
        .bind(card.fingerprint.to_string())
        .bind(new.dedupe_key)
        .bind(stored(now))
        .execute(&self.pool)
        .await?;
        Ok(card)
    }

    fn row(sql: RowSql, now: DateTime<Utc>) -> anyhow::Result<CardRow> {
        let (card, fingerprint, state, method, decided_at, receipt) = sql;
        let card: DecisionCardV2 =
            serde_json::from_str(&card).context("a stored v2 card does not parse")?;
        anyhow::ensure!(
            Digest::from_str(&fingerprint)? == card.fingerprint,
            "card {}: stored fingerprint differs",
            card.id
        );
        let mut state = CardState::parse(&state)?;
        if state == CardState::Waiting && card.effect.is_expired(now) {
            state = CardState::Expired;
        }
        Ok(CardRow {
            card,
            state,
            method,
            decided_at: decided_at.as_deref().map(parse_time).transpose()?,
            receipt,
        })
    }

    /// The site's cards, newest first.
    pub async fn list(&self, site: &str) -> anyhow::Result<Vec<CardRow>> {
        let now = Utc::now();
        let rows: Vec<RowSql> = match sqlx::query_as(
            "SELECT card, fingerprint, state, method, decided_at, receipt FROM decision_cards_v2 WHERE site_id = ? ORDER BY created_at DESC, rowid DESC",
        )
        .bind(site)
        .fetch_all(&self.pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) if missing_table(&e) => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        rows.into_iter().map(|r| Self::row(r, now)).collect()
    }

    /// One card of the site.
    pub async fn get(&self, site: &str, id: &str) -> anyhow::Result<Option<CardRow>> {
        let row: Option<RowSql> = match sqlx::query_as(
            "SELECT card, fingerprint, state, method, decided_at, receipt FROM decision_cards_v2 WHERE site_id = ? AND id = ?",
        )
        .bind(site)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        {
            Ok(row) => row,
            Err(e) if missing_table(&e) => None,
            Err(e) => return Err(e.into()),
        };
        row.map(|r| Self::row(r, Utc::now())).transpose()
    }

    async fn card_any_site(&self, id: &str) -> Result<CardRow, Refusal> {
        let row: Option<RowSql> = sqlx::query_as("SELECT card, fingerprint, state, method, decided_at, receipt FROM decision_cards_v2 WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| internal(e.into()))?;
        let row =
            row.ok_or_else(|| refuse(StatusCode::NOT_FOUND, "We couldn't find that card."))?;
        Self::row(row, Utc::now()).map_err(internal)
    }

    /// Whether `account` owns the card's site. Agents never hold an owner session.
    async fn owner_of(&self, site: &SiteId, account: &str) -> Result<(), Refusal> {
        let owner: Option<(Option<String>,)> =
            sqlx::query_as("SELECT owner_account_id FROM sites WHERE site_id = ?")
                .bind(site.as_str())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| internal(e.into()))?;
        match owner {
            Some((Some(o),)) if o == account => Ok(()),
            Some(_) => Err(refuse(
                StatusCode::FORBIDDEN,
                "Only the site's owner can decide this.",
            )),
            None => Err(refuse(StatusCode::NOT_FOUND, "We couldn't find that site.")),
        }
    }

    async fn record(
        &self,
        row: &CardRow,
        state: CardState,
        method: &str,
        account: &str,
        receipt: &str,
        now: DateTime<Utc>,
    ) -> Result<(), Refusal> {
        let done = sqlx::query(
            "UPDATE decision_cards_v2 SET state = ?, method = ?, decided_by = ?, decided_at = ?, receipt = ? WHERE id = ? AND state = 'waiting'",
        )
        .bind(state.as_str())
        .bind(method)
        .bind(account)
        .bind(stored(now))
        .bind(receipt)
        .bind(&row.card.id)
        .execute(&self.pool)
        .await
        .map_err(|e| internal(e.into()))?;
        if done.rows_affected() == 1 {
            Ok(())
        } else {
            Err(refuse(
                StatusCode::CONFLICT,
                "This card was already decided.",
            ))
        }
    }

    fn check_waiting(row: &CardRow) -> Result<(), Refusal> {
        match row.state {
            CardState::Waiting => Ok(()),
            CardState::Expired => Err(refuse(
                StatusCode::GONE,
                "This card expired, so nothing was done. Ask for a fresh one.",
            )),
            _ => Err(refuse(
                StatusCode::CONFLICT,
                "This card was already decided.",
            )),
        }
    }

    /// Refuses before any approval is recorded when nothing on the platform runs the card's tool.
    fn check_runnable(&self, row: &CardRow) -> Result<(), Refusal> {
        if self
            .runners
            .iter()
            .any(|r| r.tool() == row.card.effect.tool)
        {
            Ok(())
        } else {
            Err(refuse(
                StatusCode::CONFLICT,
                "Nothing on this platform can carry this out yet, so it can't be approved. You can decline it; nothing changes either way.",
            ))
        }
    }

    fn check_fingerprint(row: &CardRow) -> Result<(), Refusal> {
        match row.card.effect.fingerprint() {
            Ok(fp) if fp == row.card.fingerprint => Ok(()),
            _ => Err(refuse(
                StatusCode::CONFLICT,
                "This card changed after it was raised, so it can't be approved.",
            )),
        }
    }

    async fn issue(
        &self,
        row: &CardRow,
        kind: ReceiptKind,
        detail: Value,
        title: String,
        now: DateTime<Utc>,
    ) -> Result<String, Refusal> {
        let mut body = ReceiptBody::new(
            kind,
            row.card.site.clone(),
            Subject::Decision(DecisionId::new(row.card.id.clone())),
            row.card.created_at,
            now,
        );
        body.detail = Some(detail);
        self.receipts
            .issue(body.with_title(title))
            .await
            .map_err(internal)
    }

    async fn run(&self, row: &CardRow, receipt: &str) -> Result<Option<String>, Refusal> {
        let Some(runner) = self
            .runners
            .iter()
            .find(|r| r.tool() == row.card.effect.tool)
        else {
            tracing::warn!(tool = %row.card.effect.tool, card = %row.card.id, "no runner for an approved card's tool");
            return Err(refuse(
                StatusCode::CONFLICT,
                "Your approval is recorded, but nothing on this platform can carry it out yet. Nothing else happened.",
            ));
        };
        runner.run(&row.card, receipt).await.map_err(|e| {
            tracing::warn!(error = %format!("{e:#}"), card = %row.card.id, "an approved effect did not finish");
            refuse(StatusCode::CONFLICT, format!("Your approval is recorded, but it didn't finish: {}", e.to_string().lines().next().unwrap_or("try again")))
        })
    }

    /// Approves card `id` for `account` with `attempt` (the frozen approve contract).
    /// `posted_fingerprint` is the fingerprint a code form names; it must be the card's.
    pub async fn approve(
        &self,
        id: &str,
        account: &str,
        attempt: Attempt,
        posted_fingerprint: Option<&str>,
    ) -> Result<Option<String>, Refusal> {
        let row = self.card_any_site(id).await?;
        self.owner_of(&row.card.site, account).await?;
        Self::check_waiting(&row)?;
        Self::check_fingerprint(&row)?;
        self.check_runnable(&row)?;
        if matches!(attempt, Attempt::Totp(_)) {
            let ours = row.card.fingerprint.to_string();
            let named = posted_fingerprint
                .is_some_and(|p| p == ours || Some(p) == ours.strip_prefix(Digest::PREFIX));
            if !named {
                return Err(refuse(
                    StatusCode::CONFLICT,
                    "This card changed since you opened it. Reload it and approve again.",
                ));
            }
        }
        let uuid = uuid::Uuid::from_str(account)
            .map_err(|_| refuse(StatusCode::UNAUTHORIZED, "Sign in first."))?;
        let request = match attempt {
            Attempt::Passkey(credential) => json!({ "passkey": credential }),
            Attempt::Totp(code) => json!({ "totp": code }),
        };
        let proof = self
            .approvals
            .verify(uuid, *row.card.fingerprint.as_bytes(), request)
            .await
            .map_err(|e| refuse(StatusCode::FORBIDDEN, e.to_string()))?;
        let method = if proof.get("method").and_then(Value::as_str) == Some("passkey") {
            "passkey"
        } else {
            "totp"
        };
        let now = Utc::now();
        let how = if method == "passkey" {
            "your passkey"
        } else {
            "a code from your authenticator app"
        };
        let receipt = self
            .issue(
                &row,
                ReceiptKind::DecisionApproved,
                json!({ "v": 2, "decision_id": row.card.id, "kind": row.card.kind, "tool": row.card.effect.tool, "fingerprint": row.card.fingerprint, "method": method, "approval": "reviewed", "approver": account, "raised_by": row.card.raised_by, "evidence": proof }),
                format!("You approved \"{}\" with {how}", row.card.title),
                now,
            )
            .await?;
        self.record(&row, CardState::Approved, method, account, &receipt, now)
            .await?;
        let to = self.run(&row, &receipt).await?;
        Ok(to.or_else(|| row.card.back.clone()))
    }

    /// Declines card `id` for `account`. The owner's session is enough: declining changes nothing.
    pub async fn decline(
        &self,
        id: &str,
        account: &str,
        how: &str,
    ) -> Result<Option<String>, Refusal> {
        let row = self.card_any_site(id).await?;
        self.owner_of(&row.card.site, account).await?;
        Self::check_waiting(&row)?;
        let now = Utc::now();
        let receipt = self
            .issue(
                &row,
                ReceiptKind::DecisionDeclined,
                json!({ "v": 2, "decision_id": row.card.id, "kind": row.card.kind, "tool": row.card.effect.tool, "fingerprint": row.card.fingerprint, "by": account, "how": how }),
                format!("You declined \"{}\"; nothing changed", row.card.title),
                now,
            )
            .await?;
        self.record(&row, CardState::Declined, how, account, &receipt, now)
            .await?;
        if let Some(runner) = self
            .runners
            .iter()
            .find(|r| r.tool() == row.card.effect.tool)
            && let Err(e) = runner.declined(&row.card, &receipt).await
        {
            tracing::warn!(error = %format!("{e:#}"), card = %row.card.id, "tidying up after a decline failed");
        }
        Ok(row.card.back.clone())
    }

    /// A spoken or chat answer (07 §5.3): "allow once" only for a low-risk one-tap card,
    /// "deny" for any card. `heard` is what the owner said, kept on the receipt as words only;
    /// no recording is kept. Returns the receipt's digest.
    pub async fn decide_spoken(
        &self,
        site: &str,
        account: &str,
        id: &str,
        allow: bool,
        channel: &str,
        heard: &str,
    ) -> Result<String, Refusal> {
        let row = self
            .get(site, id)
            .await
            .map_err(internal)?
            .ok_or_else(|| refuse(StatusCode::NOT_FOUND, "We couldn't find that card."))?;
        self.owner_of(&row.card.site, account).await?;
        Self::check_waiting(&row)?;
        let surface = surface::Surface::parse(channel).unwrap_or(surface::Surface::Voice);
        let powers = surface::powers(surface, &row.card);
        if !powers.decline {
            return Err(refuse(
                StatusCode::FORBIDDEN,
                "Email can't decide anything. Open the card in the console.",
            ));
        }
        let method = if surface == surface::Surface::WhatsApp {
            "chat"
        } else {
            "spoken"
        };
        let now = Utc::now();
        if !allow {
            let receipt = self
                .issue(
                    &row,
                    ReceiptKind::DecisionDeclined,
                    json!({ "v": 2, "decision_id": row.card.id, "kind": row.card.kind, "fingerprint": row.card.fingerprint, "by": account, "how": method, "heard": heard }),
                    format!("You declined \"{}\" by {}; nothing changed", row.card.title, if method == "chat" { "message" } else { "voice" }),
                    now,
                )
                .await?;
            self.record(&row, CardState::Declined, method, account, &receipt, now)
                .await?;
            return Ok(receipt);
        }
        if !powers.allow_once {
            return Err(refuse(
                StatusCode::FORBIDDEN,
                "This one needs your screen and your passkey. Open it in the console.",
            ));
        }
        Self::check_fingerprint(&row)?;
        self.check_runnable(&row)?;
        let receipt = self
            .issue(
                &row,
                ReceiptKind::DecisionApproved,
                json!({ "v": 2, "decision_id": row.card.id, "kind": row.card.kind, "tool": row.card.effect.tool, "fingerprint": row.card.fingerprint, "method": method, "approval": "one-tap", "counts_for_promotion": false, "approver": account, "heard": heard }),
                format!("You allowed \"{}\" once by {}", row.card.title, if method == "chat" { "message" } else { "voice" }),
                now,
            )
            .await?;
        self.record(&row, CardState::Approved, method, account, &receipt, now)
            .await?;
        self.run(&row, &receipt).await?;
        Ok(receipt)
    }
}

// ---------- Routes: the frozen approve contract's shape ----------

/// A reply: 303 to a form, `{redirect}` or `{message}` to JSON.
struct Reply {
    json: bool,
    outcome: Result<String, Refusal>,
}

impl IntoResponse for Reply {
    fn into_response(self) -> Response {
        match (self.outcome, self.json) {
            (Ok(to), true) => axum::Json(json!({ "redirect": to })).into_response(),
            (Ok(to), false) => Redirect::to(&to).into_response(),
            (Err(r), _) => (r.status, axum::Json(json!({ "message": r.words }))).into_response(),
        }
    }
}

fn wants_json(headers: &HeaderMap) -> bool {
    [header::CONTENT_TYPE, header::ACCEPT].into_iter().any(|h| {
        headers
            .get(h)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("application/json"))
    })
}

async fn account(d: &Decisions, headers: &HeaderMap) -> Result<String, Refusal> {
    match d.owners.owner(headers).await {
        Ok(Some(o)) => Ok(o.account.to_string()),
        Ok(None) => Err(refuse(StatusCode::UNAUTHORIZED, "Sign in first.")),
        Err(e) => Err(internal(e)),
    }
}

fn back_or(row_back: Option<String>, headers: &HeaderMap) -> String {
    row_back
        .or_else(|| {
            headers
                .get(header::REFERER)
                .and_then(|v| v.to_str().ok())
                .and_then(|r| url::Url::parse(r).ok())
                .map(|u| u.path().to_owned())
                .filter(|p| p.starts_with("/console/"))
        })
        .unwrap_or_else(|| "/os".to_owned())
}

async fn approve(
    State(d): State<Decisions>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Reply {
    let json_reply = wants_json(&headers);
    let outcome = async {
        let who = account(&d, &headers).await?;
        let is_json = headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("application/json"));
        let (attempt, posted) = if is_json {
            let v: Value = serde_json::from_slice(&body)
                .map_err(|_| refuse(StatusCode::BAD_REQUEST, "That approval couldn't be read."))?;
            match v.get("method").and_then(Value::as_str) {
                Some("passkey") => (
                    Attempt::Passkey(v.get("credential").cloned().unwrap_or(Value::Null)),
                    None,
                ),
                Some("totp") => (
                    Attempt::Totp(
                        v.get("code")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                    ),
                    v.get("fingerprint")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                ),
                _ => {
                    return Err(refuse(
                        StatusCode::BAD_REQUEST,
                        "Approve with your passkey or a code from your authenticator app.",
                    ));
                }
            }
        } else {
            let f: std::collections::HashMap<String, String> =
                url::form_urlencoded::parse(&body).into_owned().collect();
            match f.get("method").map(String::as_str) {
                Some("totp") => (
                    Attempt::Totp(f.get("code").cloned().unwrap_or_default()),
                    f.get("fingerprint").cloned(),
                ),
                _ => {
                    return Err(refuse(
                        StatusCode::BAD_REQUEST,
                        "Approve with your passkey or a code from your authenticator app.",
                    ));
                }
            }
        };
        let to = d.approve(&id, &who, attempt, posted.as_deref()).await?;
        Ok(back_or(to, &headers))
    }
    .await;
    Reply {
        json: json_reply,
        outcome,
    }
}

async fn decline(State(d): State<Decisions>, Path(id): Path<String>, headers: HeaderMap) -> Reply {
    let json_reply = wants_json(&headers);
    let outcome = async {
        let who = account(&d, &headers).await?;
        let to = d.decline(&id, &who, "console").await?;
        Ok(back_or(to, &headers))
    }
    .await;
    Reply {
        json: json_reply,
        outcome,
    }
}

/// The v2 approve and decline routes.
pub fn router(d: Decisions) -> Router {
    Router::new()
        .route("/decisions/v2/{id}/approve", post(approve))
        .route("/decisions/v2/{id}/decline", post(decline))
        .with_state(d)
}

/// Mounts the v2 routes on the platform host, with no runners (C11). Node uses
/// [`mount_with`] once crates register runners for their tools.
pub fn mount(ctx: &NodeContext) -> anyhow::Result<agentrix_host::Mounted> {
    mount_with(ctx, Vec::new())
}

/// Mounts the v2 routes with the effect runners other crates register.
pub fn mount_with(
    ctx: &NodeContext,
    runners: Vec<Arc<dyn EffectRunner>>,
) -> anyhow::Result<agentrix_host::Mounted> {
    Ok(agentrix_host::Mounted::default()
        .platform(router(Decisions::new(ctx).with_runners(runners))))
}
