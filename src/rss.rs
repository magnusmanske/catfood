//! Helpers for writing RSS feeds.

use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Response};
use std::fmt;
use wikimisc::escape::xml;

/// A writer that XML-escapes everything written to it, e.g. to embed HTML in
/// an RSS `<description>`
pub struct XmlText<'a, W: fmt::Write>(pub &'a mut W);

impl<W: fmt::Write> fmt::Write for XmlText<'_, W> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0.write_str(&xml(s))
    }
}

pub fn response(
    feed: impl fmt::Display,
    content_type: &'static str,
    cache_control: &'static str,
) -> Response {
    (
        [(CONTENT_TYPE, content_type), (CACHE_CONTROL, cache_control)],
        feed.to_string(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    #[test]
    fn escapes_what_is_written() {
        let mut out = String::new();
        let text = "&lt;";
        write!(XmlText(&mut out), "<a href=\"x\">{text}</a>").unwrap();
        assert_eq!(out, "&lt;a href=&quot;x&quot;&gt;&amp;lt;&lt;/a&gt;");
    }
}
