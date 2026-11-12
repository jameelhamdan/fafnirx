//! Folders: what holds files. A folder of a user (`user`), one of the master folders (`master`) or a directory a device sends
//! (`backup`). Their totals, members and removal.

use fafnirx_db::client::GenericClient;
use fafnirx_db::queries::folders as q;
use postgres::Error;

text_enum! {
    /// What a folder is (the column is text with a CHECK for these three).
    #[derive(serde::Serialize)]
    #[serde(rename_all = "lowercase")]
    pub enum FolderKind {
        /// A directory a device sends: it belongs to the device, and only the device writes to it.
        Backup = "backup",
        /// A folder of a user on the master, which the web app changes.
        User = "user",
        /// A folder of the master files: it has no owner.
        Master = "master",
    }
}

impl FolderKind {
    /// A stored value; the database only holds the three.
    pub fn from_db(s: &str) -> FolderKind {
        FolderKind::parse(s).unwrap_or(FolderKind::Backup)
    }

    /// Whether the web app changes the files of a folder of this kind: folders of users and the master folders. What a device
    /// or a source sends is theirs to change; here it can be read, downloaded and brought back from the trash.
    pub fn editable(self) -> bool {
        matches!(self, FolderKind::User | FolderKind::Master)
    }
}

/// Whether a user already has a device or a folder of this name (they share the directory `files/users/<user>/`), or, for no user,
/// a master folder.
pub fn name_taken(c: &mut impl GenericClient, user_id: Option<i64>, name: &str) -> Result<bool, Error> {
    q::name_taken().bind(c, &user_id, &name).one()
}

/// Adds a folder to a user. Returns its key, or `None` when the user already has a device or folder of that name.
pub fn create(c: &mut impl GenericClient, owner: i64, name: &str, now: i64) -> Result<Option<String>, Error> {
    if name_taken(c, Some(owner), name)? {
        return Ok(None);
    }
    q::create_user_folder().bind(c, &name, &now, &owner).opt()
}

/// The folder of a directory of a device (made the first time it is needed): its id and key.
pub fn backup(c: &mut impl GenericClient, device_id: i64, name: &str, now: i64) -> Result<(i64, String), Error> {
    let found = q::ensure_backup_folder().bind(c, &name, &now, &device_id).one()?;
    Ok((found.id, found.key))
}

/// The folder of a directory of a device, if the device has sent files of it: its id and key.
pub fn backup_of(c: &mut impl GenericClient, device_id: i64, name: &str) -> Result<Option<(i64, String)>, Error> {
    Ok(q::backup_folder_of_device().bind(c, &device_id, &name).opt()?.map(|f| (f.id, f.key)))
}

/// The directories of a device that have a folder: (id, key, name).
pub fn backups_of(c: &mut impl GenericClient, device_id: i64) -> Result<Vec<(i64, String, String)>, Error> {
    Ok(q::backup_folders_of_device()
        .bind(c, &device_id)
        .all()?
        .into_iter()
        .map(|f| (f.id, f.key, f.name))
        .collect())
}

/// Locks the row of a folder and returns its key, so that the totals added up next are taken after any writer
/// that is still committing has marked them stale.
pub fn lock(c: &mut impl GenericClient, id: i64) -> Result<Option<String>, Error> {
    q::lock_folder_key().bind(c, &id).opt()
}

/// What the visible folders hold together, as of their last sum: (files, bytes, files marked deleted).
pub fn visible_totals(c: &mut impl GenericClient, all: bool, user: i64) -> Result<(i64, i64, i64), Error> {
    let t = q::visible_totals().bind(c, &user, &all).one()?;
    Ok((t.files, t.bytes, t.deleted))
}

/// Every folder `who` can see with its kind and what it holds, as of its last sum: (key, kind, files, bytes), by key.
pub fn visible(c: &mut impl GenericClient, all: bool, user: i64) -> Result<Vec<(String, FolderKind, i64, i64)>, Error> {
    Ok(q::visible_folders()
        .bind(c, &user, &all)
        .all()?
        .into_iter()
        .map(|f| (f.key, FolderKind::from_db(&f.kind), f.files, f.bytes))
        .collect())
}

/// What each visible folder holds, as of its last sum, the biggest first: (key, bytes, files). Folders without files are left out.
pub fn totals(c: &mut impl GenericClient, all: bool, user: i64) -> Result<Vec<(String, i64, i64)>, Error> {
    Ok(q::folder_totals()
        .bind(c, &user, &all)
        .all()?
        .into_iter()
        .map(|f| (f.key, f.bytes, f.files))
        .collect())
}

/// Notes that something was collected into a folder and that its totals are out of date.
pub fn touch_collected(c: &mut impl GenericClient, id: i64, now: i64) -> Result<(), Error> {
    fafnirx_db::queries::files::touch_collected().bind(c, &now, &id).map(drop)
}

/// Makes the totals of a folder be added up again.
pub fn mark_stale(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    fafnirx_db::queries::files::mark_stale().bind(c, &id).map(drop)
}

/// backup, user or master.
pub fn kind_of(c: &mut impl GenericClient, id: i64) -> Result<FolderKind, Error> {
    Ok(FolderKind::from_db(&q::folder_kind().bind(c, &id).one()?))
}

