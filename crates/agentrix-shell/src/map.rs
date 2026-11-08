//! Pure mappers from `control.db` rows and `agentrix-types` values to view models.
//!
//! Ported from Brick 1's `agentrix-node/src/pages/map.rs` (the console's pages move into the
//! shell at handover), so a page in the shell says exactly what Brick 1's page said. No I/O and
//! no clock: callers pass `now`.

use agentrix_console::view;
use agentrix_types::{
    ApprovalMethod, DecisionCard, Digest, GauntletAttack, JobLevel, Level, Receipt, ReceiptKind,
    Subject,
};
use chrono::{DateTime, FixedOffset, Utc};

/// A time in the owner's zone: "10:04" today, otherwise "Sat 7 Nov, 10:04".
pub fn when(at: DateTime<Utc>, offset_minutes: i32, now: DateTime<Utc>) -> String {
    let zone = FixedOffset::east_opt(offset_minutes * 60)
        .unwrap_or(FixedOffset::east_opt(0).expect("UTC"));
    let (at, now) = (at.with_timezone(&zone), now.with_timezone(&zone));
    if at.date_naive() == now.date_naive() {
        at.format("%H:%M").to_string()
    } else {
        at.format("%a %-d %b, %H:%M").to_string()
    }
}

/// "sha256:7c1e…a90b" shortened to "7c1e…a90b".
pub fn short(d: &Digest) -> String {
    let hex = d.hex();
    format!("{}…{}", &hex[..4], &hex[hex.len() - 4..])
}

/// A WebAgent's mark: its role colour and a two-letter monogram. W1's living-mark generator
/// replaces this when it lands (ADR-031).
pub fn mark(webagent: &str, state: view::MarkState, state_words: &str) -> view::AgentMark {
    let color = match webagent.to_ascii_lowercase().as_str() {
        "mover" => "#2f55c4",
        "guardian" => "#45597f",
        "editor" => "#7a4b00",
        "cashier" => "#11734a",
        "concierge" | "cakes" | "orders" => "#b72d50",
        "caretaker" => "#3d5a1f",
        "sales" => "#0f6b62",
        "reputation" => "#6b2f86",
        _ => "#38356d",
    };
    let mut glyph: String = webagent.chars().take(2).collect();
    if let Some(first) = glyph.get(..1) {
        glyph = format!("{}{}", first.to_uppercase(), &glyph[1..]);
    }
    view::AgentMark {
        name: display_name(webagent),
        glyph,
        color: color.to_owned(),
        state,
        state_words: state_words.to_owned(),
    }
}

/// "mover" → "Mover", "chief" → "Chief WebAgent".
pub fn display_name(webagent: &str) -> String {
    match webagent.to_ascii_lowercase().as_str() {
        "chief" | "chief webagent" => "Chief WebAgent".to_owned(),
        _ => {
            let mut c = webagent.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        }
    }
}

/// The owner's words for a receipt kind. Matches on the stored name, so a kind another
/// workstream adds shows its own name until it gets words here, and never breaks the build.
pub fn kind_words(kind: ReceiptKind) -> &'static str {
    match kind.as_str() {
        "job.fetch" => "Fetch job",
        "job.replay" => "Replay job",
        "job.render" => "Render job",
        "job.build" => "Build job",
        "move.step" => "Move step",
        "claim.verified" => "Domain proved",
        "decision.approved" => "Decision approved",
        "decision.declined" => "Decision declined",
        "release.signed" => "Release signed",
        "release.rollback" => "Rolled back",
        "quarantine.restored" => "Item restored",
        "concierge.refusal" => "The Concierge refused",
        "door.order-held" => "Order held for you",
        "gauntlet.run" => "Gauntlet run",
        "routing.changed" => "Model routing changed",
        "consent.given" => "Customer consent given",
        "consent.withdrawn" => "Customer consent withdrawn",
        "payment.started" => "Payment started",
        "payment.captured" => "Payment received",
        "payment.refunded" => "Payment refunded",
        "subscription.changed" => "Plan changed",
        "operator.access" => "Zavora accessed your site",
        "platform.hold" => "Platform hold",
        "account.code-sign-in" => "Signed in with an email code",
        "account.recovery-hold" => "Account recovery started",
        other => other,
    }
}

