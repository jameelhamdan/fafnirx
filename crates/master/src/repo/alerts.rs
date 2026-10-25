//! Alerts: what is wrong and needs somebody's attention, one open alert per kind and key.

use crate::platform::db::notify;
use crate::platform::pg::Pg;
use fafnirx_common::unix_now;
use fafnirx_db::client::Params;
use fafnirx_db::queries::alerts as q;
use postgres::{Error, GenericClient};

/// Opens an alert, or refreshes the message of the already open one.
pub fn upsert(c: &mut impl GenericClient, kind: &str, key: &str, message: &str) -> Result<(), Error> {
    q::upsert_alert().bind(&mut Pg(&mut *c), &kind, &key, &message, &unix_now())?;
    notify(c, "alerts", key)
}

/// Closes the open alert of this kind and key, if there is one.
pub fn resolve(c: &mut impl GenericClient, kind: &str, key: &str) -> Result<(), Error> {
    if q::resolve_alert().bind(&mut Pg(&mut *c), &unix_now(), &kind, &key)? > 0 {
        notify(c, "alerts", key)?;
    }
    Ok(())
}

/// Closes the open alert of this kind and key when it was last raised before `before`. Returns whether it did.
pub fn resolve_if_quiet(c: &mut impl GenericClient, kind: &str, key: &str, before: i64) -> Result<bool, Error> {
    let params = q::ResolveStaleAlertsParams { now: unix_now(), kind, key, before };
    Ok(q::resolve_stale_alerts().params(&mut Pg(c), &params)? > 0)
}
