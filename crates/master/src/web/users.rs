//! Users (administrators only). A user is disabled first and can then be deleted with what is theirs.

use super::access::{Access, Role};
use super::auth::{self, MIN_PASSWORD};
use super::{act, run, ApiError, Done, Res, Shared};
use crate::platform::db;
use crate::repo::{devices, users};
use crate::web::input::NAME_RULE;
use axum::extract::{Path, State};
use axum::{Extension, Json};
use fafnirx_common::{unix_now, valid_device_name};
use fafnirx_db::client::GenericClient;
use fafnirx_db::queries as sql;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct UserInfo {
    name: String,
    #[cfg_attr(test, ts(type = "'admin' | 'normal' | 'readonly'"))]
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
            .map(|u| UserInfo { name: u.name, role: u.role, disabled: u.disabled, created_at: u.created_at, devices: u.devices })
            .collect())
    })
    .await?;
    Ok(Json(users))
}

/// Makes sure every user has a folder, so that a network share shows it from the start.
pub fn ensure_dirs(conn: &mut postgres::Client, store: &crate::platform::store::Store) -> anyhow::Result<()> {
    for name in users::names(conn)? {
        std::fs::create_dir_all(store.user_dir(&name))?;
    }
    Ok(())
}

/// Adds a user and their folder.
pub fn add_user(c: &mut impl GenericClient, live_root: &std::path::Path, name: &str, role: Role, password_hash: &str) -> anyhow::Result<()> {
    if !valid_device_name(name) {
        let rule = NAME_RULE.trim_start_matches("Name: ");
        return Err(ApiError::bad(format!("User name: {rule}")).into());
    }
    let exists = users::exists(c, name)?;
    if exists {
        return Err(ApiError::conflict(format!("There is already a user named {name}")).into());
    }
    sql::users::insert_user().bind(c, &name, &role.as_str(), &password_hash, &unix_now()).map(drop)?;
    let _ = std::fs::create_dir_all(live_root.join(crate::platform::store::USERS_DIR).join(name));
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

pub async fn create(State(state): State<Shared>, Extension(who): Extension<Access>, Json(body): Json<NewUser>) -> Res<Done> {
    who.require_admin()?;
    let name = body.name.trim().to_lowercase();
    let role = role_of(&body.role)?;
    check_password(&body.password)?;
    let root = state.store.live_root().to_path_buf();
    let password = body.password;
    act(&state, move |c| {
        let hash = auth::hash_password(&password)?;
        add_user(c, &root, &name, role, &hash)?;
        db::notify(c, db::Topic::Users, &name)?;
        Ok(())
    })
    .await
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

pub async fn update(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>, Json(body): Json<UserChange>) -> Res<Done> {
    who.require_admin()?;
    let role = body.role.as_deref().map(role_of).transpose()?;
    if let Some(password) = &body.password {
        check_password(password)?;
    }
    act(&state, move |c| {
        let mut tx = c.transaction()?;
        // Serializes changes to the administrators, so that two requests cannot each leave the other as the last.
        users::lock(&mut tx)?;
        let account = users::by_name(&mut tx, &name)?.ok_or_else(|| ApiError::not_found("No such user"))?;
        let (id, current, disabled) = (account.id, account.role, account.disabled);
        let stays_admin = role.map_or(current == "admin", |r| r == Role::Admin);
        let stays_enabled = !body.disabled.unwrap_or(disabled);
        if current == "admin" && !disabled && !(stays_admin && stays_enabled) {
            let others = sql::users::other_active_admins().bind(&mut tx, &id).one()?;
            if others == 0 {
                return Err(ApiError::conflict("This is the last administrator: make another one first").into());
            }
        }
        if let Some(role) = role {
            sql::users::set_user_role().bind(&mut tx, &role.as_str(), &id).map(drop)?;
        }
        match body.disabled {
            Some(true) => sql::users::disable_user().bind(&mut tx, &unix_now(), &id).map(drop)?,
            Some(false) => users::enable(&mut tx, id)?,
            None => {}
        }
        if let Some(password) = &body.password {
            auth::set_password(&mut tx, id, password, None)?;
        }
        // A role or a lock changes what the sessions of the user may do: they log in again.
        if role.is_some() || body.disabled == Some(true) {
            sql::sessions::delete_user_sessions().bind(&mut tx, &id).map(drop)?;
        }
        db::notify(&mut tx, db::Topic::Users, &name)?;
        tx.commit()?;
        Ok(())
    })
    .await
}

/// Deletes a disabled user. Their computers go with what they sent (the computers keep their own files), and so do their
/// memberships, favorites, tags, albums and sessions. Their folders must be empty, and they must have no sources left, so that
/// nothing of theirs is lost without being looked at; a mirror or another user's source on one of their folders is in the way too.
pub async fn delete(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Done> {
    who.require_admin()?;
    if name == who.name {
        return Err(ApiError::conflict("You cannot delete your own account"));
    }
    if name == "admin" {
        return Err(ApiError::conflict("The first administrator, admin, cannot be deleted; disable it instead"));
    }
    let store = state.store.clone();
    act(&state, move |c| {
        let mut tx = c.transaction()?;
        users::lock(&mut tx)?;
        let account = users::by_name(&mut tx, &name)?.ok_or_else(|| ApiError::not_found("No such user"))?;
        if !account.disabled {
            return Err(ApiError::conflict(format!("Disable {name} first")).into());
        }
        if let Some(source) = users::sources_of(&mut tx, account.id)?.into_iter().next() {
            return Err(ApiError::conflict(format!("{name} still has the source {source}: delete their sources first")).into());
        }
        if let Some((link, folder)) = users::link_on_folders_of(&mut tx, account.id)? {
            return Err(ApiError::conflict(format!("{link} mirrors or writes into {folder}: delete it first")).into());
        }
        let owned = users::devices_of(&mut tx, account.id)?;
        let mut versions = Vec::new();
        for d in &owned {
            devices::lock(&mut tx, d.id)?;
            // What a computer sent is a copy of its files; a folder's files are kept only here.
            if d.kind == "local" && devices::has_live_files(&mut tx, d.id)? {
                let folder = &d.folder;
                return Err(ApiError::conflict(format!("{folder} still has files: delete or move them first")).into());
            }
            versions.extend(devices::delete_contents(&mut tx, d.id)?);
            devices::delete(&mut tx, d.id)?;
        }
        users::delete(&mut tx, account.id, &name, unix_now())?;
        db::notify(&mut tx, db::Topic::Users, &name)?;
        db::notify(&mut tx, db::Topic::Devices, "")?;
        tx.commit()?;

        for version in versions {
            store.remove_version(version);
        }
        for d in &owned {
            let dir = store.live_dir(&d.folder);
            if d.kind == "device" {
                let _ = std::fs::remove_dir_all(dir);
            } else {
                crate::devices::folders::remove_if_only_folders(&dir);
            }
        }
        crate::devices::folders::remove_if_only_folders(&store.live_root().join(crate::platform::store::USERS_DIR).join(&name));
        tracing::info!("deleted the user {name} with {} computers and folders", owned.len());
        Ok(())
    })
    .await
}
