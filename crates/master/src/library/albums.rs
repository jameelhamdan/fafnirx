//! Albums. An album belongs to the user who made it and holds files that user can see.

use crate::library::photos::{asset_columns, from_sql, nullable, Asset, MAX_ASSETS, WHEN};
use crate::platform::db;
use crate::web::access::Access;
use crate::web::{done, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use fafnirx_common::unix_now;
use postgres::GenericClient;
use serde::{Deserialize, Serialize};

const MAX_FILES: usize = 10_000;

/// Whether file `f` is in album `a`: picked into it, or below the folder the album shows.
macro_rules! member {
    () => {
        "(EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id)
          OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)))"
    };
}

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
    id: i64,
    name: String,
    description: String,
    created_at: i64,
    updated_at: i64,
    /// Live photos and videos in it.
    count: i64,
    bytes: i64,
    /// The file shown for the album: the one chosen, else the newest.
    cover: Option<i64>,
    /// Set when the album shows a folder (a TV show, a trip): its files are in it as they come and go.
    folder: Option<Folder>,
}

const ALBUM_COLUMNS: &str = concat!(
    "a.id, a.name, a.description, a.created_at, a.updated_at,
    (SELECT COUNT(*) FROM files f WHERE ", member!(), " AND f.deleted_at IS NULL AND f.media IS NOT NULL),
    (SELECT COALESCE(SUM(v.size), 0)::BIGINT FROM files f JOIN versions v ON v.id = f.current_version_id
         WHERE ", member!(), " AND f.deleted_at IS NULL AND f.media IS NOT NULL),
    COALESCE((SELECT f.id FROM files f WHERE f.id = a.cover_file_id AND f.deleted_at IS NULL),
             (SELECT f.id FROM files f JOIN versions v ON v.id = f.current_version_id
                  LEFT JOIN file_meta m ON m.root_hash = v.root_hash LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = a.user_id
              WHERE ", member!(), " AND f.deleted_at IS NULL AND f.media IS NOT NULL
              ORDER BY COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000) DESC, f.id DESC LIMIT 1)),
    (SELECT d.folder FROM devices d WHERE d.id = a.folder_device_id), a.folder_prefix"
);

fn album_from_row(r: &postgres::Row) -> AlbumInfo {
    let device: Option<String> = r.get(8);
    let prefix: Option<String> = r.get(9);
    AlbumInfo {
        id: r.get(0),
        name: r.get(1),
        description: r.get(2),
        created_at: r.get(3),
        updated_at: r.get(4),
        count: r.get(5),
        bytes: r.get(6),
        cover: r.get(7),
        folder: device
            .map(|device| Folder { device, path: prefix.unwrap_or_default().trim_end_matches('/').to_string() }),
    }
}

/// The album, if it is this user's: someone else's album does not exist for them.
fn load_album(c: &mut impl GenericClient, who: &Access, id: i64) -> anyhow::Result<AlbumInfo> {
    c.query_opt(
        &format!("SELECT {ALBUM_COLUMNS} FROM albums a WHERE a.id = $1 AND a.user_id = $2"),
        &[&id, &who.user_id],
    )?
    .map(|r| album_from_row(&r))
    .ok_or_else(|| ApiError::not_found("No such album").into())
}

