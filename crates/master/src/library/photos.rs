//! The photo library: a timeline of every photo and video of every device, favorites, archive, tags,
//! descriptions, ratings, corrected dates and places, the map and the trash.
//!
//! Everything people say about a file lives in `file_state` and `tags`, next to the file and keyed by it:
//! the original is never touched, and a corrected date or place can always be taken back.

use crate::library::meta;
use crate::platform::db::{self, setting};
use crate::web::{done, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, Query, State};
use axum::Json;
use fafnirx_common::unix_now;
use postgres::types::ToSql;
use postgres::{Client, GenericClient, Row};
use serde::{Deserialize, Deserializer, Serialize};

const DAY: i64 = 86400;
/// More than a library of one household holds in one month, and few enough to answer in one go.
const MAX_ASSETS: i64 = 20_000;
const MAX_POINTS: i64 = 200_000;
const MAX_BULK: usize = 10_000;
const MAX_TAGS_PER_FILE: usize = 50;
const MAX_DESCRIPTION: usize = 5000;
const TRASH_PAGE: i64 = 500;

/// Files that exist, with the facts about their current content and what people said about them.
pub const FROM: &str = "files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id
    LEFT JOIN file_meta m ON m.root_hash = v.root_hash LEFT JOIN file_state s ON s.file_id = f.id";
/// When it was taken: as people corrected it, else as the camera said, else when the file last changed.
/// Unix seconds; photos have no time zone, so everything is read as UTC.
pub const WHEN: &str = "COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000)";
const LAT: &str = "COALESCE(s.lat, m.lat)";
const LON: &str = "COALESCE(s.lon, m.lon)";

pub fn asset_columns() -> String {
    format!(
        "f.id, d.name, f.path, v.size, v.root_hash, m.width, m.height, m.duration_ms, {WHEN},
         COALESCE(s.favorite, false), COALESCE(s.archived, false), s.rating, {LAT}, {LON}"
    )
}

/// A photo or video as the grid and the viewer need it.
#[derive(Serialize)]
pub struct Asset {
    pub id: i64,
    pub device: String,
    pub path: String,
    pub name: String,
    /// image or video
    pub kind: &'static str,
    pub size: i64,
    /// The start of the content hash: a link with it never goes stale, because new content has a new hash.
    pub hash: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub when: i64,
    pub favorite: bool,
    pub archived: bool,
    pub rating: Option<i16>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

impl Asset {
    /// From a row of [`asset_columns`].
    pub fn from_row(r: &Row) -> Self {
        let path: String = r.get(2);
        let root: Vec<u8> = r.get(4);
        Self {
            id: r.get(0),
            device: r.get(1),
            name: path.rsplit('/').next().unwrap_or(&path).to_string(),
            kind: meta::kind(&path).unwrap_or("image"),
            size: r.get(3),
            hash: hex::encode(&root[..root.len().min(6)]),
            width: r.get(5),
            height: r.get(6),
            duration_ms: r.get(7),
            when: r.get(8),
            favorite: r.get(9),
            archived: r.get(10),
            rating: r.get(11),
            lat: r.get(12),
            lon: r.get(13),
            path,
        }
    }
}

/// A WHERE clause that is built piece by piece, with its parameters.
pub struct Where {
    sql: String,
    params: Vec<Box<dyn ToSql + Sync + Send>>,
}

impl Where {
    pub fn new(first: &str) -> Self {
        Self { sql: first.to_string(), params: Vec::new() }
    }

    /// Adds `AND clause`, where `?` stands for `value`.
    pub fn and(&mut self, clause: &str, value: impl ToSql + Sync + Send + 'static) {
        self.params.push(Box::new(value));
        self.sql.push_str(" AND ");
        self.sql.push_str(&clause.replace('?', &format!("${}", self.params.len())));
    }

    pub fn and_plain(&mut self, clause: &str) {
        self.sql.push_str(" AND ");
        self.sql.push_str(clause);
    }

    pub fn sql(&self) -> &str {
        &self.sql
    }

