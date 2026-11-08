//! The shell on Brick 1's real `control.db` schema, through `mount`: the reads in `control.rs`
//! run against the same tables node migrates. The schema files are read from agentrix-node's
//! folder at test time, so a schema change there breaks this test rather than the live console.

use agentrix_host::testing::{FixedOwner, context};
use agentrix_types::{CardKind, DecisionCard, DecisionId, EffectRequest, SiteId};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use chrono::{Duration, SecondsFormat, Utc};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

fn node_sql(path: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../agentrix-node/src")
        .join(path);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("reading {}: {e}", p.display()))
}

fn t(at: chrono::DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Micros, true)
}

#[tokio::test]
async fn the_shell_reads_brick_1s_tables() {
    let account = Uuid::now_v7();
    let mut ctx = context().await.unwrap();
    ctx.owners = Arc::new(FixedOwner::passkey(account));
    let pool = ctx.pool.clone();
    for f in [
        "auth/schema.sql",
        "claim/schema.sql",
        "db/migrations/0001_control.sql",
        "db/migrations/0002_passed_questions.sql",
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(node_sql(f)))
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("{f}: {e}"));
    }
    agentrix_shell::migrate(&pool).await.unwrap();
    agentrix_decisions::migrate(&pool).await.unwrap();
    agentrix_store::migrate(&pool).await.unwrap();

    let now = Utc::now();
    sqlx::query("INSERT INTO accounts (id, email, display_name, created_at) VALUES (?, 'wanjiru@example.com', 'Wanjiru Kamau', 0)")
        .bind(account.to_string())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sites (site_id, domain, name, old_origin, utc_offset_minutes, owner_account_id, created_at) VALUES ('site_1', 'kilele-bakery-sample.agentrix.website', 'Kilele Bakery', 'https://kilele.example', 180, ?, ?)")
        .bind(account.to_string())
        .bind(t(now))
        .execute(&pool)
        .await
        .unwrap();
    let effect = EffectRequest {
        site: SiteId::new("site_1"),
        webagent: "orders".into(),
        tool: "order.confirm".into(),
        args: serde_json::json!({ "order": "o-118" }),
        amount: None,
        expiry: now + Duration::hours(8),
    };
    let card = DecisionCard::new(
        DecisionId::new("dec_1"),
        CardKind::OrderHeld,
        "Approve cake order · KSh 4,500",
        "Amani W. · collect Saturday",
        effect,
        now,
    )
    .unwrap();
    sqlx::query("INSERT INTO decisions (decision_id, site_id, kind, card, fingerprint, state, created_at) VALUES ('dec_1', 'site_1', 'order-held', ?, ?, 'waiting', ?)")
        .bind(serde_json::to_string(&card).unwrap())
        .bind(card.fingerprint.to_string())
        .bind(t(now))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO passed_questions (question_id, site_id, question, contact, passed_at) VALUES ('q1', 'site_1', 'Do you make gluten-free cakes?', NULL, ?)")
        .bind(t(now))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO briefs (site_id, written_at, body) VALUES ('site_1', ?, ?)")
        .bind(t(now - Duration::hours(2)))
        .bind(serde_json::json!({ "headline": "Overnight: 3 questions answered.", "lines": [{ "text": "One order is held for you." }] }).to_string())
        .execute(&pool)
        .await
        .unwrap();

    let v2 = agentrix_decisions::Decisions::new(&ctx)
        .raise(agentrix_decisions::NewCard {
            kind: "dial.raise".into(),
            title: "Let the Editor fix typos without asking".into(),
            summary: "Raises Fix typos from Suggest to Do it and tell me".into(),
            effects: vec!["The Editor fixes typos and tells you after".into()],
            effect: EffectRequest {
                site: SiteId::new("site_1"),
                webagent: "editor".into(),
                tool: "dial.raise".into(),
                args: serde_json::json!({ "job": "freshness", "to": "do-and-tell" }),
                amount: None,
                expiry: now + Duration::days(7),
            },
            raised_by: agentrix_decisions::RaisedBy {
                webagent: "editor".into(),
                job: Some("freshness".into()),
            },
            risk: agentrix_decisions::Risk {
                level: agentrix_decisions::RiskLevel::Medium,
                reasons: vec!["Changes what it may do alone".into()],
            },
            approval: agentrix_decisions::ApprovalKind::Reviewed,
            back: None,
            dedupe_key: None,
        })
        .await
        .unwrap();

    let app = agentrix_shell::mount(&ctx).unwrap().platform;
    let get = |path: &str| {
        let app = app.clone();
        let req = Request::get(path).body(Body::empty()).unwrap();
        async move {
            let res = app.oneshot(req).await.unwrap();
            let status = res.status();
            let location = res
                .headers()
                .get(header::LOCATION)
                .map(|v| v.to_str().unwrap().to_owned());
            (
                status,
                String::from_utf8(to_bytes(res.into_body(), 1 << 22).await.unwrap().to_vec())
                    .unwrap(),
                location,
            )
        }
    };

    let (s, _, to) = get("/os").await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    assert_eq!(to.as_deref(), Some("/console/site_1"));

    let (s, html, _) = get("/console/site_1").await;
    assert_eq!(s, StatusCode::OK, "{html}");
    for want in [
        "Wanjiru",
        "Kilele Bakery",
        "Sample",
        "Approve cake order · KSh 4,500",
        "Overnight: 3 questions answered.",
        "Do you make gluten-free cakes?",
        "Orders asks",
    ] {
        assert!(html.contains(want), "home lacks {want:?}");
    }

    assert!(
        html.contains("Let the Editor fix typos without asking"),
        "the lane lists v2 cards"
    );
    let (s, html, _) = get(&format!("/console/site_1/decisions/{}", v2.id)).await;
    assert_eq!(s, StatusCode::OK, "{html}");
    assert!(html.contains(&format!("/decisions/v2/{}/approve", v2.id)));
    assert!(html.contains("Changes what it may do alone"));

    let (s, html, _) = get("/console/site_1/decisions/dec_1").await;
    assert_eq!(s, StatusCode::OK, "{html}");
    assert!(html.contains(&card.fingerprint.to_string()));
    assert!(html.contains("/decisions/dec_1/approve"));

    let (s, body, _) = get("/console/site_1/cards/dec_1/sheet").await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert!(body.contains(&card.fingerprint.to_string()));

    for path in [
        "/console/site_1/decisions",
        "/console/site_1/trust",
        "/console/site_1/webagents",
        "/console/site_1/quarantine",
        "/console/site_1/settings",
        "/console/site_1/website",
        "/console/site_1/memory",
        "/console/site_1/store",
    ] {
        let (s, html, _) = get(path).await;
        assert_eq!(s, StatusCode::OK, "{path}: {html}");
    }

    // Someone else's site is not found, and so is a site that doesn't exist.
    sqlx::query("INSERT INTO sites (site_id, domain, old_origin, created_at) VALUES ('site_2', 'other.example', 'https://other.example', ?)")
        .bind(t(now))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(get("/console/site_2").await.0, StatusCode::NOT_FOUND);
    assert_eq!(get("/console/site_9").await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn signed_out_owners_go_to_sign_in() {
    let ctx = context().await.unwrap();
    agentrix_shell::migrate(&ctx.pool).await.unwrap();
    let app = agentrix_shell::mount(&ctx).unwrap().platform;
    let res = app
        .oneshot(
            Request::get("/console/site_1/trust")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        res.headers()[header::LOCATION],
        "/account/signin?next=%2Fconsole%2Fsite_1%2Ftrust"
    );
}

#[tokio::test]
async fn adding_a_webagent_from_the_store_is_a_decision_that_puts_it_on_duty() {
    let account = Uuid::now_v7();
    let mut ctx = context().await.unwrap();
    ctx.owners = Arc::new(FixedOwner::passkey(account));
    let pool = ctx.pool.clone();
    for f in [
        "auth/schema.sql",
        "claim/schema.sql",
        "db/migrations/0001_control.sql",
        "db/migrations/0002_passed_questions.sql",
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(node_sql(f)))
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("{f}: {e}"));
    }
    agentrix_shell::migrate(&pool).await.unwrap();
    agentrix_decisions::migrate(&pool).await.unwrap();
    agentrix_store::migrate(&pool).await.unwrap();
    agentrix_kernel::migrate(&pool).await.unwrap();
    sqlx::query("INSERT INTO accounts (id, email, display_name, created_at) VALUES (?, 'wanjiru@example.com', 'Wanjiru', 0)")
        .bind(account.to_string())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sites (site_id, domain, name, old_origin, owner_account_id, created_at) VALUES ('site_1', 'kilele.example', 'Kilele Bakery', 'https://kilele.example', ?, ?)")
        .bind(account.to_string())
        .bind(t(Utc::now()))
        .execute(&pool)
        .await
        .unwrap();

    let app = agentrix_shell::mount(&ctx).unwrap().platform;
    let res = app
        .clone()
        .oneshot(
            Request::get("/console/site_1/store")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let html =
        String::from_utf8(to_bytes(res.into_body(), 1 << 22).await.unwrap().to_vec()).unwrap();
    assert!(
        html.contains("store?add=sales"),
        "Sales is in the Store for a moved site"
    );

    let res = app
        .clone()
        .oneshot(
            Request::post("/console/site_1/store/add")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("webagent=sales"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let to = res.headers()[header::LOCATION].to_str().unwrap().to_owned();
    let card_id = to.rsplit('/').next().unwrap().to_owned();
    assert!(card_id.starts_with("dec2_"), "{to}");

    // Nothing changes before the owner approves.
    let rosters = agentrix_kernel::Rosters::new(pool.clone(), ctx.blobs.clone());
    assert!(
        rosters
            .roster("site_1")
            .await
            .unwrap()
            .unwrap()
            .on_duty()
            .all(|m| m.spec.id != "sales")
    );

    let d = agentrix_decisions::Decisions::new(&ctx).with_runners(agentrix_shell::runners(&ctx));
    let row = d.get("site_1", &card_id).await.unwrap().unwrap();
    let to = d
        .approve(
            &card_id,
            &account.to_string(),
            agentrix_decisions::Attempt::Totp("123456".into()),
            Some(&row.card.fingerprint.to_string()),
        )
        .await
        .unwrap();
    assert_eq!(to.as_deref(), Some("/console/site_1/webagents#agent-sales"));
    assert!(
        rosters
            .roster("site_1")
            .await
            .unwrap()
            .unwrap()
            .on_duty()
            .any(|m| m.spec.id == "sales"),
        "Sales is on duty after the approval"
    );
}

#[tokio::test]
async fn a_mission_starts_when_its_contract_is_approved_and_counts_from_then() {
    let account = Uuid::now_v7();
    let mut ctx = context().await.unwrap();
    ctx.owners = Arc::new(FixedOwner::passkey(account));
    let pool = ctx.pool.clone();
    for f in [
        "auth/schema.sql",
        "claim/schema.sql",
        "db/migrations/0001_control.sql",
        "db/migrations/0002_passed_questions.sql",
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(node_sql(f)))
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("{f}: {e}"));
    }
    agentrix_shell::migrate(&pool).await.unwrap();
    agentrix_decisions::migrate(&pool).await.unwrap();
    agentrix_store::migrate(&pool).await.unwrap();
    agentrix_kernel::migrate(&pool).await.unwrap();
    sqlx::query("INSERT INTO accounts (id, email, display_name, created_at) VALUES (?, 'wanjiru@example.com', 'Wanjiru', 0)")
        .bind(account.to_string())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sites (site_id, domain, name, old_origin, owner_account_id, created_at) VALUES ('site_1', 'kilele.example', 'Kilele Bakery', 'https://kilele.example', ?, ?)")
        .bind(account.to_string())
        .bind(t(Utc::now()))
        .execute(&pool)
        .await
        .unwrap();
    let rosters = agentrix_kernel::Rosters::new(pool.clone(), ctx.blobs.clone());
    let member = rosters
        .roster("site_1")
        .await
        .unwrap()
        .unwrap()
        .on_duty()
        .map(|m| m.spec.id.clone())
        .find(|id| id != "coordinator")
        .expect("a WebAgent on duty besides the Chief");

    let app = agentrix_shell::mount(&ctx).unwrap().platform;
    let send = |req: Request<Body>| {
        let app = app.clone();
        async move {
            let res = app.oneshot(req).await.unwrap();
            let status = res.status();
            let to = res
                .headers()
                .get(header::LOCATION)
                .map(|v| v.to_str().unwrap().to_owned());
            let html =
                String::from_utf8(to_bytes(res.into_body(), 1 << 22).await.unwrap().to_vec())
                    .unwrap();
            (status, html, to)
        }
    };
    let (s, html, _) = send(
        Request::get("/console/site_1/missions")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{html}");
    assert!(html.contains("Write the contract"), "{html}");
    assert!(
        html.contains("your site doesn&#x27;t take orders yet")
            || html.contains("your site doesn't take orders yet"),
        "a moved site has no orders to count"
    );

    let ends = (Utc::now() + Duration::days(20))
        .format("%Y-%m-%d")
        .to_string();
    let (s, html, to) = send(
        Request::post("/console/site_1/missions")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!(
                "goal=Answer+more+questions&measure=questions&target=4&member={member}&ends={ends}"
            )))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER, "{html}");
    let to = to.unwrap();
    let card_id = to.rsplit('/').next().unwrap().to_owned();
    assert!(card_id.starts_with("dec2_"), "{to}");

    let d = agentrix_decisions::Decisions::new(&ctx).with_runners(agentrix_shell::runners(&ctx));
    let row = d.get("site_1", &card_id).await.unwrap().unwrap();
    assert_eq!(row.card.effect.tool, "mission.start");
    let missions = agentrix_shell::missions::Missions::new(pool.clone());
    let m = missions.list("site_1").await.unwrap().remove(0);
    assert_eq!(m.state, "proposed");
    // A question passed before the approval doesn't count.
    let pass = |at: chrono::DateTime<Utc>| {
        sqlx::query("INSERT INTO passed_questions (question_id, site_id, question, passed_at) VALUES (?, 'site_1', 'Do you deliver to Karen?', ?)")
            .bind(format!("pq_{}", Uuid::now_v7().simple()))
            .bind(t(at))
            .execute(&pool)
    };
    pass(Utc::now() - Duration::minutes(5)).await.unwrap();

    let back = d
        .approve(
            &card_id,
            &account.to_string(),
            agentrix_decisions::Attempt::Totp("123456".into()),
            Some(&row.card.fingerprint.to_string()),
        )
        .await
        .unwrap();
    assert_eq!(back, Some(format!("/console/site_1/missions/{}", m.id)));
    let started = missions.get("site_1", &m.id).await.unwrap().unwrap();
    assert_eq!(started.state, "active");
    pass(Utc::now() + Duration::seconds(1)).await.unwrap();

    let (s, html, _) = send(
        Request::get(format!("/console/site_1/missions/{}", m.id))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{html}");
    assert!(
        html.contains("1 of 4"),
        "one question since the start: {html}"
    );
    assert!(html.contains("Pause mission"));
    let (s, html, _) = send(Request::get("/console/site_1").body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK, "{html}");
    assert!(
        html.contains("Mission: Answer more questions"),
        "Today shows the running mission"
    );

    let (s, _, _) = send(
        Request::post(format!("/console/site_1/missions/{}/state", m.id))
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("to=paused"))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    let (_, html, _) = send(
        Request::get(format!("/console/site_1/missions/{}", m.id))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert!(html.contains("Resume mission"), "{html}");
}

/// A site on Brick 1's schema with its team, and the signed-in owner.
async fn moved_site() -> (agentrix_host::NodeContext, Uuid) {
    let account = Uuid::now_v7();
    let mut ctx = context().await.unwrap();
    ctx.owners = Arc::new(FixedOwner::passkey(account));
    let pool = ctx.pool.clone();
    for f in [
        "auth/schema.sql",
        "claim/schema.sql",
        "db/migrations/0001_control.sql",
        "db/migrations/0002_passed_questions.sql",
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(node_sql(f)))
            .execute(&pool)
            .await
            .unwrap_or_else(|e| panic!("{f}: {e}"));
    }
    agentrix_shell::migrate(&pool).await.unwrap();
    agentrix_decisions::migrate(&pool).await.unwrap();
    agentrix_store::migrate(&pool).await.unwrap();
    agentrix_kernel::migrate(&pool).await.unwrap();
    sqlx::query("INSERT INTO accounts (id, email, display_name, created_at) VALUES (?, 'wanjiru@example.com', 'Wanjiru', 0)")
        .bind(account.to_string())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO sites (site_id, domain, name, old_origin, owner_account_id, created_at) VALUES ('site_1', 'kilele.example', 'Kilele Bakery', 'https://kilele.example', ?, ?)")
        .bind(account.to_string())
        .bind(t(Utc::now()))
        .execute(&pool)
        .await
        .unwrap();
    (ctx, account)
}

