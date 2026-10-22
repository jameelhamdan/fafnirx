//! Albums.

use crate::library::meta;
use crate::library::photos::{asset_columns, Asset, FROM, WHEN};
use crate::platform::db;
use crate::web::{done, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::Json;
use fafnirx_common::unix_now;
use postgres::GenericClient;
use serde::{Deserialize, Deserializer, Serialize};

const MAX_FILES: usize = 10_000;
const MAX_ASSETS: i64 = 20_000;

fn nullable<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
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
}

const ALBUM_COLUMNS: &str = "a.id, a.name, a.description, a.created_at, a.updated_at,
    (SELECT COUNT(*) FROM album_files af JOIN files f ON f.id = af.file_id WHERE af.album_id = a.id AND f.deleted_at IS NULL),
    (SELECT COALESCE(SUM(v.size), 0)::BIGINT FROM album_files af JOIN files f ON f.id = af.file_id
         JOIN versions v ON v.id = f.current_version_id WHERE af.album_id = a.id AND f.deleted_at IS NULL),
    COALESCE((SELECT f.id FROM files f WHERE f.id = a.cover_file_id AND f.deleted_at IS NULL),
             (SELECT f.id FROM album_files af JOIN files f ON f.id = af.file_id JOIN versions v ON v.id = f.current_version_id
                  LEFT JOIN file_meta m ON m.root_hash = v.root_hash LEFT JOIN file_state s ON s.file_id = f.id
              WHERE af.album_id = a.id AND f.deleted_at IS NULL AND lower(f.path) ~ $1
              ORDER BY COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000) DESC, f.id DESC LIMIT 1))";

fn album_from_row(r: &postgres::Row) -> AlbumInfo {
    AlbumInfo {
        id: r.get(0),
        name: r.get(1),
        description: r.get(2),
        created_at: r.get(3),
        updated_at: r.get(4),
        count: r.get(5),
        bytes: r.get(6),
        cover: r.get(7),
    }
}

fn load_album(c: &mut impl GenericClient, id: i64) -> anyhow::Result<AlbumInfo> {
    c.query_opt(&format!("SELECT {ALBUM_COLUMNS} FROM albums a WHERE a.id = $2"), &[&meta::media_regex(), &id])?
        .map(|r| album_from_row(&r))
        .ok_or_else(|| ApiError::not_found("No such album").into())
}

pub async fn list(State(state): State<Shared>) -> Res<Vec<AlbumInfo>> {
    let out = run(&state, |c| {
        Ok(c.query(
            &format!("SELECT {ALBUM_COLUMNS} FROM albums a ORDER BY a.updated_at DESC, a.id DESC"),
            &[&meta::media_regex()],
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

fn clean_ids(ids: &[i64]) -> Result<(), ApiError> {
    if ids.len() > MAX_FILES {
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
}

pub async fn create(State(state): State<Shared>, Json(body): Json<NewAlbum>) -> Res<AlbumInfo> {
    let name = clean_name(&body.name)?.to_string();
    clean_ids(&body.ids)?;
    let out = run(&state, move |c| {
        let mut tx = c.transaction()?;
        let now = unix_now();
        let id: i64 = tx
            .query_one(
                "INSERT INTO albums (name, description, created_at, updated_at) VALUES ($1, $2, $3, $3) RETURNING id",
                &[&name, &body.description.trim(), &now],
            )?
            .get(0);
        add_files(&mut tx, id, &body.ids)?;
        db::notify(&mut tx, "library", "")?;
        let album = load_album(&mut tx, id)?;
        tx.commit()?;
        Ok(album)
    })
    .await?;
    Ok(Json(out))
}

fn add_files(tx: &mut impl GenericClient, album: i64, ids: &[i64]) -> Result<u64, postgres::Error> {
    tx.execute(
        "INSERT INTO album_files (album_id, file_id, added_at) SELECT $1, id, $3 FROM files
         WHERE id = ANY($2) AND deleted_at IS NULL ON CONFLICT DO NOTHING",
        &[&album, &ids, &unix_now()],
    )
}

#[derive(Serialize)]
pub struct AlbumAssets {
    album: AlbumInfo,
    assets: Vec<Asset>,
}

/// An album and what is in it, newest first.
pub async fn detail(State(state): State<Shared>, Path(id): Path<i64>) -> Res<AlbumAssets> {
    let out = run(&state, move |c| {
        let album = load_album(c, id)?;
        let assets = c
            .query(
                &format!(
                    "SELECT {} FROM album_files af JOIN {FROM} ON f.id = af.file_id
                     WHERE af.album_id = $1 AND f.deleted_at IS NULL AND lower(f.path) ~ $2
                     ORDER BY {WHEN} DESC, f.id DESC LIMIT {MAX_ASSETS}",
                    asset_columns()
                ),
                &[&id, &meta::media_regex()],
            )?
            .iter()
            .map(Asset::from_row)
            .collect();
        Ok(AlbumAssets { album, assets })
    })
    .await?;
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct AlbumEdit {
    name: Option<String>,
    description: Option<String>,
    /// A file of the album, or null to show the newest again.
    #[serde(default, deserialize_with = "nullable")]
    cover: Option<Option<i64>>,
}

pub async fn edit(State(state): State<Shared>, Path(id): Path<i64>, Json(body): Json<AlbumEdit>) -> Res<AlbumInfo> {
    let name = body.name.as_deref().map(clean_name).transpose()?.map(str::to_string);
    let out = run(&state, move |c| {
        let mut tx = c.transaction()?;
        load_album(&mut tx, id)?;
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
                        "SELECT EXISTS (SELECT 1 FROM album_files WHERE album_id = $1 AND file_id = $2)",
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
        let album = load_album(&mut tx, id)?;
        tx.commit()?;
        Ok(album)
    })
    .await?;
    Ok(Json(out))
}

/// Deletes the album. The photos stay where they are.
pub async fn delete(State(state): State<Shared>, Path(id): Path<i64>) -> Res<Done> {
    run(&state, move |c| {
        if c.execute("DELETE FROM albums WHERE id = $1", &[&id])? == 0 {
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
    ids: Vec<i64>,
}

#[derive(Serialize)]
pub struct Count {
    count: u64,
}

pub async fn add(State(state): State<Shared>, Path(id): Path<i64>, Json(body): Json<Ids>) -> Res<Count> {
    clean_ids(&body.ids)?;
    let count = run(&state, move |c| {
        let mut tx = c.transaction()?;
        load_album(&mut tx, id)?;
        let count = add_files(&mut tx, id, &body.ids)?;
        tx.execute("UPDATE albums SET updated_at = $2 WHERE id = $1", &[&id, &unix_now()])?;
        db::notify(&mut tx, "library", "")?;
        tx.commit()?;
        Ok(count)
    })
    .await?;
    Ok(Json(Count { count }))
}

pub async fn remove(State(state): State<Shared>, Path(id): Path<i64>, Json(body): Json<Ids>) -> Res<Count> {
    clean_ids(&body.ids)?;
    let count = run(&state, move |c| {
        let mut tx = c.transaction()?;
        load_album(&mut tx, id)?;
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
