//! [`ShellPort`] over the node: `control.db` through [`NodeContext::pool`], the signed-in owner
//! through [`NodeContext::owners`], approvals through [`NodeContext::approvals`] (the frozen
//! approve contract) and receipts through [`NodeContext::receipts`].
//!
//! The SQL is Brick 1's (`agentrix-node/src/pages/store.rs`, `db/usage.rs`), ported with the
//! console's pages. `control.db`'s tables keep their writers; the shell only reads them.

use std::str::FromStr;

use agentrix_console::view as cv;
use agentrix_host::NodeContext;
use agentrix_types::{
    ApprovalMethod, Brief, Charter, DecisionCard, Digest, GauntletAttack, JobLevel, Receipt,
    ReceiptBody,
};
use anyhow::{Context, bail};
use async_trait::async_trait;
use axum::http::HeaderMap;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::map::{self, DecisionRow, DecisionState};
use crate::port::{
    Access, Attempt, CardRow, Happened, Owner, PortResult, Proof, ShellPort, SiteInfo, Step,
    WorkCard,
};

/// The shell's reads and approvals on a running node.
#[derive(Clone)]
pub struct ControlPort {
    ctx: NodeContext,
}

impl ControlPort {
    /// Reads through `ctx`.
    pub fn new(ctx: NodeContext) -> Self {
        Self { ctx }
    }

    fn pool(&self) -> &SqlitePool {
        &self.ctx.pool
    }
}

/// Times as `control.db` stores them (RFC 3339, microseconds, `Z`).
fn stored(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn time(text: &str) -> PortResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(text)
        .with_context(|| format!("a stored time does not parse: {text}"))?
        .with_timezone(&Utc))
}

fn digest(text: &str) -> PortResult<Digest> {
    text.parse()
        .with_context(|| format!("a stored digest does not parse: {text}"))
}

/// True for the `-sample` hosts (W2): `kilele-bakery-sample.agentrix.website`.
pub fn is_sample(host: &str) -> bool {
    host.split('.')
        .next()
        .is_some_and(|label| label.ends_with("-sample"))
}

type DecisionSql = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);
/// domain, name, fixture, offset, live, moving, live since, owner.
type SiteSql = (
    String,
    String,
    i64,
    i64,
    i64,
    i64,
    Option<String>,
    Option<String>,
);
/// item, title, reason, location, evidence, state, decision, decision state.
type QuarantineSql = (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
);

fn decision_row(
    (card, fingerprint, state, method, decided_at, receipt): DecisionSql,
) -> PortResult<DecisionRow> {
    let card: DecisionCard =
        serde_json::from_str(&card).context("a stored decision card does not parse")?;
    if digest(&fingerprint)? != card.fingerprint {
        bail!(
            "decision {}: the stored fingerprint does not match its card",
            card.decision_id
        );
    }
    let state = match state.as_str() {
        "waiting" => DecisionState::Waiting,
        "approved" => DecisionState::Approved,
        "declined" => DecisionState::Declined,
        "expired" => DecisionState::Expired,
        other => bail!("decision {}: unknown state {other}", card.decision_id),
    };
    let method = match method.as_deref() {
        Some("passkey") => Some(ApprovalMethod::Passkey),
        Some("totp") => Some(ApprovalMethod::StepUp),
        _ => None,
    };
    Ok(DecisionRow {
        card,
        state,
        method,
        decided_at: decided_at.as_deref().map(time).transpose()?,
        receipt: receipt.as_deref().map(digest).transpose()?,
    })
}

/// A `decisions` query: [`DecisionSql`]'s columns, then the rest. sqlx takes only static SQL.
macro_rules! select_decisions {
    ($rest:literal) => {
        concat!(
            "SELECT card, fingerprint, state, method, decided_at, receipt FROM decisions ",
            $rest
        )
    };
}

