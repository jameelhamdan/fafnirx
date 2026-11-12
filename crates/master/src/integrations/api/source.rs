//! Sources: a storage or a module's kind copied into one folder of a user. Added, edited, run now, switched and removed.

use super::backend_options;
use super::find_source;
use super::hours_to_secs;
use super::is_secret;
use super::rclone_config;
use super::touch_link;
use super::Touch;
use super::SOURCE_PREFIX;
use crate::files::browse::find_folder_to_change;
use crate::files::ingest;
use crate::integrations::fetch_source;
use crate::integrations::spec::normalize_storage;
use crate::platform::db;
use crate::platform::store::Store;
use crate::repo::alerts;
use crate::repo::folders;
use crate::repo::links;
use crate::repo::links::Dir;
use crate::repo::links::SourceKind;
use crate::repo::sources;
use crate::web::access::{Access, Ctx};
use crate::web::act;
use crate::web::input;
use crate::web::run;
use crate::web::ApiError;
use crate::web::Done;
use crate::web::Res;
use axum::extract::Path;
use axum::Json;
use fafnirx_common::unix_now;
use fafnirx_common::valid_name;
use postgres::Client;
use serde::Deserialize;
use serde_json::Value;

fn day() -> f64 {
    24.0
}

#[derive(Deserialize)]
pub struct NewSource {
    name: String,
    /// How the source fetches: `rclone` (any storage, and Google Drive) or a kind a module reads (the module lists `fetch.<kind>`).
    /// A request without it is a storage read through rclone.
    #[serde(default)]
    kind: SourceKind,
    /// The settings of a kind a module reads (see `fetch_source::validate`); rclone sources use the fields below instead.
    #[serde(default)]
    config: Value,
    /// A remote of the rclone config file (`name:path`); when set, the backend, options and path are not used.
    #[serde(default)]
    remote: String,
    #[serde(default)]
    remote_type: String,
    #[serde(default)]
    options: String,
    /// Passwords that rclone keeps in a scrambled form, given plain: key -> password.
    #[serde(default)]
    secrets: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    remote_path: String,
    #[serde(default = "day")]
    interval_hours: f64,
    /// Where it writes. Left out: a new local folder of its own, called `source-<name>`.
    #[serde(default)]
    folder: Option<Place>,
}

/// A folder a source writes into.
#[derive(Deserialize, Clone)]
pub struct Place {
    folder: String,
}

/// The folder `place`, if `who` may change it and the web app edits it in place.
fn destination(c: &mut Client, who: &Access, place: &Place) -> anyhow::Result<db::FolderRef> {
    let folder = find_folder_to_change(c, who, &place.folder)?;
    if !folders::kind_of(c, folder.id)?.editable() {
        return Err(ApiError::bad("A source can write into your own folders, shared folders and the master folders").into());
    }
    Ok(folder)
}

/// Adds a source that copies a folder of any storage rclone supports.
pub async fn create_source(Ctx { state, who }: Ctx, Json(b): Json<NewSource>) -> Res<Done> {
    who.require_workspace()?;
    let name = input::name(&b.name)?;
    let folder = format!("{SOURCE_PREFIX}{name}");
    if !valid_name(&folder) {
        return Err(ApiError::bad("Name: too long for a source"));
    }
    let secs = hours_to_secs(b.interval_hours)?;
    let config = if b.kind == SourceKind::Rclone {
        let options = backend_options(&b.options, &b.secrets).await?;
        let (mut remote, mut remote_type, mut remote_path) = (b.remote, b.remote_type, b.remote_path);
        let mut options: std::collections::BTreeMap<String, Value> = options.into_iter().collect();
        normalize_storage(&mut remote, &mut remote_type, &mut remote_path, &mut options).map_err(|e| ApiError::bad(format!("{e:#}")))?;
        rclone_config(remote, remote_type, remote_path, options)
    } else {
        fetch_source::validate(b.kind.as_str(), &b.config)
            .await
            .map_err(|e| ApiError::bad(format!("{e:#}")))?
    };
    let (store, kind, place) = (state.store.clone(), b.kind, b.folder);
    act(&state, move |c| {
        let chosen = place.as_ref().map(|place| destination(c, &who, place)).transpose()?;
        let mut tx = c.transaction()?;
        let (dest, own) = match chosen {
            Some(dest) => (dest, false),
            None => {
                // A folder of its own in the workspace of the user, for a source that was given none.
                let (id, folder) = sources::insert_folder(&mut tx, who.user_id, &folder, unix_now())?.ok_or_else(|| ApiError::conflict(format!("You have a device or folder called {folder} already")))?;
                (db::FolderRef { id, key: folder }, true)
            }
        };
        let new = links::New {
            dir: Dir::In,
            user_id: Some(who.user_id),
            name: &name,
            kind: kind.as_str(),
            config: &config.to_string(),
            folder_id: Some(dest.id),
            device_id: None,
            interval_secs: secs,
        };
        if links::add(&mut tx, &new, unix_now())?.is_none() {
            return Err(ApiError::conflict(format!("You already have a source named {name}")).into());
        }
        db::notify(&mut tx, db::Topic::Jobs, &format!("{}/{name}", who.name))?;
        tx.commit()?;
        if own {
            let _ = std::fs::create_dir_all(store.live_dir(&dest.key));
        }
        Ok(())
    })
    .await
}

