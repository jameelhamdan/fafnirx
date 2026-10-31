//! Mirrors: offsite copies of the whole tree, kept up to date by rclone on a schedule, and what each one holds.

use crate::integrations::scope::Place;
use fafnirx_db::client::GenericClient;
use fafnirx_db::client::Params;
use fafnirx_db::queries::mirrors as q;
use postgres::Error;

/// A live file that belongs on every mirror.
pub type Live = q::LiveFilesForMirror;

/// What a new mirror is made of.
pub struct New<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub crypt_password: &'a str,
    pub crypt_salt: &'a str,
    pub remote: &'a str,
    pub remote_type: &'a str,
    pub options: &'a str,
    pub remote_path: &'a str,
    pub scope: &'a str,
    pub interval_secs: i64,
    pub keep_runs: i64,
}

/// Adds a mirror. Returns false when one of that name exists.
pub fn insert(c: &mut impl GenericClient, m: &New, now: i64) -> Result<bool, Error> {
    let params = q::InsertMirrorParams {
        name: m.name,
        kind: m.kind,
        crypt_password: m.crypt_password,
        crypt_salt: m.crypt_salt,
        remote: m.remote,
        remote_type: m.remote_type,
        options: m.options,
        remote_path: m.remote_path,
        scope: m.scope,
        interval_secs: m.interval_secs,
        keep_runs: m.keep_runs,
        now,
    };
    Ok(q::insert_mirror().params(c, &params)? > 0)
}

/// What restoring a backup mirror needs to know besides the storage's own credentials.
pub type Key = q::MirrorKey;

pub fn delete(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::delete_mirror_items().bind(&mut *c, &id)?;
    q::delete_mirror().bind(c, &id).map(drop)
}

pub fn request_run(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::request_mirror_run().bind(c, &id).map(drop)
}

pub fn toggle(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::toggle_mirror().bind(c, &id).map(drop)
}

/// Records how a run went; `ok_at` is when it worked, if it did.
pub fn finish(c: &mut impl GenericClient, id: i64, status: &str, message: &str, ok_at: Option<i64>) -> Result<(), Error> {
    q::finish_mirror().params(c, &q::FinishMirrorParams { status, message, ok_at, id }).map(drop)
}

/// How many files and bytes the last run left on the mirror.
pub fn set_totals(c: &mut impl GenericClient, id: i64, files: i64, bytes: i64) -> Result<(), Error> {
    q::set_mirror_totals().bind(c, &files, &bytes, &id).map(drop)
}

/// Every live file at or below the folder `scope` (everything when it is empty), in the order they are uploaded.
pub fn live_files(c: &mut impl GenericClient, scope: &str) -> Result<Vec<Live>, Error> {
    q::live_files_for_mirror().bind(c, &scope).all()
}

/// What a mirror holds, as (device id, path, root hash).
pub fn items(c: &mut impl GenericClient, mirror_id: i64) -> Result<Vec<(i64, String, Vec<u8>)>, Error> {
    Ok(q::mirror_items()
        .bind(c, &mirror_id)
        .all()?
        .into_iter()
        .map(|i| (i.device_id, i.path, i.root_hash))
        .collect())
}

pub fn remember_item(c: &mut impl GenericClient, mirror_id: i64, device_id: i64, path: &str, root_hash: &[u8]) -> Result<(), Error> {
    q::upsert_mirror_item()
        .params(c, &q::UpsertMirrorItemParams { mirror_id, device_id, path, root_hash })
        .map(drop)
}

pub fn forget_item(c: &mut impl GenericClient, mirror_id: i64, device_id: i64, path: &str) -> Result<(), Error> {
    q::delete_mirror_item()
        .params(c, &q::DeleteMirrorItemParams { mirror_id, device_id, path })
        .map(drop)
}

/// The folder of every device, by id.
pub fn device_folders(c: &mut impl GenericClient) -> Result<Vec<(i64, String)>, Error> {
    Ok(q::device_folders().bind(c).all()?.into_iter().map(|d| (d.id, d.folder)).collect())
}

/// Every device, folder and source, with the kind, for choosing what a mirror holds.
pub fn places(c: &mut impl GenericClient) -> Result<Vec<Place>, Error> {
    Ok(q::scope_places()
        .bind(c)
        .all()?
        .into_iter()
        .map(|p| Place { id: p.id, folder: p.folder, kind: p.kind })
        .collect())
}
