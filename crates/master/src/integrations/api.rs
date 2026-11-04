//! Third-party sources (inbound) and mirrors (outbound): their configuration. The background scheduler runs them;
//! this only stores their settings and asks for runs (`run_requested`). A source belongs to a user, a mirror to the
//! administrators. Credentials
//! are write-only: they are never sent back to the browser (the encryption key of a backup mirror, in its restore kit,
//! is the one exception).

use crate::files::browse::{editable_kind, find_device_to_change};
use crate::files::ingest;
use crate::integrations::spec::{normalize_storage, MirrorSpec};
use crate::integrations::Table;
use crate::integrations::{fetch_source, scope};
use crate::platform::db;
use crate::platform::store::Store;
use crate::repo::{alerts, devices, files, mirrors, sources};
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
    detail: Value,
}

/// What the list of sources and the list of mirrors have in common.
struct Schedule {
    name: String,
    key: String,
    status: Option<String>,
    enabled: i64,
    interval_secs: i64,
    last_run_at: Option<i64>,
    last_ok_at: Option<i64>,
    message: Option<String>,
}

fn state_of(s: Schedule, detail: Value) -> JobState {
    JobState {
        key: s.key,
        name: s.name,
        running: s.status.as_deref() == Some("running"),
        status: s.status.unwrap_or_else(|| "never run".into()),
        enabled: s.enabled != 0,
        interval_secs: s.interval_secs,
        last_run_at: s.last_run_at,
        last_ok_at: s.last_ok_at,
        message: s.message.unwrap_or_default(),
        detail,
    }
}

/// The sources `who` may see: their own, or all of them for an administrator.
fn source_states(c: &mut Client, who: &Access) -> Result<Vec<JobState>, postgres::Error> {
    let (all, user) = who.scope_args();
    let rows = sql::sources::list_sources().bind(c, &all, &user).all()?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let config: Value = serde_json::from_str(&r.config).unwrap_or(Value::Null);
            let detail = json!({
                "bytes": r.bytes,
                "files": r.files,
                "kind": r.kind,
                "remote": storage_label(&r.kind, &config),
                "device": r.folder,
                "folder": { "device": r.folder, "path": r.folder_prefix.trim_end_matches('/') },
                "config": shown_config(&r.kind, &config),
            });
            let key = format!("{}/{}", r.owner, r.name);
            state_of(
                Schedule {
                    name: r.name,
                    key,
                    status: r.last_status,
                    enabled: r.enabled,
                    interval_secs: r.interval_secs,
                    last_run_at: r.last_run_at,
                    last_ok_at: r.last_ok_at,
                    message: r.last_message,
                },
                detail,
            )
        })
        .collect())
}

fn mirror_states(c: &mut Client) -> Result<Vec<JobState>, postgres::Error> {
    Ok(sql::mirrors::list_mirrors()
        .bind(c)
        .all()?
        .into_iter()
        .map(|r| {
            let options: Value = serde_json::from_str(&r.options).unwrap_or(Value::Null);
            let mut keys: Vec<&String> = options.as_object().map_or_else(Vec::new, |o| o.keys().collect());
            keys.sort();
            let detail = json!({
                "kind": r.kind,
                "remote": r.remote,
                "remote_type": r.remote_type,
                "remote_path": r.remote_path,
                "scope": r.scope,
                "keep_runs": r.keep_runs,
                "option_keys": keys,
                "files": r.last_files,
                "bytes": r.last_bytes,
            });
            state_of(
                Schedule {
                    key: r.name.clone(),
                    name: r.name,
                    status: r.last_status,
                    enabled: r.enabled,
                    interval_secs: r.interval_secs,
                    last_run_at: r.last_run_at,
                    last_ok_at: r.last_ok_at,
                    message: r.last_message,
                },
                detail,
            )
        })
        .collect())
}

