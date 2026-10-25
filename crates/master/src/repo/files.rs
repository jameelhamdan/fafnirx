//! Files and their versions: one row per path of a device, and one version per distinct content it has had.

use crate::platform::pg::Pg;
use fafnirx_common::HASH_LEN;
use fafnirx_db::client::Params;
use fafnirx_db::queries::files as q;
use postgres::{Error, GenericClient};

/// The row of a path.
pub struct Row {
    pub id: i64,
    pub current: Option<i64>,
    pub deleted_at: Option<i64>,
}

impl Row {
    /// The current version, unless the path is deleted.
    pub fn live_version(&self) -> Option<i64> {
        self.current.filter(|_| self.deleted_at.is_none())
    }
}

/// The row of a path, locked until the transaction ends.
pub fn lock_row(c: &mut impl GenericClient, device_id: i64, path: &str) -> Result<Option<Row>, Error> {
    Ok(q::lock_file_row().bind(&mut Pg(c), &device_id, &path).opt()?.map(|r| Row {
        id: r.id,
        current: r.current_version_id,
        deleted_at: r.deleted_at,
    }))
}

/// Whether a version has exactly this content.
pub fn has_content(c: &mut impl GenericClient, version_id: i64, root: &[u8]) -> Result<bool, Error> {
    q::version_has_content().params(&mut Pg(c), &q::VersionHasContentParams { root, version_id }).one()
}

/// The id of the row of a path, made if the path is new.
pub fn ensure(c: &mut impl GenericClient, device_id: i64, path: &str) -> Result<i64, Error> {
    q::ensure_file().bind(&mut Pg(c), &device_id, &path).one()
}

/// A version to record.
pub struct NewVersion<'a> {
    pub file_id: i64,
    pub root: &'a [u8],
    pub size: u64,
    /// Unix nanoseconds.
    pub mtime: i64,
    pub mode: u32,
    pub created_at: i64,
}

pub fn insert_version(c: &mut impl GenericClient, v: &NewVersion) -> Result<i64, Error> {
    q::insert_version()
        .params(
            &mut Pg(c),
            &q::InsertVersionParams {
                file_id: v.file_id,
                root: v.root,
                size: v.size as i64,
                mtime: v.mtime,
                mode: i64::from(v.mode),
                created_at: v.created_at,
            },
        )
        .one()
}

/// Makes a version the current one of its file, undeleting it.
pub fn make_current(c: &mut impl GenericClient, file_id: i64, version_id: i64) -> Result<(), Error> {
    q::make_current().bind(&mut Pg(c), &version_id, &file_id)?;
    Ok(())
}

/// Keeps the newest `keep` versions of a file; older ones are marked pruned and removed by retention.
pub fn prune_versions(c: &mut impl GenericClient, file_id: i64, keep: i64, ts: i64) -> Result<(), Error> {
    q::prune_versions().bind(&mut Pg(c), &ts, &file_id, &keep)?;
    Ok(())
}

/// Remembers the modification time of a version whose content did not change.
pub fn set_version_mtime(c: &mut impl GenericClient, version_id: i64, mtime: i64) -> Result<(), Error> {
    q::set_version_mtime().bind(&mut Pg(c), &mtime, &version_id)?;
    Ok(())
}

/// Whether the kind, size and EXIF data of this content were read already.
pub fn content_known(c: &mut impl GenericClient, root: &[u8]) -> Result<bool, Error> {
    q::content_known().bind(&mut Pg(c), &root).one()
}

/// Path of a live file of the device with this content, preferring `prefer`.
pub fn live_path_with_root(
    c: &mut impl GenericClient,
    device_id: i64,
    root: &[u8],
    prefer: Option<&str>,
) -> Result<Option<String>, Error> {
    q::live_path_with_root().bind(&mut Pg(c), &device_id, &root, &prefer.unwrap_or("")).opt()
}

/// Whether the path was deleted in the web app.
pub fn deleted_in_web_app(c: &mut impl GenericClient, device_id: i64, path: &str) -> Result<bool, Error> {
    Ok(q::deleted_in_web_app().bind(&mut Pg(c), &device_id, &path).opt()?.is_some())
}

/// (root hash, size) of the live (not deleted) version of a path, if any.
pub fn live_content(
    c: &mut impl GenericClient,
    device_id: i64,
    path: &str,
) -> Result<Option<([u8; HASH_LEN], u64)>, Error> {
    let found = q::live_content().bind(&mut Pg(c), &device_id, &path).opt()?;
    Ok(found.and_then(|v| Some((v.root_hash.try_into().ok()?, v.size as u64))))
}

