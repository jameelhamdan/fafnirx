//! Users (administrators only). A user is never deleted, only disabled: the name is the folder of their files.

use super::access::{Access, Role};
use super::auth::{self, MIN_PASSWORD};
use super::{done, run, ApiError, Done, Res, Shared};
use crate::devices::api::NAME_RULE;
use crate::platform::db;
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
        let rows = c.query(
            "SELECT u.name, u.role, u.disabled_at IS NOT NULL, u.created_at,
                    (SELECT COUNT(*) FROM devices d WHERE d.user_id = u.id AND d.kind IN ('device', 'local'))
             FROM users u ORDER BY u.name",
            &[],
        )?;
        Ok(rows
            .iter()
            .map(|r| UserInfo {
                name: r.get(0),
                role: r.get(1),
                disabled: r.get(2),
                created_at: r.get(3),
                devices: r.get(4),
            })
            .collect())
    })
    .await?;
    Ok(Json(users))
}

/// Whether `name` cannot be a user because the master files already use it: a folder or file at the root of the live tree
/// (compared without regard to case, as a network share does) or a master file path that starts with it.
pub fn taken_by_master(c: &mut impl GenericClient, live_root: &std::path::Path, name: &str) -> anyhow::Result<bool> {
    let in_db = c
        .query_one(
            "SELECT EXISTS (SELECT 1 FROM files WHERE device_id = 0 AND lower(split_part(path, '/', 1)) = $1)",
            &[&name],
        )?
        .get::<_, bool>(0);
    let on_disk = std::fs::read_dir(live_root)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().to_lowercase() == name);
    Ok(in_db || on_disk)
}

/// Makes sure every user has a folder, so that a network share shows it from the start.
pub fn ensure_dirs(conn: &mut postgres::Client, store: &crate::platform::store::Store) -> anyhow::Result<()> {
    for row in conn.query("SELECT name FROM users", &[])? {
        let name: String = row.get(0);
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
    let exists = c.query_opt("SELECT 1 FROM users WHERE name = $1", &[&name])?.is_some();
    if exists {
        return Err(ApiError::conflict(format!("There is already a user named {name}")).into());
    }
    if taken_by_master(c, live_root, name)? {
        return Err(ApiError::conflict(format!(
            "The master files already have something called {name}: rename it first, a user's name is their folder"
        ))
        .into());
    }
    c.execute(
        "INSERT INTO users (name, role, password_hash, created_at) VALUES ($1, $2, $3, $4)",
        &[&name, &role.as_str(), &password_hash, &unix_now()],
    )?;
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
        tx.execute("LOCK TABLE users IN SHARE ROW EXCLUSIVE MODE", &[])?;
        let row = tx
            .query_opt("SELECT id, role, disabled_at IS NOT NULL FROM users WHERE name = $1", &[&name])?
            .ok_or_else(|| ApiError::not_found("No such user"))?;
        let (id, current, disabled): (i64, String, bool) = (row.get(0), row.get(1), row.get(2));
        let stays_admin = role.map_or(current == "admin", |r| r == Role::Admin);
        let stays_enabled = !body.disabled.unwrap_or(disabled);
        if current == "admin" && !disabled && !(stays_admin && stays_enabled) {
            let others: i64 = tx
                .query_one(
                    "SELECT COUNT(*) FROM users WHERE role = 'admin' AND disabled_at IS NULL AND id <> $1",
                    &[&id],
                )?
                .get(0);
            if others == 0 {
                return Err(ApiError::conflict("This is the last administrator: make another one first").into());
            }
        }
        if let Some(role) = role {
            tx.execute("UPDATE users SET role = $2 WHERE id = $1", &[&id, &role.as_str()])?;
        }
        match body.disabled {
            Some(true) => {
                tx.execute(
                    "UPDATE users SET disabled_at = COALESCE(disabled_at, $2) WHERE id = $1",
                    &[&id, &unix_now()],
                )?;
            }
            Some(false) => {
                tx.execute("UPDATE users SET disabled_at = NULL WHERE id = $1", &[&id])?;
            }
            None => {}
        }
        if let Some(password) = &body.password {
            auth::set_password(&mut tx, id, password, None)?;
        }
        // A role or a lock changes what the sessions of the user may do: they log in again.
        if role.is_some() || body.disabled == Some(true) {
            tx.execute("DELETE FROM sessions WHERE user_id = $1", &[&id])?;
        }
        db::notify(&mut tx, "users", &name)?;
        tx.commit()?;
        Ok(())
    })
    .await?;
    done()
}