impl ControlPort {
    async fn site_info(
        &self,
        site_id: &str,
        now: DateTime<Utc>,
    ) -> PortResult<Option<(SiteInfo, Option<String>)>> {
        let row: Option<SiteSql> = sqlx::query_as(
            "SELECT s.domain, s.name, s.fixture, s.utc_offset_minutes, \
                EXISTS (SELECT 1 FROM site_pointer p WHERE p.site_id = s.site_id), \
                EXISTS (SELECT 1 FROM moves m WHERE m.site_id = s.site_id), \
                (SELECT MIN(r.signed_at) FROM releases r WHERE r.site_id = s.site_id AND r.state <> 'candidate'), \
                s.owner_account_id \
             FROM sites s WHERE s.site_id = ?",
        )
        .bind(site_id)
        .fetch_optional(self.pool())
        .await?;
        let Some((host, name, fixture, offset, live, moving, live_since, owner)) = row else {
            return Ok(None);
        };
        let offset = i32::try_from(offset).context("utc_offset_minutes is out of range")?;
        let live_since = live_since.as_deref().map(time).transpose()?;
        let state_words = match (live != 0, moving != 0, live_since) {
            (true, _, Some(since)) => format!("Live since {}", map::when(since, offset, now)),
            (true, _, None) => "Live".to_owned(),
            (false, true, _) => "Moving".to_owned(),
            _ => "In setup".to_owned(),
        };
        let platform = &self.ctx.platform_host;
        let live_url = (live != 0).then(|| {
            if platform.contains('.')
                && platform.parse::<std::net::IpAddr>().is_err()
                && !is_sample(&host)
            {
                format!(
                    "https://{}.{platform}/",
                    host.trim_end_matches('.')
                        .to_ascii_lowercase()
                        .replace('.', "-")
                )
            } else {
                format!("https://{}/", host.to_ascii_lowercase())
            }
        });
        Ok(Some((
            SiteInfo {
                id: site_id.to_owned(),
                name: if name.is_empty() { host.clone() } else { name },
                sample: is_sample(&host),
                host,
                state_words,
                live_url,
                fixture: fixture != 0,
                live: live != 0,
                utc_offset_minutes: offset,
            },
            owner,
        )))
    }

    async fn decision_rows(&self, site: &str) -> PortResult<Vec<DecisionRow>> {
        let rows: Vec<DecisionSql> = sqlx::query_as(select_decisions!(
            "WHERE site_id = ? ORDER BY created_at DESC, rowid DESC"
        ))
        .bind(site)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter().map(decision_row).collect()
    }

    async fn levels(&self, site: &str) -> PortResult<Vec<JobLevel>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT body FROM charters WHERE site_id = ? ORDER BY version DESC LIMIT 1",
        )
        .bind(site)
        .fetch_optional(self.pool())
        .await?;
        let Some((body,)) = row else {
            return Ok(Vec::new());
        };
        let charter: Charter =
            serde_json::from_str(&body).context("a stored charter does not parse")?;
        Ok(charter.jobs)
    }

    async fn receipts(&self, site: &str, limit: i64) -> PortResult<Vec<Receipt>> {
        let rows: Vec<(String,)> = sqlx::query_as("SELECT body FROM receipts WHERE site_id = ? ORDER BY issued_at DESC, rowid DESC LIMIT ?")
            .bind(site)
            .bind(limit)
            .fetch_all(self.pool())
            .await?;
        rows.into_iter()
            .map(|(b,)| serde_json::from_str(&b).context("a stored receipt does not parse"))
            .collect()
    }

    fn account(owner: &Owner) -> PortResult<Uuid> {
        Uuid::from_str(&owner.account).context("the owner's account id is not a UUID")
    }
}

#[async_trait]
impl ShellPort for ControlPort {
    async fn access(
        &self,
        headers: &HeaderMap,
        site: Option<&str>,
        back: &str,
    ) -> PortResult<Access> {
        let Some(session) = self.ctx.owners.owner(headers).await? else {
            let next: String = url::form_urlencoded::byte_serialize(back.as_bytes()).collect();
            return Ok(Access::Redirect(format!("/account/signin?next={next}")));
        };
        let account = session.account.to_string();
        let now = Utc::now();
        let site_id = match site {
            Some(s) => s.to_owned(),
            None => {
                let first: Option<(String,)> = sqlx::query_as("SELECT site_id FROM sites WHERE owner_account_id = ? ORDER BY created_at LIMIT 1")
                    .bind(&account)
                    .fetch_optional(self.pool())
                    .await?;
                match first {
                    Some((s,)) => s,
                    None => {
                        // Signed in without a site: their pending claim, else Assess (`/` is
                        // the public home page since the front door moved there).
                        let claim: Option<(String,)> = sqlx::query_as(
                            "SELECT c.site_id FROM claims c JOIN sites s ON s.site_id = c.site_id \
                             WHERE c.account_id = ? AND c.state = 'pending' AND s.owner_account_id IS NULL \
                             ORDER BY c.created_at DESC LIMIT 1",
                        )
                        .bind(&account)
                        .fetch_optional(self.pool())
                        .await
                        .unwrap_or(None);
                        return Ok(Access::Redirect(claim.map_or_else(
                            || "/assess".to_owned(),
                            |(s,)| format!("/sites/{s}/claim"),
                        )));
                    }
                }
            }
        };
        let Some((info, owner)) = self.site_info(&site_id, now).await? else {
            return Ok(Access::NotFound);
        };
        if owner.as_deref() != Some(account.as_str()) {
            return Ok(Access::NotFound);
        }
        let person: Option<(String, i64)> = sqlx::query_as(
            "SELECT display_name, EXISTS (SELECT 1 FROM credentials c WHERE c.account_id = a.id) FROM accounts a WHERE a.id = ?",
        )
        .bind(&account)
        .fetch_optional(self.pool())
        .await?;
        let (name, has_passkey) = person.map(|(n, p)| (n, p != 0)).unwrap_or_default();
        let first = name
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_owned();
        Ok(Access::Owner(Owner {
            account,
            name: first,
            has_passkey,
            site: info,
        }))
    }

