//! Devices (clients, local folders, sources), their activity and their totals.

use crate::platform::pg::Pg;
use fafnirx_db::client::Params;
use fafnirx_db::queries::devices as q;
use postgres::{Error, GenericClient};

/// What the token of a device finds: the device and the installation of the client that holds it.
pub struct Holder {
    pub id: i64,
    pub folder: String,
    pub instance: Option<String>,
    pub hostname: Option<String>,
    pub ip: Option<String>,
}

/// The device that owns this token, if it is active: not revoked, a client (not a folder), and of a user who is not disabled.
pub fn find_by_token(c: &mut impl GenericClient, token_hash: &str) -> Result<Option<Holder>, Error> {
    let found = q::device_by_token().bind(&mut Pg(c), &token_hash).opt()?;
    Ok(found.map(|d| Holder { id: d.id, folder: d.folder, instance: d.instance_id, hostname: d.hostname, ip: d.ip }))
}

/// Makes `instance` the holder of the device, unless another instance holds it and was heard from since `lease_start`.
/// Returns whether it is the holder now. One statement, so two computers connecting at once cannot both win.
pub fn take_over(
    c: &mut impl GenericClient,
    id: i64,
    instance: &str,
    now: i64,
    lease_start: i64,
) -> Result<bool, Error> {
    let n = q::take_over_device().bind(&mut Pg(c), &instance, &now, &id, &lease_start)?;
    Ok(n == 1)
}

/// What a client says about itself with each heartbeat.
pub struct Heartbeat<'a> {
    pub hostname: &'a str,
    pub os: &'a str,
    pub ip: Option<&'a str>,
    pub client_version: &'a str,
    pub disk_total: i64,
    pub disk_free: i64,
}

pub fn record_heartbeat(c: &mut impl GenericClient, id: i64, hb: &Heartbeat, now: i64) -> Result<(), Error> {
    q::record_heartbeat().bind(
        &mut Pg(c),
        &hb.hostname,
        &hb.os,
        &hb.ip,
        &hb.client_version,
        &hb.disk_total,
        &hb.disk_free,
        &now,
        &id,
    )?;
    Ok(())
}

/// What a device reports it is doing while it syncs.
pub struct Progress<'a> {
    pub phase: &'a str,
    pub current_file: &'a str,
    pub files_done: i64,
    pub files_total: i64,
    pub bytes_done: i64,
    pub bytes_total: i64,
    pub speed_bps: i64,
}

pub fn save_progress(c: &mut impl GenericClient, id: i64, p: &Progress, now: i64) -> Result<(), Error> {
    q::save_activity().params(
        &mut Pg(c),
        &q::SaveActivityParams {
            device_id: id,
            now,
            phase: p.phase,
            current_file: p.current_file,
            files_done: p.files_done,
            files_total: p.files_total,
            bytes_done: p.bytes_done,
            bytes_total: p.bytes_total,
            speed_bps: p.speed_bps,
        },
    )?;
    Ok(())
}

/// Forgets what the device was doing. Returns whether it had been syncing.
pub fn clear_progress(c: &mut impl GenericClient, id: i64) -> Result<bool, Error> {
    Ok(q::clear_activity().bind(&mut Pg(c), &id)? > 0)
}

/// A device as the device list shows it, with its totals and what it is doing.
pub type Summary = q::DeviceSummaries;

/// The devices visible to a user (`all`: everything; otherwise the master files and the devices of `user`), or just
/// the one with this `folder`.
pub fn summaries(
    c: &mut impl GenericClient,
    folder: Option<&str>,
    all: bool,
    user: i64,
) -> Result<Vec<Summary>, Error> {
    q::device_summaries().bind(&mut Pg(c), &folder, &all, &user).all()
}

/// The id and role of an enabled user, by name.
pub fn user_for_device(c: &mut impl GenericClient, name: &str) -> Result<Option<(i64, String)>, Error> {
    Ok(q::user_for_device().bind(&mut Pg(c), &name).opt()?.map(|u| (u.id, u.role)))
}

/// Adds a device to a user. Returns its folder, or `None` when the user already has a device of that name.
pub fn insert(
    c: &mut impl GenericClient,
    user_id: i64,
    name: &str,
    kind: &str,
    token_hash: &str,
    now: i64,
) -> Result<Option<String>, Error> {
    q::insert_device().bind(&mut Pg(c), &name, &kind, &token_hash, &now, &user_id).opt()
}

