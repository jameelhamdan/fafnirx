//! Sources: folders of another service (Google Drive) the master collects on a schedule. A source owns a device of
//! kind `source`, so its files live in the same tables as a client's.

use crate::platform::pg::Pg;
use fafnirx_db::client::Params;
use fafnirx_db::queries::sources as q;
use postgres::{Error, GenericClient};

/// A source with its schedule, last run and what it holds.
pub type Listed = q::ListSources;
/// A source that was due and has been marked running.
pub type Claimed = q::ClaimSource;

/// The sources of `user` (everyone's when `all`).
pub fn list(c: &mut impl GenericClient, all: bool, user: i64) -> Result<Vec<Listed>, Error> {
    q::list_sources().bind(&mut Pg(c), &all, &user).all()
}

/// The id and device folder of the source `name` of the user `owner`.
pub fn by_owner(c: &mut impl GenericClient, owner: &str, name: &str) -> Result<Option<(i64, String)>, Error> {
    Ok(q::source_by_owner().bind(&mut Pg(c), &owner, &name).opt()?.map(|s| (s.id, s.folder)))
}

/// Adds the device a source stores its files in. `None` when the user already has one of that name.
pub fn insert_device(
    c: &mut impl GenericClient,
    user_id: i64,
    name: &str,
    now: i64,
) -> Result<Option<(i64, String)>, Error> {
    Ok(q::insert_source_device().bind(&mut Pg(c), &name, &now, &user_id).opt()?.map(|d| (d.id, d.folder)))
}

pub fn insert(
    c: &mut impl GenericClient,
    device_id: i64,
    name: &str,
    config: &str,
    interval_secs: i64,
    now: i64,
) -> Result<(), Error> {
    q::insert_source().params(&mut Pg(c), &q::InsertSourceParams { name, config, interval_secs, device_id, now })?;
    Ok(())
}

/// The device id and folder of a source.
pub fn device_of(c: &mut impl GenericClient, id: i64) -> Result<Option<(i64, String)>, Error> {
    Ok(q::source_device().bind(&mut Pg(c), &id).opt()?.map(|s| (s.device_id, s.folder)))
}

/// Forgets a source and what it remembered of the remote files.
pub fn delete(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::delete_source_items().bind(&mut Pg(&mut *c), &id)?;
    q::delete_source().bind(&mut Pg(c), &id)?;
    Ok(())
}

/// Asks for a run as soon as the worker looks.
pub fn request_run(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::request_source_run().bind(&mut Pg(c), &id)?;
    Ok(())
}

pub fn toggle(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::toggle_source().bind(&mut Pg(c), &id)?;
    Ok(())
}

/// Picks one due source and marks it running. One statement, so a source can only be claimed once.
pub fn claim(c: &mut impl GenericClient, now: i64) -> Result<Option<Claimed>, Error> {
    q::claim_source().bind(&mut Pg(c), &now).opt()
}

/// Records how a run went; `ok_at` is when it worked, if it did.
pub fn finish(
    c: &mut impl GenericClient,
    id: i64,
    status: &str,
    message: &str,
    ok_at: Option<i64>,
) -> Result<(), Error> {
    q::finish_source().params(&mut Pg(c), &q::FinishSourceParams { status, message, ok_at, id })?;
    Ok(())
}

/// Remembers a remote file: where it is stored and the version of it that was collected. Brings back a file that
/// had been removed in the web app, since it changed at the source.
pub fn remember_item(
    c: &mut impl GenericClient,
    source_id: i64,
    remote_id: &str,
    path: &str,
    remote_version: &str,
) -> Result<(), Error> {
    q::upsert_source_item()
        .params(&mut Pg(c), &q::UpsertSourceItemParams { source_id, remote_id, path, remote_version })?;
    Ok(())
}

/// What was collected from the remote, as (remote id, path, version).
pub fn items(c: &mut impl GenericClient, source_id: i64) -> Result<Vec<(String, String, String)>, Error> {
    Ok(q::source_items()
        .bind(&mut Pg(c), &source_id)
        .all()?
        .into_iter()
        .map(|i| (i.remote_id, i.path, i.remote_version))
        .collect())
}

/// The live paths of a device.
pub fn live_paths(c: &mut impl GenericClient, device_id: i64) -> Result<Vec<String>, Error> {
    q::live_paths().bind(&mut Pg(c), &device_id).all()
}

/// Paths deleted in the web app: still in the trash, or already removed by retention (then only the record of the
/// source remembers it).
pub fn removed_in_web_app(c: &mut impl GenericClient, device_id: i64, source_id: i64) -> Result<Vec<String>, Error> {
    q::removed_in_web_app().params(&mut Pg(c), &q::RemovedInWebAppParams { device_id, source_id }).all()
}

pub fn forget_item(c: &mut impl GenericClient, source_id: i64, remote_id: &str) -> Result<(), Error> {
    q::delete_source_item().bind(&mut Pg(c), &source_id, &remote_id)?;
    Ok(())
}