    async fn today(&self, o: &Owner) -> PortResult<cv::TodayView> {
        let now = Utc::now();
        let offset = o.site.utc_offset_minutes;
        let brief: Option<(String, String)> = sqlx::query_as("SELECT written_at, body FROM briefs WHERE site_id = ? ORDER BY written_at DESC LIMIT 1")
            .bind(&o.site.id)
            .fetch_optional(self.pool())
            .await?;
        let brief = match brief {
            Some((at, body)) => {
                let b: Brief =
                    serde_json::from_str(&body).context("a stored brief does not parse")?;
                Some(map::brief_view(time(&at)?, &b, offset, now))
            }
            None => None,
        };
        let passed: Vec<(String, Option<String>, String)> =
            sqlx::query_as("SELECT question, contact, passed_at FROM passed_questions WHERE site_id = ? ORDER BY passed_at DESC LIMIT 10")
                .bind(&o.site.id)
                .fetch_all(self.pool())
                .await?;
        let (total,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM passed_questions WHERE site_id = ?")
                .bind(&o.site.id)
                .fetch_one(self.pool())
                .await?;
        let recent = self
            .receipts(&o.site.id, 5)
            .await?
            .iter()
            .filter_map(|r| map::receipt_row(r, offset, now))
            .collect();
        Ok(cv::TodayView {
            greeting: String::new(),
            chief: map::mark("chief", cv::MarkState::Idle, ""),
            brief,
            first_brief_words: "Your first brief comes after your WebAgents have done something worth telling you, written only from what they recorded.".into(),
            passed: passed
                .into_iter()
                .map(|(question, contact, at)| Ok(cv::PassedQuestion { question, contact, when_words: map::when(time(&at)?, offset, now) }))
                .collect::<PortResult<_>>()?,
            passed_total: u32::try_from(total).unwrap_or(0),
            recent,
            ..Default::default()
        })
    }

    async fn cards(&self, o: &Owner) -> PortResult<Vec<CardRow>> {
        let now = Utc::now();
        let mut rows: Vec<CardRow> = self
            .decision_rows(&o.site.id)
            .await?
            .iter()
            .map(|row| {
                let (risk, risk_words) = map::risk(&row.card);
                // A waiting card past its expiry can't be approved: show it as expired.
                let state =
                    if row.state == DecisionState::Waiting && row.card.effect.is_expired(now) {
                        cv::CardState::Expired
                    } else {
                        map::card_state(row.state)
                    };
                let waiting = state == cv::CardState::Waiting;
                let id = row.card.decision_id.to_string();
                CardRow {
                    kind: match row.card.kind {
                        agentrix_types::CardKind::GoLive => "go-live",
                        agentrix_types::CardKind::QuarantineRestore => "quarantine-restore",
                        agentrix_types::CardKind::OrderHeld => "order-held",
                    }
                    .to_owned(),
                    title: row.card.title.clone(),
                    summary: row
                        .card
                        .body
                        .lines()
                        .map(str::trim)
                        .find(|l| !l.is_empty())
                        .unwrap_or_default()
                        .to_owned(),
                    asked_by: map::mark(
                        &row.card.effect.webagent,
                        if waiting {
                            cv::MarkState::Waiting
                        } else {
                            cv::MarkState::Idle
                        },
                        if waiting { "Waiting for you" } else { "" },
                    ),
                    risk,
                    risk_words,
                    created_at: row.card.created_at,
                    expires_at: row.card.effect.expiry,
                    outcome_words: if state == cv::CardState::Expired {
                        "Expired unanswered; nothing was done".into()
                    } else {
                        map::outcome_words(row)
                    },
                    state,
                    href: format!("/console/{}/decisions/{id}", o.site.id),
                    approve_url: format!("/decisions/{id}/approve"),
                    decline_url: format!("/decisions/{id}/decline"),
                    id,
                }
            })
            .collect();
        // Card v2 (agentrix-decisions) beside Brick 1's, newest first.
        for r in agentrix_decisions::Decisions::new(&self.ctx)
            .list(&o.site.id)
            .await?
        {
            rows.push(v2_row(&r, &o.site.id));
        }
        rows.sort_by_key(|r| std::cmp::Reverse(r.created_at));
        Ok(rows)
    }

    async fn card(&self, o: &Owner, id: &str, ceremony: bool) -> PortResult<Option<cv::Card>> {
        let now = Utc::now();
        if id.starts_with("dec2_") {
            let Some(r) = agentrix_decisions::Decisions::new(&self.ctx)
                .get(&o.site.id, id)
                .await?
            else {
                return Ok(None);
            };
            let options =
                if ceremony && r.state == agentrix_decisions::CardState::Waiting && o.has_passkey {
                    self.passkey_options(o, &r.card.fingerprint).await?
                } else {
                    None
                };
            return Ok(Some(v2_card(
                &r,
                options,
                o.has_passkey,
                o.site.utc_offset_minutes,
                now,
            )));
        }
        let row: Option<DecisionSql> =
            sqlx::query_as(select_decisions!("WHERE site_id = ? AND decision_id = ?"))
                .bind(&o.site.id)
                .bind(id)
                .fetch_optional(self.pool())
                .await?;
        let Some(row) = row.map(decision_row).transpose()? else {
            return Ok(None);
        };
        let options = if ceremony && row.state == DecisionState::Waiting && o.has_passkey {
            self.passkey_options(o, &row.card.fingerprint).await?
        } else {
            None
        };
        let levels = if row.card.kind == agentrix_types::CardKind::GoLive {
            self.levels(&o.site.id).await?
        } else {
            Vec::new()
        };
        Ok(Some(map::card(
            &row,
            &levels,
            options,
            o.has_passkey,
            o.site.utc_offset_minutes,
            now,
        )))
    }

    async fn webagents(&self, o: &Owner) -> PortResult<cv::WebAgentsView> {
        Ok(cv::WebAgentsView {
            agents: map::agents(&self.levels(&o.site.id).await?),
            ..Default::default()
        })
    }

    async fn trust(&self, o: &Owner) -> PortResult<cv::TrustView> {
        let now = Utc::now();
        let receipts = self
            .receipts(&o.site.id, 100)
            .await?
            .iter()
            .filter_map(|r| map::receipt_row(r, o.site.utc_offset_minutes, now))
            .collect();
        let key: Option<(String,)> = sqlx::query_as(
            "SELECT issuer FROM receipts WHERE site_id = ? ORDER BY issued_at DESC LIMIT 1",
        )
        .bind(&o.site.id)
        .fetch_optional(self.pool())
        .await?;
        Ok(cv::TrustView {
            gauntlet: self.gauntlet(o).await?,
            receipts,
            key_id: key.map(|(k,)| k).unwrap_or_else(|| "none yet".into()),
            ..Default::default()
        })
    }

    async fn quarantine(&self, o: &Owner) -> PortResult<cv::QuarantineView> {
        let rows: Vec<QuarantineSql> = sqlx::query_as(
            "SELECT q.item_id, q.title, q.reason, q.location, q.evidence, q.state, q.decision_id, d.state \
             FROM quarantine q LEFT JOIN decisions d ON d.decision_id = q.decision_id \
             WHERE q.site_id = ? AND q.state <> 'deleted' ORDER BY q.created_at, q.rowid",
        )
        .bind(&o.site.id)
        .fetch_all(self.pool())
        .await?;
        let items = rows
            .into_iter()
            .map(
                |(id, title, reason, location, evidence, state, decision, decision_state)| {
                    let evidence: Vec<Digest> = serde_json::from_str(&evidence)
                        .with_context(|| format!("quarantine {id}: evidence does not parse"))?;
                    let state = match (state.as_str(), decision_state.as_deref()) {
                        ("restored", _) => cv::QuarantineState::Restored,
                        ("held", Some("waiting")) => cv::QuarantineState::RestoreAsked,
                        ("held", _) => cv::QuarantineState::LeftBehind,
                        (other, _) => bail!("quarantine {id}: unknown state {other}"),
                    };
                    Ok(cv::QuarantineItem {
                        restore_url: format!("/sites/{}/quarantine/{id}/restore", o.site.id),
                        decision_href: decision
                            .map(|d| format!("/console/{}/decisions/{d}", o.site.id)),
                        evidence: evidence
                            .iter()
                            .map(|d| cv::Evidence {
                                label: format!("Evidence file {}", map::short(d)),
                                excerpt: None,
                                receipt: None,
                            })
                            .collect(),
                        id,
                        title,
                        plain: reason,
                        location,
                        state,
                    })
                },
            )
            .collect::<PortResult<_>>()?;
        Ok(cv::QuarantineView {
            items,
            ..Default::default()
        })
    }

    async fn raise(&self, _o: &Owner, card: agentrix_decisions::NewCard) -> PortResult<String> {
        Ok(agentrix_decisions::Decisions::new(&self.ctx)
            .raise(card)
            .await?
            .id)
    }

    async fn draft_mission(
        &self,
        o: &Owner,
        ask: &crate::missions::DraftAsk,
    ) -> PortResult<Option<crate::missions::RawDraft>> {
        // The Chief's dot pulses while it drafts, from the real call (07 §3.2).
        let hub = agentrix_kernel::presence::global();
        hub.on_chief(&o.site.id, Some("Drafting your mission"));
        let out = crate::missions::draft(&self.ctx, &o.site.id, ask).await;
        hub.on_chief(&o.site.id, None);
        out.map(Some)
    }

    async fn releases(&self, o: &Owner) -> PortResult<Vec<crate::port::ReleaseRow>> {
        let rows: Vec<(
            i64,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i64>,
        )> = sqlx::query_as(
            "SELECT r.seq, r.state, r.signed_at, r.decision_id, d.method, p.seq FROM releases r \
             LEFT JOIN decisions d ON d.decision_id = r.decision_id \
             LEFT JOIN site_pointer p ON p.site_id = r.site_id \
             WHERE r.site_id = ? ORDER BY r.seq DESC LIMIT 50",
        )
        .bind(&o.site.id)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(|(seq, state, signed_at, decision, method, live)| {
                Ok(crate::port::ReleaseRow {
                    seq,
                    state,
                    live: live == Some(seq),
                    signed_at: signed_at.as_deref().map(time).transpose()?,
                    method,
                    decision,
                })
            })
            .collect()
    }

    async fn roster(&self, o: &Owner) -> PortResult<Option<agentrix_kernel::Roster>> {
        agentrix_kernel::Rosters::new(self.ctx.pool.clone(), self.ctx.blobs.clone())
            .roster(&o.site.id)
            .await
    }

    /// Spend from W8's metric catalogue (agentrix_store::metrics), so the console, the operator
    /// console and the home page count the same calls the same way.
    async fn meter(&self, o: &Owner) -> PortResult<Option<cv::Meter>> {
        use agentrix_store::metrics::Metrics as _;
        use agentrix_store::time::Window;
        let m = agentrix_store::metrics::SqliteMetrics::from_env(self.ctx.pool.clone());
        let site =
            agentrix_store::metrics::Scope::Site(agentrix_types::SiteId::new(o.site.id.clone()));
        let ever = m.spend(&site, Window::All).await?;
        if ever.calls == 0 && ever.refused == 0 {
            return Ok(None);
        }
        let today = m.spend(&site, Window::Today).await?;
        let month = m.spend(&site, Window::ThisMonth).await?;
        // The pill counts against the limit that binds first, in the gateway's own UTC day.
        let (spent, limit) = today
            .day_limit()
            .map(|l| (l.spent, l.limit))
            .unwrap_or((today.cost, agentrix_store::words::MicroUsd(5_000_000)));
        let pct = if limit.0 <= 0 {
            100
        } else {
            u32::try_from(
                ((spent.0.max(0) as i128 * 100 + limit.0 as i128 - 1) / limit.0 as i128).min(100),
            )
            .unwrap_or(100)
        };
        let mut month_words = format!(
            "{} this month across {} call{}",
            month.cost.usd(),
            month.calls,
            if month.calls == 1 { "" } else { "s" }
        );
        if month.replayed > 0 {
            month_words.push_str(&format!(", {} of them replayed at no cost", month.replayed));
        }
        let line = |l: &agentrix_store::metrics::SpendLine| cv::MeterLine {
            label: l
                .label
                .clone()
                .unwrap_or_else(|| map::purpose_words(&l.key)),
            amount: l.cost.usd(),
        };
        Ok(Some(cv::Meter {
            today_words: format!("{} of your {} daily limit", spent.usd(), limit.usd()),
            today_pct: pct,
            month_words,
            by_purpose: today
                .by_purpose
                .iter()
                .filter(|l| l.cost.0 > 0)
                .map(line)
                .collect(),
            by_provider: today
                .by_provider
                .iter()
                .filter(|l| l.cost.0 > 0)
                .map(line)
                .collect(),
            refused_words: (today.refused > 0).then(|| {
                format!(
                    "{} call{} stopped by the budget today",
                    today.refused,
                    if today.refused == 1 { "" } else { "s" }
                )
            }),
        }))
    }

    async fn gauntlet(&self, o: &Owner) -> PortResult<Option<cv::Gauntlet>> {
        let row: Option<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT receipt, started_at, ended_at, detail FROM gauntlet_runs WHERE site_id = ? AND receipt IS NOT NULL ORDER BY started_at DESC, rowid DESC LIMIT 1",
        )
        .bind(&o.site.id)
        .fetch_optional(self.pool())
        .await?;
        let Some((receipt, started_at, ended_at, detail)) = row else {
            return Ok(None);
        };
        let attacks: Vec<GauntletAttack> = match detail {
            Some(d) => {
                serde_json::from_str(&d).context("a stored gauntlet run's detail does not parse")?
            }
            None => Vec::new(),
        };
        Ok(Some(map::gauntlet(
            time(ended_at.as_deref().unwrap_or(&started_at))?,
            &digest(&receipt)?,
            &attacks,
            o.site.utc_offset_minutes,
            Utc::now(),
        )))
    }