/// A new token for a client; ends a revocation and frees the device for whichever computer connects next.
pub fn set_token(c: &mut impl GenericClient, folder: &str, token_hash: &str) -> Result<bool, Error> {
    Ok(q::set_device_token().bind(&mut Pg(c), &token_hash, &folder)? > 0)
}

/// Frees the device for whichever computer connects next, without waiting for the holder to go quiet.
pub fn release(c: &mut impl GenericClient, folder: &str) -> Result<bool, Error> {
    Ok(q::release_device().bind(&mut Pg(c), &folder)? > 0)
}

/// Ends the token of a client. Returns false for a folder that is not a client.
pub fn revoke(c: &mut impl GenericClient, folder: &str, now: i64) -> Result<bool, Error> {
    Ok(q::revoke_device().bind(&mut Pg(c), &now, &folder)? > 0)
}

/// Whether the folder is a client or a local folder (the devices a user can manage).
pub fn is_device_or_local(c: &mut impl GenericClient, folder: &str) -> Result<bool, Error> {
    Ok(q::device_or_local_exists().bind(&mut Pg(c), &folder).opt()?.is_some())
}

/// Locks the row of a device and returns its folder, so that the totals added up next are taken after any writer
/// that is still committing has marked them stale.
pub fn lock(c: &mut impl GenericClient, id: i64) -> Result<Option<String>, Error> {
    q::lock_device_folder().bind(&mut Pg(c), &id).opt()
}

/// Adds up the files of a device and stores the result on its row.
pub fn store_totals(c: &mut impl GenericClient, id: i64, now: i64) -> Result<(), Error> {
    q::update_totals().bind(&mut Pg(c), &now, &id)?;
    Ok(())
}

/// Devices whose totals are stale and that have not reported progress since `syncing_since`.
pub fn stale_totals(c: &mut impl GenericClient, syncing_since: i64) -> Result<Vec<i64>, Error> {
    q::stale_device_ids().bind(&mut Pg(c), &syncing_since).all()
}

/// Every active client with when it was last heard from: (folder, last_seen).
pub fn clients_last_seen(c: &mut impl GenericClient) -> Result<Vec<(String, i64)>, Error> {
    Ok(q::clients_last_seen().bind(&mut Pg(c)).all()?.into_iter().map(|d| (d.folder, d.last_seen)).collect())
}

/// What the visible devices hold together, as of their last sync: (files, bytes, files marked deleted).
pub fn visible_totals(c: &mut impl GenericClient, all: bool, user: i64) -> Result<(i64, i64, i64), Error> {
    let t = q::visible_totals().bind(&mut Pg(c), &all, &user).one()?;
    Ok((t.files, t.bytes, t.deleted))
}

/// Makes the totals of these devices be added up again.
pub fn mark_totals_stale(c: &mut impl GenericClient, ids: &[i64]) -> Result<(), Error> {
    q::mark_totals_stale().bind(&mut Pg(c), &ids)?;
    Ok(())
}

/// Notes that a device was heard from and that its totals are out of date.
pub fn touch(c: &mut impl GenericClient, id: i64, now: i64) -> Result<(), Error> {
    fafnirx_db::queries::files::touch_device().bind(&mut Pg(c), &now, &id)?;
    Ok(())
}

/// Notes that a device collected something and that its totals are out of date.
pub fn touch_collected(c: &mut impl GenericClient, id: i64, now: i64) -> Result<(), Error> {
    fafnirx_db::queries::files::touch_collected().bind(&mut Pg(c), &now, &id)?;
    Ok(())
}

/// Makes the totals of a device be added up again.
pub fn mark_stale(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    fafnirx_db::queries::files::mark_stale().bind(&mut Pg(c), &id)?;
    Ok(())
}

/// (files, bytes) of the master files.
pub fn master_totals(c: &mut impl GenericClient) -> Result<(i64, i64), Error> {
    let t = q::master_totals().bind(&mut Pg(c)).one()?;
    Ok((t.file_count, t.total_bytes))
}

/// device, local, source or master.
pub fn kind_of(c: &mut impl GenericClient, id: i64) -> Result<String, Error> {
    q::device_kind().bind(&mut Pg(c), &id).one()
}

/// The id and folder of the device with this folder name.
pub fn by_folder(c: &mut impl GenericClient, folder: &str) -> Result<Option<(i64, String)>, Error> {
    Ok(q::device_by_folder().bind(&mut Pg(c), &folder).opt()?.map(|d| (d.id, d.folder)))
}
