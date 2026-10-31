//! Sources: folders of another storage (via rclone) the master collects on a schedule. A source owns a device of
//! kind `source`, so its files live in the same tables as a client's.

use fafnirx_db::client::GenericClient;
use fafnirx_db::client::Params;
use fafnirx_db::queries::sources as q;
use postgres::Error;

/// The id and device folder of the source `name` of the user `owner`.
pub fn by_owner(c: &mut impl GenericClient, owner: &str, name: &str) -> Result<Option<(i64, String)>, Error> {
    Ok(q::source_by_owner().bind(c, &owner, &name).opt()?.map(|s| (s.id, s.folder)))
}

/// Adds the device a source stores its files in. `None` when the user already has one of that name.
pub fn insert_device(
    c: &mut impl GenericClient,
    user_id: i64,
    name: &str,
    now: i64,
) -> Result<Option<(i64, String)>, Error> {
    Ok(q::insert_source_device().bind(c, &name, &now, &user_id).opt()?.map(|d| (d.id, d.folder)))
}

pub fn insert(
    c: &mut impl GenericClient,
    device_id: i64,
    name: &str,
    kind: &str,
    config: &str,
    interval_secs: i64,
    now: i64,
) -> Result<(), Error> {
    q::insert_source().params(c, &q::InsertSourceParams { name, kind, config, interval_secs, device_id, now })?;
    Ok(())
}

/// The device id and folder of a source.
pub fn device_of(c: &mut impl GenericClient, id: i64) -> Result<Option<(i64, String)>, Error> {
    Ok(q::source_device().bind(c, &id).opt()?.map(|s| (s.device_id, s.folder)))
}

/// Forgets a source and what it remembered of the remote files.
pub fn delete(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::delete_source_items().bind(&mut *c, &id)?;
    q::delete_source().bind(c, &id).map(drop)
}

/// Records how a run went; `ok_at` is when it worked, if it did.
pub fn finish(
    c: &mut impl GenericClient,
    id: i64,
    status: &str,
    message: &str,
    ok_at: Option<i64>,
) -> Result<(), Error> {
    q::finish_source().params(c, &q::FinishSourceParams { status, message, ok_at, id }).map(drop)
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
    q::upsert_source_item().params(c, &q::UpsertSourceItemParams { source_id, remote_id, path, remote_version })?;
    Ok(())
}

/// What was collected from the remote, as (remote id, path, version).
pub fn items(c: &mut impl GenericClient, source_id: i64) -> Result<Vec<(String, String, String)>, Error> {
    Ok(q::source_items()
        .bind(c, &source_id)
        .all()?
        .into_iter()
        .map(|i| (i.remote_id, i.path, i.remote_version))
        .collect())
}

/// Paths deleted in the web app: still in the trash, or already removed by retention (then only the record of the
/// source remembers it).
pub fn removed_in_web_app(c: &mut impl GenericClient, device_id: i64, source_id: i64) -> Result<Vec<String>, Error> {
    q::removed_in_web_app().params(c, &q::RemovedInWebAppParams { device_id, source_id }).all()
}

/// Marks the sources that were running when the master stopped as failed.
pub fn fail_interrupted(c: &mut impl GenericClient) -> Result<(), Error> {
    q::fail_interrupted_sources().bind(c).map(drop)
}
