//! Which wiki a feed is for: its replica database and its web address.

use wikimisc::title::{self, Case};

/// Projects whose database name is `<language><project>`
const PROJECTS: &[&str] = &[
    "wikibooks",
    "wikinews",
    "wikiquote",
    "wikisource",
    "wikiversity",
    "wikivoyage",
    "wiktionary",
];

/// Canonical namespace names, which every wiki accepts
const NAMESPACES: [&str; 16] = [
    "",
    "Talk",
    "User",
    "User_talk",
    "Project",
    "Project_talk",
    "File",
    "File_talk",
    "MediaWiki",
    "MediaWiki_talk",
    "Template",
    "Template_talk",
    "Help",
    "Help_talk",
    "Category",
    "Category_talk",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wiki {
    /// Database name without `_p`, e.g. `enwiki`
    pub dbname: String,
    /// e.g. `https://en.wikipedia.org`
    pub server: String,
    /// Whether titles get their first letter capitalized
    pub case: Case,
}

impl Wiki {
    /// The wiki for the `language` and `project` parameters, e.g. `en` and
    /// `wikipedia`. Whether it exists is checked when connecting to it.
    pub fn new(language: &str, project: &str) -> Option<Self> {
        let language = language.trim().to_ascii_lowercase();
        let project = project.trim().to_ascii_lowercase();
        if !is_language_code(&language) {
            return None;
        }
        let db_language = language.replace('-', "_");
        let (dbname, host) = match (language.as_str(), project.as_str()) {
            ("commons" | "meta" | "species", _) => (
                format!("{language}wiki"),
                format!("{language}.wikimedia.org"),
            ),
            ("wikidata", _) | (_, "wikidata") => ("wikidatawiki".into(), "www.wikidata.org".into()),
            ("mediawiki", _) | (_, "mediawiki") => {
                ("mediawikiwiki".into(), "www.mediawiki.org".into())
            }
            (_, "wikipedia") => (
                format!("{db_language}wiki"),
                format!("{language}.wikipedia.org"),
            ),
            (_, p) if PROJECTS.contains(&p) => {
                (format!("{db_language}{p}"), format!("{language}.{p}.org"))
            }
            _ => return None,
        };
        let case = match project.as_str() {
            "wiktionary" => Case::Sensitive,
            _ => Case::FirstLetter,
        };
        Some(Self {
            dbname,
            server: format!("https://{host}"),
            case,
        })
    }

    pub fn commons() -> Self {
        Self {
            dbname: "commonswiki".into(),
            server: "https://commons.wikimedia.org".into(),
            case: Case::FirstLetter,
        }
    }

    /// e.g. `en.wikipedia.org`
    pub fn host(&self) -> &str {
        self.server.strip_prefix("https://").unwrap_or(&self.server)
    }

    pub fn logo_url(&self) -> String {
        format!(
            "{}/static/images/project-logos/{}.png",
            self.server, self.dbname
        )
    }

    /// The URL of a page, given its title without the namespace prefix
    pub fn page_url(&self, namespace: u32, title: &str, page_id: u64) -> String {
        match usize::try_from(namespace)
            .ok()
            .and_then(|ns| NAMESPACES.get(ns))
        {
            Some(&"") => title::page_url(&self.server, title),
            Some(prefix) => title::page_url(&self.server, &format!("{prefix}:{title}")),
            None => format!("{}/wiki/Special:Redirect/page/{page_id}", self.server),
        }
    }
}

fn is_language_code(s: &str) -> bool {
    (1..=20).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commons() {
        let wiki = Wiki::new("commons", "wikimedia").unwrap();
        assert_eq!(wiki, Wiki::commons());
        assert_eq!(wiki.host(), "commons.wikimedia.org");
    }

    #[test]
    fn wikipedia() {
        let wiki = Wiki::new(" EN ", "wikipedia").unwrap();
        assert_eq!(wiki.dbname, "enwiki");
        assert_eq!(wiki.server, "https://en.wikipedia.org");
        assert_eq!(
            Wiki::new("zh-yue", "wikipedia").unwrap().dbname,
            "zh_yuewiki"
        );
    }

    #[test]
    fn other_projects() {
        let wiki = Wiki::new("de", "wiktionary").unwrap();
        assert_eq!(wiki.dbname, "dewiktionary");
        assert_eq!(wiki.case, Case::Sensitive);
        assert_eq!(
            Wiki::new("x", "wikidata").unwrap().host(),
            "www.wikidata.org"
        );
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(Wiki::new("", "wikipedia"), None);
        assert_eq!(Wiki::new("en", "example"), None);
        assert_eq!(Wiki::new("evil.com/", "wikipedia"), None);
        assert_eq!(Wiki::new("en wiki", "wikipedia"), None);
    }

    #[test]
    fn page_urls() {
        let wiki = Wiki::commons();
        assert_eq!(
            wiki.page_url(0, "Main_Page", 1),
            "https://commons.wikimedia.org/wiki/Main_Page"
        );
        assert_eq!(
            wiki.page_url(6, "A b.jpg", 1),
            "https://commons.wikimedia.org/wiki/File:A_b.jpg"
        );
        assert_eq!(
            wiki.page_url(100, "X", 42),
            "https://commons.wikimedia.org/wiki/Special:Redirect/page/42"
        );
    }
}
