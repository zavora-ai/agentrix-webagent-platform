//! The Store (01b C14, C15; concept/console/store.html): the WebAgents a site doesn't have yet,
//! from W1's roster, and adding one. Adding is a decision: the console raises a card v2
//! (`webagent.add`) the owner approves with their passkey or a code, and [`AddWebAgent`] puts
//! the WebAgent on duty once that approval's receipt is written. Packs and connections show
//! when their catalogues exist; nothing here invents a listing.

use std::sync::Arc;

use agentrix_decisions::{DecisionCardV2, EffectRunner};
use agentrix_host::NodeContext;
use agentrix_kernel::{DeskDraft, Rosters, SiteLook};
use anyhow::Context;
use async_trait::async_trait;

/// The effect tool a Store card carries.
pub const TOOL: &str = "webagent.add";

/// Puts a WebAgent on duty for a site after the owner approved its card.
pub struct AddWebAgent {
    rosters: Rosters,
}

impl AddWebAgent {
    /// Over node's roster tables.
    pub fn new(ctx: &NodeContext) -> Self {
        Self {
            rosters: Rosters::new(ctx.pool.clone(), ctx.blobs.clone()),
        }
    }
}

#[async_trait]
impl EffectRunner for AddWebAgent {
    fn tool(&self) -> &str {
        TOOL
    }

    async fn run(&self, card: &DecisionCardV2, _receipt: &str) -> anyhow::Result<Option<String>> {
        let site = card.site.as_str();
        let id = card.effect.args["webagent"]
            .as_str()
            .context("the card names no WebAgent")?;
        let roster = self
            .rosters
            .roster(site)
            .await?
            .context("this site has no WebAgents to add to")?;
        anyhow::ensure!(
            roster.get(id).is_some(),
            "{id} isn't a WebAgent this platform offers"
        );
        // Keep the site's recorded team and desks; a moved site with nothing recorded yet starts
        // from the roster it already shows.
        let row: Option<(String, String, String)> =
            sqlx::query_as("SELECT look, on_duty, desks FROM webagent_sites WHERE site_id = ?")
                .bind(site)
                .fetch_optional(self.rosters.pool())
                .await
                .unwrap_or(None);
        let (look, mut on_duty, desks): (SiteLook, Vec<String>, Vec<DeskDraft>) = match row {
            Some((look, on, desks)) => (
                serde_json::from_str(&look)?,
                serde_json::from_str(&on)?,
                serde_json::from_str(&desks)?,
            ),
            None => (
                roster.look.clone(),
                roster.on_duty().map(|m| m.spec.id.clone()).collect(),
                Vec::new(),
            ),
        };
        if !on_duty.iter().any(|w| w == id) {
            on_duty.push(id.to_owned());
        }
        let on: Vec<&str> = on_duty.iter().map(String::as_str).collect();
        self.rosters.set_team(site, &look, &on, &desks).await?;
        Ok(Some(format!("/console/{site}/webagents#agent-{id}")))
    }
}

/// The effect runners the console registers with `agentrix_decisions::mount_with`.
pub fn runners(ctx: &NodeContext) -> Vec<Arc<dyn EffectRunner>> {
    vec![
        Arc::new(AddWebAgent::new(ctx)),
        Arc::new(crate::missions::StartMission::new(ctx)),
    ]
}
