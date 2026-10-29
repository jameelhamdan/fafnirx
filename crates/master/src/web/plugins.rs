//! The plugins page's API (administrators only): which modules there are and in what state, which plugins the release of this
//! version carries, and installing and removing them. See `modules::plugins`.

use super::access::Access;
use super::{done, ApiError, Done, Res, Shared};
use crate::modules::{Offer, PluginStatus};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::Serialize;

#[derive(Serialize)]
pub struct Plugins {
    installed: Vec<PluginStatus>,
}

pub async fn list(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Plugins> {
    who.require_admin()?;
    Ok(Json(Plugins { installed: state.plugins.statuses() }))
}

/// What the release of this version of the master offers. Asked for apart from the list, because it goes to the internet.
pub async fn available(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<Offer>> {
    who.require_admin()?;
    let offers = state.plugins.available().await.map_err(|e| ApiError(StatusCode::BAD_GATEWAY, format!("{e:#}")))?;
    Ok(Json(offers))
}

pub async fn install(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(id): Path<String>,
) -> Res<Done> {
    who.require_admin()?;
    let version = state.plugins.install(&id).await.map_err(|e| ApiError(StatusCode::BAD_GATEWAY, format!("{e:#}")))?;
    tracing::info!("{} installed the {id} plugin {version}", who.name);
    done()
}

pub async fn remove(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(id): Path<String>,
) -> Res<Done> {
    who.require_admin()?;
    state.plugins.remove(&id).await.map_err(|e| ApiError::bad(format!("{e:#}")))?;
    tracing::info!("{} removed the {id} plugin", who.name);
    done()
}
