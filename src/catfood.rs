//! Feeds of the latest files (or pages) in a category tree, or of a user's
//! latest uploads.

use crate::AppState;
use crate::db::{FileInfo, MemberOrder, WikiDb};
use crate::error::AppError;
use crate::html;
use crate::params::Params;
use crate::routes::BASE_URL;
use crate::rss::{self, XmlText, php_urlencode};
use crate::timestamp::Timestamp;
use crate::wiki::{self, Wiki};
use axum::extract::{OriginalUri, Query, State};
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Redirect, Response};
use std::collections::HashMap;
use std::fmt::{self, Write};
use wikimisc::commons;
use wikimisc::escape::{html as escape_html, xml};
use wikimisc::title::{self, Case};

const TOOL_URL: &str = "https://catfood.toolforge.org/catfood";
/// Commons' own feed, which replaces the never implemented `motd` mode
const MOTD_FEED: &str = "https://commons.wikimedia.org/w/api.php?action=featuredfeed&feed=motd&feedformat=rss&language=en";
const FILE_NAMESPACE: i64 = 6;
const MAX_DEPTH: u32 = 10;
const MAX_ITEMS: u32 = 100;
const DEFAULT_ITEMS: u32 = 10;
const DEFAULT_SIZE: u32 = 300;

pub async fn feed(
    State(state): State<AppState>,
    OriginalUri(uri): OriginalUri,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let params = Params::new(params);
    if !params.text("motd", "").is_empty() {
        return Ok(Redirect::temporary(MOTD_FEED).into_response());
    }
    let language = params.text("language", "commons");
    let project = params.text("project", "wikimedia");
    let depth = params.int("depth").unwrap_or(0);
    let namespace = params.int("namespace").unwrap_or(FILE_NAMESPACE);
    let Some(source) = Source::new(&params, depth, namespace) else {
        return Ok(form(language, project, depth, namespace));
    };
    if namespace < 0 {
        return Ok(no_files());
    }
    let wiki = Wiki::resolve(&state.replicas, &state.servers, language, project).await?;
    let limit = params
        .int("last")
        .unwrap_or(DEFAULT_ITEMS.into())
        .clamp(1, MAX_ITEMS.into());
    let limit = u32::try_from(limit).unwrap_or(DEFAULT_ITEMS);
    let items = fetch(&WikiDb::new(&state.replicas, &wiki.dbname), &source, limit).await?;
    if items.is_empty() {
        return Ok(no_files());
    }
    let size = params
        .int("size")
        .filter(|&size| size > 0)
        .unwrap_or(DEFAULT_SIZE.into());
    let feed = Feed {
        wiki: &wiki,
        source: &source,
        items: &items,
        size: u32::try_from(size.clamp(20, 2000)).unwrap_or(DEFAULT_SIZE),
        self_url: &format!(
            "{BASE_URL}{}",
            uri.path_and_query().map_or("/", |pq| pq.as_str())
        ),
        legacy_host: &format!(
            "{}.{}.org",
            params.raw("language", "commons"),
            params.raw("project", "wikimedia")
        ),
    };
    wikimisc::metrics::counter!("feeds_served_total", "feed" => "catfood").increment(1);
    Ok(rss::response(
        feed,
        "application/rss+xml; charset=utf-8",
        "public, max-age=300",
    ))
}

/// What the PHP version answered when there was nothing to show
fn no_files() -> Response {
    (
        [(CONTENT_TYPE, "text/html; charset=utf-8")],
        "No files found.",
    )
        .into_response()
}

/// What a feed lists
enum Source {
    /// The pages in a category tree; the category as DB key, case-sensitive
    /// like in the PHP version, as some wikis are
    Category {
        name: String,
        /// As requested
        depth: i64,
        namespace: i64,
    },
    /// A user's uploads
    User(String),
}

impl Source {
    fn new(params: &Params, depth: i64, namespace: i64) -> Option<Self> {
        let category = title::normalize(params.text("category", ""), Case::Sensitive);
        let user = title::normalize(params.text("user", ""), Case::FirstLetter);
        if !category.is_empty() {
            let name = title::to_db_key(&category);
            Some(Self::Category {
                name,
                depth,
                namespace,
            })
        } else if !user.is_empty() {
            Some(Self::User(user))
        } else {
            None
        }
    }

