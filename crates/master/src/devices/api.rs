//! Devices in the web app: list them, add one (it gets a token), rotate the token, release or revoke it. Their folders are in
//! `folders`.

use crate::monitoring::api::{recent, Recent};
use crate::platform::db;
use crate::repo::{alerts, devices, settings};
use crate::web::access::{Access, Ctx};
use crate::web::input;
use crate::web::{act, run, ApiError, Done, Res};
use axum::extract::Path;
use axum::Json;
use fafnirx_common::unix_now;
use fafnirx_db::queries as sql;
use postgres::Client;
use serde::{Deserialize, Serialize};

/// A device sends progress every few seconds while it uploads. Older than this, it has stopped or gone away.
const ACTIVITY_FRESH_SECS: i64 = 15;

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Activity {
    phase: String,
    file: String,
    files_done: i64,
    files_total: i64,
    bytes_done: i64,
    bytes_total: i64,
    speed_bps: i64,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct DeviceSummary {
    /// The name of the device; it is unique among the devices of its user.
    pub name: String,
    /// The user the device belongs to.
    pub user: String,
    /// `<user>/<name>`: how the files API, the library and the live updates name the device.
    pub key: String,
    pub online: bool,
    pub revoked: bool,
    created_at: i64,
    hostname: Option<String>,
    os: Option<String>,
    ip: Option<String>,
    client_version: Option<String>,
    disk_total: Option<i64>,
    disk_free: Option<i64>,
    last_seen: Option<i64>,
    last_collected: Option<i64>,
    pub bytes: i64,
    pub files: i64,
    activity: Option<Activity>,
}

/// The devices `who` can see (or just the one with this `key`) with what their folders hold (as of the last sync, see
/// `folders::totals`), online state and current activity.
pub fn device_summaries(c: &mut Client, who: &Access, key: Option<&str>) -> Result<Vec<DeviceSummary>, postgres::Error> {
    let offline_after = settings::get_i64(c, "offline_after_secs", 60);
    let now = unix_now();
    let (all, user) = who.scope_args();
    let rows = sql::devices::device_summaries().bind(c, &key, &all, &user).all()?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let revoked = r.revoked_at.is_some();
            let online = !revoked && r.last_seen.is_some_and(|t| now - t <= offline_after);
            let fresh = r.activity_at.is_some_and(|t| now - t <= ACTIVITY_FRESH_SECS);
            DeviceSummary {
                name: r.name,
                user: r.user_name,
                key: r.key,
                online,
                revoked,
                created_at: r.created_at,
                hostname: r.hostname,
                os: r.os,
                ip: r.ip,
                client_version: r.client_version,
                disk_total: r.disk_total,
                disk_free: r.disk_free,
                last_seen: r.last_seen,
                last_collected: r.last_collected,
                bytes: r.total_bytes,
                files: r.file_count,
                activity: (online && fresh).then(|| Activity {
                    phase: r.phase.unwrap_or_default(),
                    file: r.current_file.unwrap_or_default(),
                    files_done: r.files_done.unwrap_or_default(),
                    files_total: r.files_total.unwrap_or_default(),
                    bytes_done: r.bytes_done.unwrap_or_default(),
                    bytes_total: r.bytes_total.unwrap_or_default(),
                    speed_bps: r.speed_bps.unwrap_or_default(),
                }),
            }
        })
        .collect())
}

pub async fn list(Ctx { state, who }: Ctx) -> Res<Vec<DeviceSummary>> {
    Ok(Json(run(&state, move |c| Ok(device_summaries(c, &who, None)?)).await?))
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct DeviceDetail {
    device: DeviceSummary,
    events: Vec<Recent>,
    /// The folders attached to the device, as it last said.
    folders: Vec<devices::FolderState>,
    /// Only when a token was just made: shown once.
    token: Option<String>,
}

fn detail_of(c: &mut Client, who: &Access, key: &str, token: Option<String>) -> anyhow::Result<DeviceDetail> {
    let device = device_summaries(c, who, Some(key))?
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::not_found("No such device"))?;
    let folders = match devices::id_of(c, key)? {
        Some(id) => devices::folder_states_of_device(c, id)?,
        None => Vec::new(),
    };
    Ok(DeviceDetail { device, events: recent(c, who, 50, Some(key))?, folders, token })
}

