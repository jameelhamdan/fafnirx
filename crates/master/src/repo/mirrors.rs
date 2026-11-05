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

/// What restoring a backup mirror needs to know besides the storage's own credentials.
pub struct Key {
    pub kind: String,
    pub remote: String,
    pub remote_type: String,
    pub options: String,
    pub remote_path: String,
    pub crypt_password: String,
    pub crypt_salt: String,
}

pub fn key(c: &mut impl GenericClient, name: &str) -> Result<Option<Key>, Error> {
    Ok(q::mirror_key().bind(c, &name).opt()?.map(|m| {
        let config: Config = serde_json::from_str(&m.config).unwrap_or_default();
        Key {
            kind: m.kind,
            remote: config.remote,
            remote_type: config.remote_type,
            options: config.options,
            remote_path: config.remote_path,
            crypt_password: config.crypt_password,
            crypt_salt: config.crypt_salt,
        }
    }))
}

/// Every live file of the folder a mirror holds (everything when it holds none in particular), in the order they are uploaded.
pub fn live_files(c: &mut impl GenericClient, folder_id: Option<i64>) -> Result<Vec<Live>, Error> {
    q::live_files_for_mirror().bind(c, &folder_id).all()
}

/// The folder of every device, by id.
pub fn device_folders(c: &mut impl GenericClient) -> Result<Vec<(i64, String)>, Error> {
    Ok(q::device_folders().bind(c).all()?.into_iter().map(|d| (d.id, d.folder)).collect())
}

/// What a mirror holds, as (device id, path, root hash).
pub fn items(c: &mut impl GenericClient, mirror_id: i64) -> Result<Vec<(i64, String, Vec<u8>)>, Error> {
    Ok(links::items(c, mirror_id)?
        .into_iter()
        .filter_map(|i| Some((i.device_id, i.path, hex::decode(i.version).ok()?)))
        .collect())
}

pub fn remember_item(c: &mut impl GenericClient, mirror_id: i64, device_id: i64, path: &str, root_hash: &[u8]) -> Result<(), Error> {
    links::remember(c, mirror_id, device_id, path, &hex::encode(root_hash))
}

pub fn forget_item(c: &mut impl GenericClient, mirror_id: i64, device_id: i64, path: &str) -> Result<(), Error> {
    links::forget(c, mirror_id, device_id, path)
}
