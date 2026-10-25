//! Mirrors: offsite copies of the whole tree, kept up to date by rclone on a schedule, and what each one holds.

use crate::platform::pg::Pg;
use fafnirx_db::client::Params;
use fafnirx_db::queries::mirrors as q;
use postgres::{Error, GenericClient};

pub type Listed = q::ListMirrors;
/// A mirror that was due and has been marked running.
pub type Claimed = q::ClaimMirror;
/// A live file that belongs on every mirror.
pub type Live = q::LiveFilesForMirror;

pub fn list(c: &mut impl GenericClient) -> Result<Vec<Listed>, Error> {
    q::list_mirrors().bind(&mut Pg(c)).all()
}

pub fn id_of(c: &mut impl GenericClient, name: &str) -> Result<Option<i64>, Error> {
    q::mirror_id().bind(&mut Pg(c), &name).opt()
}

/// What a new mirror is made of.
pub struct New<'a> {
    pub name: &'a str,
    pub remote: &'a str,
    pub remote_type: &'a str,
    pub options: &'a str,
    pub remote_path: &'a str,
    pub interval_secs: i64,
    pub keep_runs: i64,
}

/// Adds a mirror. Returns false when one of that name exists.
pub fn insert(c: &mut impl GenericClient, m: &New, now: i64) -> Result<bool, Error> {
    let params = q::InsertMirrorParams {
        name: m.name,
        remote: m.remote,
        remote_type: m.remote_type,
        options: m.options,
        remote_path: m.remote_path,
        interval_secs: m.interval_secs,
        keep_runs: m.keep_runs,
        now,
    };
    Ok(q::insert_mirror().params(&mut Pg(c), &params)? > 0)
}

/// Forgets what was uploaded: the next run sends everything again.
pub fn forget_items(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::delete_mirror_items().bind(&mut Pg(c), &id)?;
    Ok(())
}

pub fn delete(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::delete_mirror_items().bind(&mut Pg(&mut *c), &id)?;
    q::delete_mirror().bind(&mut Pg(c), &id)?;
    Ok(())
}

pub fn request_run(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::request_mirror_run().bind(&mut Pg(c), &id)?;
    Ok(())
}

pub fn toggle(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::toggle_mirror().bind(&mut Pg(c), &id)?;
    Ok(())
}

/// Picks one due mirror and marks it running. One statement, so a mirror can only be claimed once.
pub fn claim(c: &mut impl GenericClient, now: i64) -> Result<Option<Claimed>, Error> {
    q::claim_mirror().bind(&mut Pg(c), &now).opt()
}

/// Records how a run went; `ok_at` is when it worked, if it did.
pub fn finish(
    c: &mut impl GenericClient,
    id: i64,
    status: &str,
    message: &str,
    ok_at: Option<i64>,
) -> Result<(), Error> {
    q::finish_mirror().params(&mut Pg(c), &q::FinishMirrorParams { status, message, ok_at, id })?;
    Ok(())
}

/// How many files and bytes the last run left on the mirror.
pub fn set_totals(c: &mut impl GenericClient, id: i64, files: i64, bytes: i64) -> Result<(), Error> {
    q::set_mirror_totals().bind(&mut Pg(c), &files, &bytes, &id)?;
    Ok(())
}

/// Every live file of every device, in the order they are uploaded.
pub fn live_files(c: &mut impl GenericClient) -> Result<Vec<Live>, Error> {
    q::live_files_for_mirror().bind(&mut Pg(c)).all()
}

/// What a mirror holds, as (device id, path, root hash).
pub fn items(c: &mut impl GenericClient, mirror_id: i64) -> Result<Vec<(i64, String, Vec<u8>)>, Error> {
    Ok(q::mirror_items()
        .bind(&mut Pg(c), &mirror_id)
        .all()?
        .into_iter()
        .map(|i| (i.device_id, i.path, i.root_hash))
        .collect())
}

pub fn remember_item(
    c: &mut impl GenericClient,
    mirror_id: i64,
    device_id: i64,
    path: &str,
    root_hash: &[u8],
) -> Result<(), Error> {
    q::upsert_mirror_item().params(&mut Pg(c), &q::UpsertMirrorItemParams { mirror_id, device_id, path, root_hash })?;
    Ok(())
}

pub fn forget_item(c: &mut impl GenericClient, mirror_id: i64, device_id: i64, path: &str) -> Result<(), Error> {
    q::delete_mirror_item().params(&mut Pg(c), &q::DeleteMirrorItemParams { mirror_id, device_id, path })?;
    Ok(())
}

/// The folder of every device, by id.
pub fn device_folders(c: &mut impl GenericClient) -> Result<Vec<(i64, String)>, Error> {
    Ok(q::device_folders().bind(&mut Pg(c)).all()?.into_iter().map(|d| (d.id, d.folder)).collect())
}

/// Marks the mirrors that were running when the master stopped as failed.
pub fn fail_interrupted(c: &mut impl GenericClient) -> Result<(), Error> {
    q::fail_interrupted_mirrors().bind(&mut Pg(c))?;
    Ok(())
}
