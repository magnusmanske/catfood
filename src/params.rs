//! Query parameter parsing shared by the handlers.

use serde::{Deserialize, Deserializer};
use std::str::FromStr;

/// Deserializes a number leniently: empty or malformed values become `None`,
/// so the caller's default applies, instead of failing the whole request.
pub fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
{
    Ok(Option::<String>::deserialize(deserializer)?.and_then(|s| s.trim().parse().ok()))
}

/// `value`, or `default` if it is blank
pub fn non_empty<'a>(value: &'a str, default: &'a str) -> &'a str {
    match value.trim() {
        "" => default,
        trimmed => trimmed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Query;
    use axum::http::Uri;

    #[derive(Debug, Default, Deserialize)]
    #[serde(default)]
    struct Params {
        #[serde(deserialize_with = "lenient")]
        n: Option<u32>,
    }

    fn parse(query: &str) -> Option<u32> {
        let uri: Uri = format!("/?{query}").parse().unwrap();
        Query::<Params>::try_from_uri(&uri).unwrap().0.n
    }

    #[test]
    fn lenient_numbers() {
        assert_eq!(parse("n=5"), Some(5));
        assert_eq!(parse("n=+5"), Some(5));
        assert_eq!(parse("n="), None);
        assert_eq!(parse("n=abc"), None);
        assert_eq!(parse("n=-1"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn non_empty_falls_back() {
        assert_eq!(non_empty(" en ", "commons"), "en");
        assert_eq!(non_empty("  ", "commons"), "commons");
    }
}
