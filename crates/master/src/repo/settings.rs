//! Settings the administrator can change (versions kept, retention) and what the master remembers about itself.

use crate::platform::pg::Pg;
use fafnirx_db::queries::settings as q;
use postgres::{Error, GenericClient};

pub fn get(c: &mut impl GenericClient, key: &str) -> Result<Option<String>, Error> {
    q::get_setting().bind(&mut Pg(c), &key).opt()
}

/// A numeric setting, or `default` when it is not set, not a number, or cannot be read.
pub fn get_i64(c: &mut impl GenericClient, key: &str, default: i64) -> i64 {
    get(c, key).ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(default)
}

pub fn set(c: &mut impl GenericClient, key: &str, value: &str) -> Result<(), Error> {
    q::set_setting().bind(&mut Pg(c), &key, &value)?;
    Ok(())
}
