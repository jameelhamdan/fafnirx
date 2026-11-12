//! Master folders in the web app: named folders of the master files that belong to no user, that folders sync both ways. An
//! administrator makes, renames and removes them; every user sees them (in the list of folders, see `api`), and a read-only user
//! only reads. A master folder is a folder like any other: a row of `folders` of the kind `master`, named by its path in `files/`,
//! so renaming and removing it is what it is for any folder (`rename`, `api`).

use super::{api, rename};
use crate::platform::db;
use crate::repo::folders;
use crate::web::access::Ctx;
use crate::web::input;
use crate::web::{act, ApiError, Done, Res};
use axum::extract::Path;
use axum::Json;
use fafnirx_common::unix_now;
use postgres::Client;
use serde::Deserialize;

/// The path of a master folder, checked: a name Windows can keep, not `users`, and neither inside another master folder nor
/// holding one (`except` is the folder that is being renamed: it does not count).
pub fn check(c: &mut Client, path: &str, except: Option<&str>) -> anyhow::Result<String> {
    let path = input::path_inside(path)?;
    if let Some(why) = fafnirx_common::not_portable(&path) {
        return Err(ApiError::bad(format!("Windows could not keep that: {why}")).into());
    }
    let first = path.split('/').next().unwrap_or("").to_lowercase();
    if first == crate::platform::store::USERS_DIR {
        return Err(ApiError::conflict("users is where the folders of the users are kept: a master folder cannot use that name").into());
    }
    if let Some(other) = folders::master_names_overlapping(c, &path)?.into_iter().find(|o| Some(o.as_str()) != except) {
        return Err(ApiError::conflict(format!(
            "{other} is a master folder already, and a master folder cannot be inside another or hold one"
        ))
        .into());
    }
    Ok(path)
}

#[derive(Deserialize)]
pub struct NewMasterFolder {
    path: String,
}

pub async fn create(Ctx { state, who }: Ctx, Json(body): Json<NewMasterFolder>) -> Res<Done> {
    who.require_admin()?;
    let store = state.store.clone();
    act(&state, move |c| {
        let path = check(c, &body.path, None)?;
        let Some(key) = folders::insert_master(c, &path, unix_now())? else {
            return Err(ApiError::conflict(format!("{path} is a master folder already")).into());
        };
        // The directory exists from the start so that a network share shows it.
        let _ = std::fs::create_dir_all(store.live_dir(&key));
        db::notify(c, db::Topic::Folders, &key)?;
        Ok(())
    })
    .await
}

/// Gives the folder another path, like renaming any folder: its files are moved, and the albums, sources and mirrors that point into it
/// follow. Folders that sync it keep working: they address it by its id.
pub async fn rename(Ctx { state, who }: Ctx, Path(id): Path<uuid::Uuid>, Json(body): Json<NewMasterFolder>) -> Res<Done> {
    who.require_admin()?;
    let store = state.store.clone();
    act(&state, move |c| {
        let (_, _, key) = folders::master_by_public_id(c, id)?.ok_or_else(|| ApiError::not_found("No such master folder"))?;
        rename::rename(c, &store, &who, &key, &body.path)?;
        Ok(())
    })
    .await
}

/// Removes the folder, with its trash and history, once it has no files left: the same as removing any other folder.
pub async fn delete(Ctx { state, who }: Ctx, Path(id): Path<uuid::Uuid>) -> Res<Done> {
    who.require_admin()?;
    let store = state.store.clone();
    act(&state, move |c| {
        let (id, _, key) = folders::master_by_public_id(c, id)?.ok_or_else(|| ApiError::not_found("No such master folder"))?;
        api::remove(c, &store, id, &key)
    })
    .await
}
