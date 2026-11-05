//! Files and their versions: one row per path of a device, and one version per distinct content it has had.

use fafnirx_common::HASH_LEN;
use fafnirx_db::client::GenericClient;
use fafnirx_db::client::Params;
use fafnirx_db::queries::files as q;
use postgres::Error;
use uuid::Uuid;

/// The row of a path.
pub struct Row {
    pub id: Uuid,
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
    Ok(q::lock_file_row()
        .bind(c, &device_id, &path)
        .opt()?
        .map(|r| Row { id: r.id, current: r.current_version_id, deleted_at: r.deleted_at }))
}

/// Whether a version has exactly this content.
pub fn has_content(c: &mut impl GenericClient, version_id: i64, root: &[u8]) -> Result<bool, Error> {
    q::version_has_content().params(c, &q::VersionHasContentParams { root, version_id }).one()
}

/// The id of the row of a path, made if the path is new.
pub fn ensure(c: &mut impl GenericClient, device_id: i64, path: &str) -> Result<Uuid, Error> {
    q::ensure_file().bind(c, &device_id, &path).one()
}

/// A version to record.
pub struct NewVersion<'a> {
    pub file_id: Uuid,
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
            c,
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
pub fn make_current(c: &mut impl GenericClient, file_id: Uuid, version_id: i64) -> Result<(), Error> {
    q::make_current().bind(c, &version_id, &file_id).map(drop)
}

/// Whether the kind, size and EXIF data of this content were read already.
pub fn content_known(c: &mut impl GenericClient, root: &[u8]) -> Result<bool, Error> {
    q::content_known().bind(c, &root).one()
}

/// Path of a live file of the device with this content, preferring `prefer`.
pub fn live_path_with_root(c: &mut impl GenericClient, device_id: i64, root: &[u8], prefer: Option<&str>) -> Result<Option<String>, Error> {
    q::live_path_with_root().bind(c, &device_id, &root, &prefer.unwrap_or("")).opt()
}

/// A live path of the device that is the same name as `path` but spelled differently (another case), unless `path` is itself
/// there. The path `path` is renamed from does not count: it is on its way out.
pub fn case_clash(c: &mut impl GenericClient, device_id: i64, path: &str, renamed_from: Option<&str>) -> Result<Option<String>, Error> {
    q::case_clash().bind(c, &device_id, &path, &renamed_from.unwrap_or("")).opt()
}

/// Whether the path was deleted in the web app.
pub fn deleted_in_web_app(c: &mut impl GenericClient, device_id: i64, path: &str) -> Result<bool, Error> {
    Ok(q::deleted_in_web_app().bind(c, &device_id, &path).opt()?.is_some())
}

/// (root hash, size) of the live (not deleted) version of a path, if any.
pub fn live_content(c: &mut impl GenericClient, device_id: i64, path: &str) -> Result<Option<([u8; HASH_LEN], u64)>, Error> {
    let found = q::live_content().bind(c, &device_id, &path).opt()?;
    Ok(found.and_then(|v| Some((v.root_hash.try_into().ok()?, v.size as u64))))
}

/// A path of a device that was written or deleted since some time, as it is now.
pub struct Change {
    pub path: String,
    /// The content hash of a live path; `None` for a deleted one (and then the rest is zero).
    pub root: Option<[u8; HASH_LEN]>,
    pub size: u64,
    /// Unix nanoseconds.
    pub mtime: i64,
    pub mode: u32,
    /// A deletion made in the web app.
    pub from_web: bool,
}
/// The paths of a device written or deleted at or after `since` (unix seconds; 0 for everything). Needs no history:
/// it reads the state of the files, so it works however long the client was away, as long as the trash keeps the deletion.
pub fn changes_since(c: &mut impl GenericClient, device_id: i64, since: i64) -> Result<Vec<Change>, Error> {
    let rows = q::changes_since().bind(c, &device_id, &since).all()?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let root = if r.deleted_at.is_none() { r.root_hash.and_then(|h| h.try_into().ok()) } else { None };
            let live = root.is_some();
            Change {
                path: r.path,
                root,
                size: if live { r.size.unwrap_or(0) as u64 } else { 0 },
                mtime: if live { r.mtime.unwrap_or(0) } else { 0 },
                mode: if live { r.mode.unwrap_or(0) as u32 } else { 0 },
                from_web: !live && r.deleted_by.as_deref() == Some("web"),
            }
        })
        .collect())
}