    pub fn params(&self) -> Vec<&(dyn ToSql + Sync)> {
        self.params.iter().map(|p| &**p as &(dyn ToSql + Sync)).collect()
    }
}

/// Which files a view of the library shows.
#[derive(Deserialize, Default)]
pub struct Filter {
    /// timeline (everything not archived), favorites or archive
    scope: Option<String>,
    device: Option<String>,
    /// image or video
    kind: Option<String>,
    tag: Option<i64>,
    /// YYYY-MM
    month: Option<String>,
}

impl Filter {
    fn clause(&self) -> Result<Where, ApiError> {
        let mut w = Where::new("f.deleted_at IS NULL");
        w.and_plain("f.media IS NOT NULL");
        match self.scope.as_deref().unwrap_or("timeline") {
            "timeline" => w.and_plain("COALESCE(s.archived, false) = false"),
            "favorites" => w.and_plain("COALESCE(s.favorite, false)"),
            "archive" => w.and_plain("COALESCE(s.archived, false)"),
            other => return Err(ApiError::bad(format!("unknown scope {other}"))),
        }
        if let Some(device) = self.device.as_deref().filter(|d| !d.is_empty()) {
            w.and("d.name = ?", device.to_string());
        }
        match self.kind.as_deref().filter(|k| !k.is_empty()) {
            None => {}
            Some("image") => w.and_plain("f.media = 'image'"),
            Some("video") => w.and_plain("f.media = 'video'"),
            Some(other) => return Err(ApiError::bad(format!("unknown kind {other}"))),
        }
        if let Some(tag) = self.tag {
            w.and("EXISTS (SELECT 1 FROM file_tags t WHERE t.file_id = f.id AND t.tag_id = ?)", tag);
        }
        if let Some(month) = self.month.as_deref().filter(|m| !m.is_empty()) {
            valid_month(month)?;
            w.and(&format!("{} = ?", month_of(WHEN)), month.to_string());
        }
        Ok(w)
    }
}

/// SQL that turns unix seconds into `YYYY-MM`.
fn month_of(when: &str) -> String {
    format!("to_char(to_timestamp({when}) AT TIME ZONE 'UTC', 'YYYY-MM')")
}

fn valid_month(text: &str) -> Result<(), ApiError> {
    let ok = text.len() >= 7
        && text.as_bytes()[text.len() - 3] == b'-'
        && text[..text.len() - 3].bytes().all(|b| b.is_ascii_digit())
        && text[text.len() - 2..].parse::<u8>().is_ok_and(|m| (1..=12).contains(&m));
    if ok {
        Ok(())
    } else {
        Err(ApiError::bad("month must look like 2024-05"))
    }
}

#[derive(Serialize)]
pub struct Month {
    month: String,
    count: i64,
    /// How many files each day of the month holds, newest day first. With these the page knows how tall the
    /// month will be before it has loaded any picture, so the scrollbar and the scrubber are exact.
    days: Vec<i64>,
}

#[derive(Serialize)]
pub struct Buckets {
    months: Vec<Month>,
    total: i64,
}

/// How many photos and videos each month and day holds, newest first: the skeleton of the timeline.
pub async fn buckets(State(state): State<Shared>, Query(filter): Query<Filter>) -> Res<Buckets> {
    let w = filter.clause()?;
    let out = run(&state, move |c| {
        let rows = c.query(
            &format!(
                "SELECT to_char(to_timestamp({WHEN}) AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS day, COUNT(*)
                 FROM {FROM} WHERE {} GROUP BY 1 ORDER BY 1 DESC",
                w.sql()
            ),
            &w.params(),
        )?;
        let mut months: Vec<Month> = Vec::new();
        for r in &rows {
            let (day, n): (String, i64) = (r.get(0), r.get(1));
            let month = day[..day.len().saturating_sub(3)].to_string();
            match months.last_mut() {
                Some(m) if m.month == month => {
                    m.count += n;
                    m.days.push(n);
                }
                _ => months.push(Month { month, count: n, days: vec![n] }),
            }
        }
        let total = months.iter().map(|m| m.count).sum();
        Ok(Buckets { months, total })
    })
    .await?;
    Ok(Json(out))
}

#[derive(Serialize)]
pub struct Assets {
    assets: Vec<Asset>,
}

/// The photos and videos of one month, newest first.
pub async fn timeline(State(state): State<Shared>, Query(filter): Query<Filter>) -> Res<Assets> {
    if filter.month.as_deref().unwrap_or("").is_empty() {
        return Err(ApiError::bad("month is required, like 2024-05"));
    }
    let w = filter.clause()?;
    let assets = run(&state, move |c| {
        let rows = c.query(
            &format!(
                "SELECT {} FROM {FROM} WHERE {} ORDER BY {WHEN} DESC, f.id DESC LIMIT {MAX_ASSETS}",
                asset_columns(),
                w.sql()
            ),
            &w.params(),
        )?;
        Ok(rows.iter().map(Asset::from_row).collect())
    })
    .await?;
    Ok(Json(Assets { assets }))
}

#[derive(Deserialize)]
pub struct IdsQuery {
    /// Comma separated file ids.
    ids: String,
}

/// Some files by id (the map shows places, and opens the files that are there).
pub async fn by_ids(State(state): State<Shared>, Query(q): Query<IdsQuery>) -> Res<Assets> {
    let ids: Vec<i64> = q.ids.split(',').filter_map(|s| s.trim().parse().ok()).take(500).collect();
    let assets = run(&state, move |c| {
        let rows = c.query(
            &format!(
                "SELECT {} FROM {FROM} WHERE f.id = ANY($1) AND f.deleted_at IS NULL ORDER BY {WHEN} DESC, f.id DESC",
                asset_columns()
            ),
            &[&ids],
        )?;
        Ok(rows.iter().map(Asset::from_row).collect())
    })
    .await?;
    Ok(Json(Assets { assets }))
}

#[derive(Serialize)]
pub struct TagRef {
    id: i64,
    name: String,
}

#[derive(Serialize)]
pub struct AlbumRef {
    id: i64,
    name: String,
}

#[derive(Serialize)]
pub struct Detail {
    #[serde(flatten)]
    asset: Asset,
    mime: String,
    description: String,
    /// When the camera (or the video) says it was taken, before any correction.
    camera_when: Option<i64>,
    /// Where the camera says it was, before any correction.
    camera_lat: Option<f64>,
    camera_lon: Option<f64>,
    when_edited: bool,
    place_edited: bool,
    exif: serde_json::Value,
    codec: Option<String>,
    tags: Vec<TagRef>,
    albums: Vec<AlbumRef>,
    /// When it was recorded here.
    changed: i64,
    /// The preview picture can be made (so the grid and viewer should ask for it).
    preview: bool,
}

pub async fn detail(State(state): State<Shared>, Path(id): Path<i64>) -> Res<Detail> {
    let out = run(&state, move |c| {
        let row = c
            .query_opt(
                &format!(
                    "SELECT {}, m.mime, COALESCE(s.description, ''), m.taken_at, m.lat, m.lon, s.taken_at IS NOT NULL,
                            s.lat IS NOT NULL, m.exif, m.codec, v.created_at
                     FROM {FROM} WHERE f.id = $1",
                    asset_columns()
                ),
                &[&id],
            )?
            .ok_or_else(|| ApiError::not_found("No such file"))?;
        let asset = Asset::from_row(&row);
        let exif: Option<String> = row.get(21);
        let tags = c
            .query("SELECT t.id, t.name FROM file_tags ft JOIN tags t ON t.id = ft.tag_id WHERE ft.file_id = $1 ORDER BY lower(t.name)", &[&id])?
            .iter()
            .map(|r| TagRef { id: r.get(0), name: r.get(1) })
            .collect();
        let albums = c
            .query("SELECT a.id, a.name FROM album_files af JOIN albums a ON a.id = af.album_id WHERE af.file_id = $1 ORDER BY lower(a.name)", &[&id])?
            .iter()
            .map(|r| AlbumRef { id: r.get(0), name: r.get(1) })
            .collect();
        Ok(Detail {
            mime: row.get::<_, Option<String>>(14).unwrap_or_else(|| meta::mime(&asset.path)),
            description: row.get(15),
            camera_when: row.get(16),
            camera_lat: row.get(17),
            camera_lon: row.get(18),
            when_edited: row.get(19),
            place_edited: row.get(20),
            exif: exif.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(serde_json::Value::Null),
            codec: row.get(22),
            tags,
            albums,
            changed: row.get(23),
            preview: meta::can_preview(&asset.path),
            asset,
        })
    })
    .await?;
    Ok(Json(out))
}

/// `null` in the request means "clear it", an absent field means "leave it": this tells them apart.
fn nullable<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Deserialize)]
pub struct Position {
    lat: f64,
    lon: f64,
}

