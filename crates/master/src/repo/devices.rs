//! Devices (clients, local folders, sources), their activity and their totals.

use fafnirx_db::client::GenericClient;
use fafnirx_db::client::Params;
use fafnirx_db::queries::devices as q;
use postgres::Error;

/// What the token of a device finds: the device and the installation of the client that holds it.
pub struct Holder {
    pub id: i64,
    pub folder: String,
    /// The user who owns the device.
    pub user_id: i64,
    pub instance: Option<String>,
    pub hostname: Option<String>,
    pub ip: Option<String>,
}

/// The device that owns this token, if it is active: not revoked, a client (not a folder), and of a user who is not disabled.
pub fn find_by_token(c: &mut impl GenericClient, token_hash: &str) -> Result<Option<Holder>, Error> {
    let found = q::device_by_token().bind(c, &token_hash).opt()?;
    Ok(found.map(|d| Holder {
        id: d.id,
        folder: d.folder,
        user_id: d.user_id,
        instance: d.instance_id,
        hostname: d.hostname,
        ip: d.ip,
    }))
}

/// Makes `instance` the holder of the device, unless another instance holds it and was heard from since `lease_start`.
/// Returns whether it is the holder now. One statement, so two computers connecting at once cannot both win.
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

/// Adds a device to a user. Returns its folder, or `None` when the user already has a device of that name.
pub fn insert(c: &mut impl GenericClient, user_id: i64, name: &str, kind: &str, token_hash: &str, now: i64) -> Result<Option<String>, Error> {
    q::insert_device().bind(c, &name, &kind, &token_hash, &now, &user_id).opt()
}

/// Adds a folder to a user. It has no token (no computer is it). Returns its folder, or `None` when the user already has a device
/// or folder of that name.
pub fn create_folder(c: &mut impl GenericClient, owner: i64, name: &str, now: i64) -> Result<Option<String>, Error> {
    insert(c, owner, name, "local", "!", now)
}

/// A new token for a client; ends a revocation and frees the device for whichever computer connects next.
pub fn set_token(c: &mut impl GenericClient, folder: &str, token_hash: &str) -> Result<bool, Error> {
    Ok(q::set_device_token().bind(c, &token_hash, &folder)? > 0)
}

/// Frees the device for whichever computer connects next, without waiting for the holder to go quiet.
pub fn release(c: &mut impl GenericClient, folder: &str) -> Result<bool, Error> {
    Ok(q::release_device().bind(c, &folder)? > 0)
}

/// Ends the token of a client. Returns false for a folder that is not a client.
pub fn revoke(c: &mut impl GenericClient, folder: &str, now: i64) -> Result<bool, Error> {
    Ok(q::revoke_device().bind(c, &now, &folder)? > 0)
}

/// Whether the folder is a client or a local folder (the devices a user can manage).
pub fn is_device_or_local(c: &mut impl GenericClient, folder: &str) -> Result<bool, Error> {
    Ok(q::device_or_local_exists().bind(c, &folder).opt()?.is_some())
}

/// Locks the row of a device and returns its folder, so that the totals added up next are taken after any writer
/// that is still committing has marked them stale.
pub fn lock(c: &mut impl GenericClient, id: i64) -> Result<Option<String>, Error> {
    q::lock_device_folder().bind(c, &id).opt()
}

/// Every active client with when it was last heard from: (folder, last_seen).
pub fn clients_last_seen(c: &mut impl GenericClient) -> Result<Vec<(String, i64)>, Error> {
    Ok(q::clients_last_seen().bind(c).all()?.into_iter().map(|d| (d.folder, d.last_seen)).collect())
}

/// What the visible devices hold together, as of their last sync: (files, bytes, files marked deleted).
pub fn visible_totals(c: &mut impl GenericClient, all: bool, user: i64) -> Result<(i64, i64, i64), Error> {
    let t = q::visible_totals().bind(c, &user, &all).one()?;
    Ok((t.files, t.bytes, t.deleted))
}

/// Notes that a device was heard from and that its totals are out of date.
pub fn touch(c: &mut impl GenericClient, id: i64, now: i64) -> Result<(), Error> {
    fafnirx_db::queries::files::touch_device().bind(c, &now, &id).map(drop)
}

