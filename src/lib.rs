//! CatFood: RSS feeds of the latest files and pages in Wikimedia categories.

pub mod catfood;
pub mod db;
pub mod error;
pub mod html;
pub mod image_feed;
pub mod params;
pub mod routes;
pub mod rss;
pub mod timestamp;
pub mod wiki;

use std::path::Path;
use std::time::Duration;
use wikimisc::replicas::Replicas;
use wikimisc::toolforge::db::Cluster;
use wiki::Servers;

/// The standalone Toolforge deployment.
pub const DEFAULT_BASE_URL: &str = "https://catfood.toolforge.org";
pub const DEFAULT_TOOL_URL: &str = "https://catfood.toolforge.org/catfood";

/// Per database server. Tools may hold 10 connections in total, and Commons
/// feeds use two servers (core and links tables).
const POOL_MAX: usize = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const QUERY_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct AppState {
    pub replicas: Replicas,
    pub servers: Servers,
    /// Where the service is reachable, without a trailing slash.
    pub base_url: String,
    /// The canonical URL of the main feed, without a trailing slash.
    pub tool_url: String,
}

impl AppState {
    pub fn new(
        replicas: Replicas,
        servers: Servers,
        base_url: impl Into<String>,
        tool_url: impl Into<String>,
    ) -> Self {
        Self {
            replicas,
            servers,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            tool_url: tool_url.into().trim_end_matches('/').to_string(),
        }
    }

    /// The standalone Toolforge service: credentials from the environment or
    /// `replica.my.cnf`, and the public Toolforge URLs.
    pub fn from_environment() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let replicas = Replicas::builder(Cluster::Web)
            .pool_max(POOL_MAX)
            .connect_timeout(CONNECT_TIMEOUT)
            .max_statement_time(QUERY_TIMEOUT)
            .build()?;
        Ok(Self::new(
            replicas,
            Servers::default(),
            DEFAULT_BASE_URL,
            DEFAULT_TOOL_URL,
        ))
    }

    /// Builds the replica pool from a `replica.my.cnf`, for embedding.
    pub fn from_credentials(
        credentials: &Path,
        base_url: impl Into<String>,
        tool_url: impl Into<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let replicas = Replicas::builder(Cluster::Web)
            .my_cnf_path(credentials)
            .pool_max(POOL_MAX)
            .connect_timeout(CONNECT_TIMEOUT)
            .max_statement_time(QUERY_TIMEOUT)
            .build()?;
        Ok(Self::new(replicas, Servers::default(), base_url, tool_url))
    }
}
