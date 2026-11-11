//! Albums. An album belongs to the user who made it and is a shortcut to a folder (a TV show with its seasons, a trip):
//! whatever is in that folder, as it comes and goes, is in the album, and it follows the folder when that is renamed or
//! moved. This module checks what a request says and shapes the answer; what is stored, and how it is found, is in
//! `repo::albums` and `repo::library`.

use crate::library::photos::{nullable, Asset, MAX_ASSETS};
use crate::platform::db;
use crate::platform::id::Id;
use crate::repo::{albums, library};
use crate::web::access::Access;
use crate::web::input;
use crate::web::{act, change, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use fafnirx_common::unix_now;
use fafnirx_db::client::GenericClient;
use fafnirx_db::queries as sql;
use serde::{Deserialize, Serialize};

/// A place in the file tree: a device's folder name and a path below it.
#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "AlbumFolder"))]
pub struct Folder {
    /// The device's folder name (`<user>/<device>`).
    device: String,
    /// The path below it, without slashes at the ends; empty for the whole device.
    #[serde(default)]
    path: String,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct AlbumInfo {
    id: Id,
    name: String,
    description: String,
    created_at: i64,
    updated_at: i64,
    /// Live photos and videos in it.
    count: i64,
    bytes: i64,
    /// The file shown for the album: the one chosen, else a poster, its first video or its first picture.
    cover: Option<Id>,
    /// The folder it shows: its files are in the album as they come and go.
    folder: Folder,
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
            folder: Folder { device: a.folder, path: String::new() },
        }
    }
}

/// The cover of an album that has none chosen and no poster picture: its first video as the names sort (episode 1 of season 1),
/// or its first picture when there are no videos.
fn first_of(media: Vec<(uuid::Uuid, String)>) -> Option<uuid::Uuid> {
    let video = |path: &str| crate::library::meta::kind(path) == Some("video");
    media
        .into_iter()
        .min_by(|(_, a), (_, b)| video(b).cmp(&video(a)).then_with(|| natural_cmp(a, b)))
        .map(|(id, _)| id)
}

fn with_cover(c: &mut impl GenericClient, mut album: albums::Album) -> anyhow::Result<AlbumInfo> {
    if album.cover.is_none() {
        album.cover = match sql::albums::album_poster().bind(c, &album.id).opt()? {
            Some(poster) => Some(poster),
            None => first_of(albums::media(c, album.id)?),
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
        albums::of_user(c, who.user_id)?
            .into_iter()
            .map(|album| with_cover(c, album))
            .collect::<anyhow::Result<_>>()
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

#[derive(Deserialize)]
pub struct NewAlbum {
    /// The name may be left out: the folder's is used.
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    /// The folder to show.
    folder: Folder,
}

pub async fn create(State(state): State<Shared>, Extension(who): Extension<Access>, Json(body): Json<NewAlbum>) -> Res<AlbumInfo> {
    who.require_workspace()?;
    let out = run(&state, move |c| {
        // One album per folder, so asking again gives the one there is.
        let shown = find_folder(c, &who, &body.folder)?;
        if let Some(id) = albums::of_folder(c, who.user_id, shown.device_id)? {
            return load_album(c, &who, id);
        }
        let name = match body.name.trim() {
            "" => shown.name.clone(),
            _ => clean_name(&body.name)?.to_string(),
        };
        let mut tx = c.transaction()?;
        let id = albums::insert(
            &mut tx,
            &albums::NewAlbum {
                user_id: who.user_id,
                name: &name,
                description: body.description.trim(),
                folder_device_id: shown.device_id,
                now: unix_now(),
            },
        )?;
        db::notify(&mut tx, db::Topic::Library, "")?;
        let album = load_album(&mut tx, &who, id)?;
        tx.commit()?;
        Ok(album)
    })
    .await?;
    Ok(Json(out))
}

/// A folder the user can see, as an album shows it.
struct FolderShown {
    device_id: i64,
    /// What to call an album of it.
    name: String,
}

fn find_folder(c: &mut postgres::Client, who: &Access, folder: &Folder) -> anyhow::Result<FolderShown> {
    let device = crate::files::browse::find_device(c, who, &folder.device)?;
    if !input::folder_path(&folder.path)?.is_empty() {
        return Err(ApiError::bad("An album shows a whole folder: make the folder you want a folder of its own").into());
    }
    let name = device.name.rsplit('/').next().unwrap_or(&device.name).to_string();
    Ok(FolderShown { device_id: device.id, name })
}

#[derive(Serialize)]
pub struct AlbumAssets {
    album: AlbumInfo,
    assets: Vec<Asset>,
    /// The album holds more than one answer carries (`MAX_ASSETS`); the rest is in its folder.
    more: bool,
}

/// An album and what is in it, in the order of the files' names, in which "Episode 2" comes before "Episode 10".
pub async fn detail(State(state): State<Shared>, Extension(who): Extension<Access>, Path(Id(id)): Path<Id>) -> Res<AlbumAssets> {
    let out = run(&state, move |c| {
        let album = load_album(c, &who, id)?;
        let mut assets: Vec<Asset> = library::album_assets(c, who.viewer(), id, MAX_ASSETS + 1)?.into_iter().map(Asset::from).collect();
        let more = assets.len() as i64 > MAX_ASSETS;
        assets.truncate(MAX_ASSETS as usize);
        assets.sort_by(|a, b| natural_cmp(&a.device, &b.device).then_with(|| natural_cmp(&a.path, &b.path)));
        Ok(AlbumAssets { album, assets, more })
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

pub async fn edit(State(state): State<Shared>, Extension(who): Extension<Access>, Path(Id(id)): Path<Id>, Json(body): Json<AlbumEdit>) -> Res<AlbumInfo> {
    let name = body.name.as_deref().map(clean_name).transpose()?.map(str::to_string);
    let out = change(&state, crate::platform::db::Topic::Library, move |tx| {
        load_album(tx, &who, id)?;
        if let Some(name) = &name {
            albums::rename(tx, id, name)?;
        }
        if let Some(d) = &body.description {
            albums::set_description(tx, id, d.trim())?;
        }
        if let Some(cover) = body.cover {
            if let Some(file) = cover {
                if !albums::contains(tx, id, file.0)? {
                    return Err(ApiError::bad("The cover must be a file of the album").into());
                }
            }
            albums::set_cover(tx, id, cover.map(|c| c.0))?;
        }
        albums::touch(tx, id, unix_now())?;
        let album = load_album(tx, &who, id)?;
        Ok(album)
    })
    .await?;
    Ok(Json(out))
}

/// Deletes the album. The photos stay where they are.
pub async fn delete(State(state): State<Shared>, Extension(who): Extension<Access>, Path(Id(id)): Path<Id>) -> Res<Done> {
    act(&state, move |c| {
        if !albums::delete(c, who.user_id, id)? {
            return Err(ApiError::not_found("No such album").into());
        }
        db::notify(c, db::Topic::Library, "")?;
        Ok(())
    })
    .await
}

#[cfg(test)]
#[path = "../../tests/unit/library/albums.rs"]
mod tests;
