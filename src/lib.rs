//! CatFood: RSS feeds of the latest files and pages in Wikimedia categories.

pub mod catfood;
pub mod db;
pub mod error;
pub mod html;
pub mod image_feed;
pub mod params;
pub mod routes;
pub mod rss;
pub mod source;
pub mod timestamp;
pub mod wiki;

use std::path::Path;
use wikimisc::replicas::Replicas;
use wikimisc::toolforge::db::Cluster;
use wiki::Servers;

pub use source::{Database, SingleWiki};

/// The standalone Toolforge deployment.
pub const DEFAULT_BASE_URL: &str = "https://catfood.toolforge.org";
pub const DEFAULT_TOOL_URL: &str = "https://catfood.toolforge.org/catfood";

#[derive(Debug, Clone)]
pub struct AppState {
    pub database: Database,
    /// Where the service is reachable, without a trailing slash.
    pub base_url: String,
    /// The canonical URL of the main feed, without a trailing slash.
    pub tool_url: String,
}

impl AppState {
    pub fn new(
        database: Database,
        base_url: impl Into<String>,
        tool_url: impl Into<String>,
    ) -> Self {
        Self {
            database,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            tool_url: tool_url.into().trim_end_matches('/').to_string(),
        }
    }

    /// The standalone Toolforge service: credentials from the environment or
    /// `replica.my.cnf`, and the public Toolforge URLs.
    pub fn from_environment() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let replicas = Replicas::builder(Cluster::Web)
            .pool_max(5)
            .connect_timeout(std::time::Duration::from_secs(10))
            .max_statement_time(std::time::Duration::from_secs(60))
            .build()?;
        Ok(Self::new(
            Database::Replicas { replicas, servers: Servers::default() },
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
            .pool_max(5)
            .connect_timeout(std::time::Duration::from_secs(10))
            .max_statement_time(std::time::Duration::from_secs(60))
            .build()?;
        Ok(Self::new(
            Database::Replicas { replicas, servers: Servers::default() },
            base_url,
            tool_url,
        ))
    }

    /// The embedded service: one live MediaWiki database, not the replicas.
    pub fn from_single(
        single: SingleWiki,
        base_url: impl Into<String>,
        tool_url: impl Into<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self::new(Database::single(single)?, base_url, tool_url))
    }
}