/// Sources are per user; mirrors are for the administrators.
pub fn job_states(c: &mut Client, who: &Access) -> Result<(Vec<JobState>, Vec<JobState>), postgres::Error> {
    let mirrors = if who.is_admin() { mirror_states(c)? } else { vec![] };
    Ok((source_states(c, who)?, mirrors))
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

/// The id and the device folder of the source `name` of `user`, unless it is missing or `who` may not manage it (an
/// administrator manages all). Someone else's source does not exist.
fn find_source(c: &mut Client, who: &Access, user: &str, name: &str) -> anyhow::Result<(i64, String)> {
    sources::by_owner(c, user, name)?
        .filter(|_| who.is_admin() || who.name == user)
        .ok_or_else(|| ApiError::not_found("No such source").into())
}

fn mirror_id(c: &mut Client, name: &str) -> anyhow::Result<i64> {
    Ok(sql::mirrors::mirror_id()
        .bind(c, &name)
        .opt()?
        .ok_or_else(|| ApiError::not_found("No such mirror"))?)
}

pub async fn sources(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<JobState>> {
    who.require_workspace()?;
    Ok(Json(run(&state, move |c| Ok(source_states(c, &who)?)).await?))
}

pub async fn mirrors(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<JobState>> {
    who.require_admin()?;
    Ok(Json(run(&state, |c| Ok(mirror_states(c)?)).await?))
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

/// A folder of a device, and a subfolder of it (empty for the folder itself).
#[derive(Deserialize, Clone)]
pub struct Place {
    device: String,
    #[serde(default)]
    path: String,
}

/// The folder `place` as a source writes into it: a folder `who` may change and the web app edits in place, and the subfolder
/// as a prefix of the paths (empty, or the path and a slash). The master files may not be written under `users/`.
fn destination(c: &mut Client, who: &Access, place: &Place) -> anyhow::Result<sources::Destination> {
    let device = find_device_to_change(c, who, &place.device)?;
    if !editable_kind(&devices::kind_of(c, device.id)?) {
        return Err(ApiError::bad("A source can write into your own folders, shared folders and the master files").into());
    }
    let prefix = input::folder_prefix(&place.path)?;
    ingest::check_master_path(&device, &prefix)?;
    Ok(sources::Destination { device_id: device.id, folder: device.name, prefix })
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
        if sources::name_taken(&mut tx, who.user_id, &name, 0)? {
            return Err(ApiError::conflict(format!("You already have a source named {name}")).into());
        }
        let (dest, own) = match chosen {
            Some(dest) => (dest, false),
            None => {
                // A folder of its own in the workspace of the user, for a source that was given none. '!' can never equal a
                // SHA-256 hex digest, so nothing can authenticate as it.
                let (device_id, folder) = sources::insert_device(&mut tx, who.user_id, &device, unix_now())?.ok_or_else(|| ApiError::conflict(format!("You have a device or folder called {device} already")))?;
                (sources::Destination { device_id, folder, prefix: String::new() }, true)
            }
        };
        let new = sources::New {
            user_id: who.user_id,
            name: &name,
            kind: &kind,
            config: &config.to_string(),
            interval_secs: secs,
            device_id: dest.device_id,
            prefix: &dest.prefix,
        };
        sources::insert(&mut tx, &new, unix_now())?;
        db::notify(&mut tx, "jobs", &format!("{}/{name}", who.name))?;
        tx.commit()?;
        if own {
            let _ = std::fs::create_dir_all(store.live_dir(&dest.folder));
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

/// A source gets a new place to write: what it collected goes with it. In the same folder the files are moved (and stay what the
/// source remembers); into another folder they are put in the Trash there, and the source collects everything again where it now
/// writes (the caller forgets what it remembered). Nothing is moved if a file is in the way; a failure puts back what was moved.
/// Returns whether the source has to collect everything again.
fn carry_collected(c: &mut Client, store: &Store, id: i64, from: &sources::Destination, to: &sources::Destination) -> anyhow::Result<bool> {
    let device = db::Device { id: from.device_id, name: from.folder.clone() };
    let mut live = Vec::new();
    for (_, path, _) in sources::items(c, id)? {
        let old = format!("{}{path}", from.prefix);
        if files::live_content(c, device.id, &old)?.is_some() {
            live.push((old, format!("{}{path}", to.prefix)));
        }
    }
    if from.device_id != to.device_id {
        for (old, _) in &live {
            ingest::mark_deleted(c, store, &device, old, false)?;
        }
        return Ok(true);
    }
    if let Some((_, new)) = live.iter().find(|(_, new)| files::live_content(c, device.id, new).ok().flatten().is_some()) {
        return Err(ApiError::conflict(format!("{new} exists already; the files the source collected cannot move there")).into());
    }
    let mut moved: Vec<&(String, String)> = Vec::new();
    for pair in &live {
        if let Err(e) = ingest::move_path(c, store, &device, &pair.0, &pair.1) {
            for (old, new) in moved.iter().rev().map(|p| (&p.0, &p.1)) {
                let _ = ingest::move_path(c, store, &device, new, old);
            }
            return Err(e);
        }
        moved.push(pair);
    }
    Ok(false)
}

/// Changes a source: its name, where it writes, how often it runs and what it reads. Credentials left out keep their value.
pub async fn edit_source(State(state): State<Shared>, Extension(who): Extension<Access>, Path((user, name)): Path<(String, String)>, Json(b): Json<SourceEdit>) -> Res<Done> {
    who.require_workspace()?;
    let new_name = b.name.as_deref().map(input::device_name).transpose()?;
    let secs = b.interval_hours.map(hours_to_secs).transpose()?;
    let (stored_id, _) = {
        let (who, user, name) = (who.clone(), user.clone(), name.clone());
        run(&state, move |c| find_source(c, &who, &user, &name)).await?
    };
    let stored = run(&state, move |c| Ok(sources::for_edit(c, stored_id)?))
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
        // What it collected goes to the new place before the source says it writes there.
        let mut recollect = false;
        if let Some(to) = chosen.as_ref() {
            let from = sources::destination_of(c, stored_id)?.ok_or_else(|| ApiError::not_found("No such source"))?;
            if (from.device_id, from.prefix.as_str()) != (to.device_id, to.prefix.as_str()) {
                if sources::for_edit(c, stored_id)?.is_some_and(|s| s.running) {
                    return Err(ApiError::conflict("The source is running. Try again when it has finished").into());
                }
                recollect = carry_collected(c, &store, stored_id, &from, to)?;
            }
        }
        let mut tx = c.transaction()?;
        let mut s = sources::for_edit(&mut tx, stored_id)?.ok_or_else(|| ApiError::not_found("No such source"))?;
        if s.running {
            return Err(ApiError::conflict("The source is running. Try again when it has finished").into());
        }
        if let Some(dest) = chosen {
            (s.device_id, s.prefix) = (dest.device_id, dest.prefix);
        }
        if let Some(new_name) = &new_name {
            if *new_name != s.name && sources::name_taken(&mut tx, s.user_id, new_name, stored_id)? {
                return Err(ApiError::conflict(format!("You already have a source named {new_name}")).into());
            }
            s.name = new_name.clone();
        }
        if let Some(secs) = secs {
            s.interval_secs = secs;
        }
        s.config = config.to_string();
        sources::update(&mut tx, stored_id, &s)?;
        if recollect {
            // What it remembered is about the old folder: it collects everything again into the new one.
            sources::forget_items(&mut tx, stored_id)?;
        }
        let renamed = format!("{user}/{}", s.name);
        if renamed != key {
            // The failure alert of the old name would never be closed.
            alerts::resolve(&mut tx, "source_failed", &key)?;
        }
        db::notify(&mut tx, "jobs", &key)?;
        db::notify(&mut tx, "jobs", &renamed)?;
        tx.commit()?;
        Ok(())
    })
    .await
}

/// Removes a source. Its collected files follow the normal delete rule: kept for the retention period, then removed.
/// Only what the source collected is deleted, not what else is in the folder it wrote into.
pub fn remove_source(c: &mut Client, store: &Store, id: i64, key: &str) -> anyhow::Result<()> {
    let dest = sources::destination_of(c, id)?.ok_or_else(|| ApiError::not_found("No such source"))?;
    let device = db::Device { id: dest.device_id, name: dest.folder };
    for (_, path, _) in sources::items(c, id)? {
        ingest::mark_deleted(c, store, &device, &format!("{}{path}", dest.prefix), true)?;
    }
    sources::delete(c, id)?;
    alerts::resolve(c, "source_failed", key)?;
    db::notify(c, "jobs", key)?;
    Ok(())
}

pub async fn delete_source(State(state): State<Shared>, Extension(who): Extension<Access>, Path((user, name)): Path<(String, String)>) -> Res<Done> {
    who.require_workspace()?;
    let store = state.store.clone();
    act(&state, move |c| {
        let (id, _) = find_source(c, &who, &user, &name)?;
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

/// Flips or requests something on a row of a source or a mirror. `key` tells who to announce it to: `<owner>/<name>` of
/// a source, `mirrors` for a mirror.
fn touch(c: &mut Client, table: Table, id: i64, key: &str, what: Touch) -> anyhow::Result<()> {
    match (table, what) {
        (Table::Sources, Touch::RunNow) => sql::sources::request_source_run().bind(c, &id).map(drop)?,
        (Table::Sources, Touch::ToggleEnabled) => sql::sources::toggle_source().bind(c, &id).map(drop)?,
        (Table::Mirrors, Touch::RunNow) => mirrors::request_run(c, id)?,
        (Table::Mirrors, Touch::ToggleEnabled) => mirrors::toggle(c, id)?,
    }
    db::notify(c, "jobs", key)?;
    Ok(())
}

/// Does `what` to the source `<user>/<name>`, if the user manages it.
async fn touch_source(state: &Shared, who: Access, user: String, name: String, what: Touch) -> Res<Done> {
    who.require_workspace()?;
    act(state, move |c| {
        let (id, _) = find_source(c, &who, &user, &name)?;
        touch(c, Table::Sources, id, &format!("{user}/{name}"), what)
    })
    .await
}

/// Does `what` to the mirror called `name` (administrators).
async fn touch_mirror(state: &Shared, who: Access, name: String, what: Touch) -> Res<Done> {
    who.require_admin()?;
    act(state, move |c| {
        let id = mirror_id(c, &name)?;
        touch(c, Table::Mirrors, id, "mirrors", what)
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
    /// `replica` (the files as they are) or `backup` (compressed and encrypted with the passwords below).
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
    /// The folder to hold, like `ana/laptop/Documents`; everything when empty.
    #[serde(default)]
    scope: String,
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
        scope: b.scope,
        interval_hours: b.interval_hours,
        keep_runs: b.keep_runs,
    };
    mirror.normalize().map_err(|e| ApiError::bad(format!("{e:#}")))?;
    let secs = hours_to_secs(mirror.interval_hours)?;
    let options = Value::Object(mirror.options.into_iter().collect()).to_string();
    act(&state, move |c| {
        if !scope::exists(&mirrors::places(c)?, &mirror.scope) {
            return Err(ApiError::bad(format!("There is nothing called {} to mirror", mirror.scope)).into());
        }
        let new = mirrors::New {
            name: &mirror.name,
            kind: &mirror.kind,
            crypt_password: &mirror.crypt_password,
            crypt_salt: &mirror.crypt_salt,
            remote: &mirror.remote,
            remote_type: &mirror.remote_type,
            options: &options,
            remote_path: &mirror.remote_path,
            scope: &mirror.scope,
            interval_secs: secs,
            keep_runs: mirror.keep_runs,
        };
        if !mirrors::insert(c, &new, unix_now())? {
            return Err(ApiError::conflict(format!("A mirror named {} already exists", mirror.name)).into());
        }
        db::notify(c, "jobs", "mirrors")?;
        Ok(())
    })
    .await
}

#[derive(Deserialize)]
pub struct ScopeQuery {
    #[serde(default)]
    path: String,
}

/// The folders a mirror can be told to hold inside `path` (the top when empty), for the form that adds one.
pub async fn mirror_scope(State(state): State<Shared>, Extension(who): Extension<Access>, axum::extract::Query(q): axum::extract::Query<ScopeQuery>) -> Res<Vec<scope::Choice>> {
    who.require_admin()?;
    let path = scope::normalize(&q.path).map_err(|e| ApiError::bad(format!("{e:#}")))?;
    let choices = run(&state, move |c| {
        let places = mirrors::places(c)?;
        let mut failed = None;
        let found = scope::choices(&places, &path, |device, prefix| {
            sql::mirrors::folders_below().bind(c, &prefix, &device).all().unwrap_or_else(|e| {
                failed = Some(e);
                Vec::new()
            })
        });
        failed.map_or(Ok(found), Err).map_err(Into::into)
    })
    .await?;
    Ok(Json(choices))
}

/// Forget what was uploaded: the next run re-sends everything (use after emptying the remote).
pub async fn resync_mirror(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Done> {
    who.require_admin()?;
    act(&state, move |c| {
        let id = mirror_id(c, &name)?;
        sql::mirrors::delete_mirror_items().bind(c, &id).map(drop)?;
        touch(c, Table::Mirrors, id, "mirrors", Touch::RunNow)
    })
    .await
}

/// The rclone config that restores a backup mirror without fafnirx (administrators). The one place a credential
/// leaves the master: a backup is worthless without its key.
pub async fn mirror_restore_kit(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Value> {
    who.require_admin()?;
    let key = run(&state, move |c| {
        let key = sql::mirrors::mirror_key()
            .bind(c, &name)
            .opt()?
            .ok_or_else(|| ApiError::not_found("No such mirror"))?;
        Ok(key)
    })
    .await?;
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
        mirrors::delete(c, id)?;
        alerts::resolve(c, "mirror_failed", &name)?;
        db::notify(c, "jobs", "mirrors")?;
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
