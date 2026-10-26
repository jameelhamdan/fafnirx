//! The photo library: a timeline of every photo and video a user can reach (their devices and the master files),
//! favorites, archive, tags, descriptions, ratings, corrected dates and places, the map and the trash.
//!
//! Everything people say about a file lives in `file_state` and `tags`, next to the file and keyed by it and by
//! the user: the original is never touched, a corrected date or place can always be taken back, and the master
//! files, which everybody sees, are each user's own to favorite, archive, tag and describe.
//!
//! This module checks what a request says and shapes the answer; what is stored, and how it is found, is in
//! `repo::library`, `repo::file_state` and `repo::tags`.

use crate::library::{artwork, meta};
use crate::platform::db;
use crate::platform::id::{uuids, Id};
use crate::repo::library::{self, Media, Scope};
use crate::repo::{albums, file_state, files, settings, snapshot, tags};
use crate::web::access::Access;
use crate::web::{done, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use fafnirx_common::{base_name, unix_now, DAY_SECS};
use postgres::{Client, GenericClient};
use serde::{Deserialize, Deserializer, Serialize};

/// More than a library of one household holds in one month, and few enough to answer in one go.
pub const MAX_ASSETS: i64 = 20_000;
const MAX_POINTS: i64 = 200_000;
const MAX_BULK: usize = 10_000;
const MAX_TAGS_PER_FILE: usize = 50;
const MAX_DESCRIPTION: usize = 5000;
const TRASH_PAGE: i64 = 500;

/// The start of a content hash, which is enough to tell versions of a file apart in a link.
fn short_hash(root: &[u8]) -> String {
    hex::encode(&root[..root.len().min(6)])
}

/// A photo or video as the grid and the viewer need it.
#[derive(Serialize)]
pub struct Asset {
    pub id: Id,
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
    /// Its details have not been read yet (a job is waiting or running, or failed): see Processing.
    pub processing: bool,
}

impl From<library::Asset> for Asset {
    fn from(a: library::Asset) -> Self {
        Self {
            id: Id(a.id),
            name: base_name(&a.path).to_string(),
            kind: meta::kind(&a.path).unwrap_or("image"),
            hash: short_hash(&a.root_hash),
            device: a.folder,
            size: a.size,
            width: a.width,
            height: a.height,
            duration_ms: a.duration_ms,
            when: a.shown_at,
            favorite: a.favorite,
            archived: a.archived,
            rating: a.rating,
            lat: a.lat,
            lon: a.lon,
            processing: a.processing,
            path: a.path,
        }
    }
}

/// Which files a view of the library shows, as the request names them.
#[derive(Deserialize, Default)]
pub struct Filter {
    /// timeline (everything not archived), favorites or archive
    scope: Option<String>,
    device: Option<String>,
    /// image or video
    kind: Option<String>,
    tag: Option<Id>,
    /// YYYY-MM
    month: Option<String>,
}

impl Filter {
    /// The same, checked, in the terms of the repository.
    fn checked(&self) -> Result<library::Filter<'_>, ApiError> {
        let scope = match self.scope.as_deref().unwrap_or("timeline") {
            "timeline" => Scope::Timeline,
            "favorites" => Scope::Favorites,
            "archive" => Scope::Archive,
            other => return Err(ApiError::bad(format!("unknown scope {other}"))),
        };
        let kind = match self.kind.as_deref().filter(|k| !k.is_empty()) {
            None => None,
            Some("image") => Some(Media::Image),
            Some("video") => Some(Media::Video),
            Some(other) => return Err(ApiError::bad(format!("unknown kind {other}"))),
        };
        let month = self.month.as_deref().filter(|m| !m.is_empty());
        if let Some(month) = month {
            valid_month(month)?;
        }
        Ok(library::Filter {
            scope,
            device: self.device.as_deref().filter(|d| !d.is_empty()),
            kind,
            tag: self.tag.map(|t| t.0),
            month,
        })
    }
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

/// Groups days (`YYYY-MM-DD`, newest first) into months.
fn months_of(days: Vec<(String, i64)>) -> Vec<Month> {
    let mut months: Vec<Month> = Vec::new();
    for (day, n) in days {
        let month = day[..day.len().saturating_sub(3)].to_string();
        match months.last_mut() {
            Some(m) if m.month == month => {
                m.count += n;
                m.days.push(n);
            }
            _ => months.push(Month { month, count: n, days: vec![n] }),
        }
    }
    months
}

/// How many photos and videos each month and day holds, newest first: the skeleton of the timeline.
pub async fn buckets(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Query(filter): Query<Filter>,
) -> Res<Buckets> {
    filter.checked()?;
    let out = run(&state, move |c| {
        let days = library::days(c, who.viewer(), &filter.checked()?)?;
        let months = months_of(days);
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
pub async fn timeline(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Query(filter): Query<Filter>,
) -> Res<Assets> {
    if filter.month.as_deref().unwrap_or("").is_empty() {
        return Err(ApiError::bad("month is required, like 2024-05"));
    }
    filter.checked()?;
    let assets = run(&state, move |c| {
        Ok(library::assets(c, who.viewer(), &filter.checked()?, MAX_ASSETS)?.into_iter().map(Asset::from).collect())
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
pub async fn by_ids(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Query(q): Query<IdsQuery>,
) -> Res<Assets> {
    let ids: Vec<uuid::Uuid> =
        q.ids.split(',').filter_map(|s| s.trim().parse::<Id>().ok()).map(|i| i.0).take(500).collect();
    let assets =
        run(&state, move |c| Ok(library::by_ids(c, who.viewer(), &ids)?.into_iter().map(Asset::from).collect()))
            .await?;
    Ok(Json(Assets { assets }))
}

#[derive(Serialize)]
pub struct TagRef {
    id: Id,
    name: String,
}

#[derive(Serialize)]
pub struct AlbumRef {
    id: Id,
    name: String,
}

#[derive(Serialize)]
pub struct Detail {
    #[serde(flatten)]
    asset: Asset,
    mime: String,
    description: String,
    /// The caption written into the file itself (by the camera or a photo manager), when there is one.
    file_description: Option<String>,
    /// The keywords written into the file (also where AI taggers put what they recognise): offered as tags.
    file_tags: Vec<String>,
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
    /// What the .nfo next to a video says about it (title, season, episode, plot), when there is one.
    described: Option<artwork::Described>,
    /// Work still to do on it: waiting, running or failed. Not set when everything is done.
    work: Option<String>,
}

/// The photos and videos in the folder of a file, in the order of their names: what the viewer opened from the file browser
/// steps through. The file itself is in it.
pub async fn folder(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(Id(id)): Path<Id>,
) -> Res<Assets> {
    let assets = run(&state, move |c| {
        let mut assets: Vec<Asset> =
            library::folder_of(c, who.viewer(), id, MAX_ASSETS)?.into_iter().map(Asset::from).collect();
        if !assets.iter().any(|a| a.id.0 == id) {
            return Err(ApiError::not_found("No such file").into());
        }
        assets.sort_by(|a, b| crate::library::albums::natural_cmp(&a.path, &b.path));
        Ok(assets)
    })
    .await?;
    Ok(Json(Assets { assets }))
}

pub async fn detail(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(Id(id)): Path<Id>,
) -> Res<Detail> {
    let store = state.store.clone();
    let out = run(&state, move |c| {
        let viewer = who.viewer();
        let not_found = || ApiError::not_found("No such file");
        let asset = library::by_ids(c, viewer, &[id])?.into_iter().next().ok_or_else(not_found)?;
        let extras = library::extras(c, viewer.uid, id)?.ok_or_else(not_found)?;
        let tags =
            tags::of_file(c, viewer.uid, id)?.into_iter().map(|(id, name)| TagRef { id: Id(id), name }).collect();
        let albums =
            albums::of_file(c, viewer.uid, id)?.into_iter().map(|(id, name)| AlbumRef { id: Id(id), name }).collect();
        let asset = Asset::from(asset);
        let described = described(c, &store, &asset);
        let work = work_state(c, &asset.device, &asset.path, asset.processing)?;
        Ok(Detail {
            mime: extras.mime.unwrap_or_else(|| meta::mime(&asset.path)),
            description: extras.description,
            file_description: extras.caption,
            file_tags: extras.keywords.and_then(|k| serde_json::from_str(&k).ok()).unwrap_or_default(),
            camera_when: extras.camera_when,
            camera_lat: extras.camera_lat,
            camera_lon: extras.camera_lon,
            when_edited: extras.when_edited,
            place_edited: extras.place_edited,
            exif: extras.exif.and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(serde_json::Value::Null),
            codec: extras.codec,
            tags,
            albums,
            changed: extras.changed,
            preview: meta::can_preview(&asset.path),
            described,
            work,
            asset,
        })
    })
    .await?;
    Ok(Json(out))
}

/// Whether a file still has work to do, from its unfinished jobs: `running`, `waiting` or `failed`. A file whose details were
/// never read counts as waiting even before its job is queued.
pub fn work_state(
    c: &mut impl GenericClient,
    device: &str,
    path: &str,
    unread: bool,
) -> Result<Option<String>, postgres::Error> {
    let jobs = crate::repo::jobs::pending_of(c, device, path)?;
    let has = |s: &str| jobs.iter().any(|(_, status)| status == s);
    Ok(if has("running") {
        Some("running")
    } else if has("queued") || (unread && !has("failed")) {
        Some("waiting")
    } else if has("failed") {
        Some("failed")
    } else {
        None
    }
    .map(str::to_string))
}

/// What the .nfo next to a video says about it. Anything that goes wrong means no description, never an error.
fn described(
    c: &mut postgres::Client,
    store: &crate::platform::store::Store,
    asset: &Asset,
) -> Option<artwork::Described> {
    if asset.kind != "video" {
        return None;
    }
    let near = files::neighbours(c, &asset.device, &asset.path).ok()?;
    let (nfo, _) = artwork::description(&asset.path, &near)?;
    let disk = store.live_path(&asset.device, nfo).ok()?;
    if std::fs::metadata(&disk).ok()?.len() > artwork::NFO_MAX_BYTES {
        return None;
    }
    artwork::parse_nfo(&String::from_utf8_lossy(&std::fs::read(disk).ok()?))
}

/// `null` in the request means "clear it", an absent field means "leave it": this tells them apart.
pub fn nullable<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
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

impl Edit {
    /// Refuses what cannot be stored: a rating out of range, a description too long, a date or place that does not exist.
    fn check(&self) -> Result<(), ApiError> {
        if let Some(Some(r)) = self.rating {
            if !(1..=5).contains(&r) {
                return Err(ApiError::bad("A rating is 1 to 5 stars"));
            }
        }
        if self.description.as_ref().is_some_and(|d| d.chars().count() > MAX_DESCRIPTION) {
            return Err(ApiError::bad(format!("A description can have at most {MAX_DESCRIPTION} characters")));
        }
        if let Some(Some(t)) = self.taken_at {
            // Years 1 to 9999: what a date can show.
            if !(-62_135_596_800..=253_402_300_799).contains(&t) {
                return Err(ApiError::bad("That date is out of range"));
            }
        }
        if let Some(Some(p)) = &self.location {
            if meta::valid_position(p.lat, p.lon).is_none() {
                return Err(ApiError::bad("Latitude is -90 to 90, longitude -180 to 180 (and not 0, 0)"));
            }
        }
        Ok(())
    }
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

/// The ids of these tags of the user, made when they are new.
fn tag_ids(tx: &mut impl GenericClient, uid: i64, names: &[String]) -> Result<Vec<uuid::Uuid>, postgres::Error> {
    names.iter().map(|name| tags::ensure(tx, uid, name)).collect()
}

/// Applies an edit to one file, for the user who made it. The caller's transaction makes it all or nothing.
fn apply(tx: &mut impl GenericClient, who: &Access, id: uuid::Uuid, e: &Edit) -> anyhow::Result<()> {
    who.require_workspace()?;
    let uid = who.user_id;
    e.check()?;
    let new_tags = e.tags.as_deref().map(clean_tags).transpose()?;

    if !file_state::file_is_visible(tx, who.viewer(), id)? {
        return Err(ApiError::not_found("No such file").into());
    }
    let now = unix_now();
    file_state::ensure(tx, uid, id, now)?;
    if let Some(v) = e.favorite {
        file_state::set_favorite(tx, uid, id, v)?;
    }
    if let Some(v) = e.archived {
        file_state::set_archived(tx, uid, id, v)?;
    }
    if let Some(v) = e.rating {
        file_state::set_rating(tx, uid, id, v)?;
    }
    if let Some(v) = &e.description {
        file_state::set_description(tx, uid, id, v.trim())?;
    }
    if let Some(v) = e.taken_at {
        file_state::set_taken_at(tx, uid, id, v)?;
    }
    if let Some(v) = &e.location {
        file_state::set_place(tx, uid, id, v.as_ref().map(|p| (p.lat, p.lon)))?;
    }
    file_state::touch(tx, uid, id, now)?;
    if let Some(names) = new_tags {
        let ids = tag_ids(tx, uid, &names)?;
        tags::replace_on_file(tx, uid, id, &ids)?;
        tags::drop_unused(tx)?;
    }
    Ok(())
}

pub async fn edit(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(Id(id)): Path<Id>,
    Json(e): Json<Edit>,
) -> Res<Done> {
    run(&state, move |c| {
        let mut tx = c.transaction()?;
        apply(&mut tx, &who, id, &e)?;
        db::notify(&mut tx, "library", "")?;
        tx.commit()?;
        Ok(())
    })
    .await?;
    done()
}

#[derive(Deserialize)]
pub struct Bulk {
    ids: Vec<Id>,
    favorite: Option<bool>,
    archived: Option<bool>,
    add_tags: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct Changed {
    changed: u64,
}

/// The same change to many files at once (what the selection bar does).
pub async fn bulk(State(state): State<Shared>, Extension(who): Extension<Access>, Json(b): Json<Bulk>) -> Res<Changed> {
    who.require_workspace()?;
    if b.ids.is_empty() {
        return Err(ApiError::bad("Nothing selected"));
    }
    if b.ids.len() > MAX_BULK {
        return Err(ApiError::bad(format!("Select at most {MAX_BULK} files at a time")));
    }
    let new_tags = b.add_tags.as_deref().map(clean_tags).transpose()?;
    let ids = uuids(&b.ids);
    let changed = run(&state, move |c| {
        let mut tx = c.transaction()?;
        let (viewer, now) = (who.viewer(), unix_now());
        // The files touched by any of the changes asked for are those the user can see.
        let mut changed = 0;
        if let Some(v) = b.favorite {
            changed = changed.max(file_state::bulk_set_favorite(&mut tx, viewer, &ids, v, now)?);
        }
        if let Some(v) = b.archived {
            changed = changed.max(file_state::bulk_set_archived(&mut tx, viewer, &ids, v, now)?);
        }
        if let Some(names) = new_tags.filter(|t| !t.is_empty()) {
            let tag_ids = tag_ids(&mut tx, viewer.uid, &names)?;
            changed = changed.max(tags::add_to_files(&mut tx, viewer, &ids, &tag_ids)?);
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
    id: Id,
    name: String,
    count: i64,
}

pub async fn tags(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<Tag>> {
    let out = run(&state, move |c| {
        Ok(tags::counted(c, who.viewer())?
            .into_iter()
            .map(|t| Tag { id: Id(t.id), name: t.name, count: t.files })
            .collect())
    })
    .await?;
    Ok(Json(out))
}

pub async fn delete_tag(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(Id(id)): Path<Id>,
) -> Res<Done> {
    run(&state, move |c| {
        if !tags::delete(c, who.user_id, id)? {
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
    points: Vec<(Id, f64, f64, i64)>,
    /// More photos have a place than are listed.
    truncated: bool,
}

/// Every photo and video that has a place, for the map.
pub async fn map(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Query(filter): Query<Filter>,
) -> Res<MapPoints> {
    filter.checked()?;
    let out = run(&state, move |c| {
        let mut points = library::map_points(c, who.viewer(), &filter.checked()?, MAX_POINTS + 1)?;
        let truncated = points.len() as i64 > MAX_POINTS;
        points.truncate(MAX_POINTS as usize);
        let points = points.into_iter().map(|(id, lat, lon, when)| (Id(id), lat, lon, when)).collect();
        Ok(MapPoints { points, truncated })
    })
    .await?;
    Ok(Json(out))
}

#[derive(Serialize)]
pub struct TrashEntry {
    id: Id,
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

/// Deleted files of the devices the user can see, newest deletion first. They are removed for good when the retention
/// period ends; restoring one is `POST /api/files/restore` with its `version`.
pub async fn trash(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Query(q): Query<TrashQuery>,
) -> Res<Trash> {
    let out = run(&state, move |c| {
        let retention_days = settings::get_i64(c, "retention_days", 30);
        let rows = library::trash(c, who.viewer(), TRASH_PAGE, q.offset.max(0))?;
        let total = rows.first().map_or(0, |r| r.total);
        let entries = rows
            .into_iter()
            .map(|r| TrashEntry {
                id: Id(r.id),
                name: base_name(&r.path).to_string(),
                device: r.folder,
                size: r.size,
                deleted_at: r.deleted_at,
                purge_at: r.deleted_at + retention_days * DAY_SECS,
                version: r.version_id,
                kind: meta::kind(&r.path),
                hash: short_hash(&r.root_hash),
                path: r.path,
            })
            .collect();
        Ok(Trash { entries, total, retention_days })
    })
    .await?;
    Ok(Json(out))
}

/// The tables a mirror keeps a copy of, so that albums, tags and favorites survive losing the database. Everything is
/// named by user, device folder and path rather than by id, because ids do not survive a restore.
pub fn snapshot(c: &mut Client) -> anyhow::Result<serde_json::Value> {
    use serde_json::json;
    use std::collections::BTreeMap;

    /// The files of each album or tag, by the id of the album or tag.
    fn files_by<'a>(
        rows: impl Iterator<Item = (uuid::Uuid, &'a str, &'a str)>,
    ) -> BTreeMap<uuid::Uuid, Vec<serde_json::Value>> {
        let mut map: BTreeMap<uuid::Uuid, Vec<serde_json::Value>> = BTreeMap::new();
        for (id, device, path) in rows {
            map.entry(id).or_default().push(json!({ "device": device, "path": path }));
        }
        map
    }

    let album_files = snapshot::album_files(c)?;
    let mut files_of_album = files_by(album_files.iter().map(|f| (f.album_id, f.folder.as_str(), f.path.as_str())));
    let albums: Vec<_> = snapshot::albums(c)?
        .into_iter()
        .map(|a| {
            json!({ "user": a.owner, "name": a.name, "description": a.description, "created_at": a.created_at,
                    "files": files_of_album.remove(&a.id).unwrap_or_default() })
        })
        .collect();

    let tag_files = snapshot::tag_files(c)?;
    let mut files_of_tag = files_by(tag_files.iter().map(|f| (f.tag_id, f.folder.as_str(), f.path.as_str())));
    let tags: Vec<_> = snapshot::tags(c)?
        .into_iter()
        .map(|t| json!({ "user": t.owner, "name": t.name, "files": files_of_tag.remove(&t.id).unwrap_or_default() }))
        .collect();

    let states: Vec<_> = snapshot::states(c)?
        .into_iter()
        .map(|s| {
            json!({ "user": s.owner, "device": s.folder, "path": s.path, "favorite": s.favorite, "archived": s.archived,
                    "rating": s.rating, "description": s.description, "taken_at": s.taken_at, "lat": s.lat, "lon": s.lon })
        })
        .collect();

    Ok(json!({ "generated_at_unix": unix_now(), "albums": albums, "tags": tags, "files": states }))
}
