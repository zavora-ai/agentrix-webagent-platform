//! View models for the shell's templates, with display-ready strings.
//!
//! Every page carries [`Chrome`]: the status bar, dock, lane and command bar of
//! `concept/console/shell.html`. Templates never format numbers or dates; the header comment of
//! each template lists what it reads. Field names are the template contract.
#![allow(missing_docs)]

use agentrix_console::view as cv;
use serde::Serialize;

use crate::port::{SiteInfo, WorkCard};
use crate::schedule::TimePill;

/// What every shell page carries.
#[derive(Debug, Clone, Serialize)]
pub struct Chrome {
    pub site: SiteInfo,
    /// Front of house conversations with a turn in the last 10 minutes.
    pub live_now: u32,
    /// The site's monogram for the site chip and the team mark ("K").
    pub monogram: String,
    /// `/console/{site}`.
    pub base: String,
    pub owner_name: String,
    /// The page the dock and the tabs mark as current: `home`, `decisions`, `webagents`,
    /// `trust`, `quarantine`, `settings`, or `app:{id}`.
    pub active: String,
    /// `back` or `front`.
    pub world: String,
    pub time: TimePill,
    /// Night Time now, by the schedule: the kit puts `data-time="night"` on `<html>`.
    pub night: bool,
    /// The quiet hours for kit.js, which keeps Night Time right while the page stays open.
    /// `None` when the site's offset has no IANA name (not a whole hour).
    pub quiet_hours: Option<QuietHoursAttr>,
    /// AI spend today against the daily limit, once any model call was made.
    pub budget: Option<Budget>,
    pub trust: TrustPill,
    /// The Chief WebAgent's team mark (W1's `TeamMark`) for the kit's `ui.team_mark`, or `None`
    /// for the monogram.
    pub team: Option<serde_json::Value>,
    /// W1's presence stream, which moves the living marks (marks.js connects to it).
    pub presence_url: String,
    /// Whether Freeze is offered: only once the paths it stops check it.
    pub freeze_enabled: bool,
    /// The Freeze in force, if any.
    pub freeze: Option<FreezeView>,
    /// The WebAgents, then the system apps, then the apps other workstreams registered.
    pub dock: Vec<DockItem>,
    pub system: Vec<DockItem>,
    pub apps: Vec<DockItem>,
    pub lane: Lane,
    pub has_passkey: bool,
    /// What the command bar can open (pages, apps and waiting decisions), as JSON that is safe
    /// inside a `<script type="application/json">` (no `<`, `>` or `&`).
    pub commands_json: String,
    /// Today's AI spend by job, for the Activity monitor.
    pub activity: Vec<cv::MeterLine>,
}

/// Something the command bar can open.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Command {
    pub label: String,
    /// "Page", "Decision", "App".
    pub kind: String,
    pub href: String,
}

/// `<html data-night-from data-night-to data-tz>` for kit.js.
#[derive(Debug, Clone, Serialize)]
pub struct QuietHoursAttr {
    pub from: String,
    pub to: String,
    /// An IANA name for the site's fixed offset, such as `Etc/GMT-3` for UTC+3.
    pub tz: String,
}

/// The budget pill.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Budget {
    /// "$0.22".
    pub amount: String,
    /// "$0.22 of your $5.00 daily limit".
    pub words: String,
    /// 0–100.
    pub pct: u32,
}

/// The trust pill: protections and the count of waiting decisions.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TrustPill {
    /// "Protections on", "8 of 10 held", or "Trust centre" before the first gauntlet run.
    pub words: String,
    /// Every attack of the last run held.
    pub ok: bool,
    pub waiting: u32,
}

/// The Freeze in force.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FreezeView {
    /// "Frozen at 01:12 by you".
    pub words: String,
}

/// An icon in the dock or the phone's app grid.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DockItem {
    pub id: String,
    /// The label under the icon.
    pub name: String,
    /// The tooltip: "Mover: moving your site".
    pub title: String,
    pub href: String,
    /// A WebAgent's letter mark (charter fallback), or `None`.
    pub mark: Option<cv::AgentMark>,
    /// A WebAgent's living mark for the kit's `ui.mark`: `{id, name, spec, state, owner}`.
    pub agent: Option<serde_json::Value>,
    /// The system icon's name (`store`, `activity`, `memory`, `trust`, `decisions`,
    /// `settings`, `grid`) when there is no mark.
    pub icon: String,
    pub badge: u32,
    pub current: bool,
    /// Opens this overlay instead of following `href` when scripts run.
    pub opens: Option<String>,
}