    fn namespace(&self) -> i64 {
        match self {
            Self::Category { namespace, .. } => *namespace,
            Self::User(_) => FILE_NAMESPACE,
        }
    }
}

struct Item {
    page_id: u64,
    /// DB key, without namespace prefix
    title: String,
    user: String,
    /// Uploaded or edited; to the minute, like the PHP version, which keeps
    /// item dates and GUIDs unchanged
    timestamp: Timestamp,
    added_to_category: Option<Timestamp>,
    size: u64,
    kind: Kind,
}

enum Kind {
    File {
        width: u32,
        height: u32,
        media_type: String,
        licenses: Vec<License>,
    },
    Page {
        comment: String,
    },
}

impl Item {
    fn file(
        file: FileInfo,
        added: Option<Timestamp>,
        categories: &mut HashMap<u64, Vec<String>>,
    ) -> Self {
        let licenses = License::all_in(&categories.remove(&file.page_id).unwrap_or_default());
        Self {
            page_id: file.page_id,
            title: file.name,
            user: file.user,
            timestamp: file.timestamp.to_minute(),
            added_to_category: added,
            size: file.size,
            kind: Kind::File {
                width: file.width,
                height: file.height,
                media_type: file.media_type,
                licenses,
            },
        }
    }

    /// The latest change: upload, edit, or addition to the category
    fn date(&self) -> Timestamp {
        self.added_to_category
            .map_or(self.timestamp, |added| added.max(self.timestamp))
    }
}

async fn fetch(db: &WikiDb<'_>, source: &Source, limit: u32) -> Result<Vec<Item>, AppError> {
    let files: Vec<(FileInfo, Option<Timestamp>)> = match source {
        Source::User(user) => {
            let files = db.files_by_user(user, limit).await?;
            files.into_iter().map(|file| (file, None)).collect()
        }
        Source::Category {
            name,
            depth,
            namespace,
        } => {
            // Negative meant unlimited in the PHP version
            let depth = u32::try_from(*depth).map_or(MAX_DEPTH, |depth| depth.min(MAX_DEPTH));
            let tree = db.category_tree(name, depth).await?;
            if *namespace != FILE_NAMESPACE {
                return page_items(db, &tree, *namespace, limit).await;
            }
            let files = db.category_files(&tree, limit).await?;
            files
                .into_iter()
                .map(|(file, added)| (file, Some(added)))
                .collect()
        }
    };
    let page_ids: Vec<u64> = files.iter().map(|(file, _)| file.page_id).collect();
    let mut categories = db.categories_of(&page_ids).await?;
    Ok(files
        .into_iter()
        .map(|(file, added)| Item::file(file, added, &mut categories))
        .collect())
}

