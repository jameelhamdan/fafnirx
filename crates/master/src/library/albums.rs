//! Albums. An album belongs to the user who made it and holds files that user can see.
//!
//! An album holds the files picked into it and, when it shows a folder (a TV show with its seasons, a trip), whatever
//! is in that folder as it comes and goes. This module checks what a request says and shapes the answer; what is
//! stored, and how it is found, is in `repo::albums` and `repo::library`.

use crate::library::photos::{nullable, Asset, MAX_ASSETS};
use crate::platform::db;
use crate::platform::id::{uuids, Id};
use crate::repo::{albums, library};
use crate::web::access::Access;
use crate::web::{act, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use fafnirx_common::unix_now;
use postgres::GenericClient;
use serde::{Deserialize, Serialize};

const MAX_FILES: usize = 10_000;

/// A place in the file tree: a device's folder name and a path below it.
#[derive(Serialize, Deserialize, Clone)]
pub struct Folder {
    /// The device's folder name (`<user>/<device>`).
    device: String,
    /// The path below it, without slashes at the ends; empty for the whole device.
    #[serde(default)]
    path: String,
}

#[derive(Serialize)]
pub struct AlbumInfo {
    id: Id,
    name: String,
    description: String,
    created_at: i64,
    updated_at: i64,
    /// Live photos and videos in it.
    count: i64,
    bytes: i64,
    /// The file shown for the album: the one chosen, else the newest.
    cover: Option<Id>,
    /// Set when the album shows a folder: its files are in it as they come and go.
    folder: Option<Folder>,
}

impl From<albums::Album> for AlbumInfo {
    fn from(a: albums::Album) -> Self {
        AlbumInfo {
            id: Id(a.id),
            name: a.name,
            description: a.description,
            created_at: a.created_at,
            updated_at: a.updated_at,
            count: a.files,
            bytes: a.bytes,
            cover: a.cover.map(Id),
            folder: a.folder.map(|(device, prefix)| Folder { device, path: prefix.trim_end_matches('/').to_string() }),
        }
    }
}

/// The cover of an album that shows a folder, has none chosen and no poster picture: its first video as the names sort (episode 1 of season 1),
/// or its first picture when there are no videos.
fn first_of(media: Vec<(uuid::Uuid, String)>) -> Option<uuid::Uuid> {
    let video = |path: &str| crate::library::meta::kind(path) == Some("video");
    media.into_iter().min_by(|(_, a), (_, b)| video(b).cmp(&video(a)).then_with(|| natural_cmp(a, b))).map(|(id, _)| id)
}

fn with_cover(c: &mut impl GenericClient, mut album: albums::Album) -> anyhow::Result<AlbumInfo> {
    if album.cover.is_none() {
        album.cover = match albums::poster(c, album.id)? {
            Some(poster) => Some(poster),
            None if album.folder.is_some() => first_of(albums::media(c, album.id)?),
            None => album.newest,
        };
    }
    Ok(album.into())
}

/// The album, if it is this user's: someone else's album does not exist for them.
fn load_album(c: &mut impl GenericClient, who: &Access, id: uuid::Uuid) -> anyhow::Result<AlbumInfo> {
    match albums::find(c, who.user_id, id)? {
        Some(album) => with_cover(c, album),
        None => Err(ApiError::not_found("No such album").into()),
    }
}

pub async fn list(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<AlbumInfo>> {
    let out = run(&state, move |c| {
        albums::of_user(c, who.user_id)?.into_iter().map(|album| with_cover(c, album)).collect::<anyhow::Result<_>>()
    })
    .await?;
    Ok(Json(out))
}

fn clean_name(name: &str) -> Result<&str, ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 {
        return Err(ApiError::bad("An album needs a name of 1 to 120 characters"));
    }
    Ok(name)
}

fn clean_ids(count: usize) -> Result<(), ApiError> {
    if count > MAX_FILES {
        return Err(ApiError::bad(format!("At most {MAX_FILES} files at a time")));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct NewAlbum {
    name: String,
    #[serde(default)]
    description: String,
    /// Files to start with.
    #[serde(default)]
    ids: Vec<Id>,
    /// Files (or folders: the photos and videos below them) to start with, by where they are.
    #[serde(default)]
    items: Vec<Folder>,
    /// A folder to show. The name may then be left out: the folder's is used.
    folder: Option<Folder>,
}

pub async fn create(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(body): Json<NewAlbum>,
) -> Res<AlbumInfo> {
    who.require_workspace()?;
    clean_ids(body.ids.len() + body.items.len())?;
    let out = run(&state, move |c| {
        // An album for a folder: one per folder, so asking again gives the one there is.
        let folder = body.folder.as_ref().map(|f| find_folder(c, &who, f)).transpose()?;
        if let Some(shown) = &folder {
            if let Some(id) = albums::following(c, who.user_id, shown.device_id, &shown.prefix)? {
                return load_album(c, &who, id);
            }
        }
        let name = match (&folder, body.name.trim()) {
            (Some(shown), "") => shown.name.clone(),
            _ => clean_name(&body.name)?.to_string(),
        };
        let mut tx = c.transaction()?;
        let follows = folder.as_ref().map(|f| (f.device_id, f.prefix.as_str()));
        let id = albums::insert(&mut tx, who.user_id, &name, body.description.trim(), follows, unix_now())?;
        add_files(&mut tx, &who, id, &uuids(&body.ids))?;
        add_items(&mut tx, &who, id, &body.items)?;
        db::notify(&mut tx, "library", "")?;
        let album = load_album(&mut tx, &who, id)?;
        tx.commit()?;
        Ok(album)
    })
    .await?;
    Ok(Json(out))
}

/// A path below a device from a request, cleaned and without slashes at the ends; empty for the whole device.
fn clean_folder_path(path: &str) -> Result<String, ApiError> {
    let path = path.trim_matches('/');
    if path.is_empty() {
        return Ok(String::new());
    }
    fafnirx_common::clean_remote_path(path).map_err(|e| ApiError::bad(e.to_string()))
}

/// A folder of a device the user can see, as an album shows it.
struct FolderShown {
    device_id: i64,
    /// What the paths of its files start with: empty for the whole device, else the path and a slash.
    prefix: String,
    /// What to call an album of it.
    name: String,
}

fn find_folder(c: &mut postgres::Client, who: &Access, folder: &Folder) -> anyhow::Result<FolderShown> {
    let device = crate::files::browse::find_device(c, who, &folder.device)?;
    let path = clean_folder_path(&folder.path)?;
    let name = path
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| device.name.rsplit('/').next().unwrap_or(&device.name))
        .to_string();
    let prefix = if path.is_empty() { path } else { path + "/" };
    Ok(FolderShown { device_id: device.id, prefix, name })
}

/// Puts the files in the album: those the user can see.
fn add_files(
    tx: &mut impl GenericClient,
    who: &Access,
    album: uuid::Uuid,
    ids: &[uuid::Uuid],
) -> Result<u64, postgres::Error> {
    albums::add_files(tx, who.viewer(), album, ids, unix_now())
}

/// Puts the photos and videos at these places in the album: a file, or everything below a folder. Those the user can see.
fn add_items(tx: &mut impl GenericClient, who: &Access, album: uuid::Uuid, items: &[Folder]) -> anyhow::Result<u64> {
    let mut added = 0;
    for item in items {
        let path = clean_folder_path(&item.path)?;
        added += albums::add_place(tx, who.viewer(), album, &item.device, &path, unix_now())?;
    }
    Ok(added)
}

#[derive(Serialize)]
pub struct AlbumAssets {
    album: AlbumInfo,
    assets: Vec<Asset>,
}

/// An album and what is in it: newest first, or for an album that shows a folder in the order of the files' names, in which
/// "Episode 2" comes before "Episode 10".
pub async fn detail(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(Id(id)): Path<Id>,
) -> Res<AlbumAssets> {
    let out = run(&state, move |c| {
        let album = load_album(c, &who, id)?;
        let by_path = album.folder.is_some();
        let mut assets: Vec<Asset> =
            library::album_assets(c, who.viewer(), id, by_path, MAX_ASSETS)?.into_iter().map(Asset::from).collect();
        if by_path {
            assets.sort_by(|a, b| natural_cmp(&a.device, &b.device).then_with(|| natural_cmp(&a.path, &b.path)));
        }
        Ok(AlbumAssets { album, assets })
    })
    .await?;
    Ok(Json(out))
}

/// Compares names the way people sort them: runs of digits by their value, the rest without regard to case.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let number = |it: &mut std::iter::Peekable<std::str::Chars>| {
        let mut digits = String::new();
        while let Some(d) = it.next_if(|d| d.is_ascii_digit()) {
            digits.push(d);
        }
        digits.trim_start_matches('0').to_string()
    };
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let (x, y) = (number(&mut a), number(&mut b));
                // More digits, a bigger number; as many, compare as text.
                let order = x.len().cmp(&y.len()).then_with(|| x.cmp(&y));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                a.next();
                b.next();
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

#[derive(Deserialize)]
pub struct AlbumEdit {
    name: Option<String>,
    description: Option<String>,
    /// A file of the album, or null to show the newest again.
    #[serde(default, deserialize_with = "nullable")]
    cover: Option<Option<Id>>,
}

pub async fn edit(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(Id(id)): Path<Id>,
    Json(body): Json<AlbumEdit>,
) -> Res<AlbumInfo> {
    let name = body.name.as_deref().map(clean_name).transpose()?.map(str::to_string);
    let out = run(&state, move |c| {
        let mut tx = c.transaction()?;
        load_album(&mut tx, &who, id)?;
        if let Some(name) = &name {
            albums::rename(&mut tx, id, name)?;
        }
        if let Some(d) = &body.description {
            albums::set_description(&mut tx, id, d.trim())?;
        }
        if let Some(cover) = body.cover {
            if let Some(file) = cover {
                if !albums::contains(&mut tx, id, file.0)? {
                    return Err(ApiError::bad("The cover must be a file of the album").into());
                }
            }
            albums::set_cover(&mut tx, id, cover.map(|c| c.0))?;
        }
        albums::touch(&mut tx, id, unix_now())?;
        db::notify(&mut tx, "library", "")?;
        let album = load_album(&mut tx, &who, id)?;
        tx.commit()?;
        Ok(album)
    })
    .await?;
    Ok(Json(out))
}

/// Deletes the album. The photos stay where they are.
pub async fn delete(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(Id(id)): Path<Id>,
) -> Res<Done> {
    act(&state, move |c| {
        if !albums::delete(c, who.user_id, id)? {
            return Err(ApiError::not_found("No such album").into());
        }
        db::notify(c, "library", "")?;
        Ok(())
    })
    .await
}

#[derive(Deserialize)]
pub struct Ids {
    #[serde(default)]
    ids: Vec<Id>,
    /// Files, or folders (the photos and videos below them), by where they are: for the file browser, which has no ids.
    #[serde(default)]
    items: Vec<Folder>,
}

#[derive(Serialize)]
pub struct Count {
    count: u64,
}

pub async fn add(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(Id(id)): Path<Id>,
    Json(body): Json<Ids>,
) -> Res<Count> {
    clean_ids(body.ids.len() + body.items.len())?;
    let count = run(&state, move |c| {
        let mut tx = c.transaction()?;
        load_album(&mut tx, &who, id)?;
        let count = add_files(&mut tx, &who, id, &uuids(&body.ids))? + add_items(&mut tx, &who, id, &body.items)?;
        albums::touch(&mut tx, id, unix_now())?;
        db::notify(&mut tx, "library", "")?;
        tx.commit()?;
        Ok(count)
    })
    .await?;
    Ok(Json(Count { count }))
}

pub async fn remove(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(Id(id)): Path<Id>,
    Json(body): Json<Ids>,
) -> Res<Count> {
    clean_ids(body.ids.len())?;
    let count = run(&state, move |c| {
        let mut tx = c.transaction()?;
        load_album(&mut tx, &who, id)?;
        let count = albums::remove_files(&mut tx, id, &uuids(&body.ids))?;
        albums::touch(&mut tx, id, unix_now())?;
        db::notify(&mut tx, "library", "")?;
        tx.commit()?;
        Ok(count)
    })
    .await?;
    Ok(Json(Count { count }))
}

#[cfg(test)]
mod tests {
    use super::{first_of, natural_cmp};
    use uuid::Uuid;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }
    use std::cmp::Ordering;

    #[test]
    fn the_cover_of_a_folder_is_its_first_video() {
        let media = vec![
            (id(1), "Show/Season 1/other.jpg".to_string()),
            (id(2), "Show/Season 2/E01.mkv".to_string()),
            (id(3), "Show/Season 1/E10.mkv".to_string()),
            (id(4), "Show/Season 1/E2.mkv".to_string()),
        ];
        assert_eq!(first_of(media), Some(id(4)), "a video, and episode 2 before episode 10");
        assert_eq!(first_of(vec![(id(7), "b.jpg".into()), (id(8), "a.jpg".into())]), Some(id(8)));
        assert_eq!(first_of(Vec::new()), None);
    }

    #[test]
    fn names_sort_the_way_people_read_them() {
        let mut names = ["Episode 10.mkv", "episode 2.mkv", "Episode 1.mkv", "Season 2/E01.mkv", "Season 10/E01.mkv"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            names,
            ["Episode 1.mkv", "episode 2.mkv", "Episode 10.mkv", "Season 2/E01.mkv", "Season 10/E01.mkv"]
        );
        assert_eq!(natural_cmp("Episode 02", "episode 2"), Ordering::Equal, "leading zeros and case do not count");
        assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
        assert_eq!(natural_cmp("a1", "a"), Ordering::Greater);
        assert_eq!(natural_cmp("", "a"), Ordering::Less);
    }
}