/// The lane: what needs the owner.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Lane {
    pub needs_you: Vec<LaneCard>,
    /// Items other crates own (C9 lane sources), such as support-access requests.
    pub others: Vec<crate::apps::LaneItem>,
    /// One line of what was done without the owner: "Mover read 41 pages · Guardian held 10 of 10".
    pub done_words: Vec<String>,
}

/// A decision card in the lane or the list.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LaneCard {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub asked_by: cv::AgentMark,
    /// "Mover asks".
    pub from_words: String,
    /// `ok`, `wait` or `risk`, for the chip class.
    pub risk_class: String,
    /// "Low risk", "Medium", "High risk", from Brick 1's words.
    pub risk_words: String,
    /// "Raised 06:40".
    pub created_words: String,
    /// "Waits until Sun 8 Nov 19:00, then expires; nothing happens if you don't answer".
    pub expires_words: String,
    pub href: String,
    /// Where the approval sheet reads its fingerprint and a fresh passkey ceremony.
    pub sheet_url: String,
    pub decline_url: String,
    /// For decided cards: "Approved with your passkey".
    pub outcome_words: String,
}

/// A line of the night's timeline or of "while you were away".
#[derive(Debug, Clone, Default, Serialize)]
pub struct TimelineItem {
    /// "22:14".
    pub t: String,
    pub title: String,
    /// "Done", "Failed", "You decided", "Raised for you".
    pub tag: String,
    /// `ok`, `warn` or empty, for the pin.
    pub class: String,
    pub href: String,
}

/// "While you were away", since the owner's last visit.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Away {
    /// "Since 22:04".
    pub since_words: String,
    /// "$0.62" or "Nothing".
    pub spent_words: String,
    pub waiting: u32,
    pub items: Vec<TimelineItem>,
    /// How many more happened than the list shows.
    pub more: u32,
}

/// Night Time: the last quiet window.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Night {
    /// "Quiet hours 22:00–06:00 · last night, as recorded".
    pub sub_words: String,
    pub items: Vec<TimelineItem>,
    /// "AI spend overnight: $0.09", when anything was spent.
    pub spend_words: Option<String>,
    /// Decisions that wait for the morning.
    pub waiting: u32,
}

/// Front of house: what visitors and AI agents left for the owner.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Front {
    pub passed: Vec<cv::PassedQuestion>,
    pub passed_total: u32,
    /// Orders the agent door held for the owner.
    pub held: Vec<LaneCard>,
}

/// The state line in the field head.
#[derive(Debug, Clone, Default, Serialize)]
pub struct StateLine {
    /// `working`, `waiting`, `live` or empty.
    pub class: String,
    pub words: String,
}

/// `GET /console/{site}`: Back office (Day Time), Night Time and Front of house.
#[derive(Debug, Clone, Serialize)]
pub struct HomeView {
    #[serde(flatten)]
    pub chrome: Chrome,
    pub greeting: String,
    /// "Sunday 8 November · 2 decisions waiting".
    pub sub_words: String,
    pub state_line: StateLine,
    pub brief: Option<cv::Brief>,
    pub first_brief_words: String,
    /// The first waiting card, offered on the brief.
    pub first_card: Option<LaneCard>,
    pub work: Vec<WorkCard>,
    pub passed: Vec<cv::PassedQuestion>,
    pub passed_total: u32,
    pub meter: Option<cv::Meter>,
    pub gauntlet: Option<cv::Gauntlet>,
    /// Missions running now, with their progress.
    pub missions: Vec<MissionRow>,
    pub away: Option<Away>,
    pub night: Night,
    pub front: Front,
    pub recent: Vec<cv::ReceiptRow>,
}

