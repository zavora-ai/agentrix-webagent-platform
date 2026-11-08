//! Card v2: raise, approve through the approvals port, decline, expiry, tampering, and the
//! limits on spoken answers. Sites come from the first release's `sites` table, created here.

use std::sync::{Arc, Mutex};

use agentrix_decisions::{
    ApprovalKind, Attempt, CardState, Decisions, EffectRunner, NewCard, RaisedBy, Risk, RiskLevel,
};
use agentrix_host::testing::{FixedOwner, context};
use agentrix_types::{EffectRequest, SiteId};
use async_trait::async_trait;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use uuid::Uuid;

#[derive(Default)]
struct Confirm(Mutex<Vec<String>>);

#[async_trait]
impl EffectRunner for Confirm {
    fn tool(&self) -> &str {
        "order.confirm"
    }
    async fn run(
        &self,
        card: &agentrix_decisions::DecisionCardV2,
        _receipt: &str,
    ) -> anyhow::Result<Option<String>> {
        self.0.lock().unwrap().push(card.id.clone());
        Ok(None)
    }
}

async fn setup() -> (Decisions, Arc<Confirm>, String) {
    let owner = Uuid::now_v7();
    let mut ctx = context().await.unwrap();
    ctx.owners = Arc::new(FixedOwner::passkey(owner));
    sqlx::raw_sql("CREATE TABLE sites (site_id TEXT PRIMARY KEY, owner_account_id TEXT); INSERT INTO sites VALUES ('s1', NULL);")
        .execute(&ctx.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE sites SET owner_account_id = ?")
        .bind(owner.to_string())
        .execute(&ctx.pool)
        .await
        .unwrap();
    agentrix_decisions::migrate(&ctx.pool).await.unwrap();
    let runner = Arc::new(Confirm::default());
    (
        Decisions::new(&ctx).with_runners(vec![runner.clone()]),
        runner,
        owner.to_string(),
    )
}

fn card(expiry_hours: i64, risk: RiskLevel, approval: ApprovalKind, key: Option<&str>) -> NewCard {
    NewCard {
        kind: "order.confirm".into(),
        title: "Approve cake order · KSh 4,500".into(),
        summary: "Amani W. · collect Saturday".into(),
        effects: vec!["Amani gets a payment request for KSh 4,500".into()],
        effect: EffectRequest {
            site: SiteId::new("s1"),
            webagent: "cashier".into(),
            tool: "order.confirm".into(),
            args: serde_json::json!({ "order": "o-118" }),
            amount: None,
            expiry: Utc::now() + Duration::hours(expiry_hours),
        },
        raised_by: RaisedBy {
            webagent: "cashier".into(),
            job: Some("payments".into()),
        },
        risk: Risk {
            level: risk,
            reasons: vec!["Returning customer".into()],
        },
        approval,
        back: None,
        dedupe_key: key.map(str::to_owned),
    }
}

#[tokio::test]
async fn raise_once_then_approve_with_a_code_runs_the_effect() {
    let (d, runner, owner) = setup().await;
    let c = d
        .raise(card(
            8,
            RiskLevel::Low,
            ApprovalKind::Reviewed,
            Some("o-118"),
        ))
        .await
        .unwrap();
    let again = d
        .raise(card(
            8,
            RiskLevel::Low,
            ApprovalKind::Reviewed,
            Some("o-118"),
        ))
        .await
        .unwrap();
    assert_eq!(c.id, again.id, "the same key raises one card");
    assert_eq!(d.list("s1").await.unwrap().len(), 1);

    // A code must name the card's fingerprint.
    let r = d
        .approve(
            &c.id,
            &owner,
            Attempt::Totp("123456".into()),
            Some("sha256:00"),
        )
        .await
        .unwrap_err();
    assert_eq!(r.status, StatusCode::CONFLICT);
    // Someone who doesn't own the site can't decide it.
    let r = d
        .approve(
            &c.id,
            &Uuid::now_v7().to_string(),
            Attempt::Totp("123456".into()),
            Some(&c.fingerprint.to_string()),
        )
        .await
        .unwrap_err();
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    d.approve(
        &c.id,
        &owner,
        Attempt::Totp("123456".into()),
        Some(&c.fingerprint.to_string()),
    )
    .await
    .unwrap();
    assert_eq!(*runner.0.lock().unwrap(), vec![c.id.clone()]);
    let row = d.get("s1", &c.id).await.unwrap().unwrap();
    assert_eq!(row.state, CardState::Approved);
    assert_eq!(row.method.as_deref(), Some("totp"));
    assert!(row.receipt.is_some());
    let r = d
        .approve(
            &c.id,
            &owner,
            Attempt::Totp("123456".into()),
            Some(&c.fingerprint.to_string()),
        )
        .await
        .unwrap_err();
    assert_eq!(r.status, StatusCode::CONFLICT, "a card runs once");
}

#[tokio::test]
async fn decline_and_expiry_never_run_anything() {
    let (d, runner, owner) = setup().await;
    let c = d
        .raise(card(8, RiskLevel::Low, ApprovalKind::Reviewed, None))
        .await
        .unwrap();
    d.decline(&c.id, &owner, "console").await.unwrap();
    assert_eq!(
        d.get("s1", &c.id).await.unwrap().unwrap().state,
        CardState::Declined
    );

    let old = d
        .raise(card(-1, RiskLevel::Low, ApprovalKind::Reviewed, None))
        .await
        .unwrap();
    assert_eq!(
        d.get("s1", &old.id).await.unwrap().unwrap().state,
        CardState::Expired
    );
    let r = d
        .approve(
            &old.id,
            &owner,
            Attempt::Passkey(serde_json::json!({})),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(r.status, StatusCode::GONE);
    assert!(runner.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn spoken_answers_allow_only_low_risk_one_tap_cards() {
    let (d, runner, owner) = setup().await;
    let reviewed = d
        .raise(card(8, RiskLevel::Low, ApprovalKind::Reviewed, None))
        .await
        .unwrap();
    let risky = d
        .raise(card(8, RiskLevel::High, ApprovalKind::OneTap, None))
        .await
        .unwrap();
    let everyday = d
        .raise(card(8, RiskLevel::Low, ApprovalKind::OneTap, None))
        .await
        .unwrap();
    for id in [&reviewed.id, &risky.id] {
        let r = d
            .decide_spoken("s1", &owner, id, true, "voice", "allow once")
            .await
            .unwrap_err();
        assert_eq!(r.status, StatusCode::FORBIDDEN);
    }
    d.decide_spoken("s1", &owner, &everyday.id, true, "voice", "allow once")
        .await
        .unwrap();
    assert_eq!(
        d.get("s1", &everyday.id)
            .await
            .unwrap()
            .unwrap()
            .method
            .as_deref(),
        Some("spoken")
    );
    // Deny works on any card.
    d.decide_spoken("s1", &owner, &risky.id, false, "chat", "no")
        .await
        .unwrap();
    assert_eq!(
        d.get("s1", &risky.id).await.unwrap().unwrap().state,
        CardState::Declined
    );
    assert_eq!(*runner.0.lock().unwrap(), vec![everyday.id.clone()]);
}

#[tokio::test]
async fn a_card_nothing_can_carry_out_is_never_approved() {
    let (d, _runner, owner) = setup().await;
    let mut new = card(8, RiskLevel::Low, ApprovalKind::OneTap, None);
    new.effect.tool = "review.reply".into();
    let c = d.raise(new).await.unwrap();
    let r = d
        .approve(
            &c.id,
            &owner,
            Attempt::Totp("123456".into()),
            Some(&c.fingerprint.to_string()),
        )
        .await
        .unwrap_err();
    assert_eq!(r.status, StatusCode::CONFLICT);
    let r = d
        .decide_spoken("s1", &owner, &c.id, true, "voice", "allow once")
        .await
        .unwrap_err();
    assert_eq!(r.status, StatusCode::CONFLICT);
    let row = d.get("s1", &c.id).await.unwrap().unwrap();
    assert_eq!(row.state, CardState::Waiting, "no approval was recorded");
    assert!(row.receipt.is_none());
    d.decline(&c.id, &owner, "console").await.unwrap();
}
