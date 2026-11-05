//! Sources (inbound) and mirrors (outbound), the two directions of a link: their configuration. The background scheduler
//! runs them; this only stores their settings and asks for runs. A source belongs to a user and writes into one folder, a
//! mirror to the administrators and holds one folder or everything. Credentials are write-only: they are never sent back to the
//! browser (the encryption key of a backup mirror, in its restore kit, is the one exception).

use crate::files::browse::{editable_kind, find_device_to_change};
use crate::files::ingest;
use crate::integrations::fetch_source;
use crate::integrations::spec::{normalize_storage, MirrorSpec};
use crate::platform::db;
use crate::platform::store::Store;
use crate::repo::links::{self, Dir};
use crate::repo::{alerts, devices, mirrors, sources};
use crate::web::access::Access;
use crate::web::input;
use crate::web::{act, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use fafnirx_common::{unix_now, valid_device_name};
use fafnirx_db::queries as sql;
use postgres::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const INTERVAL_HOURS: (f64, f64) = (0.25, 24.0 * 365.0);
/// Source devices are named after their source.
const SOURCE_PREFIX: &str = "source-";

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct JobState {
    name: String,
    /// What the API calls it: `<user>/<name>` for a source, the name for a mirror.
    key: String,
    /// running | ok | partial | failed | never run
    status: String,
    running: bool,
    enabled: bool,
    interval_secs: i64,
    last_run_at: Option<i64>,
    last_ok_at: Option<i64>,
    message: String,
    /// What is specific to a source or a mirror.
    #[cfg_attr(test, ts(type = "Record<string, unknown>"))]
    detail: Value,
}

/// The links of one direction `who` may see: a user's own sources or all of them for an administrator; the mirrors.
fn states(c: &mut Client, dir: Dir, who: &Access) -> Result<Vec<JobState>, postgres::Error> {
    let (all, user) = who.scope_args();
    Ok(sql::links::list_links()
        .bind(c, &dir.as_str(), &all, &user)
        .all()?
        .into_iter()
        .map(|r| {
            let config: Value = serde_json::from_str(&r.config).unwrap_or(Value::Null);
            let detail = match dir {
                Dir::In => json!({
                    "bytes": r.bytes,
                    "files": r.files,
                    "kind": r.kind,
                    "remote": storage_label(&r.kind, &config),
                    "folder": r.folder,
                    "config": shown_config(&r.kind, &config),
                }),
                Dir::Out => {
                    let options: Value = config["options"].as_str().and_then(|o| serde_json::from_str(o).ok()).unwrap_or(Value::Null);
                    let mut keys: Vec<&String> = options.as_object().map_or_else(Vec::new, |o| o.keys().collect());
                    keys.sort();
                    json!({
                        "kind": r.kind,
                        "remote": config["remote"],
                        "remote_type": config["remote_type"],
                        "remote_path": config["remote_path"],
                        "folder": r.folder,
                        "keep_runs": config["keep_runs"],
                        "option_keys": keys,
                        "files": r.files,
                        "bytes": r.bytes,
                    })
                }
            };
            JobState {
                key: if r.owner.is_empty() { r.name.clone() } else { format!("{}/{}", r.owner, r.name) },
                name: r.name,
                running: r.last_status.as_deref() == Some("running"),
                status: r.last_status.unwrap_or_else(|| "never run".into()),
                enabled: r.enabled != 0,
                interval_secs: r.interval_secs,
                last_run_at: r.last_run_at,
                last_ok_at: r.last_ok_at,
                message: r.last_message.unwrap_or_default(),
                detail,
            }
        })
        .collect())
}

/// Sources are per user; mirrors are for the administrators.
pub fn job_states(c: &mut Client, who: &Access) -> Result<(Vec<JobState>, Vec<JobState>), postgres::Error> {
    let mirrors = if who.is_admin() { states(c, Dir::Out, who)? } else { vec![] };
    Ok((states(c, Dir::In, who)?, mirrors))
}

/// What the edit form is given of the settings of a source: a module kind's settings without the values that look like secrets
/// (the form shows those as unchanged), an rclone source's remote and path (its options and passwords stay on the master).
fn shown_config(kind: &str, config: &Value) -> Value {
    if kind == "rclone" {
        return json!({
            "remote": config["remote"],
            "remote_type": config["remote_type"],
            "remote_path": config["remote_path"],
            "option_keys": config["options"].as_str().and_then(|o| serde_json::from_str::<serde_json::Map<String, Value>>(o).ok()).map_or_else(Vec::new, |o| {
                let mut keys: Vec<String> = o.keys().cloned().collect();
                keys.sort();
                keys
            }),
        });
    }
    let mut out = config.clone();
    if let Some(map) = out.as_object_mut() {
        // What the master keeps for itself is not the form's.
        map.remove("acknowledged_at");
        map.remove("label");
        for (key, value) in map.iter_mut() {
            if is_secret(key) && value.as_str().is_some_and(|v| !v.is_empty()) {
                *value = Value::String(String::new());
            }
        }
    }
    out
}

/// Whether a setting of a module's source is one that is only ever written (a password, a token, cookies).
fn is_secret(key: &str) -> bool {
    let key = key.to_lowercase();
    ["password", "secret", "token", "cookie", "key"].iter().any(|w| key.contains(w))
}

/// The id of the source `name` of `user`, unless it is missing or `who` may not manage it (an administrator manages all).
/// Someone else's source does not exist.
fn find_source(c: &mut Client, who: &Access, user: &str, name: &str) -> anyhow::Result<i64> {
    links::id_by_name(c, Dir::In, Some(user), name)?
        .filter(|_| who.is_admin() || who.name == user)
        .ok_or_else(|| ApiError::not_found("No such source").into())
}

fn mirror_id(c: &mut Client, name: &str) -> anyhow::Result<i64> {
    Ok(links::id_by_name(c, Dir::Out, None, name)?.ok_or_else(|| ApiError::not_found("No such mirror"))?)
}

pub async fn sources(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<JobState>> {
    who.require_workspace()?;
    Ok(Json(run(&state, move |c| Ok(states(c, Dir::In, &who)?)).await?))
}

pub async fn mirrors(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<JobState>> {
    who.require_admin()?;
    Ok(Json(run(&state, move |c| Ok(states(c, Dir::Out, &who)?)).await?))
}

/// `key=value` per line; blank lines and `#` comments are ignored.
fn parse_options(text: &str) -> Result<serde_json::Map<String, Value>, ApiError> {
    let mut opts = serde_json::Map::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let (key, value) = line.split_once('=').unwrap_or((line, ""));
        let key = key.trim().to_lowercase().replace('-', "_");
        let ok = !key.is_empty() && key.len() <= 64 && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !line.contains('=') || !ok {
            return Err(ApiError::bad(format!(
                "Option {:?}: use key=value with lowercase letters, digits and _",
                line.split('=').next().unwrap_or(line)
            )));
        }
        opts.insert(key, Value::String(value.trim().to_string()));
    }
    Ok(opts)
}

/// The `key=value` lines of a form, plus passwords given plain that rclone keeps in a scrambled form.
async fn backend_options(text: &str, secrets: &std::collections::BTreeMap<String, String>) -> Result<Vec<(String, Value)>, ApiError> {
    let mut options: Vec<(String, Value)> = parse_options(text)?.into_iter().collect();
    let bin = crate::integrations::mirror::rclone_bin();
    for (key, plain) in secrets {
        let scrambled = crate::integrations::mirror::obscure(&bin, plain)
            .await
            .map_err(|e| ApiError::bad(format!("{e:#}")))?;
        options.push((key.clone(), Value::String(scrambled)));
    }
    Ok(options)
}

/// What a source shows of where it reads: `type:path` or the remote of the config file for rclone, the link for a module's kind.
fn storage_label(kind: &str, config: &Value) -> Value {
    if fetch_source::is_kind(kind) {
        return Value::String(fetch_source::label(config));
    }
    match config["remote"].as_str().filter(|r| !r.is_empty()) {
        Some(remote) => Value::String(remote.to_string()),
        None => Value::String(format!(
            "{}:{}",
            config["remote_type"].as_str().unwrap_or_default(),
            config["remote_path"].as_str().unwrap_or_default()
        )),
    }
}

pub(crate) fn hours_to_secs(hours: f64) -> Result<i64, ApiError> {
    if !(INTERVAL_HOURS.0..=INTERVAL_HOURS.1).contains(&hours) {
        return Err(ApiError::bad("Interval: between 0.25 hours and one year"));
    }
    Ok((hours * 3600.0).round() as i64)
}

fn day() -> f64 {
    24.0
}

fn rclone() -> String {
    "rclone".into()
}

#[derive(Deserialize)]
pub struct NewSource {
    name: String,
    /// How the source fetches: `rclone` (any storage, and Google Drive) or a kind a module reads (the module lists `fetch.<kind>`).
    /// A request without it is a storage read through rclone.
    #[serde(default = "rclone")]
    kind: String,
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
    device: String,
}

/// The folder `place`, if `who` may change it and the web app edits it in place.
fn destination(c: &mut Client, who: &Access, place: &Place) -> anyhow::Result<db::Device> {
    let device = find_device_to_change(c, who, &place.device)?;
    if !editable_kind(&devices::kind_of(c, device.id)?) {
        return Err(ApiError::bad("A source can write into your own folders, shared folders and the master folders").into());
    }
    Ok(device)
}

/// The settings of an rclone source as they are stored.
fn rclone_config(remote: String, remote_type: String, remote_path: String, options: std::collections::BTreeMap<String, Value>) -> Value {
    json!({
        "remote": remote,
        "remote_type": remote_type,
        "remote_path": remote_path,
        "options": Value::Object(options.into_iter().collect()).to_string(),
    })
}

/// Adds a source that copies a folder of any storage rclone supports.
pub async fn create_source(State(state): State<Shared>, Extension(who): Extension<Access>, Json(b): Json<NewSource>) -> Res<Done> {
    who.require_workspace()?;
    let name = input::device_name(&b.name)?;
    let device = format!("{SOURCE_PREFIX}{name}");
    if !valid_device_name(&device) {
        return Err(ApiError::bad("Name: too long for a source"));
    }
    let secs = hours_to_secs(b.interval_hours)?;
    let config = if b.kind == "rclone" {
        let options = backend_options(&b.options, &b.secrets).await?;
        let (mut remote, mut remote_type, mut remote_path) = (b.remote, b.remote_type, b.remote_path);
        let mut options: std::collections::BTreeMap<String, Value> = options.into_iter().collect();
        normalize_storage(&mut remote, &mut remote_type, &mut remote_path, &mut options).map_err(|e| ApiError::bad(format!("{e:#}")))?;
        rclone_config(remote, remote_type, remote_path, options)
    } else if fetch_source::is_kind(&b.kind) {
        fetch_source::validate(&b.kind, &b.config).await.map_err(|e| ApiError::bad(format!("{e:#}")))?
    } else {
        unreachable!("every kind but rclone is read by a module")
    };
    let (store, kind, place) = (state.store.clone(), b.kind, b.folder);
    act(&state, move |c| {
        let chosen = place.as_ref().map(|place| destination(c, &who, place)).transpose()?;
        let mut tx = c.transaction()?;
        let (dest, own) = match chosen {
            Some(dest) => (dest, false),
            None => {
                // A folder of its own in the workspace of the user, for a source that was given none.
                let (id, folder) = sources::insert_device(&mut tx, who.user_id, &device, unix_now())?.ok_or_else(|| ApiError::conflict(format!("You have a device or folder called {device} already")))?;
                (db::Device { id, name: folder }, true)
            }
        };
        let new = links::New {
            dir: Dir::In,
            user_id: Some(who.user_id),
            name: &name,
            kind: &kind,
            config: &config.to_string(),
            folder_id: Some(dest.id),
            interval_secs: secs,
        };
        if links::add(&mut tx, &new, unix_now())?.is_none() {
            return Err(ApiError::conflict(format!("You already have a source named {name}")).into());
        }
        db::notify(&mut tx, db::Topic::Jobs, &format!("{}/{name}", who.name))?;
        tx.commit()?;
        if own {
            let _ = std::fs::create_dir_all(store.live_dir(&dest.name));
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
    let device = db::Device { id: due.folder_id.unwrap_or_default(), name: due.folder.clone().unwrap_or_default() };
    for item in links::items(c, id)? {
        ingest::mark_deleted(c, store, &device, &item.path, false)?;
    }
    Ok(())
}

/// Changes a source: its name, where it writes, how often it runs and what it reads. Credentials left out keep their value.
pub async fn edit_source(State(state): State<Shared>, Extension(who): Extension<Access>, Path((user, name)): Path<(String, String)>, Json(b): Json<SourceEdit>) -> Res<Done> {
    who.require_workspace()?;
    let new_name = b.name.as_deref().map(input::device_name).transpose()?;
    let secs = b.interval_hours.map(hours_to_secs).transpose()?;
    let stored_id = {
        let (who, user, name) = (who.clone(), user.clone(), name.clone());
        run(&state, move |c| find_source(c, &who, &user, &name)).await?
    };
    let stored = run(&state, move |c| Ok(links::for_edit(c, stored_id)?))
        .await?
        .ok_or_else(|| ApiError::not_found("No such source"))?;
    let old: Value = serde_json::from_str(&stored.config).unwrap_or(Value::Null);
    let config = if stored.kind == "rclone" {
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
    let device = db::Device { id: due.folder_id.unwrap_or_default(), name: due.folder.clone().unwrap_or_default() };
    for item in links::items(c, id)? {
        ingest::mark_deleted(c, store, &device, &item.path, true)?;
    }
    links::delete(c, id)?;
    alerts::resolve(c, "source_failed", key)?;
    db::notify(c, db::Topic::Jobs, key)?;
    Ok(())
}

pub async fn delete_source(State(state): State<Shared>, Extension(who): Extension<Access>, Path((user, name)): Path<(String, String)>) -> Res<Done> {
    who.require_workspace()?;
    let store = state.store.clone();
    act(&state, move |c| {
        let id = find_source(c, &who, &user, &name)?;
        remove_source(c, &store, id, &format!("{user}/{name}"))
    })
    .await
}

/// What `touch` does to a source or a mirror.
#[derive(Clone, Copy)]
enum Touch {
    RunNow,
    ToggleEnabled,
}

/// Flips or requests something on a source or a mirror. `key` tells who to announce it to: `<owner>/<name>` of a source,
/// `mirrors` for a mirror.
fn touch(c: &mut Client, id: i64, key: &str, what: Touch) -> anyhow::Result<()> {
    match what {
        Touch::RunNow => links::request_run(c, id)?,
        Touch::ToggleEnabled => links::toggle(c, id)?,
    }
    db::notify(c, db::Topic::Jobs, key)?;
    Ok(())
}

/// Does `what` to the source `<user>/<name>`, if the user manages it.
async fn touch_source(state: &Shared, who: Access, user: String, name: String, what: Touch) -> Res<Done> {
    who.require_workspace()?;
    act(state, move |c| {
        let id = find_source(c, &who, &user, &name)?;
        touch(c, id, &format!("{user}/{name}"), what)
    })
    .await
}

/// Does `what` to the mirror called `name` (administrators).
async fn touch_mirror(state: &Shared, who: Access, name: String, what: Touch) -> Res<Done> {
    who.require_admin()?;
    act(state, move |c| {
        let id = mirror_id(c, &name)?;
        touch(c, id, "mirrors", what)
    })
    .await
}

pub async fn run_source(State(state): State<Shared>, Extension(who): Extension<Access>, Path((user, name)): Path<(String, String)>) -> Res<Done> {
    touch_source(&state, who, user, name, Touch::RunNow).await
}

pub async fn toggle_source(State(state): State<Shared>, Extension(who): Extension<Access>, Path((user, name)): Path<(String, String)>) -> Res<Done> {
    touch_source(&state, who, user, name, Touch::ToggleEnabled).await
}

pub async fn run_mirror(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Done> {
    touch_mirror(&state, who, name, Touch::RunNow).await
}

pub async fn toggle_mirror(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Done> {
    touch_mirror(&state, who, name, Touch::ToggleEnabled).await
}

#[derive(Deserialize)]
pub struct NewMirror {
    name: String,
    /// `replica` (the files as they are) or `backup` (encrypted with the passwords below).
    #[serde(default = "replica")]
    kind: String,
    #[serde(default)]
    crypt_password: String,
    #[serde(default)]
    crypt_salt: String,
    /// A remote of the rclone config file (`name:path`); when set, the backend, options and path are not used.
    #[serde(default)]
    remote: String,
    #[serde(default)]
    remote_type: String,
    #[serde(default)]
    options: String,
    /// Passwords that rclone keeps in a scrambled form (`rclone obscure`), given plain: key -> password.
    #[serde(default)]
    secrets: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    remote_path: String,
    /// The folder to hold, like `ana/laptop` or `master:Photos`; everything when empty.
    #[serde(default)]
    folder: String,
    #[serde(default = "three_days")]
    interval_hours: f64,
    #[serde(default = "three")]
    keep_runs: i64,
}

fn replica() -> String {
    "replica".into()
}
fn three_days() -> f64 {
    72.0
}
fn three() -> i64 {
    3
}

pub async fn create_mirror(State(state): State<Shared>, Extension(who): Extension<Access>, Json(b): Json<NewMirror>) -> Res<Done> {
    who.require_admin()?;
    let options = backend_options(&b.options, &b.secrets).await?;
    let mut mirror = MirrorSpec {
        name: input::device_name(&b.name)?,
        kind: b.kind,
        crypt_password: b.crypt_password,
        crypt_salt: b.crypt_salt,
        remote: b.remote,
        remote_type: b.remote_type,
        remote_path: b.remote_path,
        options: options.into_iter().collect(),
        folder: b.folder,
        interval_hours: b.interval_hours,
        keep_runs: b.keep_runs,
    };
    mirror.normalize().map_err(|e| ApiError::bad(format!("{e:#}")))?;
    let secs = hours_to_secs(mirror.interval_hours)?;
    let mut config = rclone_config(mirror.remote, mirror.remote_type, mirror.remote_path, mirror.options);
    config["crypt_password"] = mirror.crypt_password.into();
    config["crypt_salt"] = mirror.crypt_salt.into();
    config["keep_runs"] = mirror.keep_runs.into();
    act(&state, move |c| {
        let folder_id = match mirror.folder.as_str() {
            "" => None,
            folder => Some(
                devices::by_folder(c, folder)?
                    .ok_or_else(|| ApiError::bad(format!("There is no folder called {folder} to mirror")))?
                    .0,
            ),
        };
        let new = links::New {
            dir: Dir::Out,
            user_id: None,
            name: &mirror.name,
            kind: &mirror.kind,
            config: &config.to_string(),
            folder_id,
            interval_secs: secs,
        };
        if links::add(c, &new, unix_now())?.is_none() {
            return Err(ApiError::conflict(format!("A mirror named {} already exists", mirror.name)).into());
        }
        db::notify(c, db::Topic::Jobs, "mirrors")?;
        Ok(())
    })
    .await
}

/// The folders a mirror can be told to hold, for the form that adds one.
pub async fn mirror_folders(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<String>> {
    who.require_admin()?;
    let mut folders: Vec<String> = run(&state, |c| Ok(mirrors::device_folders(c)?))
        .await?
        .into_iter()
        .map(|(_, folder)| folder)
        .collect();
    folders.sort();
    Ok(Json(folders))
}

/// Forget what was uploaded: the next run re-sends everything (use after emptying the remote).
pub async fn resync_mirror(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Done> {
    who.require_admin()?;
    act(&state, move |c| {
        let id = mirror_id(c, &name)?;
        links::forget_items(c, id)?;
        touch(c, id, "mirrors", Touch::RunNow)
    })
    .await
}

/// The rclone config that restores a backup mirror without fafnirx (administrators). The one place a credential
/// leaves the master: a backup is worthless without its key.
pub async fn mirror_restore_kit(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Value> {
    who.require_admin()?;
    let key = run(&state, move |c| Ok(mirrors::key(c, &name)?.ok_or_else(|| ApiError::not_found("No such mirror"))?)).await?;
    if key.kind != crate::integrations::mirror::BACKUP {
        return Err(ApiError::bad("Only a backup is encrypted: a replica is restored by copying its files"));
    }
    let bin = crate::integrations::mirror::rclone_bin();
    let obscured = |p: &str| {
        let p = p.to_string();
        let bin = bin.clone();
        async move { crate::integrations::mirror::obscure(&bin, &p).await.map_err(|e| ApiError::bad(format!("{e:#}"))) }
    };
    let (password, salt) = (obscured(&key.crypt_password).await?, obscured(&key.crypt_salt).await?);
    let config = crate::integrations::mirror::restore_config(&key, &password, &salt);
    Ok(Json(json!({ "config": config })))
}

pub async fn delete_mirror(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Done> {
    who.require_admin()?;
    act(&state, move |c| {
        let id = mirror_id(c, &name)?;
        links::delete(c, id)?;
        alerts::resolve(c, "mirror_failed", &name)?;
        db::notify(c, db::Topic::Jobs, "mirrors")?;
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_are_key_value_lines() {
        let opts = parse_options("# comment\nProvider = AWS\naccess-key-id=abc=def\n\n").unwrap();
        assert_eq!(opts["provider"], "AWS");
        assert_eq!(opts["access_key_id"], "abc=def");
        assert!(parse_options("novalue").is_err());
        assert!(parse_options("bad key=1").is_err());
    }

    #[test]
    fn intervals_are_limited() {
        assert_eq!(hours_to_secs(1.5).unwrap(), 5400);
        assert!(hours_to_secs(0.1).is_err() && hours_to_secs(24.0 * 400.0).is_err());
    }
}
