//! Feeds of random images from a Commons category, e.g. for screensavers.

use crate::AppState;
use crate::db::{FileInfo, WikiDb};
use crate::error::AppError;
use crate::html;
use crate::params::Params;
use crate::rss::{self, XmlText, php_urlencode};
use crate::timestamp::Timestamp;
use crate::wiki::Wiki;
use axum::extract::{Query, State};
use axum::response::Response;
use std::collections::HashMap;
use std::fmt::{self, Write};
use wikimisc::commons;
use wikimisc::escape::xml;
use wikimisc::title::{self, Case};

const DEFAULT_IMAGES: u32 = 100;
/// The PHP version had no limit
const MAX_IMAGES: u32 = 1000;
/// The smallest standard thumbnail width of at least the traditional 75px
const THUMB_WIDTH: u32 = 120;
const FILE_NAMESPACE: i64 = 6;

pub async fn feed(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, AppError> {
    let params = Params::new(params);
    let max_images = params.int("max_images").unwrap_or(DEFAULT_IMAGES.into());
    let max_images =
        u32::try_from(max_images.clamp(1, MAX_IMAGES.into())).unwrap_or(DEFAULT_IMAGES);
    let category = title::normalize(params.text("category", ""), Case::FirstLetter);
    if category.is_empty() {
        return Ok(form(max_images));
    }
    let wiki = state.database.commons();
    let db = WikiDb::new(&state.database, &wiki.dbname);
    // Some files may still not be bitmaps, so ask for more than needed
    let names = db
        .random_bitmaps(&title::to_db_key(&category), max_images * 2)
        .await?;
    let mut files = db.files_by_name(&names).await?;
    let images: Vec<FileInfo> = names
        .iter()
        .filter_map(|name| files.remove(name))
        .filter(|file| file.media_type == "BITMAP")
        .take(max_images as usize)
        .collect();
    let feed = ImageFeed {
        category: &category,
        images: &images,
        now: Timestamp::now(),
        base_url: &state.base_url,
        wiki: &wiki,
        single: state.database.is_single(),
    };
    wikimisc::metrics::counter!("feeds_served_total", "feed" => "commons_image_feed").increment(1);
    Ok(rss::response(
        feed,
        "text/xml; charset=utf-8",
        "no-cache, must-revalidate",
    ))
}

struct ImageFeed<'a> {
    /// Display form
    category: &'a str,
    images: &'a [FileInfo],
    now: Timestamp,
    /// Where the service is reachable
    base_url: &'a str,
    /// Commons for the replicas, the local wiki when embedded
    wiki: &'a Wiki,
    single: bool,
}

impl fmt::Display for ImageFeed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let now = self.now.rss();
        let title = if self.single {
            format!("Category \"{}\" on {}", xml(self.category), xml(self.wiki.host()))
        } else {
            format!("Category \"{}\" on Wikimedia Commons", xml(self.category))
        };
        write!(
            f,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:media="http://search.yahoo.com/mrss/">
<channel>
<title>{title}</title>
<link>{}</link>
<description/>
<pubDate>{now}</pubDate>
<lastBuildDate>{now}</lastBuildDate>
<generator>{}/commons_image_feed</generator>
"#,
            xml(&title::page_url(
                &self.wiki.server,
                &format!("Category:{}", self.category)
            )),
            self.base_url,
        )?;
        for image in self.images {
            write_image(f, image, &now, self.wiki, self.single)?;
        }
        f.write_str("</channel>\n</rss>\n")
    }
}

