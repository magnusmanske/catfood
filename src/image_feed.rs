//! Feeds of random images from a Commons category, e.g. for screensavers.

use crate::db::{FileInfo, WikiDb};
use crate::error::AppError;
use crate::html;
use crate::params::lenient;
use crate::rss::{self, XmlText};
use crate::timestamp::Timestamp;
use crate::wiki::Wiki;
use axum::extract::{Query, State};
use axum::response::Response;
use serde::Deserialize;
use std::fmt::{self, Write};
use wikimisc::commons;
use wikimisc::escape::xml;
use wikimisc::title::{self, Case};
use wikimisc::toolforge::pool::WikiPool;

const DEFAULT_IMAGES: u32 = 100;
const MAX_IMAGES: u32 = 500;
/// The smallest standard thumbnail width of at least the traditional 75px
const THUMB_WIDTH: u32 = 120;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Params {
    category: String,
    #[serde(deserialize_with = "lenient")]
    max_images: Option<u32>,
}

pub async fn feed(
    State(pool): State<WikiPool>,
    Query(params): Query<Params>,
) -> Result<Response, AppError> {
    let max_images = params
        .max_images
        .unwrap_or(DEFAULT_IMAGES)
        .clamp(1, MAX_IMAGES);
    let category = title::normalize(&params.category, Case::FirstLetter);
    if category.is_empty() {
        return Ok(form(max_images));
    }
    let wiki = Wiki::commons();
    let db = WikiDb::new(&pool, &wiki.dbname);
    // Some of the files are not bitmaps, so ask for more than needed
    let names = db
        .random_files(&title::to_db_key(&category), max_images * 2)
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
    };
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
}

impl fmt::Display for ImageFeed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let now = self.now.rss();
        write!(
            f,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:media="http://search.yahoo.com/mrss/">
<channel>
<title>Category "{}" on Wikimedia Commons</title>
<link>{}</link>
<description/>
<pubDate>{now}</pubDate>
<lastBuildDate>{now}</lastBuildDate>
<generator>https://catfood.toolforge.org/commons_image_feed.php</generator>
"#,
            xml(self.category),
            xml(&title::page_url(
                commons::COMMONS_SERVER,
                &format!("Category:{}", self.category)
            )),
        )?;
        for image in self.images {
            write_image(f, image, &now)?;
        }
        f.write_str("</channel>\n</rss>\n")
    }
}

fn write_image(f: &mut fmt::Formatter, image: &FileInfo, now: &impl fmt::Display) -> fmt::Result {
    let title = xml(&nice_title(&image.name)).into_owned();
    let page_url = xml(&commons::file_page_url(&image.name)).into_owned();
    let image_url = xml(&commons::upload_url(&image.name)).into_owned();
    let thumb_url = xml(&commons::thumb_url(&image.name, THUMB_WIDTH)).into_owned();
    let mime = xml(&image.mime).into_owned();
    let FileInfo {
        width,
        height,
        size,
        ..
    } = image;
    write!(
        f,
        "<item>\n<title>{title}</title>\n<link>{page_url}</link>\n<description>"
    )?;
    write!(XmlText(f), "<img src=\"{image_url}\" alt=\"\" />")?;
    write!(
        f,
        "</description>\n<pubDate>{now}</pubDate>\n<guid>{image_url}</guid>\n\
        <media:title>{title}</media:title>\n\
        <media:content url=\"{image_url}\" type=\"{mime}\" width=\"{width}\" height=\"{height}\"/>\n\
        <media:thumbnail url=\"{thumb_url}\" width=\"{THUMB_WIDTH}\"/>\n\
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
        r#"<p>Generates RSS feeds for screensavers from Wikimedia Commons categories (<a href="?category=Varanus%20komodoensis">example</a>).</p>
<form method="get">
<table class="table table-sm">
<tr><th>Category</th><td><input class="form-control" type="text" name="category" value=""></td><td></td></tr>
<tr><th>Max images</th><td><input class="form-control" type="number" name="max_images" min="1" max="{MAX_IMAGES}" value="{max_images}"></td><td><small>(image order is randomized, so you always get new images from a large category even with a relatively low number)</small></td></tr>
<tr><td></td><td><button type="submit" class="btn btn-primary">Gimme RSS!</button></td><td></td></tr>
</table>
</form>"#
    );
    html::page("Commons image feed", &body)
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
        assert!(rss.contains("length=\"12345\""));
        assert!(rss.contains("<description>&lt;img src=&quot;https://upload.wikimedia.org/"));
    }
}
