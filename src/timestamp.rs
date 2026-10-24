//! MediaWiki timestamps (`YYYYMMDDHHMMSS`, UTC) and their RSS form.

use chrono::{NaiveDateTime, Utc};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(NaiveDateTime);

impl Timestamp {
    pub fn now() -> Self {
        Self(Utc::now().naive_utc())
    }

    /// RFC 822 date, as RSS wants it: `Sat, 24 Oct 2026 08:15:00 GMT`
    pub fn rss(&self) -> impl fmt::Display + use<> {
        self.0.format("%a, %d %b %Y %H:%M:%S GMT")
    }
}

impl FromStr for Timestamp {
    type Err = chrono::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        NaiveDateTime::parse_from_str(s, "%Y%m%d%H%M%S").map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats() {
        let ts: Timestamp = "20261024081500".parse().unwrap();
        assert_eq!(ts.rss().to_string(), "Sat, 24 Oct 2026 08:15:00 GMT");
    }

    #[test]
    fn rejects_garbage() {
        assert!("2026-10-24".parse::<Timestamp>().is_err());
        assert!("".parse::<Timestamp>().is_err());
    }

    #[test]
    fn orders_chronologically() {
        let a: Timestamp = "20251231235959".parse().unwrap();
        let b: Timestamp = "20260101000000".parse().unwrap();
        assert!(a < b);
    }
}