/// `GET /console/{site}/decisions`.
#[derive(Debug, Clone, Serialize)]
pub struct DecisionsView {
    #[serde(flatten)]
    pub chrome: Chrome,
    pub waiting: Vec<LaneCard>,
    pub decided: Vec<LaneCard>,
    pub expired: Vec<LaneCard>,
}

/// `GET /console/{site}/decisions/{id}`.
#[derive(Debug, Clone, Serialize)]
pub struct DecisionView {
    #[serde(flatten)]
    pub chrome: Chrome,
    pub card: cv::Card,
    /// The row with the words the lane uses (expiry, raised by).
    pub row: LaneCard,
}

/// `GET /console/{site}/webagents`.
#[derive(Debug, Clone, Serialize)]
pub struct WebAgentsView {
    #[serde(flatten)]
    pub chrome: Chrome,
    /// The charter's WebAgents, when the kernel has no roster for the site.
    pub agents: Vec<cv::AgentCard>,
    /// W1's roster: who is on duty.
    pub members: Vec<MemberView>,
    /// Who the owner could add from the Store.
    pub available: Vec<MemberView>,
}

/// One WebAgent from W1's roster, as the WebAgents page shows it.
#[derive(Debug, Clone, Serialize)]
pub struct MemberView {
    pub id: String,
    /// Whether it works for the site now: its dials move only then.
    pub on_duty: bool,
    pub name: String,
    pub role_words: String,
    pub mandate: String,
    pub never: Vec<String>,
    /// "On duty", "In the Store", "Handed over".
    pub status_words: String,
    /// For the kit's `ui.mark`: `{id, name, spec, state, owner}`.
    pub agent: serde_json::Value,
    /// `ladder`, `fixed-rules` or `undo-only`.
    pub dial: String,
    pub jobs: Vec<JobView>,
    /// "Every morning before 07:00".
    pub wakes: Vec<String>,
    /// Percent of the site's monthly AI budget.
    pub share_percent: u8,
    /// Its cards waiting for the owner.
    pub asks: u32,
}

/// One job and its dial.
#[derive(Debug, Clone, Serialize)]
pub struct JobView {
    /// The job's id, for W1's dial routes: `freshness`.
    pub id: String,
    pub name: String,
    pub note: String,
    /// The kit's ladder keys: `watch`, `suggest`, `ask`, `tell`, `budget`; `undo` for undo only.
    pub current: String,
    pub ceiling: String,
    pub ceiling_reason: String,
    /// The owner's words for where the dial is.
    pub level_words: String,
    pub can: Vec<CanView>,
}

/// `GET /console/{site}/webagents/{id}`: one WebAgent, its dials and its Appearance (01b C11, C13).
#[derive(Debug, Clone, Serialize)]
pub struct WebAgentView {
    #[serde(flatten)]
    pub chrome: Chrome,
    pub member: MemberView,
    /// Whether it works for the site now (its dials move only then).
    pub on_duty: bool,
}

/// `GET /console/{site}/memory`: memory and rules (adk-surface WebTrust; trust.html's memory).
#[derive(Debug, Clone, Serialize)]
pub struct MemoryView {
    #[serde(flatten)]
    pub chrome: Chrome,
    /// Who is on duty and how far each job may go alone.
    pub members: Vec<MemberView>,
    /// The newest receipts, and how many there are.
    pub recent: Vec<cv::ReceiptRow>,
    pub receipts_total: u32,
}

/// `GET /console/{site}/gestures` (01b C42): the page carries only the shell; the island acts on
/// the lane and Freeze the chrome already holds.
#[derive(Debug, Clone, Serialize)]
pub struct GesturesView {
    #[serde(flatten)]
    pub chrome: Chrome,
}

/// A mission as the list, the report and Back office show it.
#[derive(Debug, Clone, Serialize)]
pub struct MissionRow {
    pub id: String,
    pub goal: String,
    /// `proposed`, `active`, `paused`, `ended`.
    pub state: String,
    /// "Waiting for your approval", "Running", "Paused", "Ended".
    pub state_words: String,
    /// The contract's "Counts" line.
    pub measure_words: String,
    /// The count so far, when the measure has a source.
    pub count: Option<i64>,
    pub target: i64,
    /// 0–100, for the meter.
    pub pct: i64,
    /// "On track for about 52 by Fri 18 Dec at this pace", or why it's too early to say.
    pub pace_words: String,
    pub members: Vec<String>,
    pub lead: String,
    pub ends_words: String,
    /// "3 weeks left", "Ends today", "Ended".
    pub left_words: String,
    /// "Since Sat 7 Nov" once started.
    pub since_words: String,
    pub href: String,
    /// The contract card while it waits for approval.
    pub card_href: Option<String>,
}