/// Notes that a device collected something and that its totals are out of date.
pub fn touch_collected(c: &mut impl GenericClient, id: i64, now: i64) -> Result<(), Error> {
    fafnirx_db::queries::files::touch_collected().bind(c, &now, &id).map(drop)
}

/// Makes the totals of a device be added up again.
pub fn mark_stale(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    fafnirx_db::queries::files::mark_stale().bind(c, &id).map(drop)
}

/// device, local, source or master.
pub fn kind_of(c: &mut impl GenericClient, id: i64) -> Result<String, Error> {
    q::device_kind().bind(c, &id).one()
}

/// The id and folder of the device with this folder name.
pub fn by_folder(c: &mut impl GenericClient, folder: &str) -> Result<Option<(i64, String)>, Error> {
    Ok(q::device_by_folder().bind(c, &folder).opt()?.map(|d| (d.id, d.folder)))
}

/// The id and kind of the device with this folder name.
pub fn id_and_kind(c: &mut impl GenericClient, folder: &str) -> Result<Option<(i64, String)>, Error> {
    Ok(q::device_id_and_kind_by_folder().bind(c, &folder).opt()?.map(|d| (d.id, d.kind)))
}

/// A folder a user can use, whoever owns it, as the web app shows it. The same thing is a folder of the user, one another user shared
/// with them, or a master folder: the difference is the owner and who may change it.
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Folder {
    /// The device row it is kept in.
    #[serde(skip)]
    pub id: i64,
    /// The stable address of the folder: it stays when the folder is renamed.
    #[cfg_attr(test, ts(type = "string"))]
    pub public_id: uuid::Uuid,
    pub name: String,
    /// How the clients name it: `<owner>/<name>`, or `master:<path>`.
    pub folder: String,
    /// The user it belongs to; empty for a master folder.
    pub owner: String,
    /// Their own, not one another user shared with them.
    pub mine: bool,
    /// Whether the user may change what is in it.
    pub can_write: bool,
    /// Other users use it (or, for a master folder, all of them do).
    pub shared: bool,
    pub master: bool,
    pub created_at: i64,
    /// What is in it now (not live: see `devices::totals`).
    pub files: i64,
    pub bytes: i64,
    /// When a file in it last changed, in unix seconds.
    pub changed_at: Option<i64>,
}

fn folders_matching(c: &mut impl GenericClient, user_id: i64, key: Option<&str>) -> Result<Vec<Folder>, Error> {
    Ok(q::folders_for_user()
        .bind(c, &user_id, &key)
        .all()?
        .into_iter()
        .map(|f| Folder {
            id: f.id,
            public_id: f.public_id,
            name: f.name,
            folder: f.folder,
            owner: f.owner,
            mine: f.mine,
            can_write: f.can_write,
            shared: f.shared,
            master: f.master,
            created_at: f.created_at,
            files: f.files,
            bytes: f.bytes,
            changed_at: f.last_collected,
        })
        .collect())
}

/// Every folder a user can use: their own, those shared with them, and the master folders.
pub fn folders(c: &mut impl GenericClient, user_id: i64) -> Result<Vec<Folder>, Error> {
    folders_matching(c, user_id, None)
}

/// The folder a user means by `key`: the name of one of their own, the `<owner>/<name>` or `master:<path>` of any, or its stable id.
pub fn folder(c: &mut impl GenericClient, user_id: i64, key: &str) -> Result<Option<Folder>, Error> {
    Ok(folders_matching(c, user_id, Some(key))?.into_iter().next())
}

/// The id and folder (`master:<name>`) of every master folder.
pub fn master_devices(c: &mut impl GenericClient) -> Result<Vec<(i64, String)>, Error> {
    Ok(q::master_devices().bind(c).all()?.into_iter().map(|d| (d.id, d.folder)).collect())
}

/// The master folders that are `name`, inside it or around it.
pub fn master_names_overlapping(c: &mut impl GenericClient, name: &str) -> Result<Vec<String>, Error> {
    q::master_folder_names_under().bind(c, &name).all()
}

/// Makes a master folder, returning its folder name (`master:<name>`), or `None` when it exists.
pub fn insert_master_folder(c: &mut impl GenericClient, name: &str, now: i64) -> Result<Option<String>, Error> {
    q::insert_master_folder().bind(c, &name, &now).opt()
}