/// A receipt's title: its `detail.title` when the writer gave one, else what it is about.
pub fn receipt_title(r: &Receipt) -> String {
    if let Some(title) = r.body.title() {
        return title.to_owned();
    }
    let about = match &r.body.subject {
        Subject::Assessment(id) => format!("assessment {id}"),
        Subject::Move(id) => format!("Move {id}"),
        Subject::Job(id) => format!("job {id}"),
        Subject::Decision(id) => format!("decision {id}"),
        Subject::Site(id) => format!("site {id}"),
        #[allow(unreachable_patterns)]
        _ => "your account".to_owned(),
    };
    format!("{}: {about}", kind_words(r.body.kind))
}

/// Whether a receipt records something that went wrong: a failed Move step, or an outcome the
/// writer marked as failed.
pub fn failed(r: &Receipt) -> bool {
    let d = r.body.detail.as_ref();
    let state = d
        .and_then(|d| d.get("state").or_else(|| d.get("outcome")))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    matches!(state, "failed" | "error" | "stopped")
}

/// A receipt in a list. Signatures are checked on the receipt's own page, in the browser and on
/// the server, so the list doesn't claim it.
pub fn receipt_row(r: &Receipt, offset: i32, now: DateTime<Utc>) -> Option<view::ReceiptRow> {
    let digest = r.digest().ok()?;
    Some(view::ReceiptRow {
        digest: digest.to_string(),
        digest_short: short(&digest),
        title: receipt_title(r),
        kind_words: kind_words(r.body.kind).to_owned(),
        at_words: when(r.body.ended_at, offset, now),
        verified: false,
    })
}

/// Brick 1's decisions table states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionState {
    /// Waiting for the owner.
    Waiting,
    /// Approved.
    Approved,
    /// Declined.
    Declined,
    /// Expired unanswered.
    Expired,
}

/// A row of Brick 1's `decisions` table.
#[derive(Debug, Clone)]
pub struct DecisionRow {
    /// The card.
    pub card: DecisionCard,
    /// Its state.
    pub state: DecisionState,
    /// How it was approved.
    pub method: Option<ApprovalMethod>,
    /// When it was decided.
    pub decided_at: Option<DateTime<Utc>>,
    /// The decision's receipt.
    pub receipt: Option<Digest>,
}

/// Risk in the owner's words: never a bare score (01b §6.2 #23).
pub fn risk(card: &DecisionCard) -> (view::Risk, String) {
    match card.risk.as_ref().map(|r| r.score) {
        Some(s) if s >= 0.67 => (view::Risk::High, "High risk".into()),
        Some(s) if s >= 0.34 => (view::Risk::Medium, "Medium risk".into()),
        Some(_) => (view::Risk::Low, "Low risk".into()),
        None => (view::Risk::Medium, "Not scored".into()),
    }
}

/// The view's state.
pub fn card_state(s: DecisionState) -> view::CardState {
    match s {
        DecisionState::Waiting => view::CardState::Waiting,
        DecisionState::Approved => view::CardState::Approved,
        DecisionState::Declined => view::CardState::Declined,
        DecisionState::Expired => view::CardState::Expired,
    }
}

/// "Approved with your passkey".
pub fn outcome_words(row: &DecisionRow) -> String {
    match (row.state, row.method) {
        (DecisionState::Approved, Some(ApprovalMethod::Passkey)) => {
            "Approved with your passkey".into()
        }
        (DecisionState::Approved, Some(ApprovalMethod::StepUp)) => {
            "Approved with a code from your authenticator app".into()
        }
        (DecisionState::Approved, None) => "Approved".into(),
        (DecisionState::Declined, _) => "Declined; nothing changed".into(),
        (DecisionState::Expired, _) => "Expired unanswered; nothing was done".into(),
        (DecisionState::Waiting, _) => String::new(),
    }
}

/// The owner's words for a level.
pub fn level_name(l: Level) -> &'static str {
    match l {
        Level::Watch => "Watch",
        Level::Suggest => "Suggest",
        Level::AskFirst => "Ask me first",
        Level::DoAndTell => "Do it and tell me",
        Level::WithinBudget => "Handle it within budget",
        Level::UndoOnly => "Undo only",
    }
}