/// `GET /console/{site}/missions`.
#[derive(Debug, Clone, Serialize)]
pub struct MissionsView {
    #[serde(flatten)]
    pub chrome: Chrome,
    pub missions: Vec<MissionRow>,
    /// Who can work on a mission: the WebAgents on duty (id, name).
    pub workers: Vec<(String, String)>,
    /// Whether this site's orders are counted (W2's records hold them).
    pub counts_orders: bool,
    /// The default end date for the form, `YYYY-MM-DD`.
    pub default_end: String,
    /// The contract form's values: the defaults, or what the Chief WebAgent drafted.
    pub contract: ContractForm,
    /// After "Draft the contract": where the values came from, for the owner to check them.
    pub checking: Option<DraftNote>,
}

/// The contract form's values.
#[derive(Debug, Clone, Serialize)]
pub struct ContractForm {
    /// The owner's own words, carried to the card; empty when they filled the form in directly.
    pub said: String,
    /// Whether the Chief WebAgent drafted these values from `said`.
    pub drafted: bool,
    /// The goal.
    pub goal: String,
    /// `orders`, `questions` or `none`.
    pub measure: String,
    /// How many, when something counts.
    pub target: Option<i64>,
    /// Who's ticked (ids).
    pub members: Vec<String>,
    /// `YYYY-MM-DD`.
    pub ends: String,
}

/// Where a drafted contract came from.
#[derive(Debug, Clone, Serialize)]
pub struct DraftNote {
    /// Whether the Chief WebAgent drafted it (false: it couldn't, and the owner fills it in).
    pub drafted: bool,
    /// One plain line about it.
    pub words: String,
    /// The one question the Chief asks, when it couldn't tell something.
    pub question: Option<String>,
}

/// `GET /console/{site}/missions/{id}`.
#[derive(Debug, Clone, Serialize)]
pub struct MissionView {
    #[serde(flatten)]
    pub chrome: Chrome,
    pub mission: MissionRow,
}

/// One thing a WebAgent can do in a job, with what it carries.
#[derive(Debug, Clone, Serialize)]
pub struct CanView {
    pub words: String,
    /// The effect class in plain shorthand: `read`, `draft`, `money`, `send_external`.
    pub tag: String,
}

/// `GET /console/{site}/store` (01b C14, C15).
#[derive(Debug, Clone, Serialize)]
pub struct StoreView {
    #[serde(flatten)]
    pub chrome: Chrome,
    /// WebAgents the site doesn't have yet.
    pub listings: Vec<MemberView>,
    /// The one being read before adding, from `?add=`.
    pub selected: Option<MemberView>,
    /// Where "Add" posts.
    pub add_url: String,
}

/// `GET /console/{site}/website`: the Editor's app (01b C23).
#[derive(Debug, Clone, Serialize)]
pub struct WebsiteView {
    #[serde(flatten)]
    pub chrome: Chrome,
    pub versions: Vec<VersionView>,
    /// Whether "Put back the previous release" applies (a live release with an earlier one).
    pub can_roll_back: bool,
    /// Whether "Go back to your old site" still applies (within 30 days of going live).
    pub old_site_words: Option<String>,
    pub rollback_url: String,
    pub passed: Vec<cv::PassedQuestion>,
    pub passed_total: u32,
}

/// One release in the version list.
#[derive(Debug, Clone, Serialize)]
pub struct VersionView {
    /// "v3".
    pub name: String,
    /// "Approved by you with your passkey · Sat 7 Nov, 09:40".
    pub words: String,
    pub live: bool,
    /// `rolled-back` and `superseded` show without a chip; `candidate` says it isn't signed yet.
    pub state: String,
    pub href: Option<String>,
}

