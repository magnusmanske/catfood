//! Queries against the Wiki Replicas.
//!
//! Commons keeps its category tables in a separate database from its file
//! and revision tables, so no query joins across the two groups; results are
//! combined in code instead. Each query connects to the database holding its
//! tables, which is the same one on all other wikis.

use crate::error::AppError;
use crate::timestamp::Timestamp;
use std::collections::{HashMap, HashSet};
use wikimisc::mysql_async::prelude::{FromRow, Queryable};
use wikimisc::mysql_async::{Row, Value, from_row_opt};
use wikimisc::replicas::Replicas;
use wikimisc::toolforge::db::ExtensionDb;

const CATEGORY_TABLES: &[&str] = &["page", "categorylinks", "linktarget"];
const FILE_TABLES: &[&str] = &["page", "file", "filerevision", "filetypes", "actor"];
const REVISION_TABLES: &[&str] = &["revision", "actor_revision", "comment_revision"];

/// Upper bound for the categories collected from a category tree
const MAX_CATEGORIES: usize = 5000;

const FILE_COLUMNS: &str = "page_id, file_name, fr_width, fr_height, ft_media_type,
    CONCAT(ft_major_mime, '/', ft_minor_mime), actor_name, fr_timestamp, fr_size";
const FILE_JOINS: &str = "FROM file
    JOIN filerevision ON fr_id = file_latest
    JOIN filetypes ON ft_id = file_type
    JOIN actor ON actor_id = fr_actor
    JOIN page ON page_namespace = 6 AND page_title = file_name";
const CATEGORY_JOINS: &str = "JOIN categorylinks ON cl_from = page_id
    JOIN linktarget ON lt_id = cl_target_id";
/// `cl_timestamp` in the MediaWiki format
const ADDED: &str = "DATE_FORMAT(cl_timestamp, '%Y%m%d%H%i%s')";
/// Extensions of files that are never bitmaps
const NON_BITMAP_EXTENSIONS: &str = "'flac','glb','mid','midi','mp3','mpeg','mpg',
    'oga','ogg','ogv','opus','pdf','stl','svg','wav','webm'";

type FileRow = (u64, String, u32, u32, String, String, String, String, u64);
/// A [`FileRow`] plus when the file was added to the category
type CategoryFileRow = (
    u64,
    String,
    u32,
    u32,
    String,
    String,
    String,
    String,
    u64,
    Option<String>,
);

pub struct CategoryMember {
    pub page_id: u64,
    pub title: String,
    pub latest_revision: u64,
    /// `None` if missing or invalid
    pub added: Option<Timestamp>,
}

/// The current version of a file
pub struct FileInfo {
    pub page_id: u64,
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// e.g. `BITMAP`, `AUDIO`
    pub media_type: String,
    pub mime: String,
    pub user: String,
    pub timestamp: Timestamp,
    pub size: u64,
}

pub struct Revision {
    pub user: String,
    pub timestamp: Timestamp,
    pub size: u64,
    pub comment: String,
}

#[derive(Debug, Clone, Copy)]
pub enum MemberOrder {
    AddedToCategory,
    /// Approximated by the latest revision ID, as the category tables can't
    /// be joined with `revision` on Commons
    LastEdit,
}

/// The replica databases of one wiki
pub struct WikiDb<'a> {
    replicas: &'a Replicas,
    dbname: &'a str,
}

impl<'a> WikiDb<'a> {
    pub fn new(replicas: &'a Replicas, dbname: &'a str) -> Self {
        Self { replicas, dbname }
    }

    /// Runs a prepared statement. Rows are converted without panicking on
    /// unexpected column types, unlike `Queryable::exec`.
    async fn query<T: FromRow>(
        &self,
        tables: &[&str],
        sql: &str,
        params: Vec<Value>,
    ) -> Result<Vec<T>, AppError> {
        let mut conn = self
            .replicas
            .connect_for_tables(self.dbname, tables)
            .await?;
        let rows: Vec<Row> = conn.exec(sql, params).await?;
        rows.into_iter()
            .map(|row| from_row_opt(row).map_err(AppError::from))
            .collect()
    }

    /// The server URL of every wiki, by database name; for the `meta` database
    pub async fn servers(&self) -> Result<HashMap<String, String>, AppError> {
        let sql = "SELECT dbname, url FROM wiki WHERE url IS NOT NULL";
        let rows: Vec<(String, String)> = self.query(&["wiki"], sql, vec![]).await?;
        Ok(rows.into_iter().collect())
    }

