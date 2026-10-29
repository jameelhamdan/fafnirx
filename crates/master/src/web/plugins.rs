//! The plugins list (administrators only): which modules there are and in what state. Plugins are installed and removed by the
//! server administrator, not through the web app. See `modules::plugins`.

use super::access::Access;
use super::{Res, Shared};
use crate::modules::PluginStatus;
use axum::extract::State;
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
