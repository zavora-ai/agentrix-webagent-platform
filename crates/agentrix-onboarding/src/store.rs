//! W4's own records in `control.db`, with their own migrator (`migrations/`, range 0400–0499).

use chrono::{SecondsFormat, Utc};
use futures::future::BoxFuture;
use sqlx::SqlitePool;

/// Runs W4's migrations (node calls this once at startup, before mounting).
pub fn migrate(pool: &SqlitePool) -> BoxFuture<'_, anyhow::Result<()>> {
    Box::pin(async move {
        let mut m = sqlx::migrate!("./migrations");
        m.set_ignore_missing(true);
        m.dangerous_set_table_name("_sqlx_migrations_agentrix_onboarding");
        m.run(pool)
            .await
            .map_err(|e| anyhow::anyhow!("agentrix-onboarding migrations: {e}"))?;
        Ok(())
    })
}

/// The plan an owner chose for a site during setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanChoice {
    /// The site being set up.
    pub site_id: String,
    /// The owner's account that chose it.
    pub account: String,
    /// A plan id from W3's catalogue (`essential`, `business`, `commerce`).
    pub plan_id: String,
    /// The monthly AI budget the owner set as a limit, whole US dollars.
    pub ai_budget_usd: i64,
    /// When, RFC 3339 UTC.
    pub chosen_at: String,
}

/// Saves `choice`, replacing an earlier one for the same site.
pub async fn save_plan_choice(pool: &SqlitePool, choice: &PlanChoice) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO onboarding_plan_choices (site_id, account, plan_id, ai_budget_usd, chosen_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(site_id) DO UPDATE SET account = ?2, plan_id = ?3, ai_budget_usd = ?4, chosen_at = ?5",
    )
    .bind(&choice.site_id)
    .bind(&choice.account)
    .bind(&choice.plan_id)
    .bind(choice.ai_budget_usd)
    .bind(&choice.chosen_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// The plan chosen for `site_id`, if any.
pub async fn plan_choice(pool: &SqlitePool, site_id: &str) -> anyhow::Result<Option<PlanChoice>> {
    let row: Option<(String, String, String, i64, String)> = sqlx::query_as(
        "SELECT site_id, account, plan_id, ai_budget_usd, chosen_at FROM onboarding_plan_choices WHERE site_id = ?1",
    )
    .bind(site_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(site_id, account, plan_id, ai_budget_usd, chosen_at)| PlanChoice {
            site_id,
            account,
            plan_id,
            ai_budget_usd,
            chosen_at,
        },
    ))
}

/// Now, as stored.
pub fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