    /// `root` and its subcategories down to `depth` levels, as DB keys
    pub async fn category_tree(&self, root: &str, depth: u32) -> Result<Vec<String>, AppError> {
        let mut all = vec![root.to_string()];
        let mut seen: HashSet<String> = HashSet::from([root.to_string()]);
        let mut level = all.clone();
        for _ in 0..depth {
            let remaining = MAX_CATEGORIES.saturating_sub(all.len());
            if level.is_empty() || remaining == 0 {
                break;
            }
            let sql = format!(
                "SELECT DISTINCT page_title FROM page {CATEGORY_JOINS}
                WHERE lt_namespace = 14 AND cl_type = 'subcat' AND lt_title IN ({})
                LIMIT {MAX_CATEGORIES}",
                placeholders(level.len())
            );
            let found: Vec<String> = self
                .query(CATEGORY_TABLES, &sql, values(&level).collect())
                .await?;
            level = found
                .into_iter()
                .filter(|category| seen.insert(category.clone()))
                .take(remaining)
                .collect();
            all.extend_from_slice(&level);
        }
        Ok(all)
    }

    /// The latest pages in any of `categories`, without duplicates
    pub async fn category_members(
        &self,
        categories: &[String],
        namespace: i64,
        order: MemberOrder,
        limit: u32,
    ) -> Result<Vec<CategoryMember>, AppError> {
        if categories.is_empty() {
            return Ok(vec![]);
        }
        let order_by = match order {
            MemberOrder::AddedToCategory => "cl_timestamp",
            MemberOrder::LastEdit => "page_latest",
        };
        let sql = format!(
            "SELECT page_id, page_title, page_latest, {ADDED}
            FROM page {CATEGORY_JOINS}
            WHERE lt_namespace = 14 AND page_namespace = ? AND lt_title IN ({})
            ORDER BY {order_by} DESC LIMIT ?",
            placeholders(categories.len())
        );
        let params = std::iter::once(namespace.into())
            .chain(values(categories))
            .chain([limit.into()])
            .collect();
        let rows: Vec<(u64, String, u64, Option<String>)> =
            self.query(CATEGORY_TABLES, &sql, params).await?;
        let mut seen = HashSet::new();
        Ok(rows
            .into_iter()
            .filter(|(page_id, ..)| seen.insert(*page_id))
            .map(|(page_id, title, latest_revision, added)| CategoryMember {
                page_id,
                title,
                latest_revision,
                added: added.and_then(|added| added.parse().ok()),
            })
            .collect())
    }

    /// The files last added to any of `categories`, without duplicates, with
    /// when they were added (if that is unknown, when they were uploaded, to
    /// the minute, as in the PHP version)
    pub async fn category_files(
        &self,
        categories: &[String],
        limit: u32,
    ) -> Result<Vec<(FileInfo, Timestamp)>, AppError> {
        let tables = [FILE_TABLES, CATEGORY_TABLES].concat();
        if !matches!(ExtensionDb::for_tables(self.dbname, &tables), Ok(None)) {
            return self.category_files_split(categories, limit).await;
        }
        if categories.is_empty() {
            return Ok(vec![]);
        }
        // Joined, so that the limit only counts pages with a local file
        let sql = format!(
            "SELECT {FILE_COLUMNS}, {ADDED} {FILE_JOINS} {CATEGORY_JOINS}
            WHERE lt_namespace = 14 AND lt_title IN ({})
            ORDER BY cl_timestamp DESC LIMIT ?",
            placeholders(categories.len())
        );
        let params = values(categories).chain([limit.into()]).collect();
        let rows: Vec<CategoryFileRow> = self.query(&tables, &sql, params).await?;
        let mut seen = HashSet::new();
        rows.into_iter()
            .filter(|row| seen.insert(row.0))
            .map(
                |(page_id, name, width, height, media_type, mime, user, timestamp, size, added)| {
                    let row = (
                        page_id, name, width, height, media_type, mime, user, timestamp, size,
                    );
                    let file = file_info(row)?;
                    let added = added.and_then(|added| added.parse().ok());
                    let added = added.unwrap_or(file.timestamp.to_minute());
                    Ok((file, added))
                },
            )
            .collect()
    }

    /// [`Self::category_files`] for wikis whose category tables are in a
    /// database of their own, i.e. Commons, where all files are local
    async fn category_files_split(
        &self,
        categories: &[String],
        limit: u32,
    ) -> Result<Vec<(FileInfo, Timestamp)>, AppError> {
        let members = self
            .category_members(categories, 6, MemberOrder::AddedToCategory, limit)
            .await?;
        let names: Vec<String> = members.iter().map(|m| m.title.clone()).collect();
        let mut files = self.files_by_name(&names).await?;
        Ok(members
            .into_iter()
            .filter_map(|m| {
                let file = files.remove(&m.title)?;
                let added = m.added.unwrap_or(file.timestamp.to_minute());
                Some((file, added))
            })
            .collect())
    }

