//! CatFood: RSS feeds of the latest files and pages in Wikimedia categories.

mod catfood;
mod db;
mod error;
mod html;
mod image_feed;
mod params;
mod routes;
mod rss;
mod timestamp;
mod wiki;

use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::net::TcpListener;
use wiki::Servers;
use wikimisc::server;
use wikimisc::telemetry::Telemetry;
use wikimisc::toolforge::db::Cluster;
use wikimisc::toolforge::pool::WikiPool;

const DEFAULT_PORT: u16 = 8000;
/// Per database server. Tools may hold 10 connections in total, and Commons
/// feeds use two servers (core and links tables).
const POOL_MAX: usize = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const QUERY_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct AppState {
    pool: WikiPool,
    servers: Servers,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    wikimisc::logging::init();
    // Needs $DASHBOARD_TOKEN; `server::serve` sends the final push on shutdown
    let _telemetry = Telemetry::builder("web")
        .version(env!("CARGO_PKG_VERSION"))
        .handle_signals(false)
        .install_or_log();
    let pool = WikiPool::builder(Cluster::Web)
        .pool_max(POOL_MAX)
        .connect_timeout(CONNECT_TIMEOUT)
        .max_statement_time(QUERY_TIMEOUT)
        .build()?;
    let port = std::env::var("PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    tracing::info!("listening on port {port}");
    let state = AppState {
        pool: pool.clone(),
        servers: Servers::default(),
    };
    server::serve(listener, routes::router(state)).await?;
    pool.disconnect().await?;
    Ok(())
}
