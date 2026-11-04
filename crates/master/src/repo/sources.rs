//! Sources: folders of another storage the master collects on a schedule. A source writes into a folder of its owner's
//! choosing (a device folder and a subfolder of it), so its files live in the same tables as a client's.

use fafnirx_db::client::GenericClient;
use fafnirx_db::client::Params;
use fafnirx_db::queries::sources as q;
use postgres::Error;

/// The id and device folder of the source `name` of the user `owner`.
pub fn by_owner(c: &mut impl GenericClient, owner: &str, name: &str) -> Result<Option<(i64, String)>, Error> {
    Ok(q::source_by_owner().bind(c, &owner, &name).opt()?.map(|s| (s.id, s.folder)))
}

/// Adds the local folder a source stores its files in when its owner named none. `None` when the user already has one of that name.
pub fn insert_device(c: &mut impl GenericClient, user_id: i64, name: &str, now: i64) -> Result<Option<(i64, String)>, Error> {
    Ok(q::insert_source_device().bind(c, &name, &now, &user_id).opt()?.map(|d| (d.id, d.folder)))
}

/// Where a source writes: the device, below which `prefix` is the subfolder (empty, or a path and a slash).
#[derive(Clone, Debug, PartialEq)]
pub struct Destination {
    pub device_id: i64,
    pub folder: String,
    pub prefix: String,
}

/// What a new source is made of.
pub struct New<'a> {
    pub user_id: i64,
    pub name: &'a str,
    pub kind: &'a str,
    pub config: &'a str,
    pub interval_secs: i64,
    pub device_id: i64,
    pub prefix: &'a str,
}

pub fn insert(c: &mut impl GenericClient, s: &New, now: i64) -> Result<(), Error> {
    let params = q::InsertSourceParams {
        name: s.name,
        kind: s.kind,
        config: s.config,
        interval_secs: s.interval_secs,
        device_id: s.device_id,
        folder_prefix: s.prefix,
        user_id: s.user_id,
        now,
    };
    q::insert_source().params(c, &params)?;
    Ok(())
}

/// Where a source writes.
pub fn destination_of(c: &mut impl GenericClient, id: i64) -> Result<Option<Destination>, Error> {
    Ok(q::source_destination()
        .bind(c, &id)
        .opt()?
        .map(|s| Destination { device_id: s.device_id, folder: s.folder, prefix: s.folder_prefix }))
}

/// A source as it is stored, locked for an edit.
pub struct Stored {
    pub name: String,
    pub kind: String,
    pub config: String,
    pub interval_secs: i64,
    pub device_id: i64,
    pub prefix: String,
    pub user_id: i64,
    pub running: bool,
}

pub fn for_edit(c: &mut impl GenericClient, id: i64) -> Result<Option<Stored>, Error> {
    Ok(q::source_for_edit().bind(c, &id).opt()?.map(|s| Stored {
        running: s.last_status.as_deref() == Some("running"),
        name: s.name,
        kind: s.kind,
        config: s.config,
        interval_secs: s.interval_secs,
        device_id: s.device_id,
        prefix: s.folder_prefix,
        user_id: s.user_id,
    }))
}

/// Whether the user has another source of this name.
pub fn name_taken(c: &mut impl GenericClient, user_id: i64, name: &str, except: i64) -> Result<bool, Error> {
    q::source_name_taken().bind(c, &user_id, &name, &except).one()
}

pub fn update(c: &mut impl GenericClient, id: i64, s: &Stored) -> Result<(), Error> {
    let params = q::UpdateSourceParams {
        name: &s.name,
        config: &s.config,
        interval_secs: s.interval_secs,
        device_id: s.device_id,
        folder_prefix: &s.prefix,
        id,
    };
    q::update_source().params(c, &params).map(drop)
}

/// Forgets what a source remembered of the remote files, so that its next run collects everything again.
pub fn forget_items(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::delete_source_items().bind(c, &id).map(drop)
}

/// Forgets a source and what it remembered of the remote files.
pub fn delete(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::delete_source_items().bind(&mut *c, &id)?;
    q::delete_source().bind(c, &id).map(drop)
}

/// Records how a run went; `ok_at` is when it worked, if it did.
pub fn finish(c: &mut impl GenericClient, id: i64, status: &str, message: &str, ok_at: Option<i64>) -> Result<(), Error> {
    q::finish_source().params(c, &q::FinishSourceParams { status, message, ok_at, id }).map(drop)
}

/// Remembers a remote file: where it is stored and the version of it that was collected. Brings back a file that
/// had been removed in the web app, since it changed at the source.
pub fn remember_item(c: &mut impl GenericClient, source_id: i64, remote_id: &str, path: &str, remote_version: &str) -> Result<(), Error> {
    q::upsert_source_item().params(c, &q::UpsertSourceItemParams { source_id, remote_id, path, remote_version })?;
    Ok(())
}

/// What was collected from the remote, as (remote id, path below the source's folder, version).
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
pub fn removed_in_web_app(c: &mut impl GenericClient, device_id: i64, source_id: i64, prefix: &str) -> Result<Vec<String>, Error> {
    q::removed_in_web_app().params(c, &q::RemovedInWebAppParams { device_id, source_id, prefix }).all()
}

/// Marks the sources that were running when the master stopped as failed.
pub fn fail_interrupted(c: &mut impl GenericClient) -> Result<(), Error> {
    q::fail_interrupted_sources().bind(c).map(drop)
}

/// A folder was moved from `from` to `to` (both end with a slash): the sources that write into it, or below it, follow.
pub fn follow_folder(c: &mut impl GenericClient, device_id: i64, from: &str, to: &str) -> Result<u64, Error> {
    q::move_source_folders().bind(c, &to, &from, &device_id)
}