async fn page_items(
    db: &WikiDb<'_>,
    tree: &[String],
    namespace: i64,
    limit: u32,
) -> Result<Vec<Item>, AppError> {
    let members = db
        .category_members(tree, namespace, MemberOrder::LastEdit, limit)
        .await?;
    let revision_ids: Vec<u64> = members.iter().map(|m| m.latest_revision).collect();
    let mut revisions = db.revisions(&revision_ids).await?;
    Ok(members
        .into_iter()
        .filter_map(|m| {
            let revision = revisions.remove(&m.latest_revision)?;
            Some(Item {
                page_id: m.page_id,
                title: m.title,
                user: revision.user,
                timestamp: revision.timestamp.to_minute(),
                added_to_category: None,
                size: revision.size,
                kind: Kind::Page {
                    comment: revision.comment,
                },
            })
        })
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum License {
    PublicDomain,
    Gfdl,
    CcBySa,
    CcBy,
    CcSa,
}

impl License {
    fn from_category(category: &str) -> Option<Self> {
        if title::ucfirst(category).starts_with("PD") {
            return Some(Self::PublicDomain);
        }
        let upper = category.to_ascii_uppercase();
        [
            ("GFDL", Self::Gfdl),
            ("CC-BY-SA", Self::CcBySa),
            ("CC-BY", Self::CcBy),
            ("CC-SA", Self::CcSa),
        ]
        .into_iter()
        .find_map(|(tag, license)| upper.contains(tag).then_some(license))
    }

    /// The distinct licenses indicated by `categories`, in order of appearance
    fn all_in(categories: &[String]) -> Vec<Self> {
        let mut licenses = vec![];
        for license in categories.iter().filter_map(|c| Self::from_category(c)) {
            if !licenses.contains(&license) {
                licenses.push(license);
            }
        }
        licenses
    }

    fn name(self) -> &'static str {
        match self {
            Self::PublicDomain => "Public domain",
            Self::Gfdl => "GFDL",
            Self::CcBySa => "CC-BY-SA",
            Self::CcBy => "CC-BY",
            Self::CcSa => "CC-SA",
        }
    }
}

struct Feed<'a> {
    wiki: &'a Wiki,
    source: &'a Source,
    items: &'a [Item],
    /// Maximum thumbnail width and height
    size: u32,
    self_url: &'a str,
    /// `<language>.<project>.org` from the request, for PHP-compatible GUIDs
    legacy_host: &'a str,
}

impl fmt::Display for Feed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let host = self.wiki.host();
        let (title, description) = match self.source {
            Source::Category { name, depth, .. } => {
                let name = title::to_display(name);
                let depth = match depth {
                    1.. => format!("(depth {depth}) "),
                    _ => String::new(),
                };
                (
                    format!("{name} {depth}- {host}"),
                    format!("Category feed for category \"{name}\" from {host}"),
                )
            }
            Source::User(user) => (
                format!("Uploads by User:{user} - {host}"),
                format!("Feed for user \"{user}\" from {host}"),
            ),
        };
        let title = xml(&title);
        let date = self
            .items
            .first()
            .map_or_else(Timestamp::now, Item::date)
            .rss();
        write!(
            f,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:atom="http://www.w3.org/2005/Atom">
<channel>
<atom:link href="{self_url}" rel="self" type="application/rss+xml"/>
<title>{title}</title>
<link>{TOOL_URL}</link>
<description>{description}</description>
<language>en-us</language>
<copyright>Feed: GNU Free Documentation License; Images: see description page</copyright>
<pubDate>{date}</pubDate>
<lastBuildDate>{date}</lastBuildDate>
<generator>CatFood</generator>
<docs>{TOOL_URL}</docs>
<image><url>{logo}</url><title>{title}</title><link>{TOOL_URL}</link></image>
"#,
            self_url = xml(self.self_url),
            description = xml(&description),
            logo = xml(&self.wiki.logo_url()),
        )?;
        for item in self.items {
            self.write_item(f, item)?;
        }
        f.write_str("</channel>\n</rss>\n")
    }
}

