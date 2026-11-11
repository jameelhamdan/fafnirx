//! Folders in the web app: yours, those shared with you and the master folders, in one list. Make one, add members (read or
//! read-write), delete. Computers use them through the client sync (see `grpc`); the master folders are made by an administrator
//! (`master_folders`, which uses the same `remove` and `rename`).

use crate::platform::db;
use crate::platform::store::Store;
use crate::repo::devices;
use crate::web::access::{Access, Role};
use crate::web::input;
use crate::web::{act, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use fafnirx_common::unix_now;
use fafnirx_db::queries as sql;
use postgres::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "Folder"))]
pub struct FolderInfo {
    #[serde(flatten)]
    folder: devices::Folder,
    /// Who else uses it. Only its owner sees this.
    members: Vec<devices::Member>,
    /// The computers that have it attached, as they last said: all of them for its owner (and an administrator, for a master
    /// folder), the user's own otherwise.
    computers: Vec<devices::FolderState>,
    /// What else works with it: the sources that write into it, the mirrors that hold it, and the user's album of it.
    rules: Rules,
}

#[derive(Serialize, Default)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "FolderRules"))]
pub struct Rules {
    sources: Vec<RuleSource>,
    /// Only for administrators, who own the mirrors.
    mirrors: Vec<RuleMirror>,
    album: Option<RuleAlbum>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct RuleSource {
    /// `<owner>/<name>`: how the Sources page keys it.
    key: String,
    kind: String,
    status: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct RuleMirror {
    name: String,
    kind: String,
    status: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct RuleAlbum {
    id: crate::platform::id::Id,
    name: String,
}

/// The sources, mirrors and album that point at a folder, as far as `who` may see them.
fn rules_of(c: &mut Client, who: &Access, folder: &devices::Folder) -> anyhow::Result<Rules> {
    let (all, user) = who.scope_args();
    let (mut sources, mut mirrors) = (Vec::new(), Vec::new());
    for l in sql::devices::folder_links().bind(&mut *c, &folder.id, &all, &user).all()? {
        match l.owner {
            Some(owner) => sources.push(RuleSource { key: format!("{owner}/{}", l.name), kind: l.kind, status: l.last_status }),
            None => mirrors.push(RuleMirror { name: l.name, kind: l.kind, status: l.last_status }),
        }
    }
    let album = sql::devices::folder_album()
        .bind(&mut *c, &folder.id, &who.user_id)
        .opt()?
        .map(|a| RuleAlbum { id: crate::platform::id::Id(a.id), name: a.name });
    Ok(Rules { sources, mirrors, album })
}

/// The folders the user can use: their own, those shared with them and the master folders.
pub async fn list(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<FolderInfo>> {
    let folders = run(&state, move |c| {
        let mut out = Vec::new();
        for folder in devices::folders(c, who.user_id)? {
            let members = if folder.mine { devices::shared_members(c, folder.id)? } else { Vec::new() };
            let computers = devices::folder_states_of_folder(c, &folder.folder, folder.mine || (folder.master && who.is_admin()), who.user_id)?;
            let rules = rules_of(c, &who, &folder)?;
            out.push(FolderInfo { folder, members, computers, rules });
        }
        Ok(out)
    })
    .await?;
    Ok(Json(folders))
}

/// Makes a folder of `owner` and says so; returns its folder. The one place a name is checked and a clash refused, for
/// the web app and for the clients.
pub fn make(c: &mut Client, owner: i64, name: &str) -> anyhow::Result<String> {
    let name = input::device_name(name)?;
    let Some(folder) = devices::create_folder(c, owner, &name, unix_now())? else {
        return Err(ApiError::conflict(format!("You have a device or folder called {name} already")).into());
    };
    db::notify(c, db::Topic::Devices, &folder)?;
    Ok(folder)
}

#[derive(Deserialize)]
pub struct NewFolder {
    name: String,
}

#[derive(Serialize)]
pub struct Created {
    /// The folder as clients name it: `<owner>/<name>`.
    folder: String,
}

pub async fn create(State(state): State<Shared>, Extension(who): Extension<Access>, Json(body): Json<NewFolder>) -> Res<Created> {
    who.require_workspace()?;
    let folder = run(&state, move |c| make(c, who.user_id, &body.name)).await?;
    // The folder exists from the start so that a network share shows it.
    let _ = std::fs::create_dir_all(state.store.live_dir(&folder));
    Ok(Json(Created { folder }))
}

/// The folder `name` of the user, which only they can manage.
fn own(c: &mut Client, who: &Access, name: &str) -> anyhow::Result<devices::Folder> {
    devices::folder(c, who.user_id, name)?
        .filter(|f| f.mine)
        .ok_or_else(|| ApiError::not_found("You have no such folder").into())
}

pub async fn delete(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Done> {
    who.require_workspace()?;
    let store = state.store.clone();
    act(&state, move |c| {
        let folder = own(c, &who, &name)?;
        remove(c, &store, folder.id, &folder.folder)
    })
    .await
}

/// Removes a folder (of a user, or a master folder) with its trash and history, once it has no files left and no source writes into it.
pub fn remove(c: &mut Client, store: &Store, id: i64, folder: &str) -> anyhow::Result<()> {
    let Some(versions) = devices::purge_folder(c, id)? else {
        return Err(ApiError::conflict("It still has files, or a source writes into it. Delete them first; deleting the folder then also removes the trash and the history").into());
    };
    for version in versions {
        store.remove_version(version);
    }
    remove_if_only_folders(&store.live_dir(folder));
    db::notify(c, db::Topic::Devices, folder)?;
    Ok(())
}

/// Removes the directory of a deleted folder when nothing but empty folders is left in it. A file that is there anyway (put
/// in by hand, not yet noticed by the watcher) is not ours to remove.
pub(crate) fn remove_if_only_folders(dir: &std::path::Path) {
    fn has_files(dir: &std::path::Path) -> bool {
        std::fs::read_dir(dir)
            .map(|entries| entries.flatten().any(|e| e.file_type().map_or(true, |t| !t.is_dir() || has_files(&e.path()))))
            .unwrap_or(false)
    }
    if !has_files(dir) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[derive(Deserialize)]
pub struct Leave {
    /// The folder as `<owner>/<name>`.
    folder: String,
}

/// A user stops using a folder that another user shared with them.
pub async fn leave(State(state): State<Shared>, Extension(who): Extension<Access>, Json(body): Json<Leave>) -> Res<Done> {
    who.require_workspace()?;
    act(&state, move |c| {
        let folder = devices::folder(c, who.user_id, &body.folder)?
            .filter(|f| !f.mine && !f.master)
            .ok_or_else(|| ApiError::not_found("No folder is shared with you under that name"))?;
        devices::remove_shared_member(c, folder.id, who.user_id)?;
        db::notify(c, db::Topic::Devices, &folder.folder)?;
        Ok(())
    })
    .await
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Person {
    name: String,
    /// Whether they may be given write access (not for read-only accounts).
    can_write: bool,
}

/// The other users a folder can be shared with, for the share dialog's suggestions.
pub async fn people(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<Person>> {
    who.require_workspace()?;
    let people = run(&state, move |c| {
        Ok(sql::users::enabled_users()
            .bind(c)
            .all()?
            .into_iter()
            .filter(|u| u.name != who.name)
            .map(|u| Person { can_write: Role::parse(&u.role) != Some(Role::Readonly), name: u.name })
            .collect())
    })
    .await?;
    Ok(Json(people))
}

#[derive(Deserialize)]
pub struct MemberBody {
    user: String,
    /// Read only unless said otherwise.
    #[serde(default)]
    can_write: bool,
}

/// Lets another user use the folder, or changes whether they may write to it.
pub async fn add_member(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>, Json(body): Json<MemberBody>) -> Res<Done> {
    who.require_workspace()?;
    let user = body.user.trim().to_lowercase();
    act(&state, move |c| {
        let folder = own(c, &who, &name)?;
        if user == who.name {
            return Err(ApiError::bad("You use your own folders on all your computers already").into());
        }
        let (user_id, role) = devices::user_for_device(c, &user)?.ok_or_else(|| ApiError::bad(format!("There is no user named {user}")))?;
        if Role::parse(&role) == Some(Role::Readonly) && body.can_write {
            return Err(ApiError::bad(format!("{user} has a read-only account and can only read")).into());
        }
        devices::add_shared_member(c, folder.id, user_id, body.can_write, unix_now())?;
        db::notify(c, db::Topic::Devices, &folder.folder)?;
        Ok(())
    })
    .await
}

pub async fn remove_member(State(state): State<Shared>, Extension(who): Extension<Access>, Path((name, user)): Path<(String, String)>) -> Res<Done> {
    who.require_workspace()?;
    act(&state, move |c| {
        let folder = own(c, &who, &name)?;
        let (user_id, _) = devices::user_for_device(c, &user.trim().to_lowercase())?.ok_or_else(|| ApiError::not_found("No such user"))?;
        if !devices::remove_shared_member(c, folder.id, user_id)? {
            return Err(ApiError::not_found("That user does not use this folder").into());
        }
        db::notify(c, db::Topic::Devices, &folder.folder)?;
        Ok(())
    })
    .await
}