#[derive(Deserialize)]
pub struct SourceEdit {
    name: Option<String>,
    interval_hours: Option<f64>,
    /// Where it writes. Files it collected before stay where they are; it collects everything again into the new place.
    folder: Option<Place>,
    /// A module kind's settings, in full.
    config: Option<Value>,
    /// An rclone source's remote or backend and path; what is left out stays as it was.
    remote: Option<String>,
    remote_type: Option<String>,
    remote_path: Option<String>,
    /// All the options of the backend, replacing the stored ones.
    options: Option<String>,
    #[serde(default)]
    secrets: std::collections::BTreeMap<String, String>,
}

/// The settings of a module's source after an edit: the new ones, but a secret left empty keeps its stored value.
fn merge_secrets(stored: &Value, mut new: Value) -> Value {
    if let (Some(new), Some(old)) = (new.as_object_mut(), stored.as_object()) {
        for (key, value) in new.iter_mut() {
            if is_secret(key) && value.as_str() == Some("") {
                if let Some(kept) = old.get(key) {
                    *value = kept.clone();
                }
            }
        }
    }
    new
}

/// A source gets another folder to write into: what it collected stays where it is, as files of that folder (they go to its Trash),
/// and the source collects everything again where it now writes.
fn leave_folder(c: &mut Client, store: &Store, id: i64) -> anyhow::Result<()> {
    let due = links::due(c, id)?.ok_or_else(|| ApiError::not_found("No such source"))?;
    let folder = db::FolderRef { id: due.folder_id.unwrap_or_default(), key: due.scope.clone() };
    for item in links::items(c, id)? {
        ingest::mark_deleted(c, store, &folder, &item.path, false)?;
    }
    Ok(())
}

