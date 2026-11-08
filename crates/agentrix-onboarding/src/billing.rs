//! Billing and ownership (O9.1 to O9.4), for the site's proven owner.
//!
//! | Route | What it does |
//! |---|---|
//! | `GET /billing?site=` | The plan, its state, AI spend against the budget, plans, cancel, export |
//! | `POST /billing/cancel` (JSON `{site, passkey}` or `{site, totp}`) | Cancels the PayPal subscription once the owner's approval verifies |
//!
//! Cancelling is an owner decision (W6: "an owner decision … raises and approves first"): the
//! page shows exactly what is signed, the approval goes through node's frozen approve contract
//! (`NodeContext::approvals`, a passkey assertion or an authenticator code bound to the
//! fingerprint), and only then does W6's Effects Gateway cancel at PayPal.

use std::sync::Arc;

use agentrix_effects::Effects;
use agentrix_host::{Approvals, NodeContext, OwnerSessions};
use agentrix_types::SiteId;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::store::plan_choice;
use crate::view::{BillingPage, CancelCard, Chip, PlanPage, encode};
use crate::{OnboardingPage, Pages};

/// What the billing routes share.
#[derive(Clone)]
pub struct BillingRoutes {
    pages: Arc<Pages>,
    owners: Arc<dyn OwnerSessions>,
    approvals: Arc<dyn Approvals>,
    pool: sqlx::SqlitePool,
    effects: Effects,
}

impl BillingRoutes {
    /// The routes' state for node.
    pub fn new(ctx: &NodeContext, effects: Effects) -> anyhow::Result<Self> {
        Ok(Self {
            pages: Arc::new(Pages::new()?),
            owners: ctx.owners.clone(),
            approvals: ctx.approvals.clone(),
            pool: ctx.pool.clone(),
            effects,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
struct Q {
    site: Option<String>,
}

fn safe_site(site: &str) -> Option<String> {
    let s = site.trim();
    (!s.is_empty()
        && s.len() <= 80
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
    .then(|| s.to_owned())
}

/// What the owner's approval of a cancel signs: the site and the subscription, nothing else.
fn cancel_fingerprint(site: &str, subscription_id: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(format!(
        "agentrix.billing.plan_cancel\nsite={site}\nsubscription={subscription_id}\n"
    ));
    h.finalize().into()
}

async fn owns(pool: &sqlx::SqlitePool, site: &str, account: Uuid) -> anyhow::Result<bool> {
    let owner: Option<(Option<String>,)> =
        sqlx::query_as("SELECT owner_account_id FROM sites WHERE site_id = ?1")
            .bind(site)
            .fetch_optional(pool)
            .await?;
    Ok(matches!(owner, Some((Some(id),)) if id == account.to_string()))
}

async fn owner(st: &BillingRoutes, headers: &HeaderMap, site: &str) -> Result<Uuid, Response> {
    match st.owners.owner(headers).await {
        Ok(Some(o)) => match owns(&st.pool, site, o.account).await {
            Ok(true) => Ok(o.account),
            Ok(false) => Err((
                StatusCode::FORBIDDEN,
                "Only the site's owner sees its billing, once the domain is proved theirs.",
            )
                .into_response()),
            Err(e) => {
                tracing::error!(error = %format!("{e:#}"), "checking the site's owner");
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "We couldn't check who owns this site. Nothing was changed.",
                )
                    .into_response())
            }
        },
        Ok(None) => Err(Redirect::to(&format!(
            "/account/signin?next={}",
            encode(&format!("/billing?site={site}"))
        ))
        .into_response()),
        Err(e) => {
            tracing::error!(error = %format!("{e:#}"), "reading the owner session");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "We couldn't check who's signed in. Nothing was changed.",
            )
                .into_response())
        }
    }
}

fn dollars_micro(micro: i64) -> String {
    let cents = (micro + 5_000) / 10_000;
    format!("${}.{:02}", cents / 100, cents % 100)
}

fn day_words(at: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|t| t.with_timezone(&Utc).format("%a %-d %b").to_string())
}

