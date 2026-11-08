//! Support grants, break-glass and holds, end to end on both hosts.

use std::sync::Arc;

use agentrix_host::testing::{FixedOwner, MemoryReceipts};
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt as _;
use url::Url;

use crate::testing::{self, FakeApprovals};
use crate::{OperatorConfig, OperatorState, ledger, router};

const OPS: &str = "http://ops.localhost:18300";

struct World {
    ops: OperatorState,
    receipts: Arc<MemoryReceipts>,
    fake: Arc<FakeApprovals>,
    owner: uuid::Uuid,
    site: String,
}

async fn world() -> World {
    let pool = testing::pool().await;
    let (fake, approvals) = testing::approvals();
    let receipts = Arc::new(MemoryReceipts::default());
    let mut config = OperatorConfig::new(Url::parse(OPS).unwrap());
    config.platform_origin = Some(Url::parse("http://localhost:8080").unwrap());
    let ops = OperatorState::new(config, pool.clone(), approvals)
        .unwrap()
        .with_receipts(receipts.clone());
    let owner = testing::account(&pool, "wanjiru@kilele.example", "Wanjiru Kamau").await;
    let site = testing::seed_live_site(&pool, owner, "test-bakery.example").await;
    World {
        ops,
        receipts,
        fake,
        owner,
        site,
    }
}

async fn send(app: axum::Router, req: Request<Body>) -> (StatusCode, String) {
    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let body = axum::body::to_bytes(res.into_body(), 4 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&body).into_owned())
}

fn json_req(path: &str, cookie: &str, body: Value) -> Request<Body> {
    Request::post(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, OPS)
        .header(header::COOKIE, cookie)
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn form_req(path: &str, cookie: &str, form: &str) -> Request<Body> {
    Request::post(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, OPS)
        .header(header::COOKIE, cookie)
        .body(Body::from(form.to_owned()))
        .unwrap()
}

async fn operator(w: &World, email: &str, name: &str, roles: &[&str]) -> String {
    let id = testing::account(w.ops.pool(), email, name).await;
    testing::grant(w.ops.pool(), id, roles).await;
    let cookie = w.ops.start_session(id, "totp").await.unwrap();
    cookie
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

/// Prepares and approves an action with the fake code; returns the approve response.
async fn act(w: &World, cookie: &str, action: Value) -> (StatusCode, String) {
    let (status, body) = send(
        router(w.ops.clone()),
        json_req("/actions/prepare", cookie, action),
    )
    .await;
    if status != StatusCode::OK {
        return (status, body);
    }
    let prepared: Value = serde_json::from_str(&body).unwrap();
    assert!(
        prepared["effect"]
            .as_str()
            .unwrap()
            .contains("If you approve, exactly this happens")
    );
    send(
        router(w.ops.clone()),
        json_req(
            &format!("/actions/{}/approve", prepared["id"].as_str().unwrap()),
            cookie,
            json!({ "proof": { "method": "totp", "code": "246810" } }),
        ),
    )
    .await
}

fn kinds(r: &MemoryReceipts) -> Vec<String> {
    r.issued
        .lock()
        .unwrap()
        .iter()
        .map(|(_, b)| b.kind.as_str().to_owned())
        .collect()
}

async fn site_page(w: &World, cookie: &str) -> String {
    let req = Request::get(format!("/sites/{}", w.site))
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap();
    let (status, body) = send(router(w.ops.clone()), req).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        &body[body.len().saturating_sub(800)..]
    );
    body
}

#[tokio::test]
async fn a_support_grant_needs_the_owner_and_every_look_is_recorded() {
    let w = world().await;
    let support = operator(&w, "juma@zavora.example", "Juma Mwangi", &["op.support"]).await;
    // Before any grant: no content.
    assert!(
        !site_page(&w, &support)
            .await
            .contains("What you can read now")
    );
    let (status, _) = send(router(w.ops.clone()), form_req(&format!("/sites/{}/support-grants", w.site), &support, "area_receipts=on&area_jobs=on&access=read&hours=4&reason=Checkout+fails+for+one+customer")).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let page = site_page(&w, &support).await;
    assert!(page.contains("requested") && page.contains("/support-access/grant_"));
    assert!(
        !page.contains("What you can read now"),
        "a request alone opens nothing"
    );

    // The owner answers on the platform host.
    let grant: (String, String) =
        sqlx::query_as("SELECT grant_id, fingerprint FROM support_grants")
            .fetch_one(w.ops.pool())
            .await
            .unwrap();
    let owner_side = crate::owner::OwnerState::new(
        w.ops.pool().clone(),
        w.ops.inner.approvals.clone(),
        Arc::new(FixedOwner::passkey(w.owner)),
        Some(w.receipts.clone()),
        Url::parse(OPS).unwrap(),
    )
    .unwrap();
    let (status, body) = send(
        crate::owner::router(owner_side.clone()),
        Request::get(format!("/support-access/{}", grant.0))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("Let Juma Mwangi read test-bakery.example")
            && body.contains("Never your customers' payment details")
    );
    let approve = Request::post(format!("/support-access/{}/approve", grant.0))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "method": "totp", "code": "246810" }).to_string(),
        ))
        .unwrap();
    let (status, body) = send(crate::owner::router(owner_side.clone()), approve).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let signed = w.fake.signed.lock().unwrap().clone();
    assert_eq!(
        hex::encode(signed.last().unwrap().1),
        grant.1,
        "the owner approved the grant's exact terms"
    );

    // The operator now reads receipts and jobs, and the owner gets one receipt per session.
    let page = site_page(&w, &support).await;
    assert!(page.contains("What you can read now") && page.contains("private words"));
    site_page(&w, &support).await;
    assert_eq!(
        kinds(&w.receipts),
        vec!["decision.approved", "operator.access"]
    );
    let (accesses,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM support_access")
        .fetch_one(w.ops.pool())
        .await
        .unwrap();
    assert_eq!(accesses, 4, "two visits, two areas each");
    // Another operator isn't named on the grant.
    let other = operator(&w, "achieng@zavora.example", "Achieng Otieno", &["op.sre"]).await;
    assert!(
        !site_page(&w, &other)
            .await
            .contains("What you can read now")
    );
    // The owner ends it early.
    let end = Request::post(format!("/support-access/{}/end", grant.0))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(crate::owner::router(owner_side), end).await.0,
        StatusCode::SEE_OTHER
    );
    assert!(
        !site_page(&w, &support)
            .await
            .contains("What you can read now")
    );
    assert!(ledger::verify(w.ops.pool()).await.unwrap().ok);
}

