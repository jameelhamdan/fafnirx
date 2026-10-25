//! The file listing of a device: files of a folder, or found by name or type, in the order the user chose.
//!
//! The one place where SQL is assembled at run time, because the sort order is the user's choice. Nothing the user
//! typed is ever put into the text: the order comes from the closed [`Sort`], everything else is a parameter.

use postgres::{Error, GenericClient};

#[derive(Clone, Copy)]
pub enum Sort {
    Name,
    Size,
    Modified,
    Added,
    Taken,
    Type,
}

impl Sort {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "name" => Self::Name,
            "size" => Self::Size,
            "modified" => Self::Modified,
            "added" => Self::Added,
            "taken" => Self::Taken,
            "type" => Self::Type,
            _ => return None,
        })
    }

    fn sql(self) -> &'static str {
        match self {
            Self::Name => "lower(f.path)",
            Self::Size => "v.size",
            Self::Modified => "v.mtime",
            Self::Added => "v.created_at",
            Self::Taken => "m.taken_at",
            Self::Type => r"lower(substring(f.path from '\.([^./]*)$'))",
        }
    }
}

/// Which files of a device, in what order, and which page of them.
pub struct Page<'a> {
    pub device_id: i64,
    /// The `LIKE` pattern of the folder (`path LIKE $2`).
    pub below: &'a str,
    pub include_deleted: bool,
    pub sort: Sort,
    pub descending: bool,
    pub limit: i64,
    pub offset: i64,
}

/// A file of the listing.
pub struct Listed {
    pub id: uuid::Uuid,
    pub path: String,
    pub size: i64,
    pub mtime: i64,
    pub created_at: i64,
    pub deleted_at: Option<i64>,
    pub mime: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub taken_at: Option<i64>,
    pub versions: i64,
}

/// One page of files and how many there are in all.
pub struct Found {
    pub files: Vec<Listed>,
    pub total: i64,
}

const COLUMNS: &str = "f.path, v.size, v.mtime, v.created_at, f.deleted_at, m.mime, m.width, m.height, m.taken_at,
    (SELECT COUNT(*) FROM versions x WHERE x.file_id = f.id), COUNT(*) OVER (), f.id";

const SCOPE: &str =
    "f.device_id = $1 AND f.path LIKE $2 AND (f.deleted_at IS NULL OR ($3 AND f.deleted_by IS DISTINCT FROM 'rename'))";

fn found(rows: &[postgres::Row]) -> Found {
    let total = rows.first().map_or(0, |r| r.get(10));
    let files = rows
        .iter()
        .map(|r| Listed {
            id: r.get(11),
            path: r.get(0),
            size: r.get(1),
            mtime: r.get(2),
            created_at: r.get(3),
            deleted_at: r.get(4),
            mime: r.get(5),
            width: r.get(6),
            height: r.get(7),
            taken_at: r.get(8),
            versions: r.get(9),
        })
        .collect();
    Found { files, total }
}

fn direction(page: &Page) -> &'static str {
    if page.descending {
        "DESC"
    } else {
        "ASC"
    }
}

/// Files below a folder (any depth) whose path contains `search` or ends with one of the extensions in `type_regex`.
pub fn search(
    c: &mut impl GenericClient,
    page: &Page,
    search: Option<&str>,
    type_regex: Option<&str>,
) -> Result<Found, Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM files f JOIN versions v ON v.id = f.current_version_id
         LEFT JOIN file_meta m ON m.root_hash = v.root_hash
         WHERE {SCOPE} AND ($4::text IS NULL OR f.path ILIKE $4) AND ($5::text IS NULL OR lower(f.path) ~ $5)
         ORDER BY {order} {direction} NULLS LAST, f.path LIMIT $6 OFFSET $7",
        order = page.sort.sql(),
        direction = direction(page),
    );
    let rows = c.query(
        &sql,
        &[&page.device_id, &page.below, &page.include_deleted, &search, &type_regex, &page.limit, &page.offset],
    )?;
    Ok(found(&rows))
}

/// The files directly in a folder; `from` is the 1-based character position where the names inside it start.
pub fn files_in(c: &mut impl GenericClient, page: &Page, from: i32) -> Result<Found, Error> {
    let sql = format!(
        "SELECT {COLUMNS} FROM files f JOIN versions v ON v.id = f.current_version_id
         LEFT JOIN file_meta m ON m.root_hash = v.root_hash
         WHERE {SCOPE} AND strpos(substr(f.path, $4), '/') = 0
         ORDER BY {order} {direction} NULLS LAST, f.path LIMIT $5 OFFSET $6",
        order = page.sort.sql(),
        direction = direction(page),
    );
    let rows =
        c.query(&sql, &[&page.device_id, &page.below, &page.include_deleted, &from, &page.limit, &page.offset])?;
    Ok(found(&rows))
}