/// The page's view, from W3's billing and W6's Effects.
async fn view_for(st: &BillingRoutes, site: &str, account: Uuid) -> anyhow::Result<BillingPage> {
    let billing = agentrix_billing::Billing::new(st.pool.clone());
    let sub = billing
        .for_site(site)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let choice = plan_choice(&st.pool, site).await?;
    let plan_id = sub
        .as_ref()
        .map(|s| s.plan_id.clone())
        .or_else(|| choice.as_ref().map(|c| c.plan_id.clone()));
    let plan = plan_id.as_deref().and_then(agentrix_billing::plan);
    let plans = PlanPage::new(site, None, &crate::plans(), "/").plans;
    let mut view = BillingPage {
        site_id: site.to_owned(),
        plans,
        current_plan: plan_id.clone().unwrap_or_default(),
        plan_href: format!(
            "/setup/plan?site={}&next={}",
            encode(site),
            encode(&format!("/billing?site={site}"))
        ),
        ..BillingPage::default()
    };
    let (chip_class, chip_words) = match sub.as_ref().map(|s| s.state.as_str()) {
        Some("active") => ("chip-ok", "Active"),
        Some("past_due") => ("chip-risk", "Payment overdue"),
        Some("pending_approval") => ("chip-wait", "Waiting for PayPal"),
        Some("cancelled") => ("", "Cancelled"),
        Some("suspended") => ("chip-risk", "Suspended"),
        Some(_) => ("", "Ended"),
        None => ("", "No plan yet"),
    };
    view.state_chip = Chip {
        class: chip_class.into(),
        words: chip_words.into(),
    };
    if let Some(p) = plan {
        view.has_plan = true;
        let price = sub.as_ref().map_or(p.price_cents, |s| s.price_cents);
        let mut summary = format!(
            "{} · ${} a month · PayPal subscription",
            p.name,
            price / 100
        );
        if let Some(day) = sub
            .as_ref()
            .and_then(|s| s.period_end.as_deref())
            .and_then(day_words)
            .filter(|_| sub.as_ref().is_some_and(|s| s.state == "active"))
        {
            summary.push_str(&format!(" · next charge {day}"));
        }
        view.summary = summary;
        let budget_usd = choice
            .as_ref()
            .map_or(p.included_ai_micro_usd / 1_000_000, |c| c.ai_budget_usd);
        view.budget_words = Some(format!("${budget_usd}"));
        if let Ok(usage) = billing.usage(Utc::now()).await
            && let Some(u) = usage.iter().find(|u| u.site_id == site)
        {
            view.spent_words = Some(dollars_micro(u.spent_micro_usd));
            let cap = (budget_usd * 1_000_000).max(1);
            let pct = (u.spent_micro_usd * 100 / cap).clamp(0, 100);
            view.spent_pct = ((pct / 5) * 5) as u8;
        }
    }
    // The cancel card, for a subscription that can still be cancelled.
    if let Ok(subs) = st.effects.subscriptions_for_site(&SiteId::new(site)).await
        && let Some(live) = subs
            .iter()
            .filter(|s| matches!(s.state.as_str(), "active" | "pending_approval" | "past_due"))
            .max_by_key(|s| s.created_at)
    {
        let fp = cancel_fingerprint(site, &live.id);
        let options = st
            .approvals
            .passkey_options(account, fp)
            .await
            .ok()
            .flatten()
            .map(|v| v.to_string());
        let name = plan.map_or("your plan", |p| p.name.as_str());
        view.cancel = Some(CancelCard {
            title: format!("Cancel {name}?"),
            sign_words: format!(
                "Cancel {name} for this site: PayPal stops charging. Subscription {}.",
                live.id
            ),
            fingerprint: hex::encode(fp),
            passkey_options: options,
        });
    }
    Ok(view)
}

