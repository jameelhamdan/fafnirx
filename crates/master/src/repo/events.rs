//! The log of what happened to files (added, updated, renamed, deleted), as shown on the dashboard and folder pages.

use fafnirx_db::client::GenericClient;
use fafnirx_db::queries::events as q;
use postgres::Error;

/// An event with the key of its folder.
pub struct Recent {
    pub id: i64,
    pub ts: i64,
    pub action: String,
    pub path: String,
    pub detail: Option<String>,
    pub folder: String,
}

/// Changes per day since `since`, as (day number, action, count); `all`/`user` say which folders are visible.
pub fn activity_by_day(c: &mut impl GenericClient, since: i64, all: bool, user: i64) -> Result<Vec<(i64, String, i64)>, Error> {
    Ok(q::activity_by_day()
        .bind(c, &since, &user, &all)
        .all()?
        .into_iter()
        .map(|d| (d.day, d.action, d.n))
        .collect())
}

/// The latest events of the visible folders, or only of the folder with this folder.
pub fn recent(c: &mut impl GenericClient, folder: Option<&str>, all: bool, user: i64, limit: i64) -> Result<Vec<Recent>, Error> {
    Ok(q::recent_events()
        .bind(c, &folder, &user, &all, &limit)
        .all()?
        .into_iter()
        .map(|e| Recent { id: e.id, ts: e.ts, action: e.action, path: e.path, detail: e.detail, folder: e.folder })
        .collect())
}

/// Appends to the change log shown in the web app.
pub fn record(c: &mut impl GenericClient, folder_id: i64, e: &NewEvent) -> Result<(), Error> {
    use fafnirx_db::client::Params;
    let params = q::InsertEventParams { folder_id, path: e.path, action: e.action, ts: e.ts, hash: e.hash, detail: e.detail };
    q::insert_event().params(c, &params).map(drop)
}

/// What is appended to the log of a folder.
pub struct NewEvent<'a> {
    pub path: &'a str,
    pub action: &'a str,
    pub ts: i64,
    pub hash: Option<&'a str>,
    pub detail: Option<&'a str>,
}

/// The latest events of one path of a folder.
pub fn of_file(c: &mut impl GenericClient, folder_id: i64, path: &str, limit: i64) -> Result<Vec<Recent>, Error> {
    Ok(q::file_events()
        .bind(c, &folder_id, &path, &limit)
        .all()?
        .into_iter()
        .map(|e| Recent { id: e.id, ts: e.ts, action: e.action, path: e.path, detail: e.detail, folder: e.folder })
        .collect())
}
