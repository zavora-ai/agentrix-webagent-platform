//! The shell on sample data, for comparing pages with `concept/console/` in a browser.
//!
//! `scripts/wcargo run -p agentrix-shell --example shell-preview` then open
//! <http://localhost:18500/console/site_kilele_sample>. Set `AGENTRIX_SHELL_PORT` to move it,
//! and `AGENTRIX_SHELL_NO_PASSKEY=1` to see the authenticator-code path. The data is the
//! concept's sample bakery, marked Sample; nothing here talks to a real node.

use std::sync::Arc;

use agentrix_shell::{Shell, ShellState, router, sample::SamplePort, store::ShellStore};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port = std::env::var("AGENTRIX_SHELL_PORT").unwrap_or_else(|_| "18500".into());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    let sample = SamplePort {
        has_passkey: std::env::var("AGENTRIX_SHELL_NO_PASSKEY").is_err(),
        ..SamplePort::default()
    };
    let state = ShellState {
        shell: Arc::new(Shell::new()?),
        port: Arc::new(sample),
        store: ShellStore::open(pool).await?,
        apps: Vec::new(),
        lanes: Vec::new(),
        freeze_enabled: true,
    };
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await?;
    println!(
        "Shell preview on http://localhost:{port}/console/{}",
        agentrix_shell::sample::SITE
    );
    // Node mounts the kit's assets through agentrix-ui; the preview serves them itself.
    axum::serve(
        listener,
        router(state)
            .merge(agentrix_ui::assets())
            .merge(agentrix_kernel::site_routes()),
    )
    .await?;
    Ok(())
}