/// A change to what is said about one file. Only the fields that are present change.
#[derive(Deserialize, Default)]
pub struct Edit {
    favorite: Option<bool>,
    archived: Option<bool>,
    /// 1 to 5, or null for no rating.
    #[serde(default, deserialize_with = "nullable")]
    rating: Option<Option<i16>>,
    description: Option<String>,
    /// Unix seconds, or null to go back to what the camera said.
    #[serde(default, deserialize_with = "nullable")]
    taken_at: Option<Option<i64>>,
    /// A place, or null to go back to what the camera said.
    #[serde(default, deserialize_with = "nullable")]
    location: Option<Option<Position>>,
    /// The complete list of tags the file should have.
    tags: Option<Vec<String>>,
}

/// Tag names as they will be stored: trimmed, no empty ones, no duplicates (case does not matter).
fn clean_tags(names: &[String]) -> Result<Vec<String>, ApiError> {
    let mut out: Vec<String> = Vec::new();
    for name in names {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if name.chars().count() > 64 {
            return Err(ApiError::bad("A tag can have at most 64 characters"));
        }
        if !out.iter().any(|t| t.to_lowercase() == name.to_lowercase()) {
            out.push(name.to_string());
        }
    }
    if out.len() > MAX_TAGS_PER_FILE {
        return Err(ApiError::bad(format!("A file can have at most {MAX_TAGS_PER_FILE} tags")));
    }
    Ok(out)
}

