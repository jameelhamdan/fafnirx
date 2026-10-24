//! The numbers on the dashboard.

use crate::devices::api::{device_summaries, DeviceSummary};
use crate::integrations::api::{job_states, JobState};
use crate::web::access::{scope, Access};
use crate::web::{done, run, Done, Res, Shared};
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

impl Recent {
    /// From a row of `id, ts, action, path, detail, device name`.
    pub fn from_row(r: &postgres::Row) -> Self {
        Self { id: r.get(0), ts: r.get(1), action: r.get(2), path: r.get(3), detail: r.get(4), device: r.get(5) }
    }
}

#[derive(Serialize)]
pub struct Alert {
    id: i64,
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
    let sums = c.query_one(
        &format!(
            "SELECT COALESCE(SUM(file_count), 0)::BIGINT, COALESCE(SUM(total_bytes), 0)::BIGINT,
                    COALESCE(SUM(deleted_files), 0)::BIGINT FROM devices d WHERE {}",
            scope(1, 2)
        ),
        &[&all, &user],
    )?;
    Ok(Totals { files: sums.get(0), bytes: sums.get(1), marked_deleted: sums.get(2) })
}

/// Changes per day for the last two weeks, oldest first, with the days that had none.
fn activity(c: &mut Client, who: &Access) -> Result<Vec<Day>, postgres::Error> {
    let today = unix_now() / 86400;
    let first = today - (DAYS - 1);
    let (all, user) = who.scope_args();
    let rows = c.query(
        &format!(
            "SELECT e.ts / 86400, e.action, COUNT(*) FROM events e JOIN devices d ON d.id = e.device_id
             WHERE e.ts >= $1 AND {} GROUP BY 1, 2",
            scope(2, 3)
        ),
        &[&(first * 86400), &all, &user],
    )?;
    let mut days: Vec<Day> = (first..=today).map(|day| Day { day, added: 0, updated: 0, deleted: 0 }).collect();
    for r in rows {
        let (day, action, n): (i64, String, i64) = (r.get(0), r.get(1), r.get(2));
        if let Some(d) = days.get_mut((day - first) as usize) {
            match action.as_str() {
                "add" | "rename" => d.added += n,
                "update" => d.updated += n,
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
    Ok(c.query(
        &format!(
            "SELECT e.id, e.ts, e.action, e.path, e.detail, d.folder FROM events e JOIN devices d ON d.id = e.device_id
             WHERE ($2::text IS NULL OR d.folder = $2) AND {} ORDER BY e.id DESC LIMIT $1",
            scope(3, 4)
        ),
        &[&limit, &folder, &all, &user],
    )?
    .iter()
    .map(Recent::from_row)
    .collect())
}

pub async fn dashboard(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Dashboard> {
    let data_dir = state.data_dir.clone();
    let out = run(&state, move |c| {
        let devices = device_summaries(c, &who, None)?;
        let online = devices.iter().filter(|d| d.online).count();
        // Sources are per user (an administrator sees all); mirrors belong to the administrators.
        let (sources, mirrors) = if who.has_workspace() { job_states(c, &who)? } else { Default::default() };
        // The alerts of a device are for whoever can see the device; the others are for the administrators.
        let alerts = c
            .query(
                "SELECT id, kind, key, message, created_at, updated_at FROM alerts WHERE resolved_at IS NULL ORDER BY id DESC",
                &[],
            )?
            .iter()
            .filter(|r| who.is_admin() || who.can_see(r.get::<_, &str>(2)))
            .map(|r| Alert {
                id: r.get(0),
                kind: r.get(1),
                key: r.get(2),
                message: r.get(3),
                created_at: r.get(4),
                updated_at: r.get(5),
            })
            .collect();
        let disk = fafnirx_common::disk_usage(&data_dir)
            .filter(|(total, _)| *total > 0 && who.is_admin())
            .map(|(total, free)| Disk {
                total,
                used: total.saturating_sub(free),
                free,
                percent: 100 * total.saturating_sub(free) / total,
            });
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

pub async fn dismiss(State(state): State<Shared>, Extension(who): Extension<Access>, Path(id): Path<i64>) -> Res<Done> {
    run(&state, move |c| {
        // Only the alerts of a device the user can see.
        let key: Option<String> = c.query_opt("SELECT key FROM alerts WHERE id = $1", &[&id])?.map(|r| r.get(0));
        if key.is_some_and(|k| !who.is_admin() && !who.can_see(&k)) {
            return Err(crate::web::ApiError::not_found("No such alert").into());
        }
        c.execute("UPDATE alerts SET resolved_at = $2 WHERE id = $1 AND resolved_at IS NULL", &[&id, &unix_now()])?;
        crate::platform::db::notify(c, "alerts", "")?;
        Ok(())
    })
    .await?;
    done()
}
