//! Where the feeds read their rows: the Wikimedia replicas (Toolforge), or a
//! single live MediaWiki database when the tool is embedded.

use crate::error::AppError;
use crate::wiki::{self, Servers, Wiki};
use std::time::Duration;
use wikimisc::mysql_async::Conn;
use wikimisc::replicas::Replicas;
use wikimisc::toolforge::pool::CustomPool;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const QUERY_TIMEOUT: Duration = Duration::from_secs(60);
const POOL_MAX: usize = 5;

/// A single wiki's database, for an embedded deployment.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SingleWiki {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub user: String,
    #[serde(default)]
    pub password: String,
    /// The wiki's database name.
    pub database: String,
    /// The wiki's server root, e.g. `https://example.org`.
    pub server: String,
    /// Optional display name.
    #[serde(default)]
    pub name: String,
}

const fn default_port() -> u16 {
    3306
}

/// The database(s) behind the feeds.
#[derive(Debug, Clone)]
pub enum Database {
    /// The Wikimedia replica databases, the Toolforge default.
    Replicas { replicas: Replicas, servers: Servers },
    /// One live MediaWiki database.
    Single { pool: CustomPool, database: String, server: String, name: String },
}

impl Database {
    /// A connection able to read `tables` of `dbname`.
    pub async fn conn(&self, dbname: &str, tables: &[&str]) -> Result<Conn, AppError> {
        match self {
            Self::Replicas { replicas, .. } => Ok(replicas.connect_for_tables(dbname, tables).await?),
            Self::Single { pool, .. } => Ok(pool.connect().await?),
        }
    }

    /// The wiki for a `language`/`project` pair.
    pub async fn resolve(&self, language: &str, project: &str) -> Result<Wiki, AppError> {
        match self {
            Self::Single { database, server, name, .. } => Ok(Wiki::single(database, server, name)),
            Self::Replicas { servers, .. } => {
                let dbname = wiki::dbname(language, project).ok_or(AppError::BadRequest("Invalid language."))?;
                let server = servers.get(self, &dbname).await?.ok_or(AppError::UnknownWiki)?;
                Ok(Wiki::replica(&dbname, &server))
            }
        }
    }

    /// The wiki of the Commons feeds: Commons on Toolforge, the local wiki when
    /// embedded.
    pub fn commons(&self) -> Wiki {
        match self {
            Self::Single { database, server, name, .. } => Wiki::single(database, server, name),
            Self::Replicas { .. } => Wiki::commons(),
        }
    }

    pub const fn is_single(&self) -> bool {
        matches!(self, Self::Single { .. })
    }

    /// Whether a database connection works.
    pub async fn healthy(&self) -> bool {
        let check = async {
            match self {
                Self::Replicas { replicas, .. } => replicas.pool().connect("commonswiki").await.map(|_| ()),
                Self::Single { pool, .. } => pool.connect().await.map(|_| ()),
            }
        };
        matches!(tokio::time::timeout(CONNECT_TIMEOUT, check).await, Ok(Ok(())))
    }

    /// Closes the pools; the final push should have been sent.
    pub async fn disconnect(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self {
            Self::Replicas { replicas, .. } => Ok(replicas.clone().disconnect().await?),
            Self::Single { pool, .. } => Ok(pool.clone().disconnect().await?),
        }
    }

    /// Builds the pool for [`SingleWiki`].
    pub fn single(single: SingleWiki) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let pool = CustomPool::builder(single.host, single.user, single.password)
            .port(single.port)
            .database(single.database.clone())
            .pool_max(POOL_MAX)
            .connect_timeout(CONNECT_TIMEOUT)
            .max_statement_time(QUERY_TIMEOUT)
            .build()?;
        Ok(Self::Single {
            pool,
            database: single.database,
            server: single.server,
            name: single.name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single() -> SingleWiki {
        SingleWiki {
            host: "127.0.0.1".into(),
            port: 3306,
            user: "u".into(),
            password: "p".into(),
            database: "my_wiki".into(),
            server: "http://localhost".into(),
            name: "Test wiki".into(),
        }
    }

    #[test]
    fn single_mode_maps_to_the_local_wiki() {
        let database = Database::single(single()).unwrap();
        assert!(database.is_single());
        let wiki = database.commons();
        assert_eq!(wiki.dbname, "my_wiki");
        assert_eq!(wiki.server, "http://localhost");
    }

    #[tokio::test]
    async fn single_mode_ignores_language_and_project() {
        let database = Database::single(single()).unwrap();
        let wiki = database.resolve("de", "wiktionary").await.unwrap();
        assert_eq!(wiki.dbname, "my_wiki");
    }
}
