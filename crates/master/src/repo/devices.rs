//! Devices: the devices and phones that send files, their token, their lease and what they are doing. Their files are in
//! folders (`repo::folders`).

use fafnirx_db::client::GenericClient;
use fafnirx_db::client::Params;
use fafnirx_db::queries::devices as q;
use postgres::Error;

/// What the token of a device finds: the device and the installation of the client that holds it.
pub struct Holder {
    pub id: i64,
    /// `<user>/<name>`.
    pub key: String,
    /// The user who owns the device.
    pub user_id: i64,
    pub instance: Option<String>,
    pub hostname: Option<String>,
    pub ip: Option<String>,
}

/// The device that owns this token, if it is active: not revoked, and of a user who is not disabled.
pub fn find_by_token(c: &mut impl GenericClient, token_hash: &str) -> Result<Option<Holder>, Error> {
    let found = q::device_by_token().bind(c, &token_hash).opt()?;
    Ok(found.map(|d| Holder { id: d.id, key: d.key, user_id: d.user_id, instance: d.instance_id, hostname: d.hostname, ip: d.ip }))
}

/// Makes `instance` the holder of the device, unless another instance holds it and was heard from since `lease_start`.
/// Returns whether it is the holder now. One statement, so two devices connecting at once cannot both win.
pub fn take_over(c: &mut impl GenericClient, id: i64, instance: &str, now: i64, lease_start: i64) -> Result<bool, Error> {
    let n = q::take_over_device().bind(c, &instance, &now, &id, &lease_start)?;
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
    q::record_heartbeat().bind(c, &hb.hostname, &hb.os, &hb.ip, &hb.client_version, &hb.disk_total, &hb.disk_free, &now, &id)?;
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
        c,
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
    Ok(q::clear_activity().bind(c, &id)? > 0)
}

/// The id and role of an enabled user, by name.
pub fn user_for_device(c: &mut impl GenericClient, name: &str) -> Result<Option<(i64, String)>, Error> {
    Ok(q::user_for_device().bind(c, &name).opt()?.map(|u| (u.id, u.role)))
}

/// Adds a device to a user. Returns its key, or `None` when the user already has a device of that name.
pub fn insert(c: &mut impl GenericClient, user_id: i64, name: &str, token_hash: &str, now: i64) -> Result<Option<String>, Error> {
    if super::folders::name_taken(c, Some(user_id), name)? {
        return Ok(None);
    }
    q::insert_device().bind(c, &name, &token_hash, &now, &user_id).opt()
}

/// A new token for a device; ends a revocation and frees the device for whichever client connects next.
pub fn set_token(c: &mut impl GenericClient, key: &str, token_hash: &str) -> Result<bool, Error> {
    Ok(q::set_device_token().bind(c, &token_hash, &key)? > 0)
}

/// Frees the device for whichever client connects next, without waiting for the holder to go quiet.
pub fn release(c: &mut impl GenericClient, key: &str) -> Result<bool, Error> {
    Ok(q::release_device().bind(c, &key)? > 0)
}

/// Ends the token of a device.
pub fn revoke(c: &mut impl GenericClient, key: &str, now: i64) -> Result<bool, Error> {
    Ok(q::revoke_device().bind(c, &now, &key)? > 0)
}

/// Whether there is a device with this key.
pub fn exists(c: &mut impl GenericClient, key: &str) -> Result<bool, Error> {
    Ok(q::device_exists().bind(c, &key).opt()?.is_some())
}

/// The id of the device with this key.
pub fn id_of(c: &mut impl GenericClient, key: &str) -> Result<Option<i64>, Error> {
    q::device_id_by_key().bind(c, &key).opt()
}

/// Every active device with when it was last heard from: (key, last_seen).
pub fn last_seen(c: &mut impl GenericClient) -> Result<Vec<(String, i64)>, Error> {
    Ok(q::devices_last_seen().bind(c).all()?.into_iter().map(|d| (d.key, d.last_seen)).collect())
}

/// Notes that a device was heard from.
pub fn touch(c: &mut impl GenericClient, id: i64, now: i64) -> Result<(), Error> {
    fafnirx_db::queries::files::touch_device().bind(c, &now, &id).map(drop)
}

/// Removes a device with the folders of its directories and everything in them (files, versions, log); returns the versions, whose
/// stored copies the caller removes once the transaction is committed.
pub fn delete(c: &mut impl GenericClient, id: i64) -> Result<Vec<i64>, Error> {
    let mut versions = Vec::new();
    for (folder_id, _, _) in super::folders::backups_of(c, id)? {
        super::folders::lock(c, folder_id)?;
        versions.extend(super::folders::delete_contents(c, folder_id)?);
        super::folders::delete(c, folder_id)?;
    }
    clear_progress(c, id)?;
    q::delete_device().bind(c, &id)?;
    Ok(versions)
}

/// What a device says about a folder attached to it. `name` is the folder when a device is asked, and the device when a folder is.
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct FolderState {
    pub name: String,
    pub files: i64,
    pub bytes: i64,
    /// Unix seconds, 0 before the first cycle.
    pub last_cycle: i64,
    pub last_error: String,
    pub held: i64,
    pub failing: i64,
    /// How the device settles a file changed on both sides: `up`, `both`, `keep` or `ask`.
    pub mode: String,
    pub updated_at: i64,
}

/// Keeps what a device reports about its folders, in place of what it reported before. Each is the folder's id with its state.
pub fn save_folder_state(c: &mut impl GenericClient, device_id: i64, reported: &[(i64, FolderState)], now: i64) -> Result<(), Error> {
    q::clear_folder_state().bind(c, &device_id)?;
    for (folder_id, s) in reported {
        q::save_folder_state().bind(
            c,
            &device_id,
            folder_id,
            &s.files,
            &s.bytes,
            &s.last_cycle,
            &s.last_error,
            &s.held,
            &s.failing,
            &s.mode,
            &now,
        )?;
    }
    Ok(())
}

/// What a device last said about its folders.
pub fn folder_states_of_device(c: &mut impl GenericClient, device_id: i64) -> Result<Vec<FolderState>, Error> {
    Ok(q::folder_states_of_device()
        .bind(c, &device_id)
        .all()?
        .into_iter()
        .map(|r| FolderState {
            name: r.key,
            files: r.files,
            bytes: r.bytes,
            last_cycle: r.last_cycle,
            last_error: r.last_error,
            held: r.held,
            failing: r.failing,
            mode: r.mode,
            updated_at: r.updated_at,
        })
        .collect())
}

/// What the devices that have a folder attached say about it: all of them (`all`), or only those of `user`.
pub fn device_states_of_folder(c: &mut impl GenericClient, folder_id: i64, all: bool, user: i64) -> Result<Vec<FolderState>, Error> {
    Ok(q::device_states_of_folder()
        .bind(c, &folder_id, &all, &user)
        .all()?
        .into_iter()
        .map(|r| FolderState {
            name: r.device,
            files: r.files,
            bytes: r.bytes,
            last_cycle: r.last_cycle,
            last_error: r.last_error,
            held: r.held,
            failing: r.failing,
            mode: r.mode,
            updated_at: r.updated_at,
        })
        .collect())
}
