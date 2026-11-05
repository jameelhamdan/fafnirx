//! Devices (clients and local folders), their tokens, and the settings.

use crate::monitoring::api::{recent, Recent};
use crate::platform::db;
use crate::repo::{alerts, devices, settings};
use crate::web::access::Access;
use crate::web::input;
use crate::web::{act, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::{Extension, Json};
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
    pub folder: String,
    /// device (a client), local (uploaded or dropped into `files/<user>/`) or source (imported by the master).
    #[cfg_attr(test, ts(type = "'device' | 'local' | 'master'"))]
    pub kind: String,
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

/// The devices `who` can see (or just the one with this `folder`) with their totals (as of the last sync, see
/// `totals`), online state and current activity. That includes the master folders (kind `master`), which are not computers.
pub fn device_summaries(c: &mut Client, who: &Access, folder: Option<&str>) -> Result<Vec<DeviceSummary>, postgres::Error> {
    let offline_after = settings::get_i64(c, "offline_after_secs", 60);
    let now = unix_now();
    let (all, user) = who.scope_args();
    let rows = sql::devices::device_summaries().bind(c, &folder, &user, &all).all()?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let revoked = r.revoked_at.is_some();
            let online = r.kind == "device" && !revoked && r.last_seen.is_some_and(|t| now - t <= offline_after);
            let fresh = r.activity_at.is_some_and(|t| now - t <= ACTIVITY_FRESH_SECS);
            DeviceSummary {
                name: r.name,
                user: r.user_name,
                folder: r.folder,
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
                kind: r.kind,
            }
        })
        .collect())
}

/// The devices page lists computers and the user's own folders; the master folders have their place in the list of folders.
pub fn computers_and_folders(c: &mut Client, who: &Access) -> Result<Vec<DeviceSummary>, postgres::Error> {
    Ok(device_summaries(c, who, None)?.into_iter().filter(|d| d.kind != "master").collect())
}