/// Changes a source: its name, where it writes, how often it runs and what it reads. Credentials left out keep their value.
pub async fn edit_source(Ctx { state, who }: Ctx, Path((user, name)): Path<(String, String)>, Json(b): Json<SourceEdit>) -> Res<Done> {
    who.require_workspace()?;
    let new_name = b.name.as_deref().map(input::name).transpose()?;
    let secs = b.interval_hours.map(hours_to_secs).transpose()?;
    let stored_id = {
        let (who, user, name) = (who.clone(), user.clone(), name.clone());
        run(&state, move |c| find_source(c, &who, &user, &name)).await?
    };
    let stored = run(&state, move |c| Ok(links::for_edit(c, stored_id)?))
        .await?
        .ok_or_else(|| ApiError::not_found("No such source"))?;
    let old: Value = serde_json::from_str(&stored.config).unwrap_or(Value::Null);
    let config = if stored.source_kind() == SourceKind::Rclone {
        if b.remote.is_some() || b.remote_type.is_some() || b.remote_path.is_some() || b.options.is_some() || !b.secrets.is_empty() {
            let mut options: std::collections::BTreeMap<String, Value> = match &b.options {
                Some(text) => backend_options(text, &b.secrets).await?.into_iter().collect(),
                None => {
                    let mut kept: std::collections::BTreeMap<String, Value> = old["options"]
                        .as_str()
                        .and_then(|o| serde_json::from_str::<serde_json::Map<String, Value>>(o).ok())
                        .map(|o| o.into_iter().collect())
                        .unwrap_or_default();
                    kept.extend(backend_options("", &b.secrets).await?);
                    kept
                }
            };
            let text = |new: &Option<String>, key: &str| new.clone().unwrap_or_else(|| old[key].as_str().unwrap_or_default().to_string());
            let (mut remote, mut remote_type, mut remote_path) = (text(&b.remote, "remote"), text(&b.remote_type, "remote_type"), text(&b.remote_path, "remote_path"));
            normalize_storage(&mut remote, &mut remote_type, &mut remote_path, &mut options).map_err(|e| ApiError::bad(format!("{e:#}")))?;
            rclone_config(remote, remote_type, remote_path, options)
        } else {
            old.clone()
        }
    } else if let Some(mut config) = b.config.clone() {
        // The confirmation of the right to copy was given when the source was added.
        if config["acknowledged"].is_null() && old["acknowledged_at"].is_number() {
            config["acknowledged"] = Value::Bool(true);
        }
        fetch_source::validate(&stored.kind, &merge_secrets(&old, config))
            .await
            .map_err(|e| ApiError::bad(format!("{e:#}")))?
    } else {
        old.clone()
    };
    let place = b.folder.clone();
    let key = format!("{user}/{name}");
    let store = state.store.clone();
    act(&state, move |c| {
        let chosen = place.as_ref().map(|place| destination(c, &who, place)).transpose()?;
        // What it collected leaves the old folder before the source says it writes into another.
        let mut recollect = false;
        if let Some(to) = chosen.as_ref() {
            let before = links::for_edit(c, stored_id)?.ok_or_else(|| ApiError::not_found("No such source"))?;
            if before.folder_id != Some(to.id) {
                if before.running {
                    return Err(ApiError::conflict("The source is running. Try again when it has finished").into());
                }
                leave_folder(c, &store, stored_id)?;
                recollect = true;
            }
        }
        let mut tx = c.transaction()?;
        let mut s = links::for_edit(&mut tx, stored_id)?.ok_or_else(|| ApiError::not_found("No such source"))?;
        if s.running {
            return Err(ApiError::conflict("The source is running. Try again when it has finished").into());
        }
        if let Some(dest) = chosen {
            s.folder_id = Some(dest.id);
        }
        if let Some(new_name) = &new_name {
            if *new_name != s.name && links::name_taken(&mut tx, Dir::In, s.user_id, new_name, stored_id)? {
                return Err(ApiError::conflict(format!("You already have a source named {new_name}")).into());
            }
            s.name = new_name.clone();
        }
        if let Some(secs) = secs {
            s.interval_secs = secs;
        }
        s.config = config.to_string();
        links::update(&mut tx, stored_id, &s)?;
        if recollect {
            // What it remembered is about the old folder: it collects everything again into the new one.
            links::forget_items(&mut tx, stored_id)?;
        }
        let renamed = format!("{user}/{}", s.name);
        if renamed != key {
            // The failure alert of the old name would never be closed.
            alerts::resolve(&mut tx, "source_failed", &key)?;
        }
        db::notify(&mut tx, db::Topic::Jobs, &key)?;
        db::notify(&mut tx, db::Topic::Jobs, &renamed)?;
        tx.commit()?;
        Ok(())
    })
    .await
}

/// Removes a source. Its collected files follow the normal delete rule: kept for the retention period, then removed.
/// Only what the source collected is deleted, not what else is in the folder it wrote into.
pub fn remove_source(c: &mut Client, store: &Store, id: i64, key: &str) -> anyhow::Result<()> {
    let due = links::due(c, id)?.ok_or_else(|| ApiError::not_found("No such source"))?;
    let folder = db::FolderRef { id: due.folder_id.unwrap_or_default(), key: due.scope.clone() };
    for item in links::items(c, id)? {
        ingest::mark_deleted(c, store, &folder, &item.path, true)?;
    }
    links::delete(c, id)?;
    alerts::resolve(c, "source_failed", key)?;
    db::notify(c, db::Topic::Jobs, key)?;
    Ok(())
}

pub async fn delete_source(Ctx { state, who }: Ctx, Path((user, name)): Path<(String, String)>) -> Res<Done> {
    who.require_workspace()?;
    let store = state.store.clone();
    act(&state, move |c| {
        let id = find_source(c, &who, &user, &name)?;
        remove_source(c, &store, id, &format!("{user}/{name}"))
    })
    .await
}

pub async fn run_source(Ctx { state, who }: Ctx, Path((user, name)): Path<(String, String)>) -> Res<Done> {
    touch_link(&state, who, Some(user), name, Touch::RunNow).await
}

pub async fn toggle_source(Ctx { state, who }: Ctx, Path((user, name)): Path<(String, String)>) -> Res<Done> {
    touch_link(&state, who, Some(user), name, Touch::ToggleEnabled).await
}
