//! Third-party sources (inbound) and mirrors (outbound): their configuration. The background scheduler runs them;
//! this only stores their settings and asks for runs (`run_requested`). A source belongs to a user, a mirror to the
//! administrators. Credentials
//! are write-only: they are never sent back to the browser (the encryption key of a backup mirror, in its restore kit,
//! is the one exception).

use crate::devices::api::NAME_RULE;
use crate::files::browse::expand;
use crate::files::ingest;
use crate::integrations::scope;
use crate::integrations::spec::{normalize_storage, MirrorSpec};
use crate::integrations::Table;
use crate::platform::db;
use crate::platform::store::Store;
use crate::repo::{alerts, mirrors, sources};
use crate::web::access::Access;
use crate::web::{done, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::{Extension, Json};
use fafnirx_common::{unix_now, valid_device_name};
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
    let rows = sources::list(c, all, user)?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let config: Value = serde_json::from_str(&r.config).unwrap_or(Value::Null);
            let detail = json!({
                "bytes": r.bytes,
                "files": r.files,
                "folders": config["folders"].as_array().map_or(0, Vec::len),
                "kind": r.kind,
                "account": config["service_account"]["client_email"],
                "remote": if r.kind == "rclone" { storage_label(&config) } else { Value::Null },
                "device": r.folder,
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
    Ok(mirrors::list(c)?
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

/// The id and the device folder of the source `name` of `user`, unless it is missing or `who` may not manage it (an
/// administrator manages all). Someone else's source does not exist.
fn find_source(c: &mut Client, who: &Access, user: &str, name: &str) -> anyhow::Result<(i64, String)> {
    sources::by_owner(c, user, name)?
        .filter(|_| who.is_admin() || who.name == user)
        .ok_or_else(|| ApiError::not_found("No such source").into())
}

fn mirror_id(c: &mut Client, name: &str) -> anyhow::Result<i64> {
    Ok(mirrors::id_of(c, name)?.ok_or_else(|| ApiError::not_found("No such mirror"))?)
}

pub async fn sources(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<JobState>> {
    who.require_workspace()?;
    Ok(Json(run(&state, move |c| Ok(source_states(c, &who)?)).await?))
}

pub async fn mirrors(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<JobState>> {
    who.require_admin()?;
    Ok(Json(run(&state, |c| Ok(mirror_states(c)?)).await?))
}

pub(crate) fn valid_folder_id(id: &str) -> bool {
    (10..=100).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The id after `marker` in `text`, if any: letters, digits, `_` and `-`.
fn id_after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let rest = &text[text.find(marker)? + marker.len()..];
    let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).unwrap_or(rest.len());
    Some(&rest[..end]).filter(|id| !id.is_empty())
}

/// Drive folder ids, one per line. Pasted folder URLs are reduced to their id.
pub(crate) fn parse_folder_ids(text: &str) -> Result<Vec<String>, ApiError> {
    let mut ids: Vec<String> = Vec::new();
    for token in text.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()) {
        let folder = id_after(token, "/folders/")
            .or_else(|| id_after(token, "?id="))
            .or_else(|| id_after(token, "&id="))
            .unwrap_or(token);
        if !valid_folder_id(folder) {
            return Err(ApiError::bad(format!("{token:?} is not a Drive folder id or folder URL")));
        }
        if !ids.iter().any(|i| i == folder) {
            ids.push(folder.to_string());
        }
    }
    if ids.is_empty() {
        return Err(ApiError::bad("Add at least one folder id"));
    }
    Ok(ids)
}

pub(crate) fn parse_service_account(text: &str) -> Result<Value, ApiError> {
    let sa: Value =
        serde_json::from_str(text).map_err(|_| ApiError::bad("The service account key is not valid JSON"))?;
    let has = |k: &str| sa[k].as_str().is_some_and(|s| !s.is_empty());
    if !has("client_email") || !sa["private_key"].as_str().is_some_and(|k| k.contains("BEGIN")) {
        return Err(ApiError::bad(
            "The service account key needs client_email and private_key (download the JSON key from Google Cloud)",
        ));
    }
    let keep: serde_json::Map<String, Value> = ["client_email", "private_key", "token_uri"]
        .iter()
        .filter_map(|k| sa.get(*k).map(|v| (k.to_string(), v.clone())))
        .collect();
    Ok(Value::Object(keep))
}

/// `key=value` per line; blank lines and `#` comments are ignored.
fn parse_options(text: &str) -> Result<serde_json::Map<String, Value>, ApiError> {
    let mut opts = serde_json::Map::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let (key, value) = line.split_once('=').unwrap_or((line, ""));
        let key = key.trim().to_lowercase().replace('-', "_");
        let ok = !key.is_empty()
            && key.len() <= 64
            && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
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
async fn backend_options(
    text: &str,
    secrets: &std::collections::BTreeMap<String, String>,
) -> Result<Vec<(String, Value)>, ApiError> {
    let mut options: Vec<(String, Value)> = parse_options(text)?.into_iter().collect();
    let bin = crate::integrations::mirror::rclone_bin();
    for (key, plain) in secrets {
        let scrambled =
            crate::integrations::mirror::obscure(&bin, plain).await.map_err(|e| ApiError::bad(format!("{e:#}")))?;
        options.push((key.clone(), Value::String(scrambled)));
    }
    Ok(options)
}

/// What a source of rclone shows of its storage: `type:path`, or the remote of the config file.
fn storage_label(config: &Value) -> Value {
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

fn slug(name: &str) -> Result<String, ApiError> {
    let name = name.trim().to_lowercase();
    if !valid_device_name(&name) {
        return Err(ApiError::bad(NAME_RULE));
    }
    Ok(name)
}

#[derive(Deserialize)]
pub struct NewSource {
    name: String,
    folders: String,
    service_account: String,
    #[serde(default = "day")]
    interval_hours: f64,
}

fn day() -> f64 {
    24.0
}

pub async fn create_source(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(b): Json<NewSource>,
) -> Res<Done> {
    who.require_workspace()?;
    let name = slug(&b.name)?;
    let device = format!("{SOURCE_PREFIX}{name}");
    if !valid_device_name(&device) {
        return Err(ApiError::bad("Name: too long for a source"));
    }
    let config = json!({ "service_account": parse_service_account(&b.service_account)?, "folders": parse_folder_ids(&b.folders)? });
    let secs = hours_to_secs(b.interval_hours)?;
    add_source(&state, who, name, "gdrive", config, secs).await
}

#[derive(Deserialize)]
pub struct NewRcloneSource {
    name: String,
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
}

/// Adds a source that copies a folder of any storage rclone supports.
pub async fn create_rclone_source(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(b): Json<NewRcloneSource>,
) -> Res<Done> {
    who.require_workspace()?;
    let name = slug(&b.name)?;
    if !valid_device_name(&format!("{SOURCE_PREFIX}{name}")) {
        return Err(ApiError::bad("Name: too long for a source"));
    }
    let options = backend_options(&b.options, &b.secrets).await?;
    let (mut remote, mut remote_type, mut remote_path) = (b.remote, b.remote_type, b.remote_path);
    let mut options: std::collections::BTreeMap<String, Value> = options.into_iter().collect();
    normalize_storage(&mut remote, &mut remote_type, &mut remote_path, &mut options)
        .map_err(|e| ApiError::bad(format!("{e:#}")))?;
    let secs = hours_to_secs(b.interval_hours)?;
    let config = json!({
        "remote": remote,
        "remote_type": remote_type,
        "remote_path": remote_path,
        "options": Value::Object(options.into_iter().collect()).to_string(),
    });
    add_source(&state, who, name, "rclone", config, secs).await
}

/// Adds the source and the device that holds its files.
async fn add_source(
    state: &Shared,
    who: Access,
    name: String,
    kind: &'static str,
    config: Value,
    secs: i64,
) -> Res<Done> {
    let device = format!("{SOURCE_PREFIX}{name}");
    let store = state.store.clone();
    run(state, move |c| {
        let mut tx = c.transaction()?;
        // The source owns a device row (in the workspace of the user who added it), so its files use the
        // same tables as a device's. '!' can never equal a SHA-256 hex digest, so nothing can authenticate as a source.
        let (device_id, folder) = sources::insert_device(&mut tx, who.user_id, &device, unix_now())?
            .ok_or_else(|| ApiError::conflict(format!("You already have a source named {name}")))?;
        sources::insert(&mut tx, device_id, &name, kind, &config.to_string(), secs, unix_now())?;
        db::notify(&mut tx, "jobs", &folder)?;
        tx.commit()?;
        let _ = std::fs::create_dir_all(store.live_dir(&folder));
        Ok(())
    })
    .await?;
    done()
}

/// Removes a source. Its collected files follow the normal delete rule: kept for the retention period, then removed.
pub fn remove_source(c: &mut Client, store: &Store, id: i64) -> anyhow::Result<()> {
    let (device_id, folder) = sources::device_of(c, id)?.ok_or_else(|| ApiError::not_found("No such source"))?;
    let device = db::Device { id: device_id, name: folder };
    for path in expand(c, &device, "", false)? {
        ingest::mark_deleted(c, store, &device, &path, true)?;
    }
    sources::delete(c, id)?;
    alerts::resolve(c, "source_failed", &device.name)?;
    db::notify(c, "jobs", &device.name)?;
    Ok(())
}

pub async fn delete_source(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path((user, name)): Path<(String, String)>,
) -> Res<Done> {
    who.require_workspace()?;
    let store = state.store.clone();
    run(&state, move |c| {
        let (id, _) = find_source(c, &who, &user, &name)?;
        remove_source(c, &store, id)
    })
    .await?;
    done()
}

/// What `touch` does to a source or a mirror.
#[derive(Clone, Copy)]
enum Touch {
    RunNow,
    ToggleEnabled,
}

/// Flips or requests something on a row of a source or a mirror. `key` tells who to announce it to: the device folder of
/// a source, `mirrors` for a mirror.
fn touch(c: &mut Client, table: Table, id: i64, key: &str, what: Touch) -> anyhow::Result<()> {
    match (table, what) {
        (Table::Sources, Touch::RunNow) => sources::request_run(c, id)?,
        (Table::Sources, Touch::ToggleEnabled) => sources::toggle(c, id)?,
        (Table::Mirrors, Touch::RunNow) => mirrors::request_run(c, id)?,
        (Table::Mirrors, Touch::ToggleEnabled) => mirrors::toggle(c, id)?,
    }
    db::notify(c, "jobs", key)?;
    Ok(())
}

/// Does `what` to the source `<user>/<name>`, if the user manages it.
async fn touch_source(state: &Shared, who: Access, user: String, name: String, what: Touch) -> Res<Done> {
    who.require_workspace()?;
    run(state, move |c| {
        let (id, folder) = find_source(c, &who, &user, &name)?;
        touch(c, Table::Sources, id, &folder, what)
    })
    .await?;
    done()
}

/// Does `what` to the mirror called `name` (administrators).
async fn touch_mirror(state: &Shared, who: Access, name: String, what: Touch) -> Res<Done> {
    who.require_admin()?;
    run(state, move |c| {
        let id = mirror_id(c, &name)?;
        touch(c, Table::Mirrors, id, "mirrors", what)
    })
    .await?;
    done()
}

pub async fn run_source(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path((user, name)): Path<(String, String)>,
) -> Res<Done> {
    touch_source(&state, who, user, name, Touch::RunNow).await
}

pub async fn toggle_source(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path((user, name)): Path<(String, String)>,
) -> Res<Done> {
    touch_source(&state, who, user, name, Touch::ToggleEnabled).await
}

pub async fn run_mirror(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(name): Path<String>,
) -> Res<Done> {
    touch_mirror(&state, who, name, Touch::RunNow).await
}

pub async fn toggle_mirror(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(name): Path<String>,
) -> Res<Done> {
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

pub async fn create_mirror(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Json(b): Json<NewMirror>,
) -> Res<Done> {
    who.require_admin()?;
    let options = backend_options(&b.options, &b.secrets).await?;
    let mut mirror = MirrorSpec {
        name: slug(&b.name)?,
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
    run(&state, move |c| {
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
    .await?;
    done()
}

#[derive(Deserialize)]
pub struct ScopeQuery {
    #[serde(default)]
    path: String,
}

/// The folders a mirror can be told to hold inside `path` (the top when empty), for the form that adds one.
pub async fn mirror_scope(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    axum::extract::Query(q): axum::extract::Query<ScopeQuery>,
) -> Res<Vec<scope::Choice>> {
    who.require_admin()?;
    let path = scope::normalize(&q.path).map_err(|e| ApiError::bad(format!("{e:#}")))?;
    let choices = run(&state, move |c| {
        let places = mirrors::places(c)?;
        let mut failed = None;
        let found = scope::choices(&places, &path, |device, prefix| {
            mirrors::folders_below(c, device, prefix).unwrap_or_else(|e| {
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
pub async fn resync_mirror(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(name): Path<String>,
) -> Res<Done> {
    who.require_admin()?;
    run(&state, move |c| {
        let id = mirror_id(c, &name)?;
        mirrors::forget_items(c, id)?;
        touch(c, Table::Mirrors, id, "mirrors", Touch::RunNow)
    })
    .await?;
    done()
}

/// The rclone config that restores a backup mirror without fafnirx (administrators). The one place a credential
/// leaves the master: a backup is worthless without its key.
pub async fn mirror_restore_kit(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(name): Path<String>,
) -> Res<Value> {
    who.require_admin()?;
    let key = run(&state, move |c| {
        let key = mirrors::key_of(c, &name)?.ok_or_else(|| ApiError::not_found("No such mirror"))?;
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

pub async fn delete_mirror(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(name): Path<String>,
) -> Res<Done> {
    who.require_admin()?;
    run(&state, move |c| {
        let id = mirror_id(c, &name)?;
        mirrors::delete(c, id)?;
        alerts::resolve(c, "mirror_failed", &name)?;
        db::notify(c, "jobs", "mirrors")?;
        Ok(())
    })
    .await?;
    done()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_folder_urls_reduce_to_ids() {
        let id = "1AbCdEfGhIjKlMnOp";
        let text = format!("https://drive.google.com/drive/folders/{id}?usp=sharing\nhttps://drive.google.com/open?id={id}\n{id}, {id}");
        assert_eq!(parse_folder_ids(&text).unwrap(), [id]);
        assert!(parse_folder_ids("short").is_err());
        assert!(parse_folder_ids("  ").is_err());
    }

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

    #[test]
    fn service_account_keys_need_an_email_and_a_private_key() {
        let ok = r#"{"client_email":"a@b.iam","private_key":"-----BEGIN PRIVATE KEY-----","extra":1}"#;
        assert!(parse_service_account(ok).unwrap().get("extra").is_none(), "only the needed fields are kept");
        assert!(parse_service_account("{}").is_err() && parse_service_account("nope").is_err());
    }
}
