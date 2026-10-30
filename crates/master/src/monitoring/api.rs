//! The numbers on the dashboard.

use crate::devices::api::{device_summaries, DeviceSummary};
use crate::integrations::api::{job_states, JobState};
use crate::repo::{alerts, devices, events};
use crate::web::access::Access;
use crate::web::{act, run, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use fafnirx_common::unix_now;
use postgres::Client;
use serde::Serialize;

const DAYS: i64 = 14;

#[derive(Serialize)]
pub struct Totals {
    files: i64,
    bytes: i64,
    marked_deleted: i64,
}

#[derive(Serialize)]
pub struct Day {
    /// Days since 1970-01-01.
    day: i64,
    added: i64,
    updated: i64,
    renamed: i64,
    deleted: i64,
}

#[derive(Serialize)]
pub struct Recent {
    id: i64,
    ts: i64,
    action: String,
    path: String,
    detail: Option<String>,
    device: String,
}

impl From<events::Recent> for Recent {
    fn from(e: events::Recent) -> Self {
        Self { id: e.id, ts: e.ts, action: e.action, path: e.path, detail: e.detail, device: e.device }
    }
}

#[derive(Serialize)]
pub struct Alert {
    id: crate::platform::id::Id,
    kind: String,
    key: String,
    message: String,
    created_at: i64,
    updated_at: Option<i64>,
}

#[derive(Serialize)]
pub struct Disk {
    total: u64,
    used: u64,
    free: u64,
    percent: u64,
}

#[derive(Serialize)]
pub struct Dashboard {
    devices: Vec<DeviceSummary>,
    online: usize,
    totals: Totals,
    activity: Vec<Day>,
    recent: Vec<Recent>,
    sources: Vec<JobState>,
    mirrors: Vec<JobState>,
    alerts: Vec<Alert>,
    disk: Option<Disk>,
}

/// What the devices `who` can see hold (the master files included).
pub fn totals(c: &mut Client, who: &Access) -> Result<Totals, postgres::Error> {
    // Kept on the device rows, as of each device's last sync.
    let (all, user) = who.scope_args();
    let (files, bytes, marked_deleted) = devices::visible_totals(c, all, user)?;
    Ok(Totals { files, bytes, marked_deleted })
}

/// Changes per day for the last two weeks, oldest first, with the days that had none.
fn activity(c: &mut Client, who: &Access) -> Result<Vec<Day>, postgres::Error> {
    let today = unix_now() / 86400;
    let first = today - (DAYS - 1);
    let (all, user) = who.scope_args();
    let rows = events::activity_by_day(c, first * 86400, all, user)?;
    let mut days: Vec<Day> =
        (first..=today).map(|day| Day { day, added: 0, updated: 0, renamed: 0, deleted: 0 }).collect();
    for (day, action, n) in rows {
        if let Some(d) = days.get_mut((day - first) as usize) {
            match action.as_str() {
                "add" => d.added += n,
                "update" => d.updated += n,
                "rename" => d.renamed += n,
                "delete" => d.deleted += n,
                _ => {}
            }
        }
    }
    Ok(days)
}

/// The latest changes in the devices `who` can see, or only in the device with this folder.
pub fn recent(c: &mut Client, who: &Access, limit: i64, folder: Option<&str>) -> Result<Vec<Recent>, postgres::Error> {
    let (all, user) = who.scope_args();
    Ok(events::recent(c, folder, all, user, limit)?.into_iter().map(Recent::from).collect())
}

pub async fn dashboard(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Dashboard> {
    let data_dir = state.data_dir.clone();
    let out = run(&state, move |c| {
        let devices = device_summaries(c, &who, None)?;
        let online = devices.iter().filter(|d| d.online).count();
        // Sources are per user (an administrator sees all); mirrors belong to the administrators.
        let (sources, mirrors) = if who.has_workspace() { job_states(c, &who)? } else { Default::default() };
        // The alerts of a device are for whoever can see the device; the others are for the administrators.
        let alerts = alerts::open(c)?
            .into_iter()
            .filter(|a| who.is_admin() || who.can_see(&a.key))
            .map(|a| Alert {
                id: crate::platform::id::Id(a.id),
                kind: a.kind,
                key: a.key,
                message: a.message,
                created_at: a.created_at,
                updated_at: a.updated_at,
            })
            .collect();
        let disk = fafnirx_common::disk_usage(&data_dir).filter(|(total, _)| *total > 0 && who.is_admin()).map(
            |(total, free)| Disk {
                total,
                used: total.saturating_sub(free),
                free,
                percent: 100 * total.saturating_sub(free) / total,
            },
        );
        Ok(Dashboard {
            devices,
            online,
            totals: totals(c, &who)?,
            activity: activity(c, &who)?,
            recent: recent(c, &who, 12, None)?,
            sources,
            mirrors,
            alerts,
            disk,
        })
    })
    .await?;
    Ok(Json(out))
}

pub async fn dismiss(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(crate::platform::id::Id(id)): Path<crate::platform::id::Id>,
) -> Res<Done> {
    act(&state, move |c| {
        // Only the alerts of a device the user can see.
        let key = alerts::key_of(c, id)?;
        if key.is_some_and(|k| !who.is_admin() && !who.can_see(&k)) {
            return Err(crate::web::ApiError::not_found("No such alert").into());
        }
        alerts::dismiss(c, id)?;
        Ok(())
    })
    .await
}