/// The id of the folder with this key.
pub fn id_of(c: &mut impl GenericClient, key: &str) -> Result<Option<i64>, Error> {
    Ok(q::folder_by_key().bind(c, &key).opt()?.map(|f| f.id))
}

/// The id and kind of the folder with this key.
pub fn id_and_kind(c: &mut impl GenericClient, key: &str) -> Result<Option<(i64, FolderKind)>, Error> {
    Ok(q::folder_id_and_kind_by_key().bind(c, &key).opt()?.map(|f| (f.id, FolderKind::from_db(&f.kind))))
}

/// A folder a user can use, whoever owns it, as the web app shows it. The same thing is a folder of the user, one another user shared
/// with them, or a master folder: the difference is the owner and who may change it.
#[derive(serde::Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Folder {
    /// The row it is kept in.
    #[serde(skip)]
    pub id: i64,
    /// The stable address of the folder: it stays when the folder is renamed.
    #[cfg_attr(test, ts(type = "string"))]
    pub public_id: uuid::Uuid,
    pub name: String,
    /// How the clients name it: `<owner>/<name>`, or `master:<path>`.
    pub key: String,
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
    /// What is in it now (not live: see `folders::totals`).
    pub files: i64,
    pub bytes: i64,
    /// When a file in it last changed, in unix seconds.
    pub changed_at: Option<i64>,
}

fn matching(c: &mut impl GenericClient, user_id: i64, lookup: Option<&str>) -> Result<Vec<Folder>, Error> {
    Ok(q::folders_for_user()
        .bind(c, &user_id, &lookup)
        .all()?
        .into_iter()
        .map(|f| Folder {
            id: f.id,
            public_id: f.public_id,
            name: f.name,
            key: f.key,
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
pub fn usable(c: &mut impl GenericClient, user_id: i64) -> Result<Vec<Folder>, Error> {
    matching(c, user_id, None)
}

/// The folder a user means by `lookup`: the name of one of their own, the `<owner>/<name>` or `master:<path>` of any, or its stable id.
pub fn find(c: &mut impl GenericClient, user_id: i64, lookup: &str) -> Result<Option<Folder>, Error> {
    Ok(matching(c, user_id, Some(lookup))?.into_iter().next())
}

/// The id and key of every master folder.
pub fn master_folders(c: &mut impl GenericClient) -> Result<Vec<(i64, String)>, Error> {
    Ok(q::master_folders().bind(c).all()?.into_iter().map(|f| (f.id, f.key)).collect())
}

/// The master folders that are `name`, inside it or around it.
pub fn master_names_overlapping(c: &mut impl GenericClient, name: &str) -> Result<Vec<String>, Error> {
    q::master_folder_names_under().bind(c, &name).all()
}

/// Makes a master folder, returning its key (`master:<name>`), or `None` when it exists.
pub fn insert_master(c: &mut impl GenericClient, name: &str, now: i64) -> Result<Option<String>, Error> {
    q::insert_master_folder().bind(c, &name, &now).opt()
}

/// The id, name and key of the master folder with this stable id.
pub fn master_by_public_id(c: &mut impl GenericClient, public_id: uuid::Uuid) -> Result<Option<(i64, String, String)>, Error> {
    Ok(q::master_folder_by_public_id().bind(c, &public_id).opt()?.map(|r| (r.id, r.name, r.key)))
}

/// A user a folder is shared with.
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

/// Deletes every file of a folder (live ones too), their versions and its log; returns the versions, whose stored copies
/// the caller removes once the transaction is committed.
pub fn delete_contents(c: &mut impl GenericClient, id: i64) -> Result<Vec<i64>, Error> {
    let files = q::folder_file_ids().bind(c, &id).all()?;
    let versions = super::retention::delete_versions_of(c, &files)?;
    super::retention::delete_files(c, &files)?;
    q::delete_folder_events().bind(c, &id)?;
    Ok(versions)
}

/// Removes a folder row; its state, members and albums go with it. Call [`delete_contents`] first.
pub fn delete(c: &mut impl GenericClient, id: i64) -> Result<bool, Error> {
    Ok(q::delete_folder().bind(c, &id)? == 1)
}

/// Whether a folder has files that are not deleted.
pub fn has_live_files(c: &mut impl GenericClient, id: i64) -> Result<bool, Error> {
    q::folder_has_live_files().bind(c, &id).one()
}

/// Removes a folder for good with everything left of it: the files in the trash, their versions and their log. Returns
/// the versions, whose stored copies the caller removes. Refuses (returns `None`) while the folder has files that are not deleted.
pub fn purge(c: &mut postgres::Client, id: i64) -> Result<Option<Vec<i64>>, Error> {
    let mut tx = c.transaction()?;
    // The folder is locked first, so that nothing is written to it between the check and the removal.
    if lock(&mut tx, id)?.is_none() || has_live_files(&mut tx, id)? {
        return Ok(None);
    }
    let versions = delete_contents(&mut tx, id)?;
    // Nothing is removed when the folder stays (a source writes into it): the transaction is dropped.
    if !delete(&mut tx, id)? {
        return Ok(None);
    }
    tx.commit()?;
    Ok(Some(versions))
}
