//! Which wiki a feed is for: its replica database and its web address.

use crate::db::WikiDb;
use crate::error::AppError;
use crate::source::Database;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::OnceCell;
use wikimisc::commons;
use wikimisc::title;

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
    /// The project logo, for the RSS image element.
    logo: String,
}

impl Wiki {
    /// A Wikimedia wiki of the replicas.
    pub fn replica(dbname: &str, server: &str) -> Self {
        Self {
            dbname: dbname.to_string(),
            server: server.to_string(),
            logo: format!("{server}/static/images/project-logos/{dbname}.png"),
        }
    }

    /// The embedded deployment's single wiki.
    pub fn single(dbname: &str, server: &str, _name: &str) -> Self {
        Self {
            dbname: dbname.to_string(),
            server: server.to_string(),
            logo: format!("{server}/favicon.ico"),
        }
    }

    pub fn commons() -> Self {
        Self::replica("commonswiki", commons::COMMONS_SERVER)
    }

    /// e.g. `en.wikipedia.org`
    pub fn host(&self) -> &str {
        self.server
            .strip_prefix("https://")
            .or_else(|| self.server.strip_prefix("http://"))
            .unwrap_or(&self.server)
    }

    pub fn logo_url(&self) -> &str {
        &self.logo
    }

    /// The URL of a page, given its title without the namespace prefix
    pub fn page_url(&self, namespace: i64, title: &str, page_id: u64) -> String {
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

/// The database name, chosen like the PHP version did (which ignored
/// `project`), except that sister projects now get their own database.
pub fn dbname(language: &str, project: &str) -> Option<String> {
    let language = language.trim().to_ascii_lowercase();
    if !is_code(&language) {
        return None;
    }
    let language = language.replace('-', "_");
    let project = project.trim().to_ascii_lowercase();
    Some(match (language.as_str(), project.as_str()) {
        ("commons" | "meta" | "species" | "wikidata" | "mediawiki", _) => format!("{language}wiki"),
        (_, "wikidata" | "mediawiki") => format!("{project}wiki"),
        (_, "wikispecies") => "specieswiki".into(),
        (_, p) if PROJECTS.contains(&p) => format!("{language}{p}"),
        // `wikipedia`, but also the form's default `wikimedia`, and anything else
        _ => format!("{language}wiki"),
    })
}

/// A language or project code: lowercase letters, digits, hyphens; and
/// underscores, as in `zh_yue`, the only spelling the PHP version accepted
pub fn is_code(s: &str) -> bool {
    (1..=30).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// The server URLs of all wikis, by database name, loaded from `meta_p` on
/// first use. Wikis created later are only known after a restart.
#[derive(Debug, Clone, Default)]
pub struct Servers(Arc<OnceCell<HashMap<String, String>>>);

impl Servers {
    pub(crate) async fn get(&self, database: &Database, dbname: &str) -> Result<Option<String>, AppError> {
        let servers = self
            .0
            .get_or_try_init(|| async { WikiDb::new(database, "meta").servers().await })
            .await?;
        Ok(servers.get(dbname).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbnames() {
        let db = |language, project| dbname(language, project).unwrap();
        assert_eq!(db("commons", "wikimedia"), "commonswiki");
        assert_eq!(db(" EN ", "wikipedia"), "enwiki");
        assert_eq!(db("en", "wikimedia"), "enwiki");
        assert_eq!(db("en", "unknown project"), "enwiki");
        assert_eq!(db("incubator", "wikimedia"), "incubatorwiki");
        assert_eq!(db("zh-yue", "wikipedia"), "zh_yuewiki");
        assert_eq!(db("zh_yue", "wikipedia"), "zh_yuewiki");
        assert_eq!(db("de", "Wiktionary"), "dewiktionary");
        assert_eq!(db("www", "wikidata"), "wikidatawiki");
        assert_eq!(db("species", "wikimedia"), "specieswiki");
    }

    #[test]
    fn rejects_bad_languages() {
        assert_eq!(dbname("", "wikipedia"), None);
        assert_eq!(dbname("evil.com/", "wikipedia"), None);
        assert_eq!(dbname("en wiki", "wikipedia"), None);
    }

    #[test]
    fn hosts() {
        assert_eq!(Wiki::commons().host(), "commons.wikimedia.org");
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
