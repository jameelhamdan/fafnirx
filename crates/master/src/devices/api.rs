//! Devices (clients and local folders), their tokens, and the settings.

use crate::monitoring::api::{recent, Recent};
use crate::platform::db::{self, setting};
use crate::web::{done, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::Json;
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
    pub name: String,
    /// device (a client), local (uploaded or dropped into `files/`) or source (imported by the master).
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

/// Every device (or just `name`) with its totals (as of its last sync, see `totals`), online state and current
/// activity.
pub fn device_summaries(c: &mut Client, name: Option<&str>) -> Result<Vec<DeviceSummary>, postgres::Error> {
    let offline_after = setting(c, "offline_after_secs", 60);
    let now = unix_now();
    let rows = c.query(
        "SELECT d.name, d.kind, d.created_at, d.revoked_at, d.hostname, d.os, d.ip, d.client_version, d.disk_total,
                d.disk_free, d.last_seen, d.last_collected,
                d.total_bytes, d.file_count,
                a.updated_at, a.phase, a.current_file, a.files_done, a.files_total, a.bytes_done, a.bytes_total, a.speed_bps
         FROM devices d
         LEFT JOIN device_activity a ON a.device_id = d.id
         WHERE ($1::text IS NULL OR d.name = $1)
         ORDER BY d.name",
        &[&name],
    )?;
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

pub async fn list(State(state): State<Shared>) -> Res<Vec<DeviceSummary>> {
    Ok(Json(run(&state, |c| Ok(device_summaries(c, None)?)).await?))
}

#[derive(Serialize)]
pub struct DeviceDetail {
    device: DeviceSummary,
    events: Vec<Recent>,
    /// Only when a token was just made: shown once.
    token: Option<String>,
}

fn detail_of(c: &mut Client, name: &str, token: Option<String>) -> anyhow::Result<DeviceDetail> {
    let device = device_summaries(c, Some(name))?
        .into_iter()
        .find(|d| d.kind != "source")
        .ok_or_else(|| ApiError::not_found("No such device"))?;
    Ok(DeviceDetail { device, events: recent(c, 50, Some(name))?, token })
}

pub async fn detail(State(state): State<Shared>, Path(name): Path<String>) -> Res<DeviceDetail> {
    Ok(Json(run(&state, move |c| detail_of(c, &name, None)).await?))
}

#[derive(Deserialize)]
pub struct NewDevice {
    name: String,
    /// `device` for a client that gets a token, `local` for a folder of files uploaded in the browser.
    #[serde(default)]
    kind: Option<String>,
}

pub const NAME_RULE: &str =
    "Name: 1-63 characters, lowercase letters, digits, '.', '_' or '-'; not a reserved Windows name";

pub async fn create(State(state): State<Shared>, Json(body): Json<NewDevice>) -> Res<DeviceDetail> {
    let name = body.name.trim().to_lowercase();
    let kind = body.kind.unwrap_or_else(|| "device".into());
    if !valid_device_name(&name) {
        return Err(ApiError::bad(NAME_RULE));
    }
    if kind != "device" && kind != "local" {
        return Err(ApiError::bad("kind must be device or local"));
    }
    let out = run(&state, move |c| {
        let token = (kind == "device").then(db::new_token);
        let hash = token.as_deref().map_or_else(|| "!".to_string(), db::hash_token);
        let inserted = c.execute(
            "INSERT INTO devices (name, kind, token_hash, created_at) VALUES ($1, $2, $3, $4) ON CONFLICT (name) DO NOTHING",
            &[&name, &kind, &hash, &unix_now()],
        )?;
        if inserted == 0 {
            return Err(ApiError::conflict(format!("A device named {name} already exists")).into());
        }
        db::notify(c, "devices", &name)?;
        detail_of(c, &name, token)
    })
    .await?;
    // The folder exists from the start so that a network share shows it.
    let _ = std::fs::create_dir_all(state.store.live_dir(&out.device.name));
    state.config.nudge();
    Ok(Json(out))
}

pub async fn rotate_token(State(state): State<Shared>, Path(name): Path<String>) -> Res<DeviceDetail> {
    let out = run(&state, move |c| {
        let token = db::new_token();
        let n = c.execute(
            "UPDATE devices SET token_hash = $2, revoked_at = NULL WHERE name = $1 AND kind = 'device'",
            &[&name, &db::hash_token(&token)],
        )?;
        if n == 0 {
            return Err(ApiError::not_found("No such device").into());
        }
        db::notify(c, "devices", &name)?;
        detail_of(c, &name, Some(token))
    })
    .await?;
    state.config.nudge();
    Ok(Json(out))
}

pub async fn revoke(State(state): State<Shared>, Path(name): Path<String>) -> Res<Done> {
    run(&state, move |c| {
        let n = c.execute(
            "UPDATE devices SET revoked_at = COALESCE(revoked_at, $2) WHERE name = $1 AND kind = 'device'",
            &[&name, &unix_now()],
        )?;
        if n == 0 {
            return Err(ApiError::not_found("No such device").into());
        }
        // A revoked device is no longer expected to report, so its offline alert would never clear.
        db::resolve_alert(c, "device_offline", &name)?;
        db::notify(c, "devices", &name)?;
        Ok(())
    })
    .await?;
    state.config.nudge();
    done()
}

#[derive(Serialize, Deserialize)]
pub struct Settings {
    version_count: i64,
    retention_days: i64,
}

pub async fn settings(State(state): State<Shared>) -> Res<Settings> {
    let s = run(&state, |c| {
        Ok(Settings { version_count: setting(c, "version_count", 5), retention_days: setting(c, "retention_days", 30) })
    })
    .await?;
    Ok(Json(s))
}

pub async fn save_settings(State(state): State<Shared>, Json(s): Json<Settings>) -> Res<Settings> {
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
    state.config.nudge();
    Ok(Json(saved))
}