/// The ids of these tags, made when they are new.
fn tag_ids(tx: &mut impl GenericClient, names: &[String]) -> anyhow::Result<Vec<i64>> {
    let mut ids = Vec::new();
    for name in names {
        ids.push(
            tx.query_one(
                "INSERT INTO tags (name) VALUES ($1) ON CONFLICT ((lower(name))) DO UPDATE SET name = tags.name RETURNING id",
                &[name],
            )?
            .get(0),
        );
    }
    Ok(ids)
}

fn drop_unused_tags(tx: &mut impl GenericClient) -> Result<(), postgres::Error> {
    tx.execute("DELETE FROM tags t WHERE NOT EXISTS (SELECT 1 FROM file_tags ft WHERE ft.tag_id = t.id)", &[])?;
    Ok(())
}

fn apply(tx: &mut impl GenericClient, id: i64, e: &Edit) -> anyhow::Result<()> {
    if let Some(Some(r)) = e.rating {
        if !(1..=5).contains(&r) {
            return Err(ApiError::bad("A rating is 1 to 5 stars").into());
        }
    }
    if e.description.as_ref().is_some_and(|d| d.chars().count() > MAX_DESCRIPTION) {
        return Err(ApiError::bad(format!("A description can have at most {MAX_DESCRIPTION} characters")).into());
    }
    if let Some(Some(t)) = e.taken_at {
        // Years 1 to 9999: what a date can show.
        if !(-62_135_596_800..=253_402_300_799).contains(&t) {
            return Err(ApiError::bad("That date is out of range").into());
        }
    }
    if let Some(Some(p)) = &e.location {
        if meta::valid_position(p.lat, p.lon).is_none() {
            return Err(ApiError::bad("Latitude is -90 to 90, longitude -180 to 180 (and not 0, 0)").into());
        }
    }
    let tags = e.tags.as_deref().map(clean_tags).transpose()?;

    let exists: bool =
        tx.query_one("SELECT EXISTS (SELECT 1 FROM files WHERE id = $1 AND deleted_at IS NULL)", &[&id])?.get(0);
    if !exists {
        return Err(ApiError::not_found("No such file").into());
    }
    let now = unix_now();
    tx.execute(
        "INSERT INTO file_state (file_id, updated_at) VALUES ($1, $2) ON CONFLICT (file_id) DO NOTHING",
        &[&id, &now],
    )?;
    if let Some(v) = e.favorite {
        tx.execute("UPDATE file_state SET favorite = $2 WHERE file_id = $1", &[&id, &v])?;
    }
    if let Some(v) = e.archived {
        tx.execute("UPDATE file_state SET archived = $2 WHERE file_id = $1", &[&id, &v])?;
    }
    if let Some(v) = e.rating {
        tx.execute("UPDATE file_state SET rating = $2 WHERE file_id = $1", &[&id, &v])?;
    }
    if let Some(v) = &e.description {
        tx.execute("UPDATE file_state SET description = $2 WHERE file_id = $1", &[&id, &v.trim()])?;
    }
    if let Some(v) = e.taken_at {
        tx.execute("UPDATE file_state SET taken_at = $2 WHERE file_id = $1", &[&id, &v])?;
    }
    if let Some(v) = &e.location {
        let (lat, lon) = v.as_ref().map(|p| (p.lat, p.lon)).unzip();
        tx.execute("UPDATE file_state SET lat = $2, lon = $3 WHERE file_id = $1", &[&id, &lat, &lon])?;
    }
    tx.execute("UPDATE file_state SET updated_at = $2 WHERE file_id = $1", &[&id, &now])?;
    if let Some(names) = tags {
        let ids = tag_ids(tx, &names)?;
        tx.execute("DELETE FROM file_tags WHERE file_id = $1 AND NOT (tag_id = ANY($2))", &[&id, &ids])?;
        tx.execute(
            "INSERT INTO file_tags (file_id, tag_id) SELECT $1, unnest($2::bigint[]) ON CONFLICT DO NOTHING",
            &[&id, &ids],
        )?;
        drop_unused_tags(tx)?;
    }
    Ok(())
}