/// A job's dial with the positions above its ceiling locked (ADR-012 amendment 1).
pub fn job_levels(row: &JobLevel) -> view::JobLevels {
    let ladder: &[Level] = if row.level == Level::UndoOnly || row.ceiling == Level::UndoOnly {
        &[Level::Watch, Level::Suggest, Level::UndoOnly]
    } else {
        &[
            Level::Watch,
            Level::Suggest,
            Level::AskFirst,
            Level::DoAndTell,
            Level::WithinBudget,
        ]
    };
    let positions = ladder
        .iter()
        .map(|&l| {
            let locked = l > row.ceiling;
            view::Position {
                name: level_name(l).into(),
                current: l == row.level,
                locked,
                reason: if locked {
                    row.ceiling_reason.clone()
                } else {
                    String::new()
                },
            }
        })
        .collect();
    let level_words = match row.level {
        Level::Watch => "Watch: only reports what it sees".to_owned(),
        Level::Suggest => "Suggest: drafts changes for you to approve".to_owned(),
        Level::AskFirst => "Ask me first".to_owned(),
        Level::DoAndTell => "Does reversible work alone and tells you".to_owned(),
        Level::WithinBudget => "Handles reversible work alone, within its budget".to_owned(),
        Level::UndoOnly => {
            "Undo only: may roll back to the last signed release and tell you".to_owned()
        }
    };
    view::JobLevels {
        webagent: display_name(&row.webagent),
        job: row.job.clone(),
        level_words,
        above_ask_first: matches!(
            row.level,
            Level::DoAndTell | Level::WithinBudget | Level::UndoOnly
        ),
        positions,
    }
}

fn arg_fields(card: &DecisionCard, offset: i32, now: DateTime<Utc>) -> Vec<view::Fact> {
    let mut fields = vec![
        view::Fact {
            label: "WebAgent".into(),
            value: card.effect.webagent.clone(),
        },
        view::Fact {
            label: "Tool".into(),
            value: card.effect.tool.clone(),
        },
    ];
    if let Some(obj) = card.effect.args.as_object() {
        for (k, v) in obj {
            fields.push(view::Fact {
                label: k.clone(),
                value: v
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string()),
            });
        }
    }
    if let Some(m) = &card.effect.amount {
        fields.push(view::Fact {
            label: "Amount".into(),
            value: format!("{} {}", m.currency, m.minor),
        });
    }
    fields.push(view::Fact {
        label: "Expires".into(),
        value: when(card.effect.expiry, offset, now),
    });
    fields
}

/// One card, opened. `levels` show only on the go-live card (R7.5).
pub fn card(
    row: &DecisionRow,
    levels: &[JobLevel],
    options: Option<String>,
    has_passkey: bool,
    offset: i32,
    now: DateTime<Utc>,
) -> view::Card {
    let (risk, risk_words) = risk(&row.card);
    let waiting = row.state == DecisionState::Waiting;
    let id = row.card.decision_id.to_string();
    let mut effects: Vec<String> = row
        .card
        .body
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    if effects.is_empty() {
        effects.push(row.card.title.clone());
    }
    let mut facts = Vec::new();
    if let Some(r) = &row.card.risk {
        facts.push(view::Fact {
            label: "Risk check".into(),
            value: format!("{} ({})", r.summary, r.model),
        });
    }
    facts.push(view::Fact {
        label: "Expires".into(),
        value: when(row.card.effect.expiry, offset, now),
    });
    view::Card {
        id: id.clone(),
        kind: match row.card.kind {
            agentrix_types::CardKind::GoLive => view::CardKind::GoLive,
            agentrix_types::CardKind::QuarantineRestore => view::CardKind::QuarantineRestore,
            agentrix_types::CardKind::OrderHeld => view::CardKind::OrderHeld,
        },
        title: row.card.title.clone(),
        asked_by: mark(
            &row.card.effect.webagent,
            if waiting {
                view::MarkState::Waiting
            } else {
                view::MarkState::Idle
            },
            if waiting { "Waiting for you" } else { "" },
        ),
        risk,
        risk_words,
        why_words: row
            .card
            .risk
            .as_ref()
            .map(|r| r.summary.clone())
            .unwrap_or_else(|| {
                "This card hasn't been risk-scored, so it's treated as medium.".into()
            }),
        effects,
        facts,
        levels: if row.card.kind == agentrix_types::CardKind::GoLive {
            levels.iter().map(job_levels).collect()
        } else {
            Vec::new()
        },
        fingerprint: row.card.fingerprint.to_string(),
        fingerprint_short: short(&row.card.fingerprint),
        fields: arg_fields(&row.card, offset, now),
        state: card_state(row.state),
        decided: match (row.state, &row.receipt) {
            (DecisionState::Waiting, _) | (_, None) => None,
            (_, Some(receipt)) => Some(view::Decided {
                outcome_words: outcome_words(row),
                method_words: match row.method {
                    Some(ApprovalMethod::Passkey) => "Your passkey".into(),
                    Some(ApprovalMethod::StepUp) => "A code from your authenticator app".into(),
                    None => "Not approved".into(),
                },
                at_words: row
                    .decided_at
                    .map(|a| when(a, offset, now))
                    .unwrap_or_else(|| "Not recorded".into()),
                receipt: receipt.to_string(),
            }),
        },
        passkey_options: if waiting { options } else { None },
        has_passkey,
        approve_url: format!("/decisions/{id}/approve"),
        decline_url: format!("/decisions/{id}/decline"),
    }
}

