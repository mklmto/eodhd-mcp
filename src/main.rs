mod analytics;
mod cache;
mod client;
mod format;
mod server;
mod tools;
mod types;

use rmcp::{transport::stdio, ServiceExt};
use server::EodhdServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Log to stderr — stdout is reserved for JSON-RPC protocol
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("eodhd_mcp=info".parse()?),
        )
        .init();

    let api_key = std::env::var("EODHD_API_KEY").unwrap_or_else(|_| {
        tracing::warn!("EODHD_API_KEY not set, using demo key (limited to AAPL.US, TSLA.US, VTI.US, AMZN.US, BTC-USD.CC, EURUSD.FOREX)");
        "demo".to_string()
    });

    tracing::info!("Starting EODHD MCP server");

    let server = EodhdServer::new(api_key);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;

    Ok(())
}