fn write_image(
    f: &mut fmt::Formatter,
    image: &FileInfo,
    now: &impl fmt::Display,
    wiki: &Wiki,
    single: bool,
) -> fmt::Result {
    let title = xml(&nice_title(&image.name)).into_owned();
    let FileInfo {
        width,
        height,
        size,
        ..
    } = *image;
    let (page_url, image_url, thumb_url, thumb_width, guid) = if single {
        let server = &wiki.server;
        let page_url = wiki.page_url(FILE_NAMESPACE, &image.name, image.page_id);
        let image_url = commons::redirect_file_url(server, &image.name, None);
        let (thumb_url, thumb_width) = match width > THUMB_WIDTH {
            true => (
                commons::redirect_file_url(server, &image.name, Some(THUMB_WIDTH)),
                THUMB_WIDTH,
            ),
            false => (image_url.clone(), width),
        };
        let guid = commons::redirect_file_url(server, &image.name, None);
        (xml(&page_url).into_owned(), xml(&image_url).into_owned(), xml(&thumb_url).into_owned(), thumb_width, xml(&guid).into_owned())
    } else {
        let page_url = commons::file_page_url(&image.name);
        let image_url = commons::upload_url(&image.name);
        // Bitmaps can't be scaled up
        let (thumb_url, thumb_width) = match width > THUMB_WIDTH {
            true => (commons::thumb_url(&image.name, THUMB_WIDTH), THUMB_WIDTH),
            false => (image_url.clone(), width),
        };
        let guid = format!(
            "{}/wiki/Special:Redirect/file/{}",
            commons::COMMONS_SERVER,
            php_urlencode(&image.name)
        );
        (xml(&page_url).into_owned(), xml(&image_url).into_owned(), xml(&thumb_url).into_owned(), thumb_width, xml(&guid).into_owned())
    };
    let mime = xml(&image.mime).into_owned();
    write!(
        f,
        "<item>\n<title>{title}</title>\n<link>{page_url}</link>\n<description>"
    )?;
    write!(XmlText(f), "<img src=\"{image_url}\" alt=\"\" />")?;
    write!(
        f,
        "</description>\n<pubDate>{now}</pubDate>\n<guid>{guid}</guid>\n\
        <media:title>{title}</media:title>\n\
        <media:content url=\"{image_url}\" type=\"{mime}\" width=\"{width}\" height=\"{height}\"/>\n\
        <media:thumbnail url=\"{thumb_url}\" width=\"{thumb_width}\"/>\n\
        <media:credit role=\"uploader\">{}</media:credit>\n\
        <enclosure url=\"{image_url}\" type=\"{mime}\" length=\"{size}\"/>\n</item>\n",
        xml(&image.user)
    )
}

/// The file name with spaces and without extension
fn nice_title(name: &str) -> String {
    let name = match name.rsplit_once('.') {
        Some((base, ext)) if !ext.is_empty() && ext.bytes().all(|b| b.is_ascii_alphabetic()) => {
            base
        }
        _ => name,
    };
    title::to_display(name)
}

fn form(max_images: u32) -> Response {
    let body = format!(
        r#"<p>Generates RSS feeds for screensavers from the wiki's file categories (<a href="?category=Varanus%20komodoensis">example</a>).</p>
<form method="get">
<table class="table table-sm">
<tr><th>Category</th><td><input class="form-control" type="text" name="category" value=""></td><td></td></tr>
<tr><th>Max images</th><td><input class="form-control" type="number" name="max_images" min="1" max="{MAX_IMAGES}" value="{max_images}"></td><td><small>(image order is randomized, so you always get new images from a large category even with a relatively low number)</small></td></tr>
<tr><td></td><td><button type="submit" class="btn btn-primary">Gimme RSS!</button></td><td></td></tr>
</table>
</form>"#
    );
    html::page("Image feed", &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_titles() {
        assert_eq!(nice_title("Komodo_dragon.JPG"), "Komodo dragon");
        assert_eq!(nice_title("A.b_c.png"), "A.b c");
        assert_eq!(nice_title("No_extension"), "No extension");
        assert_eq!(nice_title("Version_2.0"), "Version 2.0");
    }

    #[test]
    fn renders_feed() {
        let image = FileInfo {
            page_id: 1,
            name: "Komodo_&_friend.jpg".into(),
            width: 800,
            height: 600,
            media_type: "BITMAP".into(),
            mime: "image/jpeg".into(),
            user: "A <b>".into(),
            timestamp: "20260101000000".parse().unwrap(),
            size: 12345,
        };
        let rss = ImageFeed {
            category: "Varanus komodoensis",
            images: &[image],
            now: "20261024000000".parse().unwrap(),
            base_url: "https://catfood.toolforge.org",
            wiki: &Wiki::commons(),
            single: false,
        }
        .to_string();
        assert!(
            rss.contains("<title>Category \"Varanus komodoensis\" on Wikimedia Commons</title>")
        );
        assert!(rss.contains(
            "<link>https://commons.wikimedia.org/wiki/Category:Varanus_komodoensis</link>"
        ));
        assert!(rss.contains("<title>Komodo &amp; friend</title>"));
        assert!(rss.contains("<media:credit role=\"uploader\">A &lt;b&gt;</media:credit>"));
        assert!(rss.contains("/thumb/"));
        assert!(rss.contains(
            "<guid>https://commons.wikimedia.org/wiki/Special:Redirect/file/Komodo_%26_friend.jpg</guid>"
        ));
        assert!(rss.contains("length=\"12345\""));
        assert!(rss.contains("<description>&lt;img src=&quot;https://upload.wikimedia.org/"));
    }
}
