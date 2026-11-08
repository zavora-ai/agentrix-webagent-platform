//! `agentrix-models`: the Model Gateway (design §4.3, D10; R11). Task S1-04.
//!
//! The only process that holds model-provider keys, Vultr's and Anthropic's (R11.1).
//! agentrix-node calls it on loopback. It:
//! - routes each purpose to a tier, and each tier to a model on the configured provider
//!   (`AGENTRIX_PROVIDER`, `AGENTRIX_TIER_*`), with specialists always on Vultr;
//! - meters every call into `control.db`'s `usage` table;
//! - stops a site that has spent its budget, or the gateway's daily limit;
//! - backs off on 429, 5xx and 529;
//! - never switches provider by itself (R11.2–R11.4).
//!
//! Development settings (D10):
//! - `AGENTRIX_MODELS_MODE=live|record|replay`. Tests and CI use `replay`, which needs no key
//!   and no network.
//! - `AGENTRIX_ROUTE_<PURPOSE>=<provider>:<model>` overrides one purpose, for example
//!   `AGENTRIX_ROUTE_BUILD=vultr:deepseek-v4.1-flash-1017`.
//! - `AGENTRIX_SITE_DAILY_CAP_USD` sets the hard limit per site per day (default 5).
//! - `AGENTRIX_SAMPLES_DAILY_CAP_USD` sets the limit for the `-sample` sites together per UTC
//!   day (default 3), and `AGENTRIX_FLEET_DAILY_CAP_USD` the limit for every site together
//!   (default 8). Zero refuses every call; it never means unlimited.
//!
//! Claude is called through adk-model's Anthropic client (R11.6). Vultr calls forward OpenAI
//! wire JSON with `reqwest`, so tools, images and reasoning parameters pass through untouched.

mod cache;
mod claude;
mod gateway;
mod meter;
mod route;
mod select;
mod upstream;

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;

/// Command-line arguments. Each also reads from the environment set by cloud-init.
#[derive(Debug, Parser)]
#[command(
    version,
    about = "Agentrix Model Gateway on Vultr Serverless Inference"
)]
struct Args {
    /// Where to listen. Loopback only.
    #[arg(long, env = "AGENTRIX_MODELS_LISTEN", default_value = "127.0.0.1:8090")]
    listen: SocketAddr,
    /// The control database, created and migrated by agentrix-node.
    #[arg(
        long,
        env = "AGENTRIX_CONTROL_DB",
        default_value = "/var/lib/agentrix/control.db"
    )]
    db: PathBuf,
    /// Vultr Serverless Inference's base URL.
    #[arg(
        long,
        env = "VULTR_INFERENCE_BASE_URL",
        default_value = "https://api.vultrinference.com/v1"
    )]
    base_url: String,
    /// live, record or replay (design D10a).
    #[arg(long, env = "AGENTRIX_MODELS_MODE", default_value = "live")]
    mode: String,
    /// Where recordings live; by default this crate's `recordings/` in the repo.
    #[arg(long, env = "AGENTRIX_MODELS_RECORDINGS")]
    recordings: Option<PathBuf>,
    /// The hard model-spend limit per site per day, in US dollars (design D10e).
    #[arg(long, env = "AGENTRIX_SITE_DAILY_CAP_USD", default_value_t = 5.0)]
    daily_cap_usd: f64,
    /// The limit for every site together per UTC day, in US dollars. 0 refuses every call.
    #[arg(long, env = "AGENTRIX_FLEET_DAILY_CAP_USD", default_value_t = meter::DEFAULT_PLATFORM_CAP_USD)]
    fleet_daily_cap_usd: f64,
    /// The limit for the `-sample` sites together per UTC day, in US dollars. 0 refuses them.
    #[arg(long, env = "AGENTRIX_SAMPLES_DAILY_CAP_USD", default_value_t = meter::DEFAULT_SAMPLES_CAP_USD)]
    samples_daily_cap_usd: f64,
    /// A dotenv file to read VULTR_INFERENCE_API_KEY from when the environment lacks it (local
    /// development only; on the VM the key comes from /etc/agentrix/secrets/).
    #[arg(long)]
    env_file: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    let args = Args::parse();
    anyhow::ensure!(
        args.listen.ip().is_loopback(),
        "agentrix-models listens on loopback only (design §4.3), not {}",
        args.listen
    );
    for (name, usd) in [
        ("AGENTRIX_SITE_DAILY_CAP_USD", args.daily_cap_usd),
        ("AGENTRIX_FLEET_DAILY_CAP_USD", args.fleet_daily_cap_usd),
        ("AGENTRIX_SAMPLES_DAILY_CAP_USD", args.samples_daily_cap_usd),
    ] {
        anyhow::ensure!(
            usd.is_finite() && usd >= 0.0,
            "{name} must be a dollar amount, not {usd}"
        );
    }
    tracing::info!(
        site = args.daily_cap_usd,
        samples = args.samples_daily_cap_usd,
        platform = args.fleet_daily_cap_usd,
        "daily model spend limits in US dollars"
    );
    let mode = cache::Mode::parse(&args.mode)?;
    let routing = route::Routing::from_env()?;
    for (purpose, choice) in routing.table() {
        tracing::info!(
            purpose = purpose.as_str(),
            provider = choice.provider.as_str(),
            model = %choice.model,
            tier = choice.tier.map_or("specialist", route::Tier::as_str),
            "route"
        );
    }
    let (vultr, claude) = match mode {
        cache::Mode::Replay => (None, None),
        cache::Mode::Live | cache::Mode::Record => {
            let env_file = args.env_file.as_deref();
            let vultr = upstream::Vultr::new(&args.base_url, upstream::ApiKey::load(env_file)?)?;
            let claude = if routing.uses(route::Provider::Anthropic) {
                Some(claude::Claude::new(upstream::read_key(
                    "ANTHROPIC_API_KEY",
                    env_file,
                )?))
            } else {
                None
            };
            (Some(vultr), claude)
        }
    };
    let recordings = cache::Recordings::new(
        args.recordings
            .unwrap_or_else(cache::Recordings::default_dir),
    );
    let pool = meter::open(&args.db).await.map_err(|e| {
        anyhow::anyhow!(
            "cannot open {} (agentrix-node creates it): {e}",
            args.db.display()
        )
    })?;
    let gateway = gateway::Gateway::new(gateway::Config {
        vultr,
        claude,
        pool,
        routing,
        mode,
        recordings,
        daily_cap_micro_usd: (args.daily_cap_usd * 1e6).round() as i64,
        shared_caps: meter::SharedCaps {
            platform_micro_usd: (args.fleet_daily_cap_usd * 1e6).round() as i64,
            samples_micro_usd: (args.samples_daily_cap_usd * 1e6).round() as i64,
        },
    })?;
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    tracing::info!(listen = %args.listen, mode = mode.as_str(), "model gateway ready");
    axum::serve(listener, gateway::router(gateway)).await?;
    Ok(())
}
