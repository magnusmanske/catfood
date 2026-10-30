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
use wikimisc::replicas::Replicas;
use wikimisc::server;
use wikimisc::telemetry;
use wikimisc::toolforge::db::Cluster;

const DEFAULT_PORT: u16 = 8000;
/// Per database server. Tools may hold 10 connections in total, and Commons
/// feeds use two servers (core and links tables).
const POOL_MAX: usize = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const QUERY_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct AppState {
    replicas: Replicas,
    servers: Servers,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    wikimisc::logging::init();
    // Needs $DASHBOARD_TOKEN; `server::serve` sends the final push on shutdown
    let _telemetry = telemetry::install_web(env!("CARGO_PKG_VERSION"), None);
    let replicas = Replicas::builder(Cluster::Web)
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
        replicas: replicas.clone(),
        servers: Servers::default(),
    };
    server::serve(listener, routes::router(state)).await?;
    replicas.disconnect().await?;
    Ok(())
}
