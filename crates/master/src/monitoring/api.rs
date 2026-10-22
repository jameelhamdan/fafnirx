//! The numbers on the dashboard.

use crate::devices::api::{device_summaries, DeviceSummary};
use crate::integrations::api::{job_states, JobState};
use crate::web::{done, run, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::Json;
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

pub fn totals(c: &mut Client) -> Result<Totals, postgres::Error> {
    let live = c.query_one(
        "SELECT COUNT(f.id), COALESCE(SUM(v.size), 0)::BIGINT FROM files f
         JOIN versions v ON v.id = f.current_version_id WHERE f.deleted_at IS NULL",
        &[],
    )?;
    let marked: i64 = c.query_one("SELECT COUNT(*) FROM files WHERE deleted_at IS NOT NULL", &[])?.get(0);
    Ok(Totals { files: live.get(0), bytes: live.get(1), marked_deleted: marked })
}

/// Changes per day for the last two weeks, oldest first, with the days that had none.
fn activity(c: &mut Client) -> Result<Vec<Day>, postgres::Error> {
    let today = unix_now() / 86400;
    let first = today - (DAYS - 1);
    let rows =
        c.query("SELECT ts / 86400, action, COUNT(*) FROM events WHERE ts >= $1 GROUP BY 1, 2", &[&(first * 86400)])?;
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

pub fn recent(c: &mut Client, limit: i64, device: Option<&str>) -> Result<Vec<Recent>, postgres::Error> {
    Ok(c.query(
        "SELECT e.id, e.ts, e.action, e.path, e.detail, d.name FROM events e JOIN devices d ON d.id = e.device_id
         WHERE ($2::text IS NULL OR d.name = $2) ORDER BY e.id DESC LIMIT $1",
        &[&limit, &device],
    )?
    .iter()
    .map(Recent::from_row)
    .collect())
}

pub async fn dashboard(State(state): State<Shared>) -> Res<Dashboard> {
    let data_dir = state.data_dir.clone();
    let out = run(&state, move |c| {
        let devices = device_summaries(c, None)?;
        let online = devices.iter().filter(|d| d.online).count();
        let (sources, mirrors) = job_states(c)?;
        let alerts = c
            .query(
                "SELECT id, kind, key, message, created_at, updated_at FROM alerts WHERE resolved_at IS NULL ORDER BY id DESC",
                &[],
            )?
            .iter()
            .map(|r| Alert {
                id: r.get(0),
                kind: r.get(1),
                key: r.get(2),
                message: r.get(3),
                created_at: r.get(4),
                updated_at: r.get(5),
            })
            .collect();
        let disk = fafnirx_common::disk_usage(&data_dir).filter(|(total, _)| *total > 0).map(|(total, free)| Disk {
            total,
            used: total.saturating_sub(free),
            free,
            percent: 100 * total.saturating_sub(free) / total,
        });
        Ok(Dashboard {
            devices,
            online,
            totals: totals(c)?,
            activity: activity(c)?,
            recent: recent(c, 12, None)?,
            sources,
            mirrors,
            alerts,
            disk,
        })
    })
    .await?;
    Ok(Json(out))
}

pub async fn dismiss(State(state): State<Shared>, Path(id): Path<i64>) -> Res<Done> {
    run(&state, move |c| {
        c.execute("UPDATE alerts SET resolved_at = $2 WHERE id = $1 AND resolved_at IS NULL", &[&id, &unix_now()])?;
        crate::platform::db::notify(c, "alerts", "")?;
        Ok(())
    })
    .await?;
    done()
}
