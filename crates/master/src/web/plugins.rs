//! The plugins: the list the administrator sees (which modules are connected and in what state), and the call with which a plugin
//! announces itself. Plugins are containers the server administrator starts; nothing is installed or changed through the web app.

use super::access::Access;
use super::{done, ApiError, Done, Res, Shared};
use crate::modules::{Refusal, Status};
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct Plugins {
    plugins: Vec<Status>,
}

pub async fn list(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Plugins> {
    who.require_admin()?;
    Ok(Json(Plugins { plugins: state.modules.plugins() }))
}

#[derive(Deserialize)]
pub struct Announcement {
    id: String,
    /// Where the master reaches it.
    address: String,
}

/// A plugin says where it is. There is no login: the shared secret (`FAFNIRX_PLUGIN_SECRET`) as bearer token is the credential.
pub async fn register(State(state): State<Shared>, headers: HeaderMap, Json(body): Json<Announcement>) -> Res<Done> {
    let given = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    match state.modules.register(given, &body.id, &body.address) {
        Ok(()) => done(),
        Err(Refusal::Secret(why)) => Err(ApiError(StatusCode::UNAUTHORIZED, why.into())),
        Err(Refusal::Invalid(why)) => Err(ApiError(StatusCode::BAD_REQUEST, why)),
    }
}