    async fn work(&self, o: &Owner) -> PortResult<Vec<WorkCard>> {
        let row: Option<(String, String)> = sqlx::query_as("SELECT move_id, state FROM moves WHERE site_id = ? ORDER BY created_at DESC, rowid DESC LIMIT 1")
            .bind(&o.site.id)
            .fetch_optional(self.pool())
            .await?;
        let Some((move_id, state)) = row else {
            return Ok(Vec::new());
        };
        let (state, words) = match state.as_str() {
            "awaiting-owner" => ("waiting", "Waiting for your go-live decision"),
            "live" | "rolled-back" | "failed" | "stopped" => return Ok(Vec::new()),
            _ => ("working", "Moving your site"),
        };
        // Each step's latest transition (SQLite returns the other columns from the MAX row).
        let latest: Vec<(String, String, String, i64)> = sqlx::query_as(
            "SELECT step, state, summary, MAX(seq) FROM move_steps WHERE move_id = ? GROUP BY step",
        )
        .bind(&move_id)
        .fetch_all(self.pool())
        .await?;
        const STEPS: [(&str, &str); 7] = [
            ("assess", "Assess"),
            ("claim", "Claim"),
            ("replay", "Replay"),
            ("clean", "Clean"),
            ("rebuild", "Rebuild"),
            ("prove", "Prove"),
            ("golive", "Go live"),
        ];
        let steps: Vec<Step> = STEPS
            .iter()
            .map(|(key, name)| match latest.iter().find(|(s, ..)| s == key) {
                Some((_, s, summary, _)) => Step {
                    text: if summary.is_empty() {
                        (*name).to_owned()
                    } else {
                        format!("{name}: {summary}")
                    },
                    state: match s.as_str() {
                        "done" => "done",
                        "running" | "waiting" => "now",
                        _ => "todo",
                    }
                    .to_owned(),
                },
                None => Step {
                    text: (*name).to_owned(),
                    state: "todo".to_owned(),
                },
            })
            .collect();
        let current = latest
            .iter()
            .find(|(_, s, ..)| s == "running")
            .map(|(_, _, summary, _)| summary.clone());
        Ok(vec![WorkCard {
            who: map::mark(
                "mover",
                if state == "waiting" {
                    cv::MarkState::Waiting
                } else {
                    cv::MarkState::Working
                },
                words,
            ),
            title: format!("The Move · {}", o.site.host),
            state_words: current
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| words.to_owned()),
            state: state.to_owned(),
            steps,
            href: "/console/move".to_owned(),
        }])
    }

    async fn happened(
        &self,
        o: &Owner,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> PortResult<Vec<Happened>> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT digest, body FROM receipts WHERE site_id = ? AND ended_at >= ? AND ended_at < ? ORDER BY ended_at, rowid LIMIT 500",
        )
        .bind(&o.site.id)
        .bind(stored(from))
        .bind(stored(to))
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(|(d, body)| {
                let r: Receipt =
                    serde_json::from_str(&body).context("a stored receipt does not parse")?;
                Ok(Happened {
                    at: r.body.ended_at,
                    title: map::receipt_title(&r),
                    kind: r.body.kind.as_str().to_owned(),
                    digest: d,
                    verified: false,
                    failed: map::failed(&r),
                })
            })
            .collect()
    }

    async fn spend_since(&self, o: &Owner, since: DateTime<Utc>) -> PortResult<Option<String>> {
        use agentrix_store::metrics::Metrics as _;
        let m = agentrix_store::metrics::SqliteMetrics::from_env(self.ctx.pool.clone());
        let site =
            agentrix_store::metrics::Scope::Site(agentrix_types::SiteId::new(o.site.id.clone()));
        let s = m
            .spend(&site, agentrix_store::time::Window::Since(since))
            .await?;
        Ok((s.calls > 0 || s.refused > 0).then(|| s.cost.usd()))
    }

    async fn passkey_options(&self, o: &Owner, fingerprint: &Digest) -> PortResult<Option<String>> {
        let options = self
            .ctx
            .approvals
            .passkey_options(Self::account(o)?, *fingerprint.as_bytes())
            .await?;
        Ok(options.map(|v| v.to_string()))
    }

    async fn verify(
        &self,
        o: &Owner,
        fingerprint: &Digest,
        attempt: Attempt,
    ) -> Result<Proof, String> {
        let account = Self::account(o).map_err(|_| "Sign in again, then approve.".to_owned())?;
        let request = match attempt {
            Attempt::Passkey(credential) => json!({ "passkey": credential }),
            Attempt::Totp(code) => json!({ "totp": code }),
        };
        let proof = self
            .ctx
            .approvals
            .verify(account, *fingerprint.as_bytes(), request)
            .await
            .map_err(|e| e.to_string())?;
        let method = if proof.get("method").and_then(Value::as_str) == Some("passkey") {
            ApprovalMethod::Passkey
        } else {
            ApprovalMethod::StepUp
        };
        Ok(Proof {
            method,
            evidence: proof,
        })
    }

    async fn issue(&self, body: ReceiptBody, title: String) -> PortResult<Digest> {
        digest(&self.ctx.receipts.issue(body.with_title(title)).await?)
    }
}

