//! The log of what happened to files (added, updated, renamed, deleted), as shown on the dashboard and device pages.

use crate::platform::pg::Pg;
use fafnirx_db::queries::events as q;
use postgres::{Error, GenericClient};

/// An event with the folder of its device.
pub struct Recent {
    pub id: i64,
    pub ts: i64,
    pub action: String,
    pub path: String,
    pub detail: Option<String>,
    pub device: String,
}

/// Changes per day since `since`, as (day number, action, count); `all`/`user` say which devices are visible.
pub fn activity_by_day(
    c: &mut impl GenericClient,
    since: i64,
    all: bool,
    user: i64,
) -> Result<Vec<(i64, String, i64)>, Error> {
    Ok(q::activity_by_day()
        .bind(&mut Pg(c), &since, &user, &all)
        .all()?
        .into_iter()
        .map(|d| (d.day, d.action, d.n))
        .collect())
}

/// The latest events of the visible devices, or only of the device with this folder.
pub fn recent(
    c: &mut impl GenericClient,
    folder: Option<&str>,
    all: bool,
    user: i64,
    limit: i64,
) -> Result<Vec<Recent>, Error> {
    Ok(q::recent_events()
        .bind(&mut Pg(c), &folder, &user, &all, &limit)
        .all()?
        .into_iter()
        .map(|e| Recent { id: e.id, ts: e.ts, action: e.action, path: e.path, detail: e.detail, device: e.device })
        .collect())
}

/// Forgets events older than `before`.
pub fn purge(c: &mut impl GenericClient, before: i64) -> Result<(), Error> {
    q::purge_events().bind(&mut Pg(c), &before).map(drop)
}

/// Appends to the change log shown in the web app.
pub fn record(
    c: &mut impl GenericClient,
    device_id: i64,
    path: &str,
    action: &str,
    ts: i64,
    hash: Option<&str>,
    detail: Option<&str>,
) -> Result<(), Error> {
    use fafnirx_db::client::Params;
    let params = q::InsertEventParams { device_id, path, action, ts, hash, detail };
    q::insert_event().params(&mut Pg(c), &params).map(drop)
}

/// The latest events of one path of a device.
pub fn of_file(c: &mut impl GenericClient, device_id: i64, path: &str, limit: i64) -> Result<Vec<Recent>, Error> {
    Ok(q::file_events()
        .bind(&mut Pg(c), &device_id, &path, &limit)
        .all()?
        .into_iter()
        .map(|e| Recent { id: e.id, ts: e.ts, action: e.action, path: e.path, detail: e.detail, device: e.device })
        .collect())
}