/// `GET /console/{site}/trust`.
#[derive(Debug, Clone, Serialize)]
pub struct TrustView {
    #[serde(flatten)]
    pub chrome: Chrome,
    pub gauntlet: Option<cv::Gauntlet>,
    pub receipts: Vec<cv::ReceiptRow>,
    pub key_id: String,
    pub left_behind: u32,
}

/// `GET /console/{site}/quarantine`.
#[derive(Debug, Clone, Serialize)]
pub struct QuarantineView {
    #[serde(flatten)]
    pub chrome: Chrome,
    pub items: Vec<cv::QuarantineItem>,
}

/// `GET /console/{site}/settings`.
#[derive(Debug, Clone, Serialize)]
pub struct SettingsView {
    #[serde(flatten)]
    pub chrome: Chrome,
    /// "Saved. Night Time now starts at 21:30." after a change.
    pub saved_words: Option<String>,
    /// What each surface may decide (agentrix_decisions::surface).
    pub surfaces: Vec<agentrix_decisions::surface::Row>,
    /// The Channels app (W13: Telegram, WhatsApp and email), when node registers it.
    pub channels_href: Option<String>,
}

/// `GET /console/{site}/apps/{app}/…`: a registered app's page inside the shell.
#[derive(Debug, Clone, Serialize)]
pub struct AppView {
    #[serde(flatten)]
    pub chrome: Chrome,
    pub title: String,
    /// The app's HTML for the field, rendered by the app (trusted: it is our own code).
    pub html: String,
}

/// The approval sheet's JSON (`GET /console/{site}/cards/{id}/sheet`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Sheet {
    pub title: String,
    /// One line of exactly what the approval allows.
    pub sign_words: String,
    pub fingerprint: String,
    pub fingerprint_short: String,
    pub approve_url: String,
    pub has_passkey: bool,
    /// The ceremony's options (challenge = SHA-256(fingerprint ‖ nonce)).
    pub passkey_options: Option<String>,
}

/// A Front of house conversation in the Live list.
#[derive(Debug, Clone, Serialize)]
pub struct LiveRow {
    pub id: String,
    pub href: String,
    /// "Visitor on /cakes".
    pub who: String,
    pub desk: String,
    /// The last turn is under 10 minutes old.
    pub live: bool,
    /// The owner has it.
    pub owner: bool,
    /// "Cakes is answering", "You have it", "Ended 14:02".
    pub state_words: String,
    /// "2 min ago".
    pub last_words: String,
    /// The visitor's last words, for the list.
    pub last_text: String,
}

/// `/console/{site}/live`.
#[derive(Debug, Serialize)]
pub struct LiveView {
    #[serde(flatten)]
    pub chrome: Chrome,
    pub threads: Vec<LiveRow>,
    pub live_count: usize,
}

/// One turn, as the owner sees it.
#[derive(Debug, Clone, Serialize)]
pub struct LiveTurnView {
    pub seq: i64,
    /// `visitor`, `desk`, `owner`, `note`.
    pub who: String,
    /// "Visitor", "Cakes · AI", "You · a person".
    pub by: String,
    pub text: String,
    pub at_words: String,
    pub cites: Vec<String>,
}

/// `/console/{site}/live/{thread}`.
#[derive(Debug, Serialize)]
pub struct LiveThreadView {
    #[serde(flatten)]
    pub chrome: Chrome,
    pub thread: LiveRow,
    pub turns: Vec<LiveTurnView>,
    pub after: i64,
    /// The name visitors see when the owner replies.
    pub owner_name: String,
    pub turns_url: String,
    pub act_url: String,
}

/// `/console/{site}/walk`: the live site beside the Chief WebAgent's voice panel.
#[derive(Debug, Serialize)]
pub struct WalkView {
    #[serde(flatten)]
    pub chrome: Chrome,
    pub site_url: Option<String>,
    /// W11's Talk panel, from its console app, when node has it.
    pub talk_html: Option<String>,
}

/// `/console/{site}/apps`: the phone's Apps tab, the dock as a home screen.
#[derive(Debug, Serialize)]
pub struct AppsView {
    #[serde(flatten)]
    pub chrome: Chrome,
    /// W11's Talk app, when node has it.
    pub talk_href: Option<String>,
}