impl Feed<'_> {
    fn write_item(&self, f: &mut fmt::Formatter, item: &Item) -> fmt::Result {
        let url = self
            .wiki
            .page_url(self.source.namespace(), &item.title, item.page_id);
        let date = item.date().rss();
        let guid = self.legacy_guid(item, &date);
        write!(
            f,
            "<item>\n<title>{}</title>\n<pubDate>{date}</pubDate>\n<link>{}</link>\n\
            <guid isPermaLink=\"false\">{}</guid>\n<description>",
            xml(&title::to_display(&item.title)),
            xml(&url),
            xml(&guid),
        )?;
        self.write_description(&mut XmlText(f), item, &url)?;
        f.write_str("</description>\n</item>\n")
    }

    /// The GUID the PHP version used, so feed readers don't show old items
    /// as new; it is opaque, so the old, sometimes broken, URL is fine
    fn legacy_guid(&self, item: &Item, date: &impl fmt::Display) -> String {
        let prefix = match self.source.namespace() {
            FILE_NAMESPACE => "File:",
            _ => "",
        };
        let url = format!(
            "https://{}/wiki/{prefix}{}",
            self.legacy_host,
            php_urlencode(&item.title)
        );
        match item.added_to_category {
            Some(_) => format!("{url} - {date}"),
            None => url,
        }
    }

    /// The HTML description of an item
    fn write_description(&self, out: &mut impl Write, item: &Item, url: &str) -> fmt::Result {
        let url = escape_html(url);
        let user = escape_html(&item.user);
        let user_page = title::page_url(&self.wiki.server, &format!("User:{}", item.user));
        let user_url = escape_html(&user_page);
        let timestamp = item.timestamp.rss();
        let size = group_thousands(item.size);
        match &item.kind {
            Kind::File {
                width,
                height,
                media_type,
                licenses,
            } => {
                let thumbnail = thumbnail_url(
                    &self.wiki.server,
                    &item.title,
                    *width,
                    *height,
                    media_type,
                    self.size,
                );
                write!(
                    out,
                    "<a href=\"{url}\"><img border=\"0\" src=\"{}\" /></a><br/>\n\
                    Uploaded by user \"<a href=\"{user_url}\">{user}</a>\" on {timestamp}<br/>\n",
                    escape_html(&thumbnail)
                )?;
                if item.added_to_category.is_some() {
                    writeln!(out, "Added to category on {}<br/>", item.date().rss())?;
                }
                match media_type.as_str() {
                    "UNKNOWN" => out.write_str("Original file: ")?,
                    "BITMAP" | "DRAWING" => {
                        write!(out, "Original image: {width}&times;{height} pixel; ")?
                    }
                    other => write!(
                        out,
                        "Original {} file: ",
                        escape_html(&other.to_lowercase())
                    )?,
                }
                writeln!(out, "{size} bytes.<br/>")?;
                write_licenses(out, licenses, &url)
            }
            Kind::Page { comment } => write!(
                out,
                "Edited by user \"<a href=\"{user_url}\">{user}</a>\" on {timestamp}<br/>\n\
                \"<i>{}</i>\"<br/>New text length: {size} bytes.<br/>\n",
                escape_html(comment)
            ),
        }
    }
}

/// `url` must be HTML-escaped
fn write_licenses(out: &mut impl Write, licenses: &[License], url: &str) -> fmt::Result {
    if licenses.is_empty() {
        return write!(
            out,
            "For license information, see the image description page <a href=\"{url}\">here</a>."
        );
    }
    out.write_str("Licensing : ")?;
    for (i, license) in licenses.iter().enumerate() {
        let name = license.name();
        let separator = if i == 0 { "" } else { "," };
        let article = title::wiki_urlencode(name);
        write!(
            out,
            "{separator}<a href=\"https://en.wikipedia.org/wiki/{article}\">{name}</a>"
        )?;
    }
    Ok(())
}

/// An image no larger than `max` pixels in either direction
fn thumbnail_url(
    server: &str,
    name: &str,
    width: u32,
    height: u32,
    media_type: &str,
    max: u32,
) -> String {
    let thumb_width = match (width, height) {
        (0, _) if media_type == "AUDIO" => {
            return commons::thumb_url("Sound-icon.svg", max.min(200));
        }
        (0, _) => None,
        _ if width <= max && height <= max => None,
        _ if width >= height => Some(max),
        _ => Some(
            u32::try_from(u64::from(width) * u64::from(max) / u64::from(height))
                .map_or(max, |w| w.max(1)),
        ),
    };
    commons::redirect_file_url(server, name, thumb_width)
}

/// `1234567` as `1.234.567`
fn group_thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() * 4 / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(digit);
    }
    out
}

