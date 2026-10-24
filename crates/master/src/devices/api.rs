//! Devices (clients and local folders), their tokens, and the settings.

use crate::monitoring::api::{recent, Recent};
use crate::platform::db::{self, setting};
use crate::web::access::{scope, Access};
use crate::web::{done, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use fafnirx_common::{unix_now, valid_device_name};
use postgres::Client;
use serde::{Deserialize, Serialize};

/// A device sends progress every few seconds while it uploads. Older than this, it has stopped or gone away.
const ACTIVITY_FRESH_SECS: i64 = 15;

#[derive(Serialize)]
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
pub struct DeviceSummary {
    /// The name of the device; it is unique among the devices of its user.
    pub name: String,
    /// The user the device belongs to.
    pub user: String,
    /// `<user>/<name>`: how the files API, the library and the live updates name the device.
    pub folder: String,
    /// device (a client), local (uploaded or dropped into `files/<user>/`) or source (imported by the master).
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
/// `totals`), online state and current activity. The master files are not a device and are not listed.
pub fn device_summaries(
    c: &mut Client,
    who: &Access,
    folder: Option<&str>,
) -> Result<Vec<DeviceSummary>, postgres::Error> {
    let offline_after = setting(c, "offline_after_secs", 60);
    let now = unix_now();
    let (all, user) = who.scope_args();
    let sql = format!(
        "SELECT d.name, d.kind, d.created_at, d.revoked_at, d.hostname, d.os, d.ip, d.client_version, d.disk_total,
                d.disk_free, d.last_seen, d.last_collected,
                d.total_bytes, d.file_count,
                a.updated_at, a.phase, a.current_file, a.files_done, a.files_total, a.bytes_done, a.bytes_total, a.speed_bps,
                u.name, d.folder
         FROM devices d
         JOIN users u ON u.id = d.user_id
         LEFT JOIN device_activity a ON a.device_id = d.id
         WHERE ($1::text IS NULL OR d.folder = $1) AND {}
         ORDER BY u.name, d.name",
        scope(2, 3)
    );
    let rows = c.query(&sql, &[&folder, &all, &user])?;
    Ok(rows
        .iter()
        .map(|r| {
            let kind: String = r.get(1);
            let revoked = r.get::<_, Option<i64>>(3).is_some();
            let last_seen: Option<i64> = r.get(10);
            let online = kind == "device" && !revoked && last_seen.is_some_and(|t| now - t <= offline_after);
            let fresh = r.get::<_, Option<i64>>(14).is_some_and(|t| now - t <= ACTIVITY_FRESH_SECS);
            DeviceSummary {
                name: r.get(0),
                user: r.get(22),
                folder: r.get(23),
                online,
                revoked,
                created_at: r.get(2),
                hostname: r.get(4),
                os: r.get(5),
                ip: r.get(6),
                client_version: r.get(7),
                disk_total: r.get(8),
                disk_free: r.get(9),
                last_seen,
                last_collected: r.get(11),
                bytes: r.get(12),
                files: r.get(13),
                activity: (online && fresh).then(|| Activity {
                    phase: r.get(15),
                    file: r.get(16),
                    files_done: r.get(17),
                    files_total: r.get(18),
                    bytes_done: r.get(19),
                    bytes_total: r.get(20),
                    speed_bps: r.get(21),
                }),
                kind,
            }
        })
        .collect())
}

pub async fn list(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<DeviceSummary>> {
    Ok(Json(run(&state, move |c| Ok(device_summaries(c, &who, None)?)).await?))
}

#[derive(Serialize)]
pub struct DeviceDetail {
    device: DeviceSummary,
    events: Vec<Recent>,
    /// Only when a token was just made: shown once.
    token: Option<String>,
}

fn detail_of(c: &mut Client, who: &Access, folder: &str, token: Option<String>) -> anyhow::Result<DeviceDetail> {
    let device = device_summaries(c, who, Some(folder))?
        .into_iter()
        .find(|d| d.kind != "source")
        .ok_or_else(|| ApiError::not_found("No such device"))?;
    Ok(DeviceDetail { device, events: recent(c, who, 50, Some(folder))?, token })
}

/// A device is named by its folder (`<user>/<name>`) in the address, with the slash encoded.
pub async fn detail(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(folder): Path<String>,
) -> Res<DeviceDetail> {
    Ok(Json(run(&state, move |c| detail_of(c, &who, &folder, None)).await?))
}

#[derive(Deserialize)]
pub struct NewDevice {
    name: String,
    /// `device` for a client that gets a token, `local` for a folder of files uploaded in the browser.
    #[serde(default)]
    kind: Option<String>,
    /// The user the device is for; only an administrator can name another than themselves.
    #[serde(default)]
    user: Option<String>,
}

pub const NAME_RULE: &str =
    "Name: 1-63 characters, lowercase letters, digits, '.', '_' or '-'; not a reserved Windows name";

pub async fn create(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(body): Json<NewDevice>,
) -> Res<DeviceDetail> {
    who.require_workspace()?;
    let name = body.name.trim().to_lowercase();
    let kind = body.kind.unwrap_or_else(|| "device".into());
    if !valid_device_name(&name) {
        return Err(ApiError::bad(NAME_RULE));
    }
    if kind != "device" && kind != "local" {
        return Err(ApiError::bad("kind must be device or local"));
    }
    let owner = body.user.map(|u| u.trim().to_lowercase()).filter(|u| *u != who.name);
    if owner.is_some() {
        who.require_admin()?;
    }
    let out = run(&state, move |c| {
        // Every device belongs to a user who has a workspace.
        let owner = owner.unwrap_or_else(|| who.name.clone());
        let row = c
            .query_opt("SELECT id, role FROM users WHERE name = $1 AND disabled_at IS NULL", &[&owner])?
            .ok_or_else(|| ApiError::bad(format!("There is no user named {owner}")))?;
        let (user_id, role): (i64, String) = (row.get(0), row.get(1));
        if role == "readonly" {
            return Err(ApiError::bad(format!("{owner} can only read the master files and cannot have devices")).into());
        }
        add_device(c, &who, user_id, &name, &kind)
    })
    .await?;
    // The folder exists from the start so that a network share shows it.
    let _ = std::fs::create_dir_all(state.store.live_dir(&out.device.folder));
    Ok(Json(out))
}

/// Adds a device to a user (who must have a workspace) and tells whoever is looking.
fn add_device(c: &mut Client, who: &Access, user_id: i64, name: &str, kind: &str) -> anyhow::Result<DeviceDetail> {
    let token = (kind == "device").then(db::new_token);
    let hash = token.as_deref().map_or_else(|| "!".to_string(), db::hash_token);
    let inserted = c.query_opt(
        "INSERT INTO devices (name, kind, token_hash, created_at, user_id) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (user_id, name) DO NOTHING RETURNING folder",
        &[&name, &kind, &hash, &unix_now(), &user_id],
    )?;
    let Some(row) = inserted else {
        return Err(ApiError::conflict(format!("There is already a device named {name} for this user")).into());
    };
    let folder: String = row.get(0);
    db::notify(c, "devices", &folder)?;
    detail_of(c, who, &folder, token)
}

pub async fn rotate_token(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(folder): Path<String>,
) -> Res<DeviceDetail> {
    who.check_write(&folder)?;
    let out = run(&state, move |c| {
        let token = db::new_token();
        let n = c.execute(
            "UPDATE devices SET token_hash = $2, revoked_at = NULL, instance_id = NULL WHERE folder = $1 AND kind = 'device'",
            &[&folder, &db::hash_token(&token)],
        )?;
        if n == 0 {
            return Err(ApiError::not_found("No such device").into());
        }
        db::notify(c, "devices", &folder)?;
        detail_of(c, &who, &folder, Some(token))
    })
    .await?;
    Ok(Json(out))
}

/// Lets the next computer that connects with the token of the device take it over, without waiting for the one that holds
/// it to go quiet (a computer was replaced, or the client was reinstalled).
pub async fn release(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(folder): Path<String>,
) -> Res<Done> {
    who.check_write(&folder)?;
    run(&state, move |c| {
        let n = c.execute("UPDATE devices SET instance_id = NULL WHERE folder = $1 AND kind = 'device'", &[&folder])?;
        if n == 0 {
            return Err(ApiError::not_found("No such device").into());
        }
        db::resolve_alert(c, "device_conflict", &folder)?;
        db::notify(c, "devices", &folder)?;
        Ok(())
    })
    .await?;
    done()
}

pub async fn revoke(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(folder): Path<String>,
) -> Res<Done> {
    who.check_write(&folder)?;
    run(&state, move |c| {
        let n = c.execute(
            "UPDATE devices SET revoked_at = COALESCE(revoked_at, $2) WHERE folder = $1 AND kind = 'device'",
            &[&folder, &unix_now()],
        )?;
        if n == 0 {
            // A local folder has no token to revoke: nothing to do. Only a name that is no device is an error.
            let exists =
                c.query_opt("SELECT 1 FROM devices WHERE folder = $1 AND kind IN ('device', 'local')", &[&folder])?;
            return exists.map(|_| ()).ok_or_else(|| ApiError::not_found("No such device").into());
        }
        // A revoked device is no longer expected to report, so its offline alert would never clear.
        db::resolve_alert(c, "device_offline", &folder)?;
        db::notify(c, "devices", &folder)?;
        Ok(())
    })
    .await?;
    done()
}

#[derive(Serialize, Deserialize)]
pub struct Settings {
    version_count: i64,
    retention_days: i64,
}

pub async fn settings(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Settings> {
    who.require_admin()?;
    let s = run(&state, |c| {
        Ok(Settings { version_count: setting(c, "version_count", 5), retention_days: setting(c, "retention_days", 30) })
    })
    .await?;
    Ok(Json(s))
}

pub async fn save_settings(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(s): Json<Settings>,
) -> Res<Settings> {
    who.require_admin()?;
    if !(1..=1000).contains(&s.version_count) || !(1..=3650).contains(&s.retention_days) {
        return Err(ApiError::bad("Versions kept: 1-1000. Retention: 1-3650 days."));
    }
    let saved = run(&state, move |c| {
        for (key, value) in [("version_count", s.version_count), ("retention_days", s.retention_days)] {
            c.execute(
                "INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                &[&key, &value.to_string()],
            )?;
        }
        Ok(s)
    })
    .await?;
    Ok(Json(saved))
}