fn v2_risk(r: &agentrix_decisions::Risk) -> (cv::Risk, String) {
    let (risk, words) = match r.level {
        agentrix_decisions::RiskLevel::Low => (cv::Risk::Low, "Low risk"),
        agentrix_decisions::RiskLevel::Medium => (cv::Risk::Medium, "Medium risk"),
        agentrix_decisions::RiskLevel::High => (cv::Risk::High, "High risk"),
    };
    (risk, words.to_owned())
}

fn v2_state(s: agentrix_decisions::CardState) -> cv::CardState {
    match s {
        agentrix_decisions::CardState::Waiting => cv::CardState::Waiting,
        agentrix_decisions::CardState::Approved => cv::CardState::Approved,
        agentrix_decisions::CardState::Declined => cv::CardState::Declined,
        agentrix_decisions::CardState::Expired => cv::CardState::Expired,
    }
}

fn v2_outcome(r: &agentrix_decisions::CardRow) -> String {
    match (r.state, r.method.as_deref()) {
        (agentrix_decisions::CardState::Approved, Some("passkey")) => {
            "Approved with your passkey".into()
        }
        (agentrix_decisions::CardState::Approved, Some("totp")) => {
            "Approved with a code from your authenticator app".into()
        }
        (agentrix_decisions::CardState::Approved, Some("spoken")) => "Allowed once by voice".into(),
        (agentrix_decisions::CardState::Approved, Some("chat")) => "Allowed once by message".into(),
        (agentrix_decisions::CardState::Approved, _) => "Approved".into(),
        (agentrix_decisions::CardState::Declined, _) => "Declined; nothing changed".into(),
        (agentrix_decisions::CardState::Expired, _) => {
            "Expired unanswered; nothing was done".into()
        }
        (agentrix_decisions::CardState::Waiting, _) => String::new(),
    }
}

