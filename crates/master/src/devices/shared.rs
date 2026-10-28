//! Shared folders in the web app: make, add members (read or read-write), delete. Computers use them through the client
//! sync (see `grpc`).

use crate::platform::db;
use crate::repo::devices;
use crate::web::access::{Access, Role};
use crate::web::{done, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use fafnirx_common::{unix_now, valid_device_name};
use postgres::Client;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct SharedFolder {
    #[serde(flatten)]
    folder: devices::Shared,
    /// Who else uses it. Only its owner sees this.
    members: Vec<devices::Member>,
    /// The computers that have it attached, as they last said: all of them for its owner, the user's own otherwise.
    computers: Vec<devices::SharedState>,
}

/// The shared folders the user owns and those shared with them.
pub async fn list(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<SharedFolder>> {
    let folders = run(&state, move |c| {
        let mut out = Vec::new();
        for folder in devices::shared_folders(c, who.user_id)? {
            let members = if folder.mine { devices::shared_members(c, folder.id)? } else { Vec::new() };
            let computers = devices::shared_state_of_folder(c, &folder.folder, folder.mine, who.user_id)?;
            out.push(SharedFolder { folder, members, computers });
        }
        Ok(out)
    })
    .await?;
    Ok(Json(folders))
}

/// Makes a shared folder of `owner` and says so; returns its folder. The one place a name is checked and a clash refused, for
/// the web app and for the clients.
pub fn make(c: &mut Client, owner: i64, name: &str) -> anyhow::Result<String> {
    if !valid_device_name(name) {
        return Err(ApiError::bad(crate::devices::api::NAME_RULE).into());
    }
    let Some(folder) = devices::create_shared(c, owner, name, unix_now())? else {
        return Err(ApiError::conflict(format!("You have a device or folder called {name} already")).into());
    };
    db::notify(c, "devices", &folder)?;
    Ok(folder)
}

#[derive(Deserialize)]
pub struct NewShared {
    name: String,
}

pub async fn create(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(body): Json<NewShared>,
) -> Res<Done> {
    who.require_workspace()?;
    let name = body.name.trim().to_lowercase();
    let folder = run(&state, move |c| make(c, who.user_id, &name)).await?;
    // The folder exists from the start so that a network share shows it.
    let _ = std::fs::create_dir_all(state.store.live_dir(&folder));
    done()
}

/// The shared folder `name` of the user, which only they can manage.
fn own(c: &mut Client, who: &Access, name: &str) -> anyhow::Result<devices::Shared> {
    devices::shared_folder(c, who.user_id, name)?
        .filter(|s| s.mine)
        .ok_or_else(|| ApiError::not_found("You have no such shared folder").into())
}

pub async fn delete(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(name): Path<String>,
) -> Res<Done> {
    who.require_workspace()?;
    let store = state.store.clone();
    run(&state, move |c| {
        let folder = own(c, &who, &name)?;
        let Some(versions) = devices::purge_shared_folder(c, folder.id)? else {
            return Err(ApiError::conflict(
                "It still has files. Delete them first; deleting the folder then also removes the trash and the history",
            )
            .into());
        };
        for version in versions {
            store.remove_version(version);
        }
        remove_if_only_folders(&store.live_dir(&folder.folder));
        db::notify(c, "devices", &folder.folder)?;
        Ok(())
    })
    .await?;
    done()
}

/// Removes the folder of a deleted shared folder when nothing but empty folders is left in it. A file that is there anyway (put
/// in by hand, not yet noticed by the watcher) is not ours to remove.
fn remove_if_only_folders(dir: &std::path::Path) {
    fn has_files(dir: &std::path::Path) -> bool {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries.flatten().any(|e| e.file_type().map_or(true, |t| !t.is_dir() || has_files(&e.path())))
            })
            .unwrap_or(false)
    }
    if !has_files(dir) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[derive(Deserialize)]
pub struct MemberBody {
    user: String,
    /// Read only unless said otherwise.
    #[serde(default)]
    can_write: bool,
}

/// Lets another user use the folder, or changes whether they may write to it.
pub async fn add_member(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(name): Path<String>,
    Json(body): Json<MemberBody>,
) -> Res<Done> {
    who.require_workspace()?;
    let user = body.user.trim().to_lowercase();
    run(&state, move |c| {
        let folder = own(c, &who, &name)?;
        if user == who.name {
            return Err(ApiError::bad("You use your own shared folders on all your computers already").into());
        }
        let (user_id, role) = devices::user_for_device(c, &user)?
            .ok_or_else(|| ApiError::bad(format!("There is no user named {user}")))?;
        if Role::parse(&role) == Some(Role::Readonly) && body.can_write {
            return Err(ApiError::bad(format!("{user} has a read-only account and can only read")).into());
        }
        devices::add_shared_member(c, folder.id, user_id, body.can_write, unix_now())?;
        db::notify(c, "devices", &folder.folder)?;
        Ok(())
    })
    .await?;
    done()
}

pub async fn remove_member(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path((name, user)): Path<(String, String)>,
) -> Res<Done> {
    who.require_workspace()?;
    run(&state, move |c| {
        let folder = own(c, &who, &name)?;
        let (user_id, _) = devices::user_for_device(c, &user.trim().to_lowercase())?
            .ok_or_else(|| ApiError::not_found("No such user"))?;
        if !devices::remove_shared_member(c, folder.id, user_id)? {
            return Err(ApiError::not_found("That user does not use this folder").into());
        }
        db::notify(c, "devices", &folder.folder)?;
        Ok(())
    })
    .await?;
    done()
}