/// What the stand-in gateway was sent: headers and body of each call.
type Calls = Arc<std::sync::Mutex<Vec<(axum::http::HeaderMap, serde_json::Value)>>>;

/// A stand-in Model Gateway whose model always replies with `content`.
async fn stand_in_gateway(content: String) -> (String, Calls) {
    let calls: Calls = Arc::default();
    let seen = calls.clone();
    let app = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(
            move |headers: axum::http::HeaderMap,
                  axum::Json(body): axum::Json<serde_json::Value>| {
                let seen = seen.clone();
                let content = content.clone();
                async move {
                    seen.lock().unwrap().push((headers, body));
                    axum::Json(serde_json::json!({
                        "choices": [{ "message": { "role": "assistant", "content": content } }]
                    }))
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), calls)
}

async fn send(app: &axum::Router, req: Request<Body>) -> (StatusCode, String, Option<String>) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let to = res
        .headers()
        .get(header::LOCATION)
        .map(|v| v.to_str().unwrap().to_owned());
    let html =
        String::from_utf8(to_bytes(res.into_body(), 1 << 22).await.unwrap().to_vec()).unwrap();
    (status, html, to)
}

fn post_form(uri: &str, body: String) -> Request<Body> {
    Request::post(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap()
}

fn enc(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

#[tokio::test]
async fn the_chief_drafts_a_missions_contract_from_the_owners_words() {
    let (mut ctx, _) = moved_site().await;
    let rosters = agentrix_kernel::Rosters::new(ctx.pool.clone(), ctx.blobs.clone());
    let on_duty: Vec<String> = rosters
        .roster("site_1")
        .await
        .unwrap()
        .unwrap()
        .on_duty()
        .map(|m| m.spec.id.clone())
        .filter(|id| id != "coordinator")
        .collect();
    let lead = on_duty.last().unwrap().clone();
    let ends = (Utc::now() + Duration::days(12))
        .format("%Y-%m-%d")
        .to_string();
    // The stand-in model names one WebAgent on duty (last in the roster, to lead), one that
    // isn't on the team, a measure, the owner's number and an end date.
    let (url, calls) = stand_in_gateway(format!(
        "```json\n{{\"goal\": \"Answer 4 more visitor questions\", \"measure\": \"questions\", \"target\": 4, \"members\": [\"{lead}\", \"reputation-x\"], \"ends\": \"{ends}\", \"question\": \"Should questions by phone count too?\"}}\n```"
    ))
    .await;
    ctx.models_url = url;
    let app = agentrix_shell::mount(&ctx).unwrap().platform;

    let (s, html, _) = send(
        &app,
        Request::get("/console/site_1/missions")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{html}");
    assert!(html.contains("Draft the contract"), "{html}");
    assert!(html.contains("Fill in the contract yourself"), "{html}");

    let said = "Get us to answer 4 more visitor questions in the next two weeks";
    let (s, html, _) = send(
        &app,
        post_form(
            "/console/site_1/missions/draft",
            format!("said={}", enc(said)),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{html}");
    assert!(html.contains("Check the contract"), "{html}");
    assert!(html.contains("drafted this from your words"), "{html}");
    assert!(
        html.contains("Should questions by phone count too?"),
        "the Chief's one question: {html}"
    );
    assert!(
        html.contains("value=\"Answer 4 more visitor questions\""),
        "{html}"
    );
    assert!(html.contains("value=\"questions\" checked"), "{html}");
    assert!(html.contains("value=\"4\""), "{html}");
    assert!(html.contains(&format!("value=\"{ends}\"")), "{html}");
    assert!(html.contains("name=\"drafted\" value=\"chief\""), "{html}");
    assert!(
        html.contains(&format!("value=\"{lead}\" checked")),
        "the lead is ticked: {html}"
    );
    assert!(!html.contains("reputation-x"), "off the team: {html}");
    let lead_at = html.find(&format!("value=\"{lead}\"")).unwrap();
    for other in on_duty.iter().filter(|id| **id != lead) {
        let at = html.find(&format!("value=\"{other}\"")).unwrap();
        assert!(lead_at < at, "the lead is listed first, so it leads");
        assert!(
            !html.contains(&format!("value=\"{other}\" checked")),
            "only who the Chief named is ticked"
        );
    }

    // The model was asked as the Chief, with the owner's words and the team, nothing else.
    let calls = calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    let (headers, body) = &calls[0];
    assert_eq!(headers["x-agentrix-purpose"], "brief");
    assert_eq!(headers["x-agentrix-webagent"], "coordinator");
    assert_eq!(headers["x-agentrix-job"], "owner-questions");
    assert_eq!(headers["x-agentrix-site"], "site_1");
    let user: serde_json::Value =
        serde_json::from_str(body["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(user["said"], said);
    assert_eq!(user["counts_orders"], false);
    assert_eq!(user["team"].as_array().unwrap().len(), on_duty.len());
    assert!(
        !body.to_string().contains("wanjiru@example.com"),
        "no owner details reach the model"
    );

    // The owner checks the draft and writes the contract: the card says the Chief drafted it.
    let (s, html, to) = send(
        &app,
        post_form(
            "/console/site_1/missions",
            format!(
                "said={}&drafted=chief&goal={}&measure=questions&target=4&member={lead}&ends={ends}",
                enc(said),
                enc("Answer 4 more visitor questions")
            ),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER, "{html}");
    let to = to.unwrap();
    let card_id = to.rsplit('/').next().unwrap();
    let d = agentrix_decisions::Decisions::new(&ctx).with_runners(agentrix_shell::runners(&ctx));
    let row = d.get("site_1", card_id).await.unwrap().unwrap();
    assert!(
        row.card.summary.ends_with("drafted by your Chief WebAgent"),
        "{}",
        row.card.summary
    );
    assert!(
        row.card.effects[0].contains(said),
        "the owner's own words are on the card: {:?}",
        row.card.effects
    );
    assert_eq!(row.card.effect.args["drafted_by"], "chief");
    assert_eq!(row.card.effect.args["said"], said);
    assert_eq!(row.card.raised_by.webagent, "chief");
}

#[tokio::test]
async fn when_the_chief_cant_draft_the_owner_fills_the_contract_in_and_the_card_says_so() {
    let (mut ctx, _) = moved_site().await;
    // A gateway that isn't there.
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    ctx.models_url = format!("http://{}", closed.local_addr().unwrap());
    drop(closed);
    let app = agentrix_shell::mount(&ctx).unwrap().platform;

    let said = "Answer more questions before Christmas";
    let (s, html, _) = send(
        &app,
        post_form(
            "/console/site_1/missions/draft",
            format!("said={}", enc(said)),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{html}");
    assert!(
        html.contains("couldn&#x27;t draft this just now")
            || html.contains("couldn't draft this just now"),
        "{html}"
    );
    assert!(
        html.contains(&format!("value=\"{said}\"")),
        "the owner's words are the goal: {html}"
    );
    assert!(!html.contains("name=\"drafted\""), "{html}");
    assert!(!html.contains("drafted this from your words"), "{html}");

    // Too long or empty is refused before any model is asked.
    let (s, _, _) = send(
        &app,
        post_form("/console/site_1/missions/draft", "said=+".into()),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // The contract the owner writes says it's theirs, even if a form claims a draft.
    let rosters = agentrix_kernel::Rosters::new(ctx.pool.clone(), ctx.blobs.clone());
    let member = rosters
        .roster("site_1")
        .await
        .unwrap()
        .unwrap()
        .on_duty()
        .map(|m| m.spec.id.clone())
        .find(|id| id != "coordinator")
        .unwrap();
    let ends = (Utc::now() + Duration::days(20))
        .format("%Y-%m-%d")
        .to_string();
    let (s, html, to) = send(
        &app,
        post_form(
            "/console/site_1/missions",
            format!("drafted=chief&goal=Answer+more+questions&measure=questions&target=4&member={member}&ends={ends}"),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::SEE_OTHER, "{html}");
    let to = to.unwrap();
    let card_id = to.rsplit('/').next().unwrap();
    let d = agentrix_decisions::Decisions::new(&ctx).with_runners(agentrix_shell::runners(&ctx));
    let row = d.get("site_1", card_id).await.unwrap().unwrap();
    assert!(
        row.card.summary.ends_with("written by you"),
        "{}",
        row.card.summary
    );
    assert_eq!(row.card.effect.args["drafted_by"], "owner");
    assert!(
        row.card.effects[0].starts_with("Goal: "),
        "{:?}",
        row.card.effects
    );
}