/// Marks a live path deleted. Returns the version that was current (0 when there was none), or `None` when the
/// path was not live.
pub fn delete_path(c: &mut impl GenericClient, device_id: i64, path: &str, ts: i64, in_web_app: bool) -> Result<Option<i64>, Error> {
    let by = in_web_app.then_some("web");
    let params = q::DeletePathParams { ts, deleted_by: by, device_id, path };
    Ok(q::delete_path().params(c, &params).opt()?.map(|v| v.unwrap_or(0)))
}

/// Takes the old path of a rename out of the live tree. Unlike [`delete_path`] it is neither logged nor shown in the trash.
/// Returns the version that was current (0 when there was none), or `None` when the path was not live.
pub fn vacate_renamed(c: &mut impl GenericClient, device_id: i64, path: &str, ts: i64) -> Result<Option<i64>, Error> {
    Ok(q::vacate_renamed().bind(c, &ts, &device_id, &path).opt()?.map(|v| v.unwrap_or(0)))
}

/// One version of a file.
pub type VersionRow = q::FileVersions;

/// A folder below a path, with what it holds.
pub type Folder = q::FoldersBelow;

/// The versions of a file, newest first.
pub fn versions(c: &mut impl GenericClient, file_id: Uuid) -> Result<Vec<VersionRow>, Error> {
    q::file_versions().bind(c, &file_id).all()
}

/// The paths a selection stands for: a file, or every file under a folder.
pub fn expand(c: &mut impl GenericClient, device_id: i64, path: &str, include_deleted: bool) -> Result<Vec<String>, Error> {
    let below = crate::platform::db::like_prefix(&format!("{path}/"));
    q::expand_paths().params(c, &q::ExpandPathsParams { device_id, path, below, include_deleted }).all()
}

/// The folders directly below a path (`below` is the `LIKE` pattern of the path, `from` the 1-based character
/// position where the folder names start).
pub fn folders_below(c: &mut impl GenericClient, device_id: i64, below: &str, include_deleted: bool, from: i32) -> Result<Vec<Folder>, Error> {
    q::folders_below().bind(c, &from, &device_id, &below, &include_deleted).all()
}

/// A path as the transfer code needs it: its row, which may be deleted.
pub struct Located {
    pub id: Uuid,
    pub current: Option<i64>,
    pub deleted_at: Option<i64>,
}

pub fn locate(c: &mut impl GenericClient, device_id: i64, path: &str) -> Result<Option<Located>, Error> {
    Ok(q::file_row()
        .bind(c, &device_id, &path)
        .opt()?
        .map(|r| Located { id: r.id, current: r.current_version_id, deleted_at: r.deleted_at }))
}

/// (root hash, size) of a version of a file.
pub fn version_content(c: &mut impl GenericClient, file_id: Uuid, version_id: i64) -> Result<Option<(Vec<u8>, i64)>, Error> {
    Ok(q::version_of_file().bind(c, &version_id, &file_id).opt()?.map(|v| (v.root_hash, v.size)))
}

/// The live files at or below a path as (path, size, mtime), for comparing with what is on disk.
pub fn live_below(c: &mut impl GenericClient, device_id: i64, path: &str) -> Result<Vec<(String, i64, i64)>, Error> {
    let below = crate::platform::db::like_prefix(&format!("{path}/"));
    Ok(q::live_files_below()
        .bind(c, &device_id, &path, &below)
        .all()?
        .into_iter()
        .map(|f| (f.path, f.size, f.mtime))
        .collect())
}

/// Where a live file is and which content it holds: (device folder, path, root hash).
pub fn location(c: &mut impl GenericClient, id: Uuid) -> Result<Option<(String, String, Vec<u8>)>, Error> {
    Ok(q::file_location().bind(c, &id).opt()?.map(|f| (f.folder, f.path, f.root_hash)))
}

/// The live files directly in the folder that holds `path`, as (path, root hash): the neighbours of a file.
pub fn neighbours(c: &mut impl GenericClient, device: &str, path: &str) -> Result<Vec<(String, Vec<u8>)>, Error> {
    let prefix = path.rfind('/').map_or(String::new(), |i| path[..=i].to_string());
    Ok(q::files_in_folder()
        .bind(c, &device, &prefix)
        .all()?
        .into_iter()
        .map(|f| (f.path, f.root_hash))
        .collect())
}

/// A version's content and attributes: root hash, size, mtime in nanoseconds, mode.
pub type Record = q::VersionRecord;

pub fn version_record(c: &mut impl GenericClient, version_id: i64) -> Result<Option<Record>, Error> {
    q::version_record().bind(c, &version_id).opt()
}
