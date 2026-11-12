//! Renaming a folder of a user (a `user` one, shared or not) or a master folder. Its files, history, albums, sources and members stay with it: they
//! point at the folder row, which stays. What is kept under the old `<user>/<name>` is moved or rewritten here: the folder on
//! disk, what the versions kept of it, the queued jobs, the open alerts, what folders said about it, and the mirrors that
//! copy it.

use crate::files::livetree;
use crate::platform::db;
use crate::platform::store::Store;
use crate::web::access::Access;
use crate::web::input;
use crate::web::ApiError;
use fafnirx_db::queries::folders as q;
use postgres::Client;

/// Gives the folder `folder` the name `new_name`. Returns the new folder. Only the owner (or an administrator) may rename a folder
/// of a user; only an administrator a master folder.
pub fn rename(c: &mut Client, store: &Store, who: &Access, folder: &str, new_name: &str) -> anyhow::Result<String> {
    let master = crate::platform::store::is_master(folder);
    let new_name = if master {
        who.require_admin()?;
        // Checked before the folder is locked: it asks the database too.
        super::master::check(c, new_name, folder.strip_prefix(crate::platform::store::MASTER_PREFIX))?
    } else {
        input::name(new_name)?
    };
    let new_name = new_name.as_str();
    // Nothing is looked at in the live tree meanwhile.
    let _layout = livetree::layout();
    let mut tx = c.transaction()?;
    let Some(row) = q::folder_for_rename().bind(&mut tx, &folder).opt()? else {
        return Err(ApiError::not_found("No such folder").into());
    };
    if !master && !who.is_admin() && row.user_name != who.name {
        // Not theirs to see, or only seen through a share.
        return Err(ApiError::not_found("No such folder").into());
    }
    if row.name == new_name {
        return Ok(row.key);
    }
    if q::name_taken().bind(&mut tx, &row.user_id, &new_name).one()? {
        return Err(ApiError::conflict(format!("There is a device or folder called {new_name} already")).into());
    }
    let versions: Vec<i64> = q::folder_version_ids().bind(&mut tx, &row.id).all()?;
    let new_folder = q::rename_folder().bind(&mut tx, &new_name, &row.id).one()?;
    q::rename_folder_in_jobs().bind(&mut tx, &new_folder, &row.key)?;
    q::rename_folder_in_alerts().bind(&mut tx, &new_folder, &row.key)?;
    if let Err(e) = store.rename_folder(&row.key, &new_folder, &versions) {
        // A part may have moved: put it back.
        let _ = store.rename_folder(&new_folder, &row.key, &versions);
        return Err(ApiError::conflict(format!("The folder cannot be moved on disk: {e:#}")).into());
    }
    if let Err(e) = db::notify(&mut tx, db::Topic::Folders, &new_folder).and_then(|_| tx.commit()) {
        let _ = store.rename_folder(&new_folder, &row.key, &versions);
        return Err(e.into());
    }
    // Tell the pages and folders that had the old name.
    for key in [&row.key, &new_folder] {
        db::notify(c, db::Topic::Folders, key)?;
        db::notify(c, db::Topic::Files, key)?;
    }
    Ok(new_folder)
}