pub async fn edit(State(state): State<Shared>, Path(id): Path<i64>, Json(e): Json<Edit>) -> Res<Done> {
    run(&state, move |c| {
        let mut tx = c.transaction()?;
        apply(&mut tx, id, &e)?;
        db::notify(&mut tx, "library", "")?;
        tx.commit()?;
        Ok(())
    })
    .await?;
    done()
}

#[derive(Deserialize)]
pub struct Bulk {
    ids: Vec<i64>,
    favorite: Option<bool>,
    archived: Option<bool>,
    add_tags: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct Changed {
    changed: u64,
}

/// The same change to many files at once (what the selection bar does).
pub async fn bulk(State(state): State<Shared>, Json(b): Json<Bulk>) -> Res<Changed> {
    if b.ids.is_empty() {
        return Err(ApiError::bad("Nothing selected"));
    }
    if b.ids.len() > MAX_BULK {
        return Err(ApiError::bad(format!("Select at most {MAX_BULK} files at a time")));
    }
    let tags = b.add_tags.as_deref().map(clean_tags).transpose()?;
    let changed = run(&state, move |c| {
        let mut tx = c.transaction()?;
        let now = unix_now();
        let mut changed = 0;
        if let Some(v) = b.favorite {
            changed = tx.execute(
                "INSERT INTO file_state (file_id, updated_at, favorite) SELECT id, $2, $3 FROM files
                 WHERE id = ANY($1) AND deleted_at IS NULL
                 ON CONFLICT (file_id) DO UPDATE SET favorite = excluded.favorite, updated_at = excluded.updated_at",
                &[&b.ids, &now, &v],
            )?;
        }
        if let Some(v) = b.archived {
            changed = tx.execute(
                "INSERT INTO file_state (file_id, updated_at, archived) SELECT id, $2, $3 FROM files
                 WHERE id = ANY($1) AND deleted_at IS NULL
                 ON CONFLICT (file_id) DO UPDATE SET archived = excluded.archived, updated_at = excluded.updated_at",
                &[&b.ids, &now, &v],
            )?;
        }
        if let Some(names) = tags.filter(|t| !t.is_empty()) {
            let ids = tag_ids(&mut tx, &names)?;
            changed = tx.execute(
                "INSERT INTO file_tags (file_id, tag_id) SELECT f.id, t FROM files f CROSS JOIN unnest($2::bigint[]) t
                 WHERE f.id = ANY($1) AND f.deleted_at IS NULL ON CONFLICT DO NOTHING",
                &[&b.ids, &ids],
            )?;
        }
        db::notify(&mut tx, "library", "")?;
        tx.commit()?;
        Ok(changed)
    })
    .await?;
    Ok(Json(Changed { changed }))
}

#[derive(Serialize)]
pub struct Tag {
    id: i64,
    name: String,
    count: i64,
}

pub async fn tags(State(state): State<Shared>) -> Res<Vec<Tag>> {
    let out = run(&state, |c| {
        Ok(c.query(
            "SELECT t.id, t.name, COUNT(f.id) FROM tags t
             LEFT JOIN file_tags ft ON ft.tag_id = t.id LEFT JOIN files f ON f.id = ft.file_id AND f.deleted_at IS NULL
             GROUP BY t.id ORDER BY lower(t.name)",
            &[],
        )?
        .iter()
        .map(|r| Tag { id: r.get(0), name: r.get(1), count: r.get(2) })
        .collect())
    })
    .await?;
    Ok(Json(out))
}

pub async fn delete_tag(State(state): State<Shared>, Path(id): Path<i64>) -> Res<Done> {
    run(&state, move |c| {
        if c.execute("DELETE FROM tags WHERE id = $1", &[&id])? == 0 {
            return Err(ApiError::not_found("No such tag").into());
        }
        db::notify(c, "library", "")?;
        Ok(())
    })
    .await?;
    done()
}

#[derive(Serialize)]
pub struct MapPoints {
    /// `[id, latitude, longitude, when]`
    points: Vec<(i64, f64, f64, i64)>,
    /// More photos have a place than are listed.
    truncated: bool,
}

/// Every photo and video that has a place, for the map.
pub async fn map(State(state): State<Shared>, Query(filter): Query<Filter>) -> Res<MapPoints> {
    let mut w = filter.clause()?;
    w.and_plain(&format!("{LAT} IS NOT NULL"));
    let out = run(&state, move |c| {
        let rows = c.query(
            &format!(
                "SELECT f.id, {LAT}, {LON}, {WHEN} FROM {FROM} WHERE {} ORDER BY {WHEN} DESC LIMIT {}",
                w.sql(),
                MAX_POINTS + 1
            ),
            &w.params(),
        )?;
        let truncated = rows.len() as i64 > MAX_POINTS;
        let points = rows.iter().take(MAX_POINTS as usize).map(|r| (r.get(0), r.get(1), r.get(2), r.get(3))).collect();
        Ok(MapPoints { points, truncated })
    })
    .await?;
    Ok(Json(out))
}

#[derive(Serialize)]
pub struct TrashEntry {
    id: i64,
    device: String,
    path: String,
    name: String,
    size: i64,
    deleted_at: i64,
    purge_at: i64,
    /// The version to restore: the last one the file had.
    version: i64,
    kind: Option<&'static str>,
    hash: String,
}

#[derive(Serialize)]
pub struct Trash {
    entries: Vec<TrashEntry>,
    total: i64,
    retention_days: i64,
}

#[derive(Deserialize)]
pub struct TrashQuery {
    #[serde(default)]
    offset: i64,
}

/// Deleted files of every device, newest deletion first. They are removed for good when the retention
/// period ends; restoring one is `POST /api/files/restore` with its `version`.
pub async fn trash(State(state): State<Shared>, Query(q): Query<TrashQuery>) -> Res<Trash> {
    let out = run(&state, move |c| {
        let retention_days = setting(c, "retention_days", 30);
        let rows = c.query(
            "SELECT f.id, d.name, f.path, v.size, f.deleted_at, v.id, v.root_hash, COUNT(*) OVER ()
             FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id
             WHERE f.deleted_at IS NOT NULL ORDER BY f.deleted_at DESC, f.id DESC LIMIT $1 OFFSET $2",
            &[&TRASH_PAGE, &q.offset.max(0)],
        )?;
        let entries = rows
            .iter()
            .map(|r| {
                let path: String = r.get(2);
                let deleted_at: i64 = r.get(4);
                let root: Vec<u8> = r.get(6);
                TrashEntry {
                    id: r.get(0),
                    device: r.get(1),
                    name: path.rsplit('/').next().unwrap_or(&path).to_string(),
                    size: r.get(3),
                    deleted_at,
                    purge_at: deleted_at + retention_days * DAY,
                    version: r.get(5),
                    kind: meta::kind(&path),
                    hash: hex::encode(&root[..root.len().min(6)]),
                    path,
                }
            })
            .collect();
        Ok(Trash { entries, total: rows.first().map_or(0, |r| r.get(7)), retention_days })
    })
    .await?;
    Ok(Json(out))
}

/// The tables a mirror keeps a copy of, so that albums, tags and favorites survive losing the database.
pub fn snapshot(c: &mut Client) -> anyhow::Result<serde_json::Value> {
    let files_of =
        |c: &mut Client, sql: &str| -> anyhow::Result<std::collections::BTreeMap<i64, Vec<serde_json::Value>>> {
            let mut map: std::collections::BTreeMap<i64, Vec<serde_json::Value>> = Default::default();
            for r in c.query(sql, &[])? {
                map.entry(r.get(0))
                    .or_default()
                    .push(serde_json::json!({ "device": r.get::<_, String>(1), "path": r.get::<_, String>(2) }));
            }
            Ok(map)
        };
    let mut album_files = files_of(
        c,
        "SELECT af.album_id, d.name, f.path FROM album_files af JOIN files f ON f.id = af.file_id
         JOIN devices d ON d.id = f.device_id ORDER BY af.album_id, f.path",
    )?;
    let albums: Vec<_> = c
        .query("SELECT id, name, description, created_at FROM albums ORDER BY id", &[])?
        .iter()
        .map(|r| {
            let id: i64 = r.get(0);
            serde_json::json!({ "name": r.get::<_, String>(1), "description": r.get::<_, String>(2),
                                "created_at": r.get::<_, i64>(3), "files": album_files.remove(&id).unwrap_or_default() })
        })
        .collect();
    let mut tag_files = files_of(
        c,
        "SELECT ft.tag_id, d.name, f.path FROM file_tags ft JOIN files f ON f.id = ft.file_id
         JOIN devices d ON d.id = f.device_id ORDER BY ft.tag_id, f.path",
    )?;
    let tags: Vec<_> = c
        .query("SELECT id, name FROM tags ORDER BY id", &[])?
        .iter()
        .map(|r| serde_json::json!({ "name": r.get::<_, String>(1), "files": tag_files.remove(&r.get::<_, i64>(0)).unwrap_or_default() }))
        .collect();
    let states: Vec<_> = c
        .query(
            "SELECT d.name, f.path, s.favorite, s.archived, s.rating, s.description, s.taken_at, s.lat, s.lon
             FROM file_state s JOIN files f ON f.id = s.file_id JOIN devices d ON d.id = f.device_id
             WHERE s.favorite OR s.archived OR s.rating IS NOT NULL OR s.description <> '' OR s.taken_at IS NOT NULL OR s.lat IS NOT NULL
             ORDER BY d.name, f.path",
            &[],
        )?
        .iter()
        .map(|r| {
            serde_json::json!({ "device": r.get::<_, String>(0), "path": r.get::<_, String>(1), "favorite": r.get::<_, bool>(2),
                "archived": r.get::<_, bool>(3), "rating": r.get::<_, Option<i16>>(4), "description": r.get::<_, String>(5),
                "taken_at": r.get::<_, Option<i64>>(6), "lat": r.get::<_, Option<f64>>(7), "lon": r.get::<_, Option<f64>>(8) })
        })
        .collect();
    Ok(serde_json::json!({ "generated_at_unix": unix_now(), "albums": albums, "tags": tags, "files": states }))
}
