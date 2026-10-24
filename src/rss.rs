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

/// Like PHP's `urlencode`, to keep the GUIDs of the PHP version
pub fn php_urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => {
                out.push(char::from(byte))
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
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
    fn urlencode_like_php() {
        assert_eq!(
            php_urlencode("Foo_(bar)_é~ x.jpg"),
            "Foo_%28bar%29_%C3%A9%7E+x.jpg"
        );
    }

    #[test]
    fn escapes_what_is_written() {
        let mut out = String::new();
        let text = "&lt;";
        write!(XmlText(&mut out), "<a href=\"x\">{text}</a>").unwrap();
        assert_eq!(out, "&lt;a href=&quot;x&quot;&gt;&amp;lt;&lt;/a&gt;");
    }
}
