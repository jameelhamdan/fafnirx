//! Changing files in the web app: delete, restore, move or rename, and rescan. The browser that lists them is `browse`.

use crate::files::browse::{find_folder_to_change, version_file, FileRef, VersionAt};
use crate::files::ingest;
use crate::platform::db::{self, FolderRef};
use crate::repo::files;
use crate::repo::folders;
use crate::web::access::Ctx;
use crate::web::input;
use crate::web::{act, run, ApiError, Done, Res};
use axum::Json;
use fafnirx_common::clean_remote_path;
use fafnirx_db::queries as sql;
use postgres::Client;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct Items {
    items: Vec<FileRef>,
}

#[derive(Serialize)]
pub struct Removed {
    deleted: usize,
}

/// The live paths a selection stands for: a file, or every file under a folder.
pub fn expand(c: &mut Client, folder: &FolderRef, path: &str, include_deleted: bool) -> anyhow::Result<Vec<String>> {
    let path = path.trim_matches('/');
    Ok(files::expand(c, folder.id, path, include_deleted)?)
}

#[derive(Serialize)]
pub struct Rescan {
    /// False when a comparison was already running.
    started: bool,
}

/// Compares the file store with the database now instead of at the next round (administrators). Files put on the disk
/// by hand, or over a network share, are picked up either way; this is for when one does not want to wait.
pub async fn rescan(Ctx { state, who }: Ctx) -> Res<Rescan> {
    who.require_admin()?;
    Ok(Json(Rescan { started: crate::files::livetree::rescan(state.db.clone(), state.store.clone()) }))
}

pub async fn remove(Ctx { state, who }: Ctx, Json(body): Json<Items>) -> Res<Removed> {
    let store = state.store.clone();
    let deleted = run(&state, move |c| {
        let mut deleted = 0;
        for item in &body.items {
            let folder = find_folder_to_change(c, &who, &item.folder)?;
            for path in expand(c, &folder, &item.path, false)? {
                if ingest::mark_deleted(c, &store, &folder, &path, true)? {
                    deleted += 1;
                }
            }
        }
        db::notify(c, db::Topic::Files, "")?;
        Ok(deleted)
    })
    .await?;
    Ok(Json(Removed { deleted }))
}

#[derive(Deserialize)]
pub struct Restore {
    folder: String,
    path: String,
    version: i64,
}

pub async fn restore(Ctx { state, who }: Ctx, Json(r): Json<Restore>) -> Res<Done> {
    let store = state.store.clone();
    act(&state, move |c| {
        let folder = find_folder_to_change(c, &who, &r.folder)?;
        let path = clean_remote_path(&r.path).map_err(|e| ApiError::bad(e.to_string()))?;
        let row = sql::files::version_to_restore()
            .bind(c, &folder.id, &path, &r.version)
            .opt()?
            .ok_or_else(|| ApiError::not_found("No such version"))?;
        let (current, deleted_at) = (row.current_version_id, row.deleted_at);
        if current == Some(r.version) && deleted_at.is_none() {
            return Err(ApiError::conflict("That is already the current version").into());
        }
        // What a device or a source sends is theirs to change: here it can be brought back from the trash, not edited.
        let kind = folders::kind_of(c, folder.id)?;
        if !kind.editable() && deleted_at.is_none() {
            return Err(ApiError::conflict("Files sent by a device cannot be changed here. Download the version instead.").into());
        }
        let from = version_file(
            &store,
            &VersionAt { folder: &folder.key, path: &path, version: r.version, current, deleted: deleted_at.is_some() },
        )?;
        if !from.is_file() {
            return Err(ApiError::not_found("The content of that version is no longer stored").into());
        }
        let staged = store.temp_path();
        std::fs::copy(&from, &staged)?;
        let root = row.root_hash;
        ingest::commit_version(
            c,
            &store,
            &staged,
            &ingest::NewVersion {
                folder: &folder,
                path: &path,
                root: root.try_into().map_err(|_| anyhow::anyhow!("stored hash has the wrong length"))?,
                size: row.size as u64,
                mtime: row.mtime,
                mode: row.mode as u32,
                renamed_from: None,
            },
        )?;
        Ok(())
    })
    .await
}

#[derive(Deserialize)]
pub struct MoveBody {
    folder: String,
    /// A file, or a folder inside the folder.
    from: String,
    /// Its new path on the same folder.
    to: String,
}

#[derive(Serialize)]
pub struct Moved {
    moved: usize,
}

/// Renames or moves a file, or a folder with everything below it, within a folder the master owns: one of a user or a master
/// folder. What a device or a source sent is theirs to move, so it answers 409 there.
pub async fn move_files(Ctx { state, who }: Ctx, Json(b): Json<MoveBody>) -> Res<Moved> {
    let store = state.store.clone();
    let moved = run(&state, move |c| {
        let folder = find_folder_to_change(c, &who, &b.folder)?;
        if !folders::kind_of(c, folder.id)?.editable() {
            return Err(ApiError::conflict("Files sent by a device or a source cannot be moved here").into());
        }
        move_tree(c, &store, &folder, &input::path_inside(&b.from)?, &input::path_inside(&b.to)?)
    })
    .await?;
    Ok(Json(Moved { moved }))
}

/// Moves a file, or a folder with everything below it, to `to` within a folder (both cleaned paths); returns how many files
/// moved. Nothing moves if something is in the way. The caller checked that the folder is changeable.
pub fn move_tree(c: &mut Client, store: &crate::platform::store::Store, folder: &FolderRef, from: &str, to: &str) -> anyhow::Result<usize> {
    if to == from || to.starts_with(&format!("{from}/")) {
        return Err(ApiError::bad("Choose a different place, not inside the folder itself").into());
    }
    let paths = expand(c, folder, from, false)?;
    if paths.is_empty() {
        return Err(ApiError::not_found("No such file or folder").into());
    }
    let targets: Vec<(String, String)> = paths.into_iter().map(|p| (format!("{to}{}", &p[from.len()..]), p)).collect();
    // Every new path, and every folder above one, must be free: in the database and on the disk.
    let mut wanted: Vec<String> = targets
        .iter()
        .flat_map(|(new, _)| new.match_indices('/').map(|(i, _)| new[..i].to_string()).chain([new.clone()]))
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    if let Some(taken) = files::first_live_of(c, folder.id, &wanted)? {
        return Err(ApiError::conflict(format!("{taken} exists already")).into());
    }
    for (new, _) in &targets {
        if std::fs::symlink_metadata(store.live_path(&folder.key, new)?).is_ok() {
            return Err(ApiError::conflict(format!("{new} exists already")).into());
        }
    }
    let mut moved: Vec<&(String, String)> = Vec::new();
    for pair in &targets {
        let (new, old) = pair;
        if let Err(e) = ingest::move_path(c, store, folder, old, new) {
            // Nothing half done: what moved goes back where it was.
            for (new, old) in moved.into_iter().rev() {
                if let Err(back) = ingest::move_path(c, store, folder, new, old) {
                    tracing::error!("could not move {new} back to {old} in {}: {back:#}", folder.key);
                }
            }
            db::notify(c, db::Topic::Files, &folder.key)?;
            return Err(e);
        }
        moved.push(pair);
    }
    db::notify(c, db::Topic::Files, &folder.key)?;
    Ok(targets.len())
}
