//! The HTML pages shown when no feed was requested.

use axum::http::header::{CONTENT_SECURITY_POLICY, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};

const CSP: &str = "default-src 'none'; style-src https://tools-static.wmflabs.org; \
    form-action 'self'; base-uri 'none'; frame-ancestors 'none'";

/// A page with the common header. `body` must be safe HTML.
pub fn page(title: &str, body: &str) -> Response {
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="robots" content="noindex, nofollow">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="https://tools-static.wmflabs.org/cdnjs/ajax/libs/twitter-bootstrap/4.6.2/css/bootstrap.min.css">
</head>
<body>
<div class="container">
<nav class="navbar navbar-light bg-light mb-3">
<a class="navbar-brand" href="?">{title}</a>
<ul class="navbar-nav flex-row">
<li class="nav-item ml-3"><a class="nav-link" href="https://magnustools.toolforge.org">Tools</a></li>
<li class="nav-item ml-3"><a class="nav-link" href="https://codeberg.org/magnusmanske">Git</a></li>
<li class="nav-item ml-3"><a class="nav-link" href="https://en.wikipedia.org/wiki/User_talk:Magnus_Manske">Talk</a></li>
</ul>
</nav>
{body}
</div>
</body>
</html>
"#
    );
    (
        [
            (CONTENT_TYPE, "text/html; charset=utf-8"),
            (CONTENT_SECURITY_POLICY, CSP),
        ],
        html,
    )
        .into_response()
}