pub async fn list(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<AlbumInfo>> {
    let uid = who.user_id;
    let out = run(&state, move |c| {
        Ok(c.query(
            &format!("SELECT {ALBUM_COLUMNS} FROM albums a WHERE a.user_id = $1 ORDER BY a.updated_at DESC, a.id DESC"),
            &[&uid],
        )?
        .iter()
        .map(album_from_row)
        .collect())
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
    ids: Vec<i64>,
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
        if let Some((device, prefix, _)) = &folder {
            if let Some(row) = c.query_opt(
                "SELECT id FROM albums WHERE user_id = $1 AND folder_device_id = $2 AND folder_prefix = $3",
                &[&who.user_id, device, prefix],
            )? {
                return load_album(c, &who, row.get(0));
            }
        }
        let name = match (&folder, body.name.trim()) {
            (Some((_, _, shown)), "") => shown.clone(),
            _ => clean_name(&body.name)?.to_string(),
        };
        let mut tx = c.transaction()?;
        let now = unix_now();
        let (device, prefix) = folder.map_or((None, None), |(d, p, _)| (Some(d), Some(p)));
        let id: i64 = tx
            .query_one(
                "INSERT INTO albums (name, description, created_at, updated_at, user_id, folder_device_id, folder_prefix)
                 VALUES ($1, $2, $3, $3, $4, $5, $6) RETURNING id",
                &[&name, &body.description.trim(), &now, &who.user_id, &device, &prefix],
            )?
            .get(0);
        add_files(&mut tx, &who, id, &body.ids)?;
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

/// A folder of a device the user can see: the device's id, the prefix its files' paths start with, and what to call it.
fn find_folder(c: &mut postgres::Client, who: &Access, folder: &Folder) -> anyhow::Result<(i64, String, String)> {
    let device = crate::files::browse::find_device(c, who, &folder.device)?;
    let path = clean_folder_path(&folder.path)?;
    let shown = path
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| device.name.rsplit('/').next().unwrap_or(&device.name));
    let prefix = if path.is_empty() { path.clone() } else { path.clone() + "/" };
    Ok((device.id, prefix, shown.to_string()))
}

/// Puts the files in the album: those the user can see.
fn add_files(tx: &mut impl GenericClient, who: &Access, album: i64, ids: &[i64]) -> Result<u64, postgres::Error> {
    tx.execute(
        &format!(
            "INSERT INTO album_files (album_id, file_id, added_at) SELECT $1, f.id, $3
             FROM files f JOIN devices d ON d.id = f.device_id
             WHERE f.id = ANY($2) AND f.deleted_at IS NULL AND {} ON CONFLICT DO NOTHING",
            who.visible_sql()
        ),
        &[&album, &ids, &unix_now()],
    )
}

/// Puts the photos and videos at these places in the album: a file, or everything below a folder. Those the user can see.
fn add_items(tx: &mut impl GenericClient, who: &Access, album: i64, items: &[Folder]) -> anyhow::Result<u64> {
    let mut added = 0;
    for item in items {
        let path = clean_folder_path(&item.path)?;
        added += tx.execute(
            &format!(
                "INSERT INTO album_files (album_id, file_id, added_at) SELECT $1, f.id, $4
                 FROM files f JOIN devices d ON d.id = f.device_id
                 WHERE d.folder = $2 AND f.deleted_at IS NULL AND f.media IS NOT NULL
                   AND ($3 = '' OR f.path = $3 OR starts_with(f.path, $3 || '/')) AND {} ON CONFLICT DO NOTHING",
                who.visible_sql()
            ),
            &[&album, &item.device, &path, &unix_now()],
        )?;
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
    Path(id): Path<i64>,
) -> Res<AlbumAssets> {
    let (from, visible) = (from_sql(&who), who.visible_sql());
    let out = run(&state, move |c| {
        let album = load_album(c, &who, id)?;
        let order = if album.folder.is_some() { "f.path, f.id".to_string() } else { format!("{WHEN} DESC, f.id DESC") };
        let mut assets: Vec<Asset> = c
            .query(
                &format!(
                    "SELECT {} FROM {from}
                     WHERE f.deleted_at IS NULL AND f.media IS NOT NULL AND {visible}
                       AND EXISTS (SELECT 1 FROM albums a WHERE a.id = $1 AND {})
                     ORDER BY {order} LIMIT {MAX_ASSETS}",
                    asset_columns(),
                    member!()
                ),
                &[&id],
            )?
            .iter()
            .map(Asset::from_row)
            .collect();
        if album.folder.is_some() {
            assets.sort_by(|a, b| natural_cmp(&a.device, &b.device).then_with(|| natural_cmp(&a.path, &b.path)));
        }
        Ok(AlbumAssets { album, assets })
    })
    .await?;
    Ok(Json(out))
}

/// Compares names the way people sort them: runs of digits by their value, the rest without regard to case.
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
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
    cover: Option<Option<i64>>,
}

pub async fn edit(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(id): Path<i64>,
    Json(body): Json<AlbumEdit>,
) -> Res<AlbumInfo> {
    let name = body.name.as_deref().map(clean_name).transpose()?.map(str::to_string);
    let out = run(&state, move |c| {
        let mut tx = c.transaction()?;
        load_album(&mut tx, &who, id)?;
        if let Some(name) = &name {
            tx.execute("UPDATE albums SET name = $2 WHERE id = $1", &[&id, name])?;
        }
        if let Some(d) = &body.description {
            tx.execute("UPDATE albums SET description = $2 WHERE id = $1", &[&id, &d.trim()])?;
        }
        if let Some(cover) = body.cover {
            if let Some(file) = cover {
                let member: bool = tx
                    .query_one(
                        concat!(
                            "SELECT EXISTS (SELECT 1 FROM albums a, files f WHERE a.id = $1 AND f.id = $2 AND ",
                            member!(),
                            ")"
                        ),
                        &[&id, &file],
                    )?
                    .get(0);
                if !member {
                    return Err(ApiError::bad("The cover must be a file of the album").into());
                }
            }
            tx.execute("UPDATE albums SET cover_file_id = $2 WHERE id = $1", &[&id, &cover])?;
        }
        tx.execute("UPDATE albums SET updated_at = $2 WHERE id = $1", &[&id, &unix_now()])?;
        db::notify(&mut tx, "library", "")?;
        let album = load_album(&mut tx, &who, id)?;
        tx.commit()?;
        Ok(album)
    })
    .await?;
    Ok(Json(out))
}

/// Deletes the album. The photos stay where they are.
pub async fn delete(State(state): State<Shared>, Extension(who): Extension<Access>, Path(id): Path<i64>) -> Res<Done> {
    run(&state, move |c| {
        if c.execute("DELETE FROM albums WHERE id = $1 AND user_id = $2", &[&id, &who.user_id])? == 0 {
            return Err(ApiError::not_found("No such album").into());
        }
        db::notify(c, "library", "")?;
        Ok(())
    })
    .await?;
    done()
}

#[derive(Deserialize)]
pub struct Ids {
    #[serde(default)]
    ids: Vec<i64>,
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
    Path(id): Path<i64>,
    Json(body): Json<Ids>,
) -> Res<Count> {
    clean_ids(body.ids.len() + body.items.len())?;
    let count = run(&state, move |c| {
        let mut tx = c.transaction()?;
        load_album(&mut tx, &who, id)?;
        let count = add_files(&mut tx, &who, id, &body.ids)? + add_items(&mut tx, &who, id, &body.items)?;
        tx.execute("UPDATE albums SET updated_at = $2 WHERE id = $1", &[&id, &unix_now()])?;
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
    Path(id): Path<i64>,
    Json(body): Json<Ids>,
) -> Res<Count> {
    clean_ids(body.ids.len())?;
    let count = run(&state, move |c| {
        let mut tx = c.transaction()?;
        load_album(&mut tx, &who, id)?;
        let count =
            tx.execute("DELETE FROM album_files WHERE album_id = $1 AND file_id = ANY($2)", &[&id, &body.ids])?;
        tx.execute(
            "UPDATE albums SET cover_file_id = NULL WHERE id = $1 AND cover_file_id = ANY($2)",
            &[&id, &body.ids],
        )?;
        tx.execute("UPDATE albums SET updated_at = $2 WHERE id = $1", &[&id, &unix_now()])?;
        db::notify(&mut tx, "library", "")?;
        tx.commit()?;
        Ok(count)
    })
    .await?;
    Ok(Json(Count { count }))
}

#[cfg(test)]
mod tests {
    use super::natural_cmp;
    use std::cmp::Ordering;

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
