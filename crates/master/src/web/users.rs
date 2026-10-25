//! Users (administrators only). A user is never deleted, only disabled: the name is the folder of their files.

use super::access::{Access, Role};
use super::auth::{self, MIN_PASSWORD};
use super::{done, run, ApiError, Done, Res, Shared};
use crate::devices::api::NAME_RULE;
use crate::platform::db;
use crate::repo::{sessions, users};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use fafnirx_common::{unix_now, valid_device_name};
use postgres::GenericClient;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct UserInfo {
    name: String,
    role: String,
    disabled: bool,
    created_at: i64,
    devices: i64,
}

pub async fn list(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<UserInfo>> {
    who.require_admin()?;
    let users = run(&state, |c| {
        Ok(users::list(c)?
            .into_iter()
            .map(|u| UserInfo {
                name: u.name,
                role: u.role,
                disabled: u.disabled,
                created_at: u.created_at,
                devices: u.devices,
            })
            .collect())
    })
    .await?;
    Ok(Json(users))
}

/// Whether `name` cannot be a user because the master files already use it: a folder or file at the root of the live tree
/// (compared without regard to case, as a network share does) or a master file path that starts with it.
pub fn taken_by_master(c: &mut impl GenericClient, live_root: &std::path::Path, name: &str) -> anyhow::Result<bool> {
    let in_db = users::master_path_taken(c, name)?;
    let on_disk = std::fs::read_dir(live_root)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().to_lowercase() == name);
    Ok(in_db || on_disk)
}

/// Makes sure every user has a folder, so that a network share shows it from the start.
pub fn ensure_dirs(conn: &mut postgres::Client, store: &crate::platform::store::Store) -> anyhow::Result<()> {
    for name in users::names(conn)? {
        std::fs::create_dir_all(store.user_dir(&name))?;
    }
    Ok(())
}

/// Adds a user and their folder. A folder or file of that name in the live tree counts as master content.
pub fn add_user(
    c: &mut impl GenericClient,
    live_root: &std::path::Path,
    name: &str,
    role: Role,
    password_hash: &str,
) -> anyhow::Result<()> {
    if !valid_device_name(name) {
        return Err(ApiError::bad(format!("User name: {}", NAME_RULE.trim_start_matches("Name: "))).into());
    }
    let exists = users::exists(c, name)?;
    if exists {
        return Err(ApiError::conflict(format!("There is already a user named {name}")).into());
    }
    if taken_by_master(c, live_root, name)? {
        return Err(ApiError::conflict(format!(
            "The master files already have something called {name}: rename it first, a user's name is their folder"
        ))
        .into());
    }
    users::insert(c, name, role.as_str(), password_hash, unix_now())?;
    let _ = std::fs::create_dir_all(live_root.join(name));
    Ok(())
}

#[derive(Deserialize)]
pub struct NewUser {
    name: String,
    role: String,
    password: String,
}

fn role_of(text: &str) -> Result<Role, ApiError> {
    Role::parse(text).ok_or_else(|| ApiError::bad("role must be admin, normal or readonly"))
}

fn check_password(password: &str) -> Result<(), ApiError> {
    if password.chars().count() < MIN_PASSWORD {
        return Err(ApiError::bad(format!("The password needs at least {MIN_PASSWORD} characters")));
    }
    Ok(())
}

pub async fn create(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(body): Json<NewUser>,
) -> Res<Done> {
    who.require_admin()?;
    let name = body.name.trim().to_lowercase();
    let role = role_of(&body.role)?;
    check_password(&body.password)?;
    let root = state.store.live_root().to_path_buf();
    let password = body.password;
    run(&state, move |c| {
        let hash = auth::hash_password(&password)?;
        add_user(c, &root, &name, role, &hash)?;
        db::notify(c, "users", &name)?;
        Ok(())
    })
    .await?;
    done()
}

#[derive(Deserialize)]
pub struct UserChange {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    disabled: Option<bool>,
    /// Sets a new password; the user's sessions end.
    #[serde(default)]
    password: Option<String>,
}

pub async fn update(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(name): Path<String>,
    Json(body): Json<UserChange>,
) -> Res<Done> {
    who.require_admin()?;
    let role = body.role.as_deref().map(role_of).transpose()?;
    if let Some(password) = &body.password {
        check_password(password)?;
    }
    run(&state, move |c| {
        let mut tx = c.transaction()?;
        // Serializes changes to the administrators, so that two requests cannot each leave the other as the last.
        users::lock(&mut tx)?;
        let account = users::by_name(&mut tx, &name)?.ok_or_else(|| ApiError::not_found("No such user"))?;
        let (id, current, disabled) = (account.id, account.role, account.disabled);
        let stays_admin = role.map_or(current == "admin", |r| r == Role::Admin);
        let stays_enabled = !body.disabled.unwrap_or(disabled);
        if current == "admin" && !disabled && !(stays_admin && stays_enabled) {
            let others = users::other_active_admins(&mut tx, id)?;
            if others == 0 {
                return Err(ApiError::conflict("This is the last administrator: make another one first").into());
            }
        }
        if let Some(role) = role {
            users::set_role(&mut tx, id, role.as_str())?;
        }
        match body.disabled {
            Some(true) => users::disable(&mut tx, id, unix_now())?,
            Some(false) => users::enable(&mut tx, id)?,
            None => {}
        }
        if let Some(password) = &body.password {
            auth::set_password(&mut tx, id, password, None)?;
        }
        // A role or a lock changes what the sessions of the user may do: they log in again.
        if role.is_some() || body.disabled == Some(true) {
            sessions::end_all(&mut tx, id)?;
        }
        db::notify(&mut tx, "users", &name)?;
        tx.commit()?;
        Ok(())
    })
    .await?;
    done()
}
