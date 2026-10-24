//! The HTTP routes, including permanent redirects from the PHP version's paths.

use crate::{AppState, catfood, image_feed};
use axum::Router;
use axum::extract::State;
use axum::http::header::{LOCATION, X_CONTENT_TYPE_OPTIONS};
use axum::http::{HeaderValue, StatusCode, Uri};
use axum::middleware::map_response;
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, get};
use std::time::Duration;
use wikimisc::server;

pub const BASE_URL: &str = "https://catfood.toolforge.org";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
/// The default wiki of both feeds
const HEALTH_DB: &str = "commonswiki";

pub fn router(state: AppState) -> Router {
    let router = Router::new()
        .route("/", get(catfood::feed))
        .route("/catfood", get(catfood::feed))
        .route("/commons_image_feed", get(image_feed::feed))
        .route("/index.php", moved_to("/"))
        .route("/catfood.php", moved_to("/catfood"))
        .route("/commons_image_feed.php", moved_to("/commons_image_feed"))
        .route("/healthz", get(health))
        .layer(map_response(security_headers))
        .with_state(state);
    let router = server::timeout(router, REQUEST_TIMEOUT, StatusCode::GATEWAY_TIMEOUT);
    server::request_metrics(server::standard_layers(router), &["/healthz"])
}

/// Redirects permanently to `path`, keeping the query string. 301 rather
/// than 308, which old feed readers may not know.
fn moved_to(path: &'static str) -> MethodRouter<AppState> {
    get(move |uri: Uri| async move {
        let location = match uri.query().filter(|query| !query.is_empty()) {
            Some(query) => format!("{BASE_URL}{path}?{query}"),
            None => format!("{BASE_URL}{path}"),
        };
        match HeaderValue::try_from(location) {
            Ok(location) => (StatusCode::MOVED_PERMANENTLY, [(LOCATION, location)]).into_response(),
            Err(_) => StatusCode::BAD_REQUEST.into_response(),
        }
    })
}

/// `OK` if a database connection is available
async fn health(State(state): State<AppState>) -> (StatusCode, &'static str) {
    match tokio::time::timeout(HEALTH_TIMEOUT, state.pool.connect(HEALTH_DB)).await {
        Ok(Ok(_)) => (StatusCode::OK, "OK"),
        Ok(Err(error)) => {
            tracing::warn!("health check failed: {error}");
            (StatusCode::SERVICE_UNAVAILABLE, "No database connection")
        }
        Err(_) => {
            tracing::warn!("health check timed out");
            (StatusCode::SERVICE_UNAVAILABLE, "No database connection")
        }
    }
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
    use crate::wiki::Servers;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use axum::http::header::CONTENT_SECURITY_POLICY;
    use std::path::PathBuf;
    use std::sync::LazyLock;
    use tower::ServiceExt;
    use wikimisc::toolforge::db::Cluster;
    use wikimisc::toolforge::pool::WikiPool;

    /// A router whose pool never connects; enough for requests that don't
    /// reach the database
    fn test_router() -> Router {
        static MY_CNF: LazyLock<PathBuf> = LazyLock::new(|| {
            let path = std::env::temp_dir().join("catfood-test-replica.my.cnf");
            std::fs::write(&path, "[client]\nuser = test\npassword = test\n").unwrap();
            path
        });
        let pool = WikiPool::builder(Cluster::Web)
            .my_cnf_path(MY_CNF.as_path())
            .build()
            .unwrap();
        router(AppState {
            pool,
            servers: Servers::default(),
        })
    }

    async fn get(uri: &str) -> (StatusCode, Response<Body>) {
        let request = Request::get(uri).body(Body::empty()).unwrap();
        let response = test_router().oneshot(request).await.unwrap();
        (response.status(), response)
    }

    async fn body(response: Response<Body>) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn form_without_category_or_user() {
        for path in ["/", "/catfood", "/commons_image_feed"] {
            assert_eq!(get(path).await.0, StatusCode::OK, "{path}");
        }
        let (status, response) = get("/catfood?language=%22%3E%3Cscript%3E").await;
        assert_eq!(status, StatusCode::OK);
        assert!(response.headers().contains_key(CONTENT_SECURITY_POLICY));
        assert_eq!(response.headers()[X_CONTENT_TYPE_OPTIONS], "nosniff");
        let html = body(response).await;
        assert!(html.contains("<form method=\"get\">"));
        assert!(html.contains("value=\"&quot;&gt;&lt;script&gt;\""));
        assert!(!html.contains("<script>"));
    }

    #[tokio::test]
    async fn legacy_paths_redirect_permanently() {
        let cases = [
            ("/index.php", "/"),
            ("/catfood.php?", "/catfood"),
            (
                "/catfood.php?category=A%20b&last=5",
                "/catfood?category=A%20b&last=5",
            ),
            (
                "/commons_image_feed.php?category=X",
                "/commons_image_feed?category=X",
            ),
        ];
        for (from, to) in cases {
            let (status, response) = get(from).await;
            assert_eq!(status, StatusCode::MOVED_PERMANENTLY, "{from}");
            assert_eq!(
                response.headers()[LOCATION],
                format!("{BASE_URL}{to}"),
                "{from}"
            );
        }
    }

    #[tokio::test]
    async fn motd_redirects_to_commons_feed() {
        let (status, response) = get("/catfood?motd=1").await;
        assert_eq!(status, StatusCode::TEMPORARY_REDIRECT);
        let location = response.headers()[LOCATION].to_str().unwrap();
        assert!(location.contains("feed=motd"));
    }

    #[tokio::test]
    async fn rejects_invalid_wiki() {
        let (status, _) = get("/catfood?category=Cats&language=evil.example/").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn lenient_like_php() {
        // Repeated keys and blank values don't fail the request
        let (status, response) = get("/?language=en&language=de&category=%20_%20&depth=x").await;
        assert_eq!(status, StatusCode::OK);
        assert!(body(response).await.contains("value=\"de\""));
        let (_, response) = get("/?namespace=-1").await;
        assert!(body(response).await.contains("<form"));
        let (status, response) = get("/catfood?category=Cats&namespace=-1").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body(response).await, "No files found.");
    }

    #[tokio::test]
    async fn health_needs_database() {
        let (status, response) = get("/healthz").await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body(response).await, "No database connection");
    }
}
