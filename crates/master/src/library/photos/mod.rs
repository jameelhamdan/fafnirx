//! The photo library: a timeline of every photo and video a user can reach (their devices and the master files),
//! favorites, archive, tags, descriptions, ratings, corrected dates and places, the map and the trash.
//!
//! Everything people say about a file lives in `file_state` and `tags`, next to the file and keyed by it and by
//! the user: the original is never touched, a corrected date or place can always be taken back, and the master
//! files, which everybody sees, are each user's own to favorite, archive, tag and describe.
//!
//! This module checks what a request says and shapes the answer; what is stored, and how it is found, is in
//! `repo::library`, `repo::file_state` and `repo::tags`.

mod edit;
mod trash;

pub use edit::*;
pub use trash::*;

use crate::library::{artwork, meta};
use crate::platform::id::Id;
use crate::repo::jobs::JobStatus;
use crate::repo::library::{self, Media, Scope};
use crate::repo::{albums, files, tags};
use crate::web::access::Access;
use crate::web::{run, ApiError, Res, Shared};
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use fafnirx_common::base_name;
use fafnirx_db::client::GenericClient;
use serde::{Deserialize, Deserializer, Serialize};

/// More than a library of one household holds in one month, and few enough to answer in one go.
pub const MAX_ASSETS: i64 = 20_000;
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
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Asset {
    pub id: Id,
    pub device: String,
    pub path: String,
    pub name: String,
    /// image or video
    #[cfg_attr(test, ts(type = "'image' | 'video'"))]
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
    let ok = text.len() >= 7 && text.as_bytes()[text.len() - 3] == b'-' && text[..text.len() - 3].bytes().all(|b| b.is_ascii_digit()) && text[text.len() - 2..].parse::<u8>().is_ok_and(|m| (1..=12).contains(&m));
    if ok {
        Ok(())
    } else {
        Err(ApiError::bad("month must look like 2024-05"))
    }
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Month {
    month: String,
    count: i64,
    /// How many files each day of the month holds, newest day first. With these the page knows how tall the
    /// month will be before it has loaded any picture, so the scrollbar and the scrubber are exact.
    days: Vec<i64>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
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
pub async fn buckets(State(state): State<Shared>, Extension(who): Extension<Access>, Query(filter): Query<Filter>) -> Res<Buckets> {
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
pub async fn timeline(State(state): State<Shared>, Extension(who): Extension<Access>, Query(filter): Query<Filter>) -> Res<Assets> {
    if filter.month.as_deref().unwrap_or("").is_empty() {
        return Err(ApiError::bad("month is required, like 2024-05"));
    }
    filter.checked()?;
    let assets = run(&state, move |c| {
        Ok(library::assets(c, who.viewer(), &filter.checked()?, MAX_ASSETS)?
            .into_iter()
            .map(Asset::from)
            .collect())
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
pub async fn by_ids(State(state): State<Shared>, Extension(who): Extension<Access>, Query(q): Query<IdsQuery>) -> Res<Assets> {
    let ids: Vec<uuid::Uuid> = q.ids.split(',').filter_map(|s| s.trim().parse::<Id>().ok()).map(|i| i.0).take(500).collect();
    let assets = run(&state, move |c| Ok(library::by_ids(c, who.viewer(), &ids)?.into_iter().map(Asset::from).collect())).await?;
    Ok(Json(Assets { assets }))
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct TagRef {
    id: Id,
    name: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct AlbumRef {
    id: Id,
    name: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "AssetDetail"))]
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
    #[cfg_attr(test, ts(type = "Record<string, string> | null"))]
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
    #[cfg_attr(test, ts(type = "'waiting' | 'running' | 'failed' | null"))]
    work: Option<String>,
}

/// The photos and videos in the folder of a file, in the order of their names: what the viewer opened from the file browser
/// steps through. The file itself is in it.
pub async fn folder(State(state): State<Shared>, Extension(who): Extension<Access>, Path(Id(id)): Path<Id>) -> Res<Assets> {
    let assets = run(&state, move |c| {
        let mut assets: Vec<Asset> = library::folder_of(c, who.viewer(), id, MAX_ASSETS)?.into_iter().map(Asset::from).collect();
        if !assets.iter().any(|a| a.id.0 == id) {
            return Err(ApiError::not_found("No such file").into());
        }
        assets.sort_by(|a, b| crate::library::albums::natural_cmp(&a.path, &b.path));
        Ok(assets)
    })
    .await?;
    Ok(Json(Assets { assets }))
}

pub async fn detail(State(state): State<Shared>, Extension(who): Extension<Access>, Path(Id(id)): Path<Id>) -> Res<Detail> {
    let store = state.store.clone();
    let out = run(&state, move |c| {
        let viewer = who.viewer();
        let not_found = || ApiError::not_found("No such file");
        let asset = library::by_ids(c, viewer, &[id])?.into_iter().next().ok_or_else(not_found)?;
        let extras = library::extras(c, viewer.uid, id)?.ok_or_else(not_found)?;
        let tags = tags::of_file(c, viewer.uid, id)?
            .into_iter()
            .map(|(id, name)| TagRef { id: Id(id), name })
            .collect();
        let albums = albums::of_file(c, viewer.uid, id)?
            .into_iter()
            .map(|(id, name)| AlbumRef { id: Id(id), name })
            .collect();
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
pub fn work_state(c: &mut impl GenericClient, device: &str, path: &str, unread: bool) -> Result<Option<String>, postgres::Error> {
    let jobs = crate::repo::jobs::pending_of(c, device, path)?;
    let has = |s: JobStatus| jobs.iter().any(|(_, status)| *status == s);
    Ok(if has(JobStatus::Running) {
        Some("running")
    } else if has(JobStatus::Queued) || (unread && !has(JobStatus::Failed)) {
        Some("waiting")
    } else if has(JobStatus::Failed) {
        Some("failed")
    } else {
        None
    }
    .map(str::to_string))
}

/// What the .nfo next to a video says about it. Anything that goes wrong means no description, never an error.
fn described(c: &mut postgres::Client, store: &crate::platform::store::Store, asset: &Asset) -> Option<artwork::Described> {
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
