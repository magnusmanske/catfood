//! CatFood: RSS feeds of the latest files and pages in Wikimedia categories.

mod catfood;
mod db;
mod error;
mod html;
mod image_feed;
mod params;
mod rss;
mod timestamp;
mod wiki;

use axum::Router;
use axum::http::header::X_CONTENT_TYPE_OPTIONS;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::map_response;
use axum::response::Response;
use axum::routing::get;
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::net::TcpListener;
use wiki::Servers;
use wikimisc::server::{self, health};
use wikimisc::telemetry::Telemetry;
use wikimisc::toolforge::db::Cluster;
use wikimisc::toolforge::pool::WikiPool;

const DEFAULT_PORT: u16 = 8000;
/// Per database server. Tools may hold 10 connections in total, and Commons
/// feeds use two servers (core and links tables).
const POOL_MAX: usize = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const QUERY_TIMEOUT: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

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
    server::serve(listener, router(pool.clone())).await?;
    pool.disconnect().await?;
    Ok(())
}

fn router(pool: WikiPool) -> Router {
    let state = AppState {
        pool,
        servers: Servers::default(),
    };
    let router = Router::new()
        .route("/", get(catfood::feed))
        .route("/index.php", get(catfood::feed))
        .route("/catfood.php", get(catfood::feed))
        .route("/commons_image_feed.php", get(image_feed::feed))
        .route("/healthz", get(health::ok))
        .layer(map_response(security_headers))
        .with_state(state);
    let router = server::timeout(router, REQUEST_TIMEOUT, StatusCode::GATEWAY_TIMEOUT);
    server::request_metrics(server::standard_layers(router), &["/healthz"])
}

async fn security_headers(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use axum::http::header::{CONTENT_SECURITY_POLICY, LOCATION};
    use std::path::PathBuf;
    use std::sync::LazyLock;
    use tower::ServiceExt;

    /// A router whose pool never connects; enough for requests that don't
    /// reach the database
    fn test_router() -> Router {
        static MY_CNF: LazyLock<PathBuf> = LazyLock::new(|| {
            let path = std::env::temp_dir().join("catfood-test-replica.my.cnf");
            std::fs::write(&path, "[client]\nuser = test\npassword = test\n").unwrap();
            path
        });
        router(
            WikiPool::builder(Cluster::Web)
                .my_cnf_path(MY_CNF.as_path())
                .build()
                .unwrap(),
        )
    }

    async fn get(uri: &str) -> (StatusCode, Response<Body>) {
        let request = Request::get(uri).body(Body::empty()).unwrap();
        let response = test_router().oneshot(request).await.unwrap();
        (response.status(), response)
    }

    async fn body(response: Response<Body>) -> String {
        String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn form_without_category_or_user() {
        let (status, response) = get("/?language=%22%3E%3Cscript%3E").await;
        assert_eq!(status, StatusCode::OK);
        assert!(response.headers().contains_key(CONTENT_SECURITY_POLICY));
        assert_eq!(response.headers()[X_CONTENT_TYPE_OPTIONS], "nosniff");
        let html = body(response).await;
        assert!(html.contains("<form method=\"get\">"));
        assert!(html.contains("value=\"&quot;&gt;&lt;script&gt;\""));
        assert!(!html.contains("<script>"));
    }

    #[tokio::test]
    async fn legacy_paths() {
        for path in ["/catfood.php", "/index.php", "/commons_image_feed.php"] {
            assert_eq!(get(path).await.0, StatusCode::OK, "{path}");
        }
    }

    #[tokio::test]
    async fn motd_redirects_to_commons_feed() {
        let (status, response) = get("/catfood.php?motd=1").await;
        assert_eq!(status, StatusCode::TEMPORARY_REDIRECT);
        assert!(
            response.headers()[LOCATION]
                .to_str()
                .unwrap()
                .contains("feed=motd")
        );
    }

    #[tokio::test]
    async fn rejects_invalid_wiki() {
        let (status, _) = get("/?category=Cats&language=evil.example/&project=wikipedia").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn lenient_like_php() {
        // Repeated keys and blank values don't fail the request
        let (status, response) = get("/?language=en&language=de&category=%20_%20&depth=x").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body(response).await.contains("value=\"de\""));
        let (status, response) = get("/?namespace=-1").await;
        assert!(body(response).await.contains("<form"));
        let (status2, response) = get("/catfood.php?category=Cats&namespace=-1").await;
        assert_eq!(status, status2);
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body(response).await, "No files found.");
    }

    #[tokio::test]
    async fn health() {
        let (status, response) = get("/healthz").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body(response).await, "ok");
    }
}
