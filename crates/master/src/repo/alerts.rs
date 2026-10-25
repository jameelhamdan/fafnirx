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

/// An alert as the dashboard shows it.
pub type Open = q::OpenAlerts;

/// Every open alert, newest first. Who may see which one is decided by the caller.
pub fn open(c: &mut impl GenericClient) -> Result<Vec<Open>, Error> {
    q::open_alerts().bind(&mut Pg(c)).all()
}

/// The key (a device folder, a source or a mirror) an alert is about.
pub fn key_of(c: &mut impl GenericClient, id: i64) -> Result<Option<String>, Error> {
    q::alert_key().bind(&mut Pg(c), &id).opt()
}

/// Closes an alert by hand.
pub fn dismiss(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::dismiss_alert().bind(&mut Pg(&mut *c), &unix_now(), &id)?;
    notify(c, "alerts", "")
}

/// Clients report an error again every time they retry, so an upload error that went quiet has been fixed.
pub fn expire_upload_errors(c: &mut impl GenericClient, quiet_since: i64) -> Result<(), Error> {
    q::expire_upload_errors().bind(&mut Pg(c), &unix_now(), &quiet_since)?;
    Ok(())
}

/// Forgets alerts that were closed before `before`.
pub fn purge_resolved(c: &mut impl GenericClient, before: i64) -> Result<(), Error> {
    q::purge_resolved_alerts().bind(&mut Pg(c), &before)?;
    Ok(())
}