async fn page(State(st): State<BillingRoutes>, headers: HeaderMap, Query(q): Query<Q>) -> Response {
    let Some(site) = q.site.as_deref().and_then(safe_site) else {
        return Redirect::to("/console/today").into_response();
    };
    let account = match owner(&st, &headers, &site).await {
        Ok(a) => a,
        Err(r) => return r,
    };
    match view_for(&st, &site, account).await {
        Ok(view) => match st.pages.render(OnboardingPage::Billing, &view) {
            Ok(body) => Html(body).into_response(),
            Err(e) => {
                tracing::error!(error = %e, "rendering billing");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "This page didn't load on our side. Nothing was changed.",
                )
                    .into_response()
            }
        },
        Err(e) => {
            tracing::error!(error = %format!("{e:#}"), "reading billing");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "We couldn't read your billing. Nothing was changed. Try again in a moment.",
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
struct CancelBody {
    site: String,
    #[serde(default)]
    passkey: Option<Value>,
    #[serde(default)]
    totp: Option<String>,
}

fn said(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "message": message }))).into_response()
}

async fn cancel(
    State(st): State<BillingRoutes>,
    headers: HeaderMap,
    Json(b): Json<CancelBody>,
) -> Response {
    let Some(site) = safe_site(&b.site) else {
        return said(
            StatusCode::BAD_REQUEST,
            "That isn't one of your sites. Nothing was changed.",
        );
    };
    let account = match owner(&st, &headers, &site).await {
        Ok(a) => a,
        Err(_) => {
            return said(
                StatusCode::FORBIDDEN,
                "Only the site's owner can cancel its plan. Nothing was changed.",
            );
        }
    };
    let subs = match st
        .effects
        .subscriptions_for_site(&SiteId::new(site.clone()))
        .await
    {
        Ok(s) => s,
        Err(e) => return said(StatusCode::BAD_GATEWAY, &e.to_string()),
    };
    let Some(live) = subs
        .iter()
        .filter(|s| matches!(s.state.as_str(), "active" | "pending_approval" | "past_due"))
        .max_by_key(|s| s.created_at)
    else {
        return said(
            StatusCode::CONFLICT,
            "There's no plan to cancel on this site. Nothing was changed.",
        );
    };
    let request = match (b.passkey, b.totp) {
        (Some(p), _) => json!({ "passkey": p }),
        (None, Some(code)) => json!({ "totp": code.trim() }),
        (None, None) => {
            return said(
                StatusCode::BAD_REQUEST,
                "Approve with your passkey or a code. Nothing was changed.",
            );
        }
    };
    let fp = cancel_fingerprint(&site, &live.id);
    if let Err(e) = st.approvals.verify(account, fp, request).await {
        tracing::info!(error = %format!("{e:#}"), "cancel approval refused");
        return said(
            StatusCode::FORBIDDEN,
            "That approval didn't check out, so nothing was cancelled. Try again.",
        );
    }
    match st
        .effects
        .cancel_subscription(&SiteId::new(site.clone()), &live.id, "The owner cancelled from Billing")
        .await
    {
        Ok(_) => Json(json!({ "redirect": format!("/billing?site={site}"), "said": "Cancelled. PayPal won't charge again; your site and data stay yours." })).into_response(),
        Err(e) => said(StatusCode::BAD_GATEWAY, &e.to_string()),
    }
}

/// The billing routes, for [`crate::mount`].
pub fn router(st: BillingRoutes) -> Router {
    Router::new()
        .route("/billing", get(page))
        .route("/billing/cancel", post(cancel))
        .with_state(st)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancel_signs_its_site_and_subscription_only() {
        assert_eq!(
            cancel_fingerprint("s1", "sub_1"),
            cancel_fingerprint("s1", "sub_1")
        );
        assert_ne!(
            cancel_fingerprint("s1", "sub_1"),
            cancel_fingerprint("s2", "sub_1")
        );
        assert_ne!(
            cancel_fingerprint("s1", "sub_1"),
            cancel_fingerprint("s1", "sub_2")
        );
    }

    #[test]
    fn spend_reads_in_dollars_and_cents() {
        assert_eq!(dollars_micro(3_400_000), "$3.40");
        assert_eq!(dollars_micro(0), "$0.00");
        assert_eq!(dollars_micro(214_999), "$0.21");
    }
}
