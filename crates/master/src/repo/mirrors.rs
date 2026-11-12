//! Mirrors: what is theirs besides being a link (`links`): the storage, the passwords of a backup, and the live files they hold.

use crate::repo::links;
use fafnirx_db::client::GenericClient;
use fafnirx_db::queries::mirrors as q;
use postgres::Error;
use serde::Deserialize;

/// A live file that belongs on a mirror.
pub type Live = q::LiveFilesForMirror;

/// The settings of a mirror as they are stored (the `config` of its link).
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
pub struct Config {
    /// A remote of the rclone config file (`name:path`); when set, the type, options and path are not used.
    pub remote: String,
    pub remote_type: String,
    pub remote_path: String,
    /// The options of the backend, as JSON.
    pub options: String,
    pub crypt_password: String,
    pub crypt_salt: String,
    pub keep_runs: i64,
}

/// What restoring a backup mirror needs to know besides the storage's own credentials: its kind and its settings.
pub struct Key {
    pub kind: super::links::MirrorKind,
    pub config: Config,
}

pub fn key(c: &mut impl GenericClient, name: &str) -> Result<Option<Key>, Error> {
    Ok(q::mirror_key().bind(c, &name).opt()?.map(|m| Key {
        kind: super::links::MirrorKind::parse(&m.kind).unwrap_or_default(),
        config: serde_json::from_str(&m.config).unwrap_or_default(),
    }))
}

/// Every live file of the folder (or of the folders of the device) a mirror holds, everything when it holds none in particular, in
/// the order they are uploaded.
pub fn live_files(c: &mut impl GenericClient, folder_id: Option<i64>, device_id: Option<i64>) -> Result<Vec<Live>, Error> {
    q::live_files_for_mirror().bind(c, &folder_id, &device_id).all()
}

/// What a mirror holds: one folder, the folders of one device, or everything (neither set).
#[derive(Clone, Copy, Default)]
pub struct Scope {
    pub folder_id: Option<i64>,
    pub device_id: Option<i64>,
}

/// The scope named by a key: a device, a folder, or everything for none. `None` when nothing has that key.
pub fn scope_of(c: &mut impl GenericClient, key: &str) -> Result<Option<Scope>, Error> {
    if key.is_empty() {
        return Ok(Some(Scope::default()));
    }
    if let Some(id) = super::devices::id_of(c, key)? {
        return Ok(Some(Scope { device_id: Some(id), folder_id: None }));
    }
    Ok(super::folders::id_of(c, key)?.map(|id| Scope { folder_id: Some(id), device_id: None }))
}

/// The keys a mirror can be told to hold: every device and every folder.
pub fn scopes(c: &mut impl GenericClient) -> Result<Vec<String>, Error> {
    q::mirror_scopes().bind(c).all()
}

/// The key of every folder, by id.
pub fn folder_keys(c: &mut impl GenericClient) -> Result<Vec<(i64, String)>, Error> {
    Ok(q::folder_keys().bind(c).all()?.into_iter().map(|d| (d.id, d.key)).collect())
}

/// What a mirror holds, as (folder id, path, root hash).
pub fn items(c: &mut impl GenericClient, mirror_id: i64) -> Result<Vec<(i64, String, Vec<u8>)>, Error> {
    Ok(links::items(c, mirror_id)?
        .into_iter()
        .filter_map(|i| Some((i.folder_id, i.path, hex::decode(i.version).ok()?)))
        .collect())
}

pub fn remember_item(c: &mut impl GenericClient, mirror_id: i64, folder_id: i64, path: &str, root_hash: &[u8]) -> Result<(), Error> {
    links::remember(c, mirror_id, folder_id, path, &hex::encode(root_hash))
}

pub fn forget_item(c: &mut impl GenericClient, mirror_id: i64, folder_id: i64, path: &str) -> Result<(), Error> {
    links::forget(c, mirror_id, folder_id, path)
}