/// What each Model Gateway purpose is for, in the owner's words.
pub fn purpose_words(purpose: &str) -> String {
    match purpose {
        "build" => "Building pages",
        "vision-parity" => "Checking pages look right",
        "screen" => "Screening for hidden instructions",
        "decide" => "Risk decisions",
        "concierge" => "Answering visitors (Ask)",
        "brief" => "Your daily brief",
        "embed" => "Search index",
        "rerank" => "Search ranking",
        "router" => "Choosing models",
        other => other,
    }
    .to_owned()
}

fn provider_words(provider: &str) -> String {
    match provider {
        "anthropic" => "Anthropic".to_owned(),
        "vultr" => "Vultr".to_owned(),
        "recording" | "replay" => "Replayed (no cost)".to_owned(),
        other => other.to_owned(),
    }
}

/// "$0.42", or "under $0.01" for a positive amount below a cent.
pub fn dollars(micro_usd: i64) -> String {
    let usd = micro_usd as f64 / 1e6;
    if micro_usd > 0 && usd < 0.01 {
        "under $0.01".to_owned()
    } else {
        format!("${usd:.2}")
    }
}

/// Spend over a window, in micro-USD.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Spend {
    /// Total estimated cost.
    pub micro_usd: i64,
    /// Calls that reached a provider or a recording.
    pub calls: i64,
    /// Calls answered from recordings, which cost nothing.
    pub replayed: i64,
    /// Calls the budget refused.
    pub refused: i64,
    /// Cost by purpose, most expensive first.
    pub by_purpose: Vec<(String, i64)>,
    /// Cost by provider, most expensive first.
    pub by_provider: Vec<(String, i64)>,
}

/// The model-spend meter (R11.3).
pub fn meter(today: &Spend, month: &Spend, daily_limit_micro_usd: i64) -> view::Meter {
    let limit = daily_limit_micro_usd.max(0);
    let pct = if limit == 0 {
        100
    } else {
        ((today.micro_usd.max(0) as f64 / limit as f64) * 100.0)
            .ceil()
            .min(100.0) as u32
    };
    let mut month_words = format!(
        "{} this month across {} call{}",
        dollars(month.micro_usd),
        month.calls,
        if month.calls == 1 { "" } else { "s" }
    );
    if month.replayed > 0 {
        month_words.push_str(&format!(", {} of them replayed at no cost", month.replayed));
    }
    view::Meter {
        today_words: format!(
            "{} of your {} daily limit",
            dollars(today.micro_usd),
            dollars(limit)
        ),
        today_pct: pct,
        month_words,
        by_purpose: today
            .by_purpose
            .iter()
            .filter(|(_, c)| *c > 0)
            .map(|(p, c)| view::MeterLine {
                label: purpose_words(p),
                amount: dollars(*c),
            })
            .collect(),
        by_provider: today
            .by_provider
            .iter()
            .filter(|(_, c)| *c > 0)
            .map(|(p, c)| view::MeterLine {
                label: provider_words(p),
                amount: dollars(*c),
            })
            .collect(),
        refused_words: (today.refused > 0).then(|| {
            format!(
                "{} call{} stopped by the budget today",
                today.refused,
                if today.refused == 1 { "" } else { "s" }
            )
        }),
    }
}

