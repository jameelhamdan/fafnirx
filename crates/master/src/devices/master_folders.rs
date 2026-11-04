//! Master folders in the web app: named subfolders of the master files that computers sync both ways. The master files as a
//! whole are never one. An administrator makes, renames and removes them; every user sees them (in the list of folders, see
//! `folders`), and a read-only user only reads. A
//! master folder is its path: to rename it, remove it and make the new one (the files stay where they are).

use crate::files::browse;
use crate::platform::db::{self, Device};
use crate::repo::devices;
use crate::web::access::Access;
use crate::web::input;
use crate::web::{act, ApiError, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use fafnirx_common::unix_now;
use postgres::Client;
use serde::Deserialize;

/// The path of a master folder, checked: a name Windows can keep, not in `users/`, and neither inside another master folder nor
/// holding one (`except` is the folder that is being renamed: it does not count).
pub fn check(c: &mut Client, path: &str, except: Option<&str>) -> anyhow::Result<String> {
    let path = input::path_inside(path)?;
    if let Some(why) = fafnirx_common::not_portable(&path) {
        return Err(ApiError::bad(format!("Windows could not keep that: {why}")).into());
    }
    let first = path.split('/').next().unwrap_or("").to_lowercase();
    if first == crate::platform::store::USERS_DIR {
        return Err(ApiError::conflict("users is where the folders of the users are kept: the master files cannot use that name").into());
    }
    let lower = path.to_lowercase();
    for other in devices::master_prefixes(c)?.into_iter().filter(|o| Some(o.as_str()) != except) {
        let o = other.to_lowercase();
        if o == lower || o.starts_with(&format!("{lower}/")) || lower.starts_with(&format!("{o}/")) {
            return Err(ApiError::conflict(format!(
                "{other} is a master folder already, and a master folder cannot be inside another or hold one"
            ))
            .into());
        }
    }
    Ok(path)
}

#[derive(Deserialize)]
pub struct NewMasterFolder {
    path: String,
}

pub async fn create(State(state): State<Shared>, Extension(who): Extension<Access>, Json(body): Json<NewMasterFolder>) -> Res<Done> {
    who.require_admin()?;
    let store = state.store.clone();
    act(&state, move |c| {
        let path = check(c, &body.path, None)?;
        devices::insert_master_folder(c, &path, unix_now())?;
        // The directory exists from the start so that a network share shows it.
        let _ = std::fs::create_dir_all(store.live_dir(crate::platform::store::MASTER_FOLDER).join(&path));
        db::notify(c, "devices", "")?;
        Ok(())
    })
    .await
}

/// Gives the folder another path, like renaming any folder: its files are moved, and the albums, sources and mirrors that point into it
/// follow. Computers that sync it keep working: they address it by its id.
pub async fn rename(State(state): State<Shared>, Extension(who): Extension<Access>, Path(id): Path<uuid::Uuid>, Json(body): Json<NewMasterFolder>) -> Res<Done> {
    who.require_admin()?;
    let store = state.store.clone();
    act(&state, move |c| {
        let old = devices::master_folder_path(c, id)?.ok_or_else(|| ApiError::not_found("No such master folder"))?;
        let new = check(c, &body.path, Some(&old))?;
        if new == old {
            return Ok(());
        }
        let master = Device { id: crate::files::ingest::MASTER_DEVICE_ID, name: crate::platform::store::MASTER_FOLDER.into() };
        if !browse::expand(c, &master, &old, false)?.is_empty() {
            browse::move_tree(c, &store, &master, &old, &new)?;
        }
        devices::rename_master_folder(c, id, &new)?;
        let scope = |p: &str| format!("{}/{p}", crate::platform::store::MASTER_FOLDER);
        fafnirx_db::queries::devices::rename_folder_in_mirror_scopes().bind(c, &scope(&new), &scope(&old))?;
        let root = store.live_dir(crate::platform::store::MASTER_FOLDER);
        let _ = std::fs::create_dir_all(root.join(&new));
        let _ = std::fs::remove_dir(root.join(&old));
        db::notify(c, "devices", "")?;
        Ok(())
    })
    .await
}

/// Removes the entry; the files stay in the master files. Computers that had it attached are refused from their next cycle.
pub async fn delete(State(state): State<Shared>, Extension(who): Extension<Access>, Path(id): Path<uuid::Uuid>) -> Res<Done> {
    who.require_admin()?;
    act(&state, move |c| {
        devices::delete_master_folder(c, id)?.ok_or_else(|| ApiError::not_found("No such master folder"))?;
        db::notify(c, "devices", "")?;
        Ok(())
    })
    .await
}