/// A v2 card in the lane and the list.
fn v2_row(r: &agentrix_decisions::CardRow, site: &str) -> CardRow {
    let (risk, words) = v2_risk(&r.card.risk);
    let waiting = r.state == agentrix_decisions::CardState::Waiting;
    let id = &r.card.id;
    CardRow {
        id: id.clone(),
        kind: r.card.kind.clone(),
        title: r.card.title.clone(),
        summary: r.card.summary.clone(),
        asked_by: map::mark(
            &r.card.raised_by.webagent,
            if waiting {
                cv::MarkState::Waiting
            } else {
                cv::MarkState::Idle
            },
            if waiting { "Waiting for you" } else { "" },
        ),
        risk,
        risk_words: words,
        created_at: r.card.created_at,
        expires_at: r.card.expires_at(),
        state: v2_state(r.state),
        outcome_words: v2_outcome(r),
        href: format!("/console/{site}/decisions/{id}"),
        approve_url: format!("/decisions/v2/{id}/approve"),
        decline_url: format!("/decisions/v2/{id}/decline"),
    }
}

/// A v2 card, opened, in the view the decision page renders.
fn v2_card(
    r: &agentrix_decisions::CardRow,
    options: Option<String>,
    has_passkey: bool,
    offset: i32,
    now: DateTime<Utc>,
) -> cv::Card {
    let (risk, risk_words) = v2_risk(&r.card.risk);
    let waiting = r.state == agentrix_decisions::CardState::Waiting;
    let id = &r.card.id;
    let mut fields = vec![
        cv::Fact {
            label: "WebAgent".into(),
            value: r.card.effect.webagent.clone(),
        },
        cv::Fact {
            label: "Tool".into(),
            value: r.card.effect.tool.clone(),
        },
    ];
    if let Some(obj) = r.card.effect.args.as_object() {
        for (k, v) in obj {
            fields.push(cv::Fact {
                label: k.clone(),
                value: v
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string()),
            });
        }
    }
    if let Some(m) = &r.card.effect.amount {
        fields.push(cv::Fact {
            label: "Amount".into(),
            value: format!("{} {}", m.currency, m.minor),
        });
    }
    fields.push(cv::Fact {
        label: "Expires".into(),
        value: map::when(r.card.effect.expiry, offset, now),
    });
    cv::Card {
        id: id.clone(),
        // The view's kind only distinguishes Brick 1's three cards; v2 cards render the same way.
        kind: cv::CardKind::OrderHeld,
        title: r.card.title.clone(),
        asked_by: map::mark(
            &r.card.raised_by.webagent,
            if waiting {
                cv::MarkState::Waiting
            } else {
                cv::MarkState::Idle
            },
            if waiting { "Waiting for you" } else { "" },
        ),
        risk,
        risk_words,
        why_words: if r.card.risk.reasons.is_empty() {
            "No reasons were recorded for this risk level.".into()
        } else {
            r.card.risk.reasons.join(". ")
        },
        effects: if r.card.effects.is_empty() {
            vec![r.card.title.clone()]
        } else {
            r.card.effects.clone()
        },
        facts: vec![cv::Fact {
            label: "Expires".into(),
            value: map::when(r.card.effect.expiry, offset, now),
        }],
        levels: Vec::new(),
        fingerprint: r.card.fingerprint.to_string(),
        fingerprint_short: map::short(&r.card.fingerprint),
        fields,
        state: v2_state(r.state),
        decided: match (&r.receipt, r.state) {
            (Some(receipt), s) if s != agentrix_decisions::CardState::Waiting => {
                Some(cv::Decided {
                    outcome_words: v2_outcome(r),
                    method_words: match r.method.as_deref() {
                        Some("passkey") => "Your passkey".into(),
                        Some("totp") => "A code from your authenticator app".into(),
                        Some("spoken") => "Your voice, allow once".into(),
                        Some("chat") => "A message, allow once".into(),
                        _ => "Not approved".into(),
                    },
                    at_words: r
                        .decided_at
                        .map(|a| map::when(a, offset, now))
                        .unwrap_or_else(|| "Not recorded".into()),
                    receipt: receipt.clone(),
                })
            }
            _ => None,
        },
        passkey_options: if waiting { options } else { None },
        has_passkey,
        approve_url: format!("/decisions/v2/{id}/approve"),
        decline_url: format!("/decisions/v2/{id}/decline"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_hosts() {
        assert!(is_sample("kilele-bakery-sample.agentrix.website"));
        assert!(!is_sample("kilelebakery.co.ke"));
        assert!(!is_sample("sample.example"));
    }
}
