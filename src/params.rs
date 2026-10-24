//! Query parameters, parsed as leniently as PHP did, so old feed URLs keep
//! working.

use std::collections::HashMap;

/// The query string; for repeated keys, the last value wins
#[derive(Debug, Default)]
pub struct Params(HashMap<String, String>);

impl Params {
    pub fn new(map: HashMap<String, String>) -> Self {
        Self(map)
    }

    /// The trimmed value, or `default` if missing or blank
    pub fn text<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        match self.0.get(key).map(|value| value.trim()) {
            None | Some("") => default,
            Some(value) => value,
        }
    }

    /// The value exactly as given, or `default` if missing, as PHP's
    /// `getRequest` returned it
    pub fn raw<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.0.get(key).map_or(default, String::as_str)
    }

    /// `None` if missing; otherwise the number the value starts with, like
    /// PHP's `(int)` cast: `"12abc"` and `"12.9"` are 12, garbage is 0, and
    /// huge numbers saturate
    pub fn int(&self, key: &str) -> Option<i64> {
        self.0.get(key).map(|value| leading_int(value))
    }
}

fn leading_int(s: &str) -> i64 {
    let s = s.trim_start();
    let (negative, unsigned) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let digits = unsigned.bytes().take_while(u8::is_ascii_digit).count();
    let magnitude = unsigned.get(..digits).and_then(|d| d.parse::<i64>().ok());
    match (magnitude, negative) {
        _ if digits == 0 => 0,
        (Some(n), true) => -n,
        (Some(n), false) => n,
        (None, true) => i64::MIN,
        (None, false) => i64::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Query;
    use axum::http::Uri;

    fn parse(query: &str) -> Params {
        let uri: Uri = format!("/?{query}").parse().unwrap();
        Params::new(Query::try_from_uri(&uri).unwrap().0)
    }

    #[test]
    fn ints_like_php() {
        let params = parse("a=5&b=12abc&c=12.9&d=&e=abc&f=-3&g=%2B4&h=99999999999999999999&i=-");
        assert_eq!(params.int("a"), Some(5));
        assert_eq!(params.int("b"), Some(12));
        assert_eq!(params.int("c"), Some(12));
        assert_eq!(params.int("d"), Some(0));
        assert_eq!(params.int("e"), Some(0));
        assert_eq!(params.int("f"), Some(-3));
        assert_eq!(params.int("g"), Some(4));
        assert_eq!(params.int("h"), Some(i64::MAX));
        assert_eq!(params.int("i"), Some(0));
        assert_eq!(params.int("missing"), None);
    }

    #[test]
    fn last_value_wins() {
        assert_eq!(parse("x=a&x=b").text("x", ""), "b");
    }

    #[test]
    fn text_falls_back_when_blank() {
        let params = parse("a=+en+&b=++");
        assert_eq!(params.text("a", "commons"), "en");
        assert_eq!(params.text("b", "commons"), "commons");
        assert_eq!(params.text("c", "commons"), "commons");
        assert_eq!(params.raw("a", "commons"), " en ");
        assert_eq!(params.raw("c", "commons"), "commons");
    }
}