fn form(language: &str, project: &str, depth: i64, namespace: i64) -> Response {
    let name = escape_html(&format!(
        "{} {}",
        title::ucfirst(project),
        title::ucfirst(language)
    ))
    .into_owned();
    let site = match wiki::is_code(language) && wiki::is_code(project) {
        true => format!("<a href=\"https://{language}.{project}.org\">{name}</a>"),
        false => name,
    };
    let language = escape_html(language);
    let project = escape_html(project);
    let motd = escape_html(MOTD_FEED);
    let body = format!(
        r#"<p>This is a category-based RSS feed for {site}.<br>
The images are ordered based on the time of the addition of the image to the category, latest additions first.<br>
Alternatively, you can see the last images uploaded by a user.<br>
You need to supply at least a category or a user name to get a feed.<br>
Enter the information below, or use URL parameters (click on "Do it" for a demo).</p>
<form method="get">
<table class="table table-sm">
<tr><th>Language</th><td><input class="form-control" type="text" name="language" value="{language}"></td><td>e.g., "en" or "commons"</td></tr>
<tr><th>Project</th><td><input class="form-control" type="text" name="project" value="{project}"></td><td>e.g. "wikipedia"</td></tr>
<tr><th>Category</th><td><input class="form-control" type="text" name="category" value="Featured pictures on Wikimedia Commons"></td><td>without the "Category:" prefix</td></tr>
<tr><th>Depth</th><td><input class="form-control" type="number" name="depth" max="{MAX_DEPTH}" value="{depth}"></td><td>0=just this category</td></tr>
<tr><th>Namespace</th><td><input class="form-control" type="number" name="namespace" value="{namespace}"></td><td>6=images; 0=article</td></tr>
<tr><th>User</th><td><input class="form-control" type="text" name="user" value=""></td><td>user name, <i>instead</i> of category</td></tr>
<tr><th>Size</th><td>
<label class="mr-3"><input type="radio" name="size" value="200"> 200px</label>
<label class="mr-3"><input type="radio" name="size" value="250"> 250px</label>
<label class="mr-3"><input type="radio" name="size" value="300" checked> 300px</label>
</td><td>maximum height/width</td></tr>
<tr><th>Number</th><td><input class="form-control" type="number" name="last" min="1" max="{MAX_ITEMS}" value="{DEFAULT_ITEMS}"></td><td>last X images/pages; 1-{MAX_ITEMS}</td></tr>
<tr><td></td><td><button type="submit" class="btn btn-primary">Do it</button></td><td></td></tr>
</table>
</form>
<p>Or get a feed of the <a href="{motd}">Media file of the Day</a>!</p>"#
    );
    html::page("CatFood (Category news feeder)", &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_item(user: &str, added: Option<&str>) -> Item {
        Item {
            page_id: 1,
            title: "Big_<cat>.jpg".into(),
            user: user.into(),
            timestamp: "20260101120000".parse().unwrap(),
            added_to_category: added.map(|ts| ts.parse().unwrap()),
            size: 1234567,
            kind: Kind::File {
                width: 4000,
                height: 3000,
                media_type: "BITMAP".into(),
                licenses: vec![License::CcBySa],
            },
        }
    }

    fn render(source: &Source, items: &[Item]) -> String {
        Feed {
            wiki: &Wiki::commons(),
            source,
            items,
            size: 300,
            self_url: "https://catfood.toolforge.org/?category=A&user=B",
            legacy_host: "commons.wikimedia.org",
        }
        .to_string()
    }

    #[test]
    fn licenses_from_categories() {
        let categories = [
            "PD-self",
            "CC-BY-SA-4.0",
            "Cc-by-sa-3.0",
            "GFDL",
            "Cats",
            "CC-BY-4.0",
        ]
        .map(String::from);
        assert_eq!(
            License::all_in(&categories),
            vec![
                License::PublicDomain,
                License::CcBySa,
                License::Gfdl,
                License::CcBy
            ]
        );
        assert_eq!(License::from_category("Self-published_work"), None);
    }

    #[test]
    fn thumbnails() {
        let server = "https://commons.wikimedia.org";
        let url = |w, h, t| thumbnail_url(server, "A.jpg", w, h, t, 300);
        assert_eq!(
            url(200, 100, "BITMAP"),
            format!("{server}/wiki/Special:Redirect/file/A.jpg")
        );
        assert_eq!(
            url(4000, 3000, "BITMAP"),
            format!("{server}/wiki/Special:Redirect/file/A.jpg?width=300")
        );
        assert_eq!(
            url(3000, 4000, "BITMAP"),
            format!("{server}/wiki/Special:Redirect/file/A.jpg?width=225")
        );
        assert!(url(0, 0, "AUDIO").ends_with("/200px-Sound-icon.svg.png"));
    }

    #[test]
    fn thousands() {
        assert_eq!(group_thousands(0), "0");
        assert_eq!(group_thousands(999), "999");
        assert_eq!(group_thousands(1000), "1.000");
        assert_eq!(group_thousands(1234567), "1.234.567");
    }

    #[test]
    fn item_date_is_latest_change() {
        let item = file_item("X", Some("20260301000000"));
        assert_eq!(item.date(), "20260301000000".parse().unwrap());
        let item = file_item("X", Some("20250301000000"));
        assert_eq!(item.date(), item.timestamp);
    }

    #[test]
    fn category_feed() {
        let source = Source::Category {
            name: "Big_cats".into(),
            depth: 2,
            namespace: 6,
        };
        let rss = render(&source, &[file_item("<script>", Some("20260301000000"))]);
        assert!(rss.contains("<title>Big cats (depth 2) - commons.wikimedia.org</title>"));
        assert!(rss.contains("href=\"https://catfood.toolforge.org/?category=A&amp;user=B\""));
        assert!(rss.contains("<pubDate>Sun, 01 Mar 2026 00:00:00 GMT</pubDate>"));
        assert!(rss.contains("<title>Big &lt;cat&gt;.jpg</title>"));
        assert!(
            rss.contains("<link>https://commons.wikimedia.org/wiki/File:Big_%3Ccat%3E.jpg</link>")
        );
        // User input is HTML-escaped, then the HTML is XML-escaped
        assert!(rss.contains("&amp;lt;script&amp;gt;"));
        assert!(!rss.contains("<script>"));
        assert!(rss.contains("Original image: 4000&amp;times;3000 pixel; 1.234.567 bytes."));
        assert!(rss.contains("Added to category on Sun, 01 Mar 2026 00:00:00 GMT"));
        assert!(rss.contains(
            "<guid isPermaLink=\"false\">https://commons.wikimedia.org/wiki/File:Big_%3Ccat%3E.jpg - Sun, 01 Mar 2026 00:00:00 GMT</guid>"
        ));
        assert!(rss.contains("Licensing : &lt;a href=&quot;https://en.wikipedia.org/wiki/CC-BY-SA&quot;&gt;CC-BY-SA&lt;/a&gt;"));
    }

    #[test]
    fn page_feed() {
        let source = Source::Category {
            name: "Cats".into(),
            depth: 0,
            namespace: 14,
        };
        let item = Item {
            page_id: 5,
            title: "Lion".into(),
            user: "Someone".into(),
            timestamp: "20260101120000".parse().unwrap(),
            added_to_category: None,
            size: 42,
            kind: Kind::Page {
                comment: "fix <b>".into(),
            },
        };
        let rss = render(&source, &[item]);
        assert!(rss.contains("<title>Cats - commons.wikimedia.org</title>"));
        // The PHP version left out the namespace; kept in the GUID only
        assert!(rss.contains(
            "<guid isPermaLink=\"false\">https://commons.wikimedia.org/wiki/Lion</guid>"
        ));
        assert!(rss.contains("<link>https://commons.wikimedia.org/wiki/Category:Lion</link>"));
        assert!(rss.contains("Edited by user"));
        assert!(rss.contains("fix &amp;lt;b&amp;gt;"));
        assert!(rss.contains("New text length: 42 bytes."));
    }

    #[test]
    fn user_feed_title() {
        let rss = render(
            &Source::User("Magnus Manske".into()),
            &[file_item("Magnus Manske", None)],
        );
        assert!(
            rss.contains("<title>Uploads by User:Magnus Manske - commons.wikimedia.org</title>")
        );
        assert!(!rss.contains("Added to category"));
        assert!(rss.contains(
            "<guid isPermaLink=\"false\">https://commons.wikimedia.org/wiki/File:Big_%3Ccat%3E.jpg</guid>"
        ));
    }
}