/// Marks a live path deleted. Returns the version that was current (0 when there was none), or `None` when the
/// path was not live.
pub fn delete_path(
    c: &mut impl GenericClient,
    device_id: i64,
    path: &str,
    ts: i64,
    in_web_app: bool,
) -> Result<Option<i64>, Error> {
    let by = in_web_app.then_some("web");
    let params = q::DeletePathParams { ts, deleted_by: by, device_id, path };
    Ok(q::delete_path().params(&mut Pg(c), &params).opt()?.map(|v| v.unwrap_or(0)))
}

/// A file a source collected stays deleted until it changes at the source, whatever the retention period does.
pub fn keep_source_item_deleted(c: &mut impl GenericClient, device_id: i64, path: &str) -> Result<(), Error> {
    q::keep_source_item_deleted().bind(&mut Pg(c), &path, &device_id)?;
    Ok(())
}

/// A file as the detail view shows it: its current version and what is known about the content.
pub type Info = q::FileInfo;
/// One version of a file.
pub type VersionRow = q::FileVersions;
/// A version that can be brought back, with the state of its file.
pub type Restorable = q::VersionToRestore;
/// A folder below a path, with what it holds.
pub type Folder = q::FoldersBelow;

pub fn info(c: &mut impl GenericClient, device_id: i64, path: &str) -> Result<Option<Info>, Error> {
    q::file_info().bind(&mut Pg(c), &device_id, &path).opt()
}

/// The versions of a file, newest first.
pub fn versions(c: &mut impl GenericClient, file_id: i64) -> Result<Vec<VersionRow>, Error> {
    q::file_versions().bind(&mut Pg(c), &file_id).all()
}

/// The paths a selection stands for: a file, or every file under a folder.
pub fn expand(
    c: &mut impl GenericClient,
    device_id: i64,
    path: &str,
    include_deleted: bool,
) -> Result<Vec<String>, Error> {
    let below = crate::platform::db::like_prefix(&format!("{path}/"));
    q::expand_paths().params(&mut Pg(c), &q::ExpandPathsParams { device_id, path, below, include_deleted }).all()
}

pub fn version_to_restore(
    c: &mut impl GenericClient,
    device_id: i64,
    path: &str,
    version_id: i64,
) -> Result<Option<Restorable>, Error> {
    q::version_to_restore().bind(&mut Pg(c), &device_id, &path, &version_id).opt()
}

/// The folders directly below a path (`below` is the `LIKE` pattern of the path, `from` the 1-based character
/// position where the folder names start).
pub fn folders_below(
    c: &mut impl GenericClient,
    device_id: i64,
    below: &str,
    include_deleted: bool,
    from: i32,
) -> Result<Vec<Folder>, Error> {
    q::folders_below().bind(&mut Pg(c), &from, &device_id, &below, &include_deleted).all()
}

/// A path as the transfer code needs it: its row, which may be deleted.
pub struct Located {
    pub id: i64,
    pub current: Option<i64>,
    pub deleted_at: Option<i64>,
}

pub fn locate(c: &mut impl GenericClient, device_id: i64, path: &str) -> Result<Option<Located>, Error> {
    Ok(q::file_row().bind(&mut Pg(c), &device_id, &path).opt()?.map(|r| Located {
        id: r.id,
        current: r.current_version_id,
        deleted_at: r.deleted_at,
    }))
}

/// (root hash, size) of a version of a file.
pub fn version_content(
    c: &mut impl GenericClient,
    file_id: i64,
    version_id: i64,
) -> Result<Option<(Vec<u8>, i64)>, Error> {
    Ok(q::version_of_file().bind(&mut Pg(c), &version_id, &file_id).opt()?.map(|v| (v.root_hash, v.size)))
}

/// The file with this id, with its device and current content.
pub type ById = q::FileById;

pub fn by_id(c: &mut impl GenericClient, id: i64) -> Result<Option<ById>, Error> {
    q::file_by_id().bind(&mut Pg(c), &id).opt()
}

/// The modification time (unix nanoseconds) of the current version of a path.
pub fn current_mtime(c: &mut impl GenericClient, device_id: i64, path: &str) -> Result<i64, Error> {
    q::current_mtime().bind(&mut Pg(c), &device_id, &path).one()
}

/// The live files at or below a path as (path, size, mtime), for comparing with what is on disk.
pub fn live_below(c: &mut impl GenericClient, device_id: i64, path: &str) -> Result<Vec<(String, i64, i64)>, Error> {
    let below = crate::platform::db::like_prefix(&format!("{path}/"));
    Ok(q::live_files_below()
        .bind(&mut Pg(c), &device_id, &path, &below)
        .all()?
        .into_iter()
        .map(|f| (f.path, f.size, f.mtime))
        .collect())
}