/// The device row, name and folder of the master folder with this stable id.
pub fn master_folder_by_public_id(c: &mut impl GenericClient, public_id: uuid::Uuid) -> Result<Option<(i64, String, String)>, Error> {
    Ok(q::master_folder_by_public_id().bind(c, &public_id).opt()?.map(|r| (r.id, r.name, r.folder)))
}

/// What a computer says about a shared folder attached to it. `name` is the shared folder when a computer is asked, and the
/// computer when a shared folder is.
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
    pub updated_at: i64,
}

/// Keeps what a computer reports about its shared folders, in place of what it reported before.
pub fn save_folder_state(c: &mut impl GenericClient, device_id: i64, reported: &[FolderState], now: i64) -> Result<(), Error> {
    q::clear_folder_state().bind(c, &device_id)?;
    for s in reported {
        q::save_folder_state().bind(c, &device_id, &s.name, &s.files, &s.bytes, &s.last_cycle, &s.last_error, &s.held, &s.failing, &now)?;
    }
    Ok(())
}

/// What a computer last said about its shared folders.
pub fn folder_states_of_device(c: &mut impl GenericClient, device_id: i64) -> Result<Vec<FolderState>, Error> {
    Ok(q::folder_states_of_device()
        .bind(c, &device_id)
        .all()?
        .into_iter()
        .map(|r| FolderState {
            name: r.folder,
            files: r.files,
            bytes: r.bytes,
            last_cycle: r.last_cycle,
            last_error: r.last_error,
            held: r.held,
            failing: r.failing,
            updated_at: r.updated_at,
        })
        .collect())
}

/// What the computers that have a shared folder attached say about it: all of them (`all`), or only those of `user`.
pub fn folder_states_of_folder(c: &mut impl GenericClient, folder: &str, all: bool, user: i64) -> Result<Vec<FolderState>, Error> {
    Ok(q::folder_states_of_folder()
        .bind(c, &folder, &all, &user)
        .all()?
        .into_iter()
        .map(|r| FolderState {
            name: r.computer,
            files: r.files,
            bytes: r.bytes,
            last_cycle: r.last_cycle,
            last_error: r.last_error,
            held: r.held,
            failing: r.failing,
            updated_at: r.updated_at,
        })
        .collect())
}

/// A user a shared folder is shared with.
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Member {
    pub user: String,
    /// Whether they may change what is in it.
    pub can_write: bool,
}

pub fn shared_members(c: &mut impl GenericClient, folder_id: i64) -> Result<Vec<Member>, Error> {
    Ok(q::shared_members()
        .bind(c, &folder_id)
        .all()?
        .into_iter()
        .map(|m| Member { user: m.user_name, can_write: m.can_write })
        .collect())
}

/// Lets a user use a shared folder (or changes whether they may write to it).
pub fn add_shared_member(c: &mut impl GenericClient, folder_id: i64, user_id: i64, can_write: bool, now: i64) -> Result<(), Error> {
    q::add_shared_member().bind(c, &folder_id, &user_id, &can_write, &now).map(drop)
}

pub fn remove_shared_member(c: &mut impl GenericClient, folder_id: i64, user_id: i64) -> Result<bool, Error> {
    Ok(q::remove_shared_member().bind(c, &folder_id, &user_id)? > 0)
}

/// Removes a folder for good with everything left of it: the files in the trash, their versions and their log. Returns
/// the versions, whose stored copies the caller removes. Refuses (returns `None`) while the folder has files that are not deleted.
pub fn purge_folder(c: &mut postgres::Client, id: i64) -> Result<Option<Vec<i64>>, Error> {
    let mut tx = c.transaction()?;
    // The folder is locked first, so that nothing is written to it between the check and the removal.
    if lock(&mut tx, id)?.is_none() || q::folder_has_live_files().bind(&mut tx, &id).one()? {
        return Ok(None);
    }
    let files = q::device_file_ids().bind(&mut tx, &id).all()?;
    let versions = super::retention::delete_versions_of(&mut tx, &files)?;
    super::retention::delete_files(&mut tx, &files)?;
    q::delete_device_events().bind(&mut tx, &id)?;
    clear_progress(&mut tx, id)?;
    // Nothing is removed when the device stays (a source writes into it): the transaction is dropped.
    if q::delete_folder_device().bind(&mut tx, &id)? != 1 {
        return Ok(None);
    }
    tx.commit()?;
    Ok(Some(versions))
}