/// The brief: its headline first, then each line linked to the receipt it rests on.
pub fn brief_view(
    written_at: DateTime<Utc>,
    b: &agentrix_types::Brief,
    offset: i32,
    now: DateTime<Utc>,
) -> view::Brief {
    let mut cited: Vec<&Digest> = b.lines.iter().filter_map(|l| l.receipt.as_ref()).collect();
    cited.sort_by_key(|d| d.to_string());
    cited.dedup();
    let headline = b.headline.iter().map(|h| view::BriefLine {
        text: h.clone(),
        href: None,
    });
    let lines = b.lines.iter().map(|l| view::BriefLine {
        text: l.text.clone(),
        href: l.receipt.as_ref().map(|d| format!("/receipts/{d}")),
    });
    view::Brief {
        written_words: when(written_at, offset, now),
        events_count: u32::try_from(cited.len()).unwrap_or(u32::MAX),
        lines: headline.chain(lines).collect(),
        spend_words: b.spend_words.clone(),
    }
}

/// The gauntlet score, from receipts only (R13.2).
pub fn gauntlet(
    run_at: DateTime<Utc>,
    receipt: &Digest,
    attacks: &[GauntletAttack],
    offset: i32,
    now: DateTime<Utc>,
) -> view::Gauntlet {
    let held = attacks.iter().filter(|a| a.held).count();
    let of = attacks.len();
    view::Gauntlet {
        held: u32::try_from(held).unwrap_or(u32::MAX),
        of: u32::try_from(of).unwrap_or(u32::MAX),
        score_words: format!("{held} / {of}"),
        run_words: format!("at {}", when(run_at, offset, now)),
        receipt: receipt.to_string(),
        attacks: attacks
            .iter()
            .map(|a| view::Attack {
                name: a.name.clone(),
                plain: a.plain.clone(),
                held: a.held,
                receipt: a.receipt.as_ref().map(Digest::to_string),
            })
            .collect(),
    }
}

/// One card per WebAgent, its jobs in charter order.
pub fn agents(levels: &[JobLevel]) -> Vec<view::AgentCard> {
    let mut agents: Vec<view::AgentCard> = Vec::new();
    for row in levels {
        let name = display_name(&row.webagent);
        match agents.iter_mut().find(|a| a.name == name) {
            Some(a) => a.jobs.push(job_levels(row)),
            None => agents.push(view::AgentCard {
                mark: mark(&row.webagent, view::MarkState::Idle, ""),
                name,
                role_words: row.role.clone(),
                jobs: vec![job_levels(row)],
            }),
        }
    }
    agents
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn marks_and_names() {
        let m = mark("mover", view::MarkState::Waiting, "Waiting for you");
        assert_eq!(m.name, "Mover");
        assert_eq!(m.glyph, "Mo");
        assert_eq!(display_name("chief"), "Chief WebAgent");
    }

    #[test]
    fn money_and_times() {
        assert_eq!(dollars(420_000), "$0.42");
        assert_eq!(dollars(4_000), "under $0.01");
        assert_eq!(dollars(0), "$0.00");
        let now = Utc.with_ymd_and_hms(2026, 11, 8, 6, 0, 0).unwrap();
        assert_eq!(when(now, 180, now), "09:00");
        assert_eq!(
            when(now - chrono::Duration::days(1), 180, now),
            "Sat 7 Nov, 09:00"
        );
    }

    #[test]
    fn meter_caps_at_the_limit() {
        let today = Spend {
            micro_usd: 7_000_000,
            calls: 3,
            ..Default::default()
        };
        let m = meter(&today, &today, 5_000_000);
        assert_eq!(m.today_pct, 100);
        assert_eq!(m.today_words, "$7.00 of your $5.00 daily limit");
    }

    #[test]
    fn ladders_lock_above_the_ceiling() {
        let row = JobLevel {
            webagent: "mover".into(),
            role: "Moves your site".into(),
            job: "go-live".into(),
            level: Level::AskFirst,
            ceiling: Level::AskFirst,
            ceiling_reason: "Going live is public".into(),
        };
        let j = job_levels(&row);
        assert_eq!(j.positions.len(), 5);
        assert!(j.positions[2].current);
        assert!(j.positions[3].locked && j.positions[3].reason == "Going live is public");
        assert!(!j.above_ask_first);
    }
}
