use catfood::AppState;
use std::net::Ipv4Addr;
use tokio::net::TcpListener;
use wikimisc::server;
use wikimisc::telemetry;

const DEFAULT_PORT: u16 = 8000;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    wikimisc::logging::init();
    // Needs $DASHBOARD_TOKEN; `server::serve` sends the final push on shutdown
    let _telemetry = telemetry::install_web(env!("CARGO_PKG_VERSION"), None);
    let state = AppState::from_environment()?;
    let database = state.database.clone();
    let port = std::env::var("PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    tracing::info!("listening on port {port}");
    server::serve(listener, catfood::routes::router(state)).await?;
    database.disconnect().await?;
    Ok(())
}