#[tokio::test]
async fn someone_elses_site_or_no_role_is_refused() {
    let w = world().await;
    let viewer = operator(&w, "audit@zavora.example", "Ann Auditor", &["op.auditor"]).await;
    let (status, _) = send(
        router(w.ops.clone()),
        form_req(
            &format!("/sites/{}/support-grants", w.site),
            &viewer,
            "area_receipts=on&access=read&hours=4&reason=x",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        act(
            &w,
            &viewer,
            json!({ "action": "hold", "site": w.site, "mode": "read-only", "reason": "x" })
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        act(
            &w,
            &viewer,
            json!({ "action": "break-glass", "site": w.site, "reason": "x" })
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    // A different owner can't answer this site's request.
    let support = operator(&w, "juma@zavora.example", "Juma Mwangi", &["op.support"]).await;
    send(
        router(w.ops.clone()),
        form_req(
            &format!("/sites/{}/support-grants", w.site),
            &support,
            "area_receipts=on&access=read&hours=4&reason=Help",
        ),
    )
    .await;
    let (grant,): (String,) = sqlx::query_as("SELECT grant_id FROM support_grants")
        .fetch_one(w.ops.pool())
        .await
        .unwrap();
    let stranger = testing::account(w.ops.pool(), "someone@else.example", "Someone Else").await;
    let owner_side = crate::owner::OwnerState::new(
        w.ops.pool().clone(),
        w.ops.inner.approvals.clone(),
        Arc::new(FixedOwner::passkey(stranger)),
        None,
        Url::parse(OPS).unwrap(),
    )
    .unwrap();
    let (status, _) = send(
        crate::owner::router(owner_side),
        Request::get(format!("/support-access/{grant}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn break_glass_takes_two_different_people_and_tells_the_owner() {
    let w = world().await;
    let security = operator(&w, "sec@zavora.example", "Sam Security", &["op.security"]).await;
    let director = operator(&w, "dir@zavora.example", "Dana Director", &["director"]).await;
    let (status, body) = act(&w, &security, json!({ "action": "break-glass", "site": w.site, "reason": "Pages leak a token; SEV1", "incident": "INC-0001" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        kinds(&w.receipts).is_empty(),
        "nobody is told until it opens"
    );
    // The first person can't sign twice.
    let (bg,): (String,) = sqlx::query_as("SELECT bg_id FROM break_glass")
        .fetch_one(w.ops.pool())
        .await
        .unwrap();
    assert_eq!(
        act(
            &w,
            &security,
            json!({ "action": "break-glass-second", "bg_id": bg })
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    // The director sees it in the lane and signs.
    let fleet = send(
        router(w.ops.clone()),
        Request::get("/fleet")
            .header(header::COOKIE, &director)
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .1;
    assert!(fleet.contains("Open break-glass on test-bakery.example"));
    let (status, body) = act(
        &w,
        &director,
        json!({ "action": "break-glass-second", "bg_id": bg }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(kinds(&w.receipts), vec!["operator.access"]);
    let page = site_page(&w, &director).await;
    assert!(page.contains("What you can read now") && page.contains("break-glass"));
    let (state, second): (String, String) =
        sqlx::query_as("SELECT state, second_role FROM break_glass")
            .fetch_one(w.ops.pool())
            .await
            .unwrap();
    assert_eq!((state.as_str(), second.as_str()), ("open", "director"));
    let (status, _) = send(
        router(w.ops.clone()),
        form_req(&format!("/break-glass/{bg}/close"), &security, ""),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(
        !site_page(&w, &director)
            .await
            .contains("What you can read now")
    );
}

#[tokio::test]
async fn a_hold_is_placed_receipted_and_lifted() {
    let w = world().await;
    let sre = operator(&w, "sre@zavora.example", "Achieng Otieno", &["op.sre"]).await;
    let (status, body) = act(&w, &sre, json!({ "action": "hold", "site": w.site, "mode": "read-only", "reason": "Phishing report A-311 under review" })).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let hold = crate::holds::active(w.ops.pool(), &w.site)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hold.serve_mode(), crate::holds::Mode::ReadOnly);
    assert!(
        site_page(&w, &sre)
            .await
            .contains("Recorded, not enforced on this build")
    );
    assert_eq!(
        act(
            &w,
            &sre,
            json!({ "action": "hold", "site": w.site, "mode": "notice", "reason": "again" })
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (status, _) = act(
        &w,
        &sre,
        json!({ "action": "lift", "hold_id": hold.hold_id, "reason": "Report was mistaken" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        crate::holds::active(w.ops.pool(), &w.site)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(kinds(&w.receipts), vec!["platform.hold", "platform.hold"]);
    let verbs: Vec<String> = ledger::recent(w.ops.pool(), 5)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.verb)
        .collect();
    assert_eq!(
        verbs[..2],
        ["hold.lift".to_owned(), "hold.place".to_owned()]
    );
}

#[tokio::test]
async fn a_wrong_code_runs_nothing() {
    let w = world().await;
    let sre = operator(&w, "sre@zavora.example", "Achieng Otieno", &["op.sre"]).await;
    let (_, body) = send(
        router(w.ops.clone()),
        json_req(
            "/actions/prepare",
            &sre,
            json!({ "action": "hold", "site": w.site, "mode": "read-only", "reason": "x" }),
        ),
    )
    .await;
    let id = serde_json::from_str::<Value>(&body).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (status, _) = send(
        router(w.ops.clone()),
        json_req(
            &format!("/actions/{id}/approve"),
            &sre,
            json!({ "proof": { "method": "totp", "code": "000000" } }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(
        crate::holds::active(w.ops.pool(), &w.site)
            .await
            .unwrap()
            .is_none()
    );
    // The prepared action is gone: a retry has to start again.
    let (status, _) = send(
        router(w.ops.clone()),
        json_req(
            &format!("/actions/{id}/approve"),
            &sre,
            json!({ "proof": { "method": "totp", "code": "246810" } }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::GONE);
}

#[tokio::test]
async fn money_shows_real_subscriptions_and_only_op_finance_issues_the_run() {
    let w = world().await;
    let billing = agentrix_billing::Billing::new(w.ops.pool().clone());
    billing
        .start(&w.site, "business", "I-TEST", None)
        .await
        .unwrap();
    let now = chrono::Utc::now();
    billing
        .on_event(&agentrix_billing::SubscriptionEvent {
            provider_ref: "I-TEST".into(),
            kind: agentrix_billing::EventKind::Activated,
            amount_cents: None,
            provider_payment_id: None,
            at: now,
            period_end: None,
            receipt: None,
        })
        .await
        .unwrap();
    billing.went_live(&w.site, now).await.unwrap();

    let sre = operator(&w, "sre@zavora.example", "Achieng Otieno", &["op.sre"]).await;
    let page = send(
        router(w.ops.clone()),
        Request::get("/money")
            .header(header::COOKIE, &sre)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        page.0,
        StatusCode::OK,
        "{}",
        &page.1[page.1.len().saturating_sub(800)..]
    );
    for needle in [
        "Money · ",
        "$79.00",
        "Business",
        "Invoice run drafted · 1",
        "Only someone with op.finance can approve this run",
        "PayPal Subscriptions · sandbox",
        "not imported",
    ] {
        assert!(page.1.contains(needle), "money lacks {needle:?}");
    }
    let run = billing
        .draft_run(&agentrix_billing::period_of(now))
        .await
        .unwrap();
    let action = json!({ "action": "invoice-run", "period": run.period, "digest": run.digest });
    assert_eq!(act(&w, &sre, action.clone()).await.0, StatusCode::FORBIDDEN);

    let finance = operator(&w, "fin@zavora.example", "Wairimu Njeri", &["op.finance"]).await;
    let stale = json!({ "action": "invoice-run", "period": run.period, "digest": "sha256:old" });
    assert_eq!(act(&w, &finance, stale).await.0, StatusCode::CONFLICT);
    let (status, body) = act(&w, &finance, action).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let m = billing.money(now).await.unwrap();
    assert_eq!((m.outstanding_invoices, m.outstanding_cents), (1, 7900));
    let entry = ledger::recent(w.ops.pool(), 1).await.unwrap().remove(0);
    assert_eq!(entry.verb, "money.invoice_run");
    assert_eq!(
        entry.summary,
        format!(
            "Wairimu Njeri issued 1 invoice for {}, $79.00 in all.",
            run.period
        )
    );
}

#[tokio::test]
async fn an_incident_is_declared_worked_resolved_and_written_up() {
    let w = world().await;
    let auditor = operator(&w, "audit@zavora.example", "Ann Auditor", &["op.auditor"]).await;
    let sre = operator(&w, "sre@zavora.example", "Achieng Otieno", &["op.sre"]).await;
    let declare = "severity=sev2&title=Ask+answers+slow+on+one+site&summary=Owners+report+slow+answers&site=test-bakery.example";
    assert_eq!(
        send(
            router(w.ops.clone()),
            form_req("/incidents", &auditor, declare)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, _) = send(router(w.ops.clone()), form_req("/incidents", &sre, declare)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (id, site): (String, Option<String>) =
        sqlx::query_as("SELECT incident_id, site_id FROM incidents")
            .fetch_one(w.ops.pool())
            .await
            .unwrap();
    assert_eq!(
        (id.as_str(), site.as_deref()),
        ("INC-0001", Some(w.site.as_str()))
    );
    for (kind, words) in [
        (
            "evidence",
            "Gateway calls to concierge failed 4 of 31 in 10 minutes",
        ),
        ("comms", "Some answers were slow between 07:00 and 07:20."),
    ] {
        let form = format!(
            "kind={kind}&words={}",
            url::form_urlencoded::byte_serialize(words.as_bytes()).collect::<String>()
        );
        assert_eq!(
            send(
                router(w.ops.clone()),
                form_req(&format!("/incidents/{id}/events"), &sre, &form)
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
    }
    for state in ["mitigated", "resolved"] {
        assert_eq!(
            send(
                router(w.ops.clone()),
                form_req(
                    &format!("/incidents/{id}/status"),
                    &sre,
                    &format!("state={state}")
                )
            )
            .await
            .0,
            StatusCode::SEE_OTHER
        );
    }
    assert_eq!(
        send(
            router(w.ops.clone()),
            form_req(
                &format!("/incidents/{id}/postmortem"),
                &sre,
                "words=The+provider+throttled+us.+We+now+alert+on+errors."
            )
        )
        .await
        .0,
        StatusCode::SEE_OTHER
    );
    let page = send(
        router(w.ops.clone()),
        Request::get(format!("/incidents/{id}"))
            .header(header::COOKIE, &sre)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        page.0,
        StatusCode::OK,
        "{}",
        &page.1[page.1.len().saturating_sub(800)..]
    );
    for needle in [
        "Ask answers slow on one site",
        "SEV2",
        "Resolved",
        "Gateway calls to concierge failed",
        "Draft to send",
        "The provider throttled us",
        "test-bakery.example",
    ] {
        assert!(page.1.contains(needle), "incident page lacks {needle:?}");
    }
    let list = send(
        router(w.ops.clone()),
        Request::get("/incidents")
            .header(header::COOKIE, &sre)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert!(
        list.1.contains("Nothing open")
            && list.1.contains("INC-0001")
            && list.1.contains("written"),
        "{}",
        list.1
    );
    let verbs: Vec<String> = ledger::recent(w.ops.pool(), 3)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.verb)
        .collect();
    assert_eq!(
        verbs,
        ["incident.resolved", "incident.mitigated", "incident.open"]
    );
    assert_eq!(
        send(
            router(w.ops.clone()),
            Request::get("/incidents/INC-9999")
                .header(header::COOKIE, &sre)
                .body(Body::empty())
                .unwrap()
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn rollouts_show_rings_and_the_route_changes_sites_were_told_about() {
    let w = world().await;
    sqlx::query(
        "INSERT INTO receipts (digest, receipt_id, site_id, kind, subject_type, subject_id, issuer, started_at, ended_at, issued_at, body) \
         VALUES ('sha256:rt', 'rcpt_rt', ?, 'routing.changed', 'site', ?, 'k', ?1, ?1, ?3, '{\"detail\":{\"title\":\"Models moved from anthropic to vultr\",\"to\":{\"provider\":\"vultr\"}}}')",
    )
    .bind(&w.site)
    .bind(&w.site)
    .bind(ledger::now())
    .execute(w.ops.pool())
    .await
    .unwrap();
    let sre = operator(&w, "sre@zavora.example", "Achieng Otieno", &["op.sre"]).await;
    let page = send(
        router(w.ops.clone()),
        Request::get("/rollouts")
            .header(header::COOKIE, &sre)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        page.0,
        StatusCode::OK,
        "{}",
        &page.1[page.1.len().saturating_sub(800)..]
    );
    for needle in [
        "No train in flight",
        "Ring 0",
        "1 of 1 customer site",
        "Models moved from anthropic to vultr",
        "1 site told",
        "1 of 1 runs held every attack",
    ] {
        assert!(page.1.contains(needle), "rollouts lacks {needle:?}");
    }
    // The seeded release was signed 84 minutes ago: it counts on that day in EAT.
    let eat = chrono::FixedOffset::east_opt(180 * 60).unwrap();
    let day = (chrono::Utc::now() - chrono::Duration::minutes(84))
        .with_timezone(&eat)
        .format("%a %-d %b")
        .to_string();
    assert!(
        page.1.contains(&day) && page.1.contains("Site releases signed, last 7 days"),
        "release day {day:?} in the operator's zone"
    );
}

#[tokio::test]
async fn a_support_request_shows_in_the_owners_lane_and_nobody_elses() {
    let w = world().await;
    let support = operator(&w, "juma@zavora.example", "Juma Mwangi", &["op.support"]).await;
    send(
        router(w.ops.clone()),
        form_req(
            &format!("/sites/{}/support-grants", w.site),
            &support,
            "area_receipts=on&access=read&hours=2&reason=Checkout+fails+for+one+customer",
        ),
    )
    .await;
    let mut ctx = agentrix_host::testing::context().await.unwrap();
    ctx.pool = w.ops.pool().clone();
    let lane = crate::support_lane(&ctx);
    let app = |account: String| agentrix_shell::apps::AppContext {
        site_id: w.site.clone(),
        site_name: "Test Bakery".into(),
        host: "test-bakery.example".into(),
        sample: false,
        account,
        owner_name: "Wanjiru".into(),
        utc_offset_minutes: 180,
        base: String::new(),
        console: String::new(),
    };
    let items = lane.needs_you(&app(w.owner.to_string())).await.unwrap();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(
        item.title,
        "Zavora support asks to read your site for 2 hours"
    );
    assert_eq!(
        (item.tone.as_str(), item.chip.as_str()),
        ("wait", "Read only")
    );
    assert_eq!(item.summary, "Checkout fails for one customer");
    assert!(
        item.href.starts_with("/support-access/grant_") && item.when_words.starts_with("Asked ")
    );
    assert!(
        lane.needs_you(&app(uuid::Uuid::now_v7().to_string()))
            .await
            .unwrap()
            .is_empty(),
        "only the owner sees it"
    );
}

#[tokio::test]
async fn a_partner_gets_a_client_only_when_the_owner_grants_it_and_its_people_see_only_their_portfolio()
 {
    let w = world().await;
    let sre = operator(&w, "sre@zavora.example", "Achieng Otieno", &["op.sre"]).await;
    let partners = operator(&w, "ps@zavora.example", "Pat Partners", &["op.partners"]).await;
    assert_eq!(
        send(
            router(w.ops.clone()),
            form_req("/partners", &sre, "name=Studio+Mwangaza&kind=agency")
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            router(w.ops.clone()),
            form_req("/partners", &partners, "name=Studio+Mwangaza&kind=agency")
        )
        .await
        .0,
        StatusCode::SEE_OTHER
    );
    let (pt,): (String,) = sqlx::query_as("SELECT partner_id FROM partners")
        .fetch_one(w.ops.pool())
        .await
        .unwrap();
    let invite = send(
        router(w.ops.clone()),
        form_req(
            &format!("/partners/{pt}/clients"),
            &partners,
            "site=test-bakery.example&role=editor&ring=canary",
        ),
    )
    .await;
    assert_eq!(invite.0, StatusCode::SEE_OTHER, "{}", invite.1);
    // A person at the agency.
    let muthoni =
        testing::account(w.ops.pool(), "muthoni@mwangaza.example", "Muthoni Wairimu").await;
    assert_eq!(
        send(
            router(w.ops.clone()),
            form_req(
                &format!("/partners/{pt}/members"),
                &partners,
                "email=muthoni%40mwangaza.example&role=partner.manager"
            )
        )
        .await
        .0,
        StatusCode::SEE_OTHER
    );

    // Before the owner answers: invited, not a client yet; the owner's lane shows the ask.
    let page = send(
        router(w.ops.clone()),
        Request::get(format!("/partners/{pt}"))
            .header(header::COOKIE, &partners)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        page.0,
        StatusCode::OK,
        "{}",
        &page.1[page.1.len().saturating_sub(600)..]
    );
    assert!(page.1.contains("Waiting for the owner's grant") && page.1.contains("Muthoni Wairimu"));
    let mut ctx = agentrix_host::testing::context().await.unwrap();
    ctx.pool = w.ops.pool().clone();
    let app = agentrix_shell::apps::AppContext {
        site_id: w.site.clone(),
        site_name: "Test Bakery".into(),
        host: "test-bakery.example".into(),
        sample: false,
        account: w.owner.to_string(),
        owner_name: "Wanjiru".into(),
        utc_offset_minutes: 180,
        base: String::new(),
        console: String::new(),
    };
    let items = crate::support_lane(&ctx).needs_you(&app).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].title,
        "Studio Mwangaza asks to be your site's editor"
    );

    // The owner grants on the platform host, approving exactly this partner, site and role.
    let owner_side = crate::owner::OwnerState::new(
        w.ops.pool().clone(),
        w.ops.inner.approvals.clone(),
        Arc::new(FixedOwner::passkey(w.owner)),
        Some(w.receipts.clone()),
        Url::parse(OPS).unwrap(),
    )
    .unwrap();
    let (status, body) = send(
        crate::owner::router(owner_side.clone()),
        Request::get(format!("/agency-access/{pt}/{}", w.site))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body.contains("Make Studio Mwangaza your site's editor"));
    let approve = Request::post(format!("/agency-access/{pt}/{}/approve", w.site))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "method": "totp", "code": "246810" }).to_string(),
        ))
        .unwrap();
    assert_eq!(
        send(crate::owner::router(owner_side.clone()), approve)
            .await
            .0,
        StatusCode::OK
    );
    let signed = w.fake.signed.lock().unwrap().clone();
    assert_eq!(
        signed.last().unwrap().1,
        crate::partners::grant_fingerprint(&pt, &w.site, "editor")
    );
    assert!(kinds(&w.receipts).contains(&"decision.approved".to_owned()));

    // Muthoni signs in and lands on her portfolio, in the portfolio scope; the fleet stays closed.
    let cookie = w.ops.start_session(muthoni, "passkey").await.unwrap();
    let cookie = cookie
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let mine = send(
        router(w.ops.clone()),
        Request::get("/portfolio")
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        mine.0,
        StatusCode::OK,
        "{}",
        &mine.1[mine.1.len().saturating_sub(600)..]
    );
    for needle in [
        "Portfolio · on Agentrix",
        "Studio Mwangaza · Clients",
        "Test Bakery",
        "Live · v1",
        "Editor",
        "canary",
        "partner.manager",
    ] {
        assert!(mine.1.contains(needle), "portfolio lacks {needle:?}");
    }
    assert!(!mine.1.contains("Operator · Zavora Cloud"));
    assert_eq!(
        send(
            router(w.ops.clone()),
            Request::get("/fleet")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap()
        )
        .await
        .0,
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        send(
            router(w.ops.clone()),
            Request::get(format!("/sites/{}", w.site))
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap()
        )
        .await
        .0,
        StatusCode::SEE_OTHER
    );

    // The owner takes it back: the portfolio drops the client.
    let revoke = Request::post(format!("/agency-access/{pt}/{}/revoke", w.site))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        send(crate::owner::router(owner_side), revoke).await.0,
        StatusCode::SEE_OTHER
    );
    let mine = send(
        router(w.ops.clone()),
        Request::get("/portfolio")
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert!(mine.1.contains("No clients yet"));
    assert!(ledger::verify(w.ops.pool()).await.unwrap().ok);
}

#[tokio::test]
async fn sites_carry_their_origin_after_0340() {
    let w = world().await;
    let pool = w.ops.pool();
    sqlx::query("INSERT INTO sites (site_id, domain, old_origin, fixture, created_at) VALUES ('s_fx', 'fixture.zavora.example', 'x', 1, 'x'), ('s_sm', 'kilele-bakery-sample.agentrix.website', 'x', 0, 'x')")
        .execute(pool)
        .await
        .unwrap();
    // 0340 ran before these rows existed in tests, so run its backfill again the way a deploy would.
    let sql = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../agentrix-node/src/db/migrations/0340_registry_columns.sql"
    ))
    .unwrap();
    let backfill = sql
        .split("UPDATE sites")
        .nth(1)
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("UPDATE sites{backfill};")))
        .execute(pool)
        .await
        .unwrap();
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT site_id, origin FROM sites ORDER BY site_id")
            .fetch_all(pool)
            .await
            .unwrap();
    let origin = |id: &str| {
        rows.iter()
            .find(|(s, _)| s == id)
            .map(|(_, o)| o.clone())
            .unwrap()
    };
    assert_eq!(
        (origin("s_fx"), origin("s_sm"), origin(&w.site)),
        ("fixture".into(), "sample".into(), "real".into())
    );
}
