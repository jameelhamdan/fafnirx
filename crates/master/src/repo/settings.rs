//! Settings the administrator can change (versions kept, retention) and what the master remembers about itself.

use fafnirx_db::client::GenericClient;
use fafnirx_db::queries::settings as q;
use postgres::Error;

/// The time (unix seconds) from which the master still knows every deletion: older ones may have been removed by retention.
pub const DELETIONS_KNOWN_FROM: &str = "deletions_known_from";

/// Whether the photo pages (timeline, favorites, albums, archive) are offered. On when it is not set. Only the web app looks at it.
pub const PHOTOS: &str = "photos";

pub fn photos_on(c: &mut impl GenericClient) -> bool {
    get_i64(c, PHOTOS, 1) != 0
}

pub fn get(c: &mut impl GenericClient, key: &str) -> Result<Option<String>, Error> {
    q::get_setting().bind(c, &key).opt()
}

/// A numeric setting, or `default` when it is not set, not a number, or cannot be read.
pub fn get_i64(c: &mut impl GenericClient, key: &str, default: i64) -> i64 {
    get(c, key).ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Days a deleted file stays recoverable before retention removes it.
pub fn retention_days(c: &mut impl GenericClient) -> i64 {
    get_i64(c, "retention_days", 30)
}

/// How many versions of a file are kept.
pub fn version_count(c: &mut impl GenericClient) -> i64 {
    get_i64(c, "version_count", 5)
}

pub fn set(c: &mut impl GenericClient, key: &str, value: &str) -> Result<(), Error> {
    q::set_setting().bind(c, &key, &value).map(drop)
}
