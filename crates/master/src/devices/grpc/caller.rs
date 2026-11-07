//! Who is calling the device-facing API, which device or folder a call works on, and the lease that lets one computer at a time use a
//! device.

use super::sanitize;
use crate::devices::call::{internal, CallError};
use crate::platform::db::{self, Device};
use crate::repo::{alerts, devices};
use fafnirx_common::unix_now;
use postgres::Client;
use tonic::{Request, Status};

pub(super) fn bearer<T>(req: &Request<T>) -> Result<String, Status> {
    req.metadata()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned)
        .ok_or_else(|| Status::unauthenticated("missing device token"))
}

/// Who is calling: the token (which device) and the instance (which installation of the client is using it).
#[derive(Clone)]
pub struct Caller {
    pub(super) token: String,
    pub(super) instance: String,
    pub(super) ip: Option<String>,
    /// The folder the call acts on instead of the device, named in the `x-fafnirx-folder` metadata.
    pub(super) folder: Option<String>,
}

/// The instance of a caller that does not name one: all such callers count as one computer.
pub(super) const UNNAMED_INSTANCE: &str = "unnamed";

/// The address a request comes from: the connection's, or behind a trusted proxy the one the proxy saw.
pub(super) fn address_of<T>(req: &Request<T>, trusted: &crate::platform::proxy::Trusted) -> Option<String> {
    let forwarded = req.metadata().get("x-forwarded-for").and_then(|v| v.to_str().ok());
    req.remote_addr().map(|a| trusted.client_ip(a.ip(), forwarded).to_string())
}

pub(super) fn caller_of<T>(req: &Request<T>, trusted: &crate::platform::proxy::Trusted) -> Result<Caller, Status> {
    let instance = req
        .metadata()
        .get("x-fafnirx-instance")
        .and_then(|v| v.to_str().ok())
        .map(|v| sanitize(v, 64))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| UNNAMED_INSTANCE.into());
    let folder = req
        .metadata()
        .get("x-fafnirx-folder")
        .and_then(|v| v.to_str().ok())
        .map(|v| sanitize(&fafnirx_common::header_decode(&sanitize(v, 700)), 130))
        .filter(|v| !v.is_empty());
    Ok(Caller { token: bearer(req)?, instance, ip: address_of(req, trusted), folder })
}

/// Where a call works: a device (a computer, or a folder it names). Paths in the messages are relative to it.
pub(super) struct Place {
    pub(super) device: Device,
    /// A call that names a folder: what is written there has to be named so that every kind of computer can hold it.
    pub(super) folder: bool,
}

/// The folder a caller names in `x-fafnirx-folder` (or in the message): any folder the user can use is found the same way, by its
/// name or its stable id, whoever owns it. The owner of a folder reaches it through any of their devices; another user only when the
/// owner added them (or it is a master folder), and to change it only when they may write. Without a name the call is about the device.
pub(super) fn place(conn: &mut Client, who: &Caller, write: bool) -> Result<Place, CallError> {
    let (dev, user) = authenticate_user(conn, who)?;
    let Some(key) = &who.folder else { return Ok(Place { device: dev, folder: false }) };
    let found = devices::folder(conn, user, key)?.ok_or_else(|| Status::not_found(format!("no folder called \"{key}\"")))?;
    if write && !found.can_write {
        // Not PermissionDenied, which is about the device (in use by another computer) and stops the whole client.
        return Err(Status::failed_precondition(format!("\"{key}\" is shared with you to read only")).into());
    }
    Ok(Place { device: Device { id: found.id, name: found.folder }, folder: true })
}

/// How long a device that is not heard from keeps its credentials to itself. After that another computer using the
/// same token takes the device over (a reinstalled or replaced computer), and the first one is the one refused.
pub(super) fn lease_secs() -> i64 {
    std::env::var("FAFNIRX_INSTANCE_LEASE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(90)
}

/// Finds the device of the token and makes sure the caller is the one computer using it.
///
/// A device is used by one installation of the client at a time: the first to connect holds it, and while it is
/// heard from (heartbeats, and everything it uploads, set `last_seen`) another is refused and an alert is raised for the
/// owner, so that two computers with one token cannot send two views of the same folders. One decision, in one
/// statement, so two computers connecting at once cannot both win.
pub(super) fn authenticate_user(conn: &mut Client, who: &Caller) -> Result<(Device, i64), CallError> {
    // The device is its folder (`<user>/<name>`). A device of a disabled user is refused like a revoked one.
    let found = devices::find_by_token(conn, &db::hash_token(&who.token))?.ok_or_else(|| Status::unauthenticated("unknown or revoked device token"))?;
    let (id, folder, user) = (found.id, found.folder, found.user_id);
    let holder = found.instance;
    if holder.as_deref() == Some(who.instance.as_str()) {
        return Ok((Device { id, name: folder }, user));
    }
    let now = unix_now();
    if devices::take_over(conn, id, &who.instance, now, now - lease_secs()).map_err(internal)? {
        if holder.is_some() {
            tracing::info!("{folder}: taken over by another computer, the first one went quiet");
            let _ = alerts::resolve(conn, "device_conflict", &folder);
        }
        return Ok((Device { id, name: folder }, user));
    }
    let (host, ip) = (found.hostname, found.ip);
    let holder_is = match (host, ip) {
        (Some(h), Some(i)) => format!("{h}, {i}"),
        (Some(h), None) => h,
        (None, Some(i)) => i,
        (None, None) => "another computer".into(),
    };
    let caller_is = who.ip.as_deref().unwrap_or("unknown address");
    let _ = alerts::upsert(
        conn,
        "device_conflict",
        &folder,
        &format!(
            "A second computer ({caller_is}) is using the credentials of this device while {holder_is} is connected. \
             It was refused. If the credentials leaked, rotate the token; to move the device to the other computer, release it."
        ),
    );
    Err(Status::permission_denied(format!(
        "device in use: another computer ({holder_is}) is connected as this device. This one sends nothing until \
         that one goes quiet or the device is released in the web app"
    ))
    .into())
}

pub(super) fn authenticate(conn: &mut Client, who: &Caller) -> Result<Device, CallError> {
    Ok(authenticate_user(conn, who)?.0)
}

/// The caller acting on the folder a request names (the calls that name it in the message, not in the metadata).
pub(super) fn on_folder(who: Caller, folder: String) -> Result<Caller, Status> {
    if folder.is_empty() {
        return Err(Status::invalid_argument("name the folder"));
    }
    Ok(Caller { folder: Some(folder), ..who })
}