    /// Up to `limit` random files in `category`, without those that are
    /// certainly not bitmaps, judging by their extension. The media type is
    /// in the core database, which can't be joined on Commons.
    pub async fn random_bitmaps(
        &self,
        category: &str,
        limit: u32,
    ) -> Result<Vec<String>, AppError> {
        let sql = format!(
            "SELECT page_title FROM page {CATEGORY_JOINS}
            WHERE lt_namespace = 14 AND lt_title = ? AND cl_type = 'file'
            AND SUBSTRING_INDEX(LOWER(CONVERT(page_title USING utf8mb4)), '.', -1)
                NOT IN ({NON_BITMAP_EXTENSIONS})
            ORDER BY RAND() LIMIT ?"
        );
        self.query(CATEGORY_TABLES, &sql, vec![category.into(), limit.into()])
            .await
    }

    /// The files with the given names, by name
    pub async fn files_by_name(
        &self,
        names: &[String],
    ) -> Result<HashMap<String, FileInfo>, AppError> {
        if names.is_empty() {
            return Ok(HashMap::new());
        }
        let sql = format!(
            "SELECT {FILE_COLUMNS} {FILE_JOINS} WHERE file_name IN ({})",
            placeholders(names.len())
        );
        let rows: Vec<FileRow> = self
            .query(FILE_TABLES, &sql, values(names).collect())
            .await?;
        rows.into_iter()
            .map(|row| file_info(row).map(|file| (file.name.clone(), file)))
            .collect()
    }

    /// The files last uploaded by `user`, latest first
    pub async fn files_by_user(&self, user: &str, limit: u32) -> Result<Vec<FileInfo>, AppError> {
        let sql = format!(
            "SELECT {FILE_COLUMNS} {FILE_JOINS}
            WHERE actor_name = ? ORDER BY fr_timestamp DESC LIMIT ?"
        );
        let rows: Vec<FileRow> = self
            .query(FILE_TABLES, &sql, vec![user.into(), limit.into()])
            .await?;
        rows.into_iter().map(file_info).collect()
    }

    /// The revisions with the given IDs, by ID
    pub async fn revisions(&self, ids: &[u64]) -> Result<HashMap<u64, Revision>, AppError> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let sql = format!(
            "SELECT rev_id, actor_name, rev_timestamp, rev_len, comment_text
            FROM revision
            LEFT JOIN actor_revision ON actor_id = rev_actor
            LEFT JOIN comment_revision ON comment_id = rev_comment_id
            WHERE rev_id IN ({})",
            placeholders(ids.len())
        );
        type RevisionRow = (u64, Option<String>, String, Option<u64>, Option<Vec<u8>>);
        let rows: Vec<RevisionRow> = self
            .query(REVISION_TABLES, &sql, values(ids).collect())
            .await?;
        rows.into_iter()
            .map(|(id, user, timestamp, size, comment)| {
                let revision = Revision {
                    user: user.unwrap_or_default(),
                    timestamp: timestamp.parse()?,
                    size: size.unwrap_or_default(),
                    // Old comments may have been cut off in the middle of a character
                    comment: String::from_utf8_lossy(&comment.unwrap_or_default()).into_owned(),
                };
                Ok((id, revision))
            })
            .collect()
    }

    /// The categories of the given pages, as DB keys, by page ID
    pub async fn categories_of(
        &self,
        page_ids: &[u64],
    ) -> Result<HashMap<u64, Vec<String>>, AppError> {
        if page_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let sql = format!(
            "SELECT cl_from, lt_title FROM categorylinks
            JOIN linktarget ON lt_id = cl_target_id
            WHERE lt_namespace = 14 AND cl_from IN ({})",
            placeholders(page_ids.len())
        );
        let rows: Vec<(u64, String)> = self
            .query(CATEGORY_TABLES, &sql, values(page_ids).collect())
            .await?;
        let mut categories: HashMap<u64, Vec<String>> = HashMap::new();
        for (page_id, category) in rows {
            categories.entry(page_id).or_default().push(category);
        }
        Ok(categories)
    }
}

fn file_info(
    (page_id, name, width, height, media_type, mime, user, timestamp, size): FileRow,
) -> Result<FileInfo, AppError> {
    Ok(FileInfo {
        page_id,
        name,
        width,
        height,
        media_type,
        mime,
        user,
        timestamp: timestamp.parse()?,
        size,
    })
}

/// `?,?,?` for `n` parameters
fn placeholders(n: usize) -> String {
    vec!["?"; n].join(",")
}

fn values<T: Clone + Into<Value>>(items: &[T]) -> impl Iterator<Item = Value> + use<'_, T> {
    items.iter().cloned().map(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_lists() {
        assert_eq!(placeholders(1), "?");
        assert_eq!(placeholders(3), "?,?,?");
    }
}