/// A device is named by its key (`<user>/<name>`) in the address, with the slash encoded.
pub async fn detail(Ctx { state, who }: Ctx, Path(key): Path<String>) -> Res<DeviceDetail> {
    Ok(Json(run(&state, move |c| detail_of(c, &who, &key, None)).await?))
}

#[derive(Deserialize)]
pub struct NewDevice {
    name: String,
    /// The user the device is for; only an administrator can name another than themselves.
    #[serde(default)]
    user: Option<String>,
}

/// Adds a device (a client that gets a token). Folders are made in `folders`.
pub async fn create(Ctx { state, who }: Ctx, Json(body): Json<NewDevice>) -> Res<DeviceDetail> {
    who.require_workspace()?;
    let name = input::name(&body.name)?;
    let owner = body.user.map(|u| u.trim().to_lowercase()).filter(|u| *u != who.name);
    if owner.is_some() {
        who.require_admin()?;
    }
    let out = run(&state, move |c| {
        // Every device belongs to a user who has a workspace.
        let owner = owner.unwrap_or_else(|| who.name.clone());
        let (user_id, role) = devices::user_for_device(c, &owner)?.ok_or_else(|| ApiError::bad(format!("There is no user named {owner}")))?;
        if role == "readonly" {
            return Err(ApiError::bad(format!("{owner} can only read the master files and cannot have devices")).into());
        }
        add_device(c, &who, user_id, &name)
    })
    .await?;
    // The directory exists from the start so that a network share shows it.
    let _ = std::fs::create_dir_all(state.store.live_dir(&out.device.key));
    Ok(Json(out))
}

/// Adds a device to a user (who must have a workspace), with a token that is shown once, and tells whoever is looking.
fn add_device(c: &mut Client, who: &Access, user_id: i64, name: &str) -> anyhow::Result<DeviceDetail> {
    let token = db::new_token();
    let Some(key) = devices::insert(c, user_id, name, &db::hash_token(&token), unix_now())? else {
        return Err(ApiError::conflict(format!("There is already a device or folder named {name} for this user")).into());
    };
    db::notify(c, db::Topic::Devices, &key)?;
    detail_of(c, who, &key, Some(token))
}

pub async fn rotate_token(Ctx { state, who }: Ctx, Path(key): Path<String>) -> Res<DeviceDetail> {
    who.check_write(&key)?;
    let out = run(&state, move |c| {
        let token = db::new_token();
        if !devices::set_token(c, &key, &db::hash_token(&token))? {
            return Err(ApiError::not_found("No such device").into());
        }
        db::notify(c, db::Topic::Devices, &key)?;
        detail_of(c, &who, &key, Some(token))
    })
    .await?;
    Ok(Json(out))
}

/// Lets the next client that connects with the token of the device take it over, without waiting for the one that holds
/// it to go quiet (a device was replaced, or the client was reinstalled).
pub async fn release(Ctx { state, who }: Ctx, Path(key): Path<String>) -> Res<Done> {
    who.check_write(&key)?;
    act(&state, move |c| {
        if !devices::release(c, &key)? {
            return Err(ApiError::not_found("No such device").into());
        }
        alerts::resolve(c, "device_conflict", &key)?;
        db::notify(c, db::Topic::Devices, &key)?;
        Ok(())
    })
    .await
}

pub async fn revoke(Ctx { state, who }: Ctx, Path(key): Path<String>) -> Res<Done> {
    who.check_write(&key)?;
    act(&state, move |c| {
        if !devices::revoke(c, &key, unix_now())? {
            return Err(ApiError::not_found("No such device").into());
        }
        // A revoked device is no longer expected to report, so its offline alert would never clear.
        alerts::resolve(c, "device_offline", &key)?;
        db::notify(c, db::Topic::Devices, &key)?;
        Ok(())
    })
    .await
}