pub async fn list(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<DeviceSummary>> {
    Ok(Json(run(&state, move |c| Ok(computers_and_folders(c, &who)?)).await?))
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct DeviceDetail {
    device: DeviceSummary,
    events: Vec<Recent>,
    /// The folders attached to the computer, as it last said (empty for anything but a client).
    folders: Vec<devices::FolderState>,
    /// Only when a token was just made: shown once.
    token: Option<String>,
}

fn detail_of(c: &mut Client, who: &Access, folder: &str, token: Option<String>) -> anyhow::Result<DeviceDetail> {
    let device = device_summaries(c, who, Some(folder))?
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::not_found("No such device"))?;
    let folders = match devices::by_folder(c, folder)? {
        Some((id, _)) => devices::folder_states_of_device(c, id)?,
        None => Vec::new(),
    };
    Ok(DeviceDetail { device, events: recent(c, who, 50, Some(folder))?, folders, token })
}

/// A device is named by its folder (`<user>/<name>`) in the address, with the slash encoded.
pub async fn detail(State(state): State<Shared>, Extension(who): Extension<Access>, Path(folder): Path<String>) -> Res<DeviceDetail> {
    Ok(Json(run(&state, move |c| detail_of(c, &who, &folder, None)).await?))
}

#[derive(Deserialize)]
pub struct NewDevice {
    name: String,
    /// The user the device is for; only an administrator can name another than themselves.
    #[serde(default)]
    user: Option<String>,
}

/// Adds a computer (a client that gets a token). Folders are made in `folders`.
pub async fn create(State(state): State<Shared>, Extension(who): Extension<Access>, Json(body): Json<NewDevice>) -> Res<DeviceDetail> {
    who.require_workspace()?;
    let name = input::device_name(&body.name)?;
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
    // The folder exists from the start so that a network share shows it.
    let _ = std::fs::create_dir_all(state.store.live_dir(&out.device.folder));
    Ok(Json(out))
}

/// Adds a computer to a user (who must have a workspace), with a token that is shown once, and tells whoever is looking.
fn add_device(c: &mut Client, who: &Access, user_id: i64, name: &str) -> anyhow::Result<DeviceDetail> {
    let token = db::new_token();
    let Some(folder) = devices::insert(c, user_id, name, "device", &db::hash_token(&token), unix_now())? else {
        return Err(ApiError::conflict(format!("There is already a device or folder named {name} for this user")).into());
    };
    db::notify(c, db::Topic::Devices, &folder)?;
    detail_of(c, who, &folder, Some(token))
}

pub async fn rotate_token(State(state): State<Shared>, Extension(who): Extension<Access>, Path(folder): Path<String>) -> Res<DeviceDetail> {
    who.check_write(&folder)?;
    let out = run(&state, move |c| {
        let token = db::new_token();
        if !devices::set_token(c, &folder, &db::hash_token(&token))? {
            return Err(ApiError::not_found("No such device").into());
        }
        db::notify(c, db::Topic::Devices, &folder)?;
        detail_of(c, &who, &folder, Some(token))
    })
    .await?;
    Ok(Json(out))
}

/// Lets the next computer that connects with the token of the device take it over, without waiting for the one that holds
/// it to go quiet (a computer was replaced, or the client was reinstalled).
pub async fn release(State(state): State<Shared>, Extension(who): Extension<Access>, Path(folder): Path<String>) -> Res<Done> {
    who.check_write(&folder)?;
    act(&state, move |c| {
        if !devices::release(c, &folder)? {
            return Err(ApiError::not_found("No such device").into());
        }
        alerts::resolve(c, "device_conflict", &folder)?;
        db::notify(c, db::Topic::Devices, &folder)?;
        Ok(())
    })
    .await
}

pub async fn revoke(State(state): State<Shared>, Extension(who): Extension<Access>, Path(folder): Path<String>) -> Res<Done> {
    who.check_write(&folder)?;
    act(&state, move |c| {
        if !devices::revoke(c, &folder, unix_now())? {
            // A local folder has no token to revoke: nothing to do. Only a name that is no device is an error.
            return if devices::is_device_or_local(c, &folder)? {
                Ok(())
            } else {
                Err(ApiError::not_found("No such device").into())
            };
        }
        // A revoked device is no longer expected to report, so its offline alert would never clear.
        alerts::resolve(c, "device_offline", &folder)?;
        db::notify(c, db::Topic::Devices, &folder)?;
        Ok(())
    })
    .await
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Settings {
    version_count: i64,
    retention_days: i64,
    /// Whether pictures (thumbnails and previews) are made, when somebody looks at a photo or video. Off: none is made.
    thumbnails: bool,
    /// Whether the web app offers the photo pages. Off: their menu items are hidden and the pages closed; nothing else changes.
    photos: bool,
}

/// What a save says. A switch that is left out keeps its value, so a caller that does not know it cannot turn it back on.
#[derive(Deserialize)]
pub struct SettingsChange {
    version_count: i64,
    retention_days: i64,
    thumbnails: Option<bool>,
    photos: Option<bool>,
}

pub async fn settings(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Settings> {
    who.require_admin()?;
    let s = run(&state, |c| {
        Ok(Settings {
            version_count: settings::get_i64(c, "version_count", 5),
            retention_days: settings::get_i64(c, "retention_days", 30),
            thumbnails: settings::get_i64(c, crate::library::meta::THUMBNAILS, 1) != 0,
            photos: settings::photos_on(c),
        })
    })
    .await?;
    Ok(Json(s))
}

pub async fn save_settings(State(state): State<Shared>, Extension(who): Extension<Access>, Json(s): Json<SettingsChange>) -> Res<Settings> {
    who.require_admin()?;
    if !(1..=1000).contains(&s.version_count) || !(1..=3650).contains(&s.retention_days) {
        return Err(ApiError::bad("Versions kept: 1-1000. Retention: 1-3650 days."));
    }
    let saved = run(&state, move |c| {
        let thumbnails = s.thumbnails.unwrap_or_else(|| settings::get_i64(c, crate::library::meta::THUMBNAILS, 1) != 0);
        let photos = s.photos.unwrap_or_else(|| settings::photos_on(c));
        for (key, value) in [
            ("version_count", s.version_count),
            ("retention_days", s.retention_days),
            (crate::library::meta::THUMBNAILS, i64::from(thumbnails)),
            (settings::PHOTOS, i64::from(photos)),
        ] {
            settings::set(c, key, &value.to_string())?;
        }
        crate::library::meta::set_thumbnails(thumbnails);
        Ok(Settings { version_count: s.version_count, retention_days: s.retention_days, thumbnails, photos })
    })
    .await?;
    Ok(Json(saved))
}

#[derive(Deserialize)]
pub struct Rename {
    name: String,
}

#[derive(Serialize)]
pub struct Renamed {
    /// The folder under its new name.
    folder: String,
}

/// Renames a folder of the user (a local or shared one). Its files, albums, sources and members stay with it.
pub async fn rename(State(state): State<Shared>, Extension(who): Extension<Access>, Path(folder): Path<String>, Json(body): Json<Rename>) -> Res<Renamed> {
    who.require_workspace()?;
    let store = state.store.clone();
    let folder = run(&state, move |c| super::rename::rename(c, &store, &who, &folder, &body.name)).await?;
    Ok(Json(Renamed { folder }))
}
