//! Third-party sources (inbound) and mirrors (outbound): their configuration. The background scheduler runs them;
//! this only stores their settings and asks for runs (`run_requested`). A source belongs to a user, a mirror to the
//! administrators. Credentials
//! are write-only: they are never sent back to the browser.

use crate::devices::api::NAME_RULE;
use crate::files::browse::expand;
use crate::files::ingest;
use crate::integrations::spec::MirrorSpec;
use crate::integrations::Table;
use crate::platform::db;
use crate::platform::store::Store;
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

fn state_of(r: &postgres::Row, owner: Option<String>, detail: Value) -> JobState {
    let status: Option<String> = r.get("last_status");
    let name: String = r.get("name");
    JobState {
        key: owner.map_or_else(|| name.clone(), |u| format!("{u}/{name}")),
        name,
        running: status.as_deref() == Some("running"),
        status: status.unwrap_or_else(|| "never run".into()),
        enabled: r.get::<_, i64>("enabled") != 0,
        interval_secs: r.get("interval_secs"),
        last_run_at: r.get("last_run_at"),
        last_ok_at: r.get("last_ok_at"),
        message: r.get::<_, Option<String>>("last_message").unwrap_or_default(),
        detail,
    }
}

/// The sources `who` may see: their own, or all of them for an administrator.
fn source_states(c: &mut Client, who: &Access) -> Result<Vec<JobState>, postgres::Error> {
    let (all, user) = who.scope_args();
    let rows = c.query(
        "SELECT s.name, u.name AS owner, s.config, s.interval_secs, s.enabled, s.last_run_at, s.last_ok_at, s.last_status, s.last_message,
                d.total_bytes AS bytes, d.file_count AS files, s.device_id, d.folder
         FROM sources s JOIN devices d ON d.id = s.device_id JOIN users u ON u.id = d.user_id
         WHERE $1::boolean OR d.user_id = $2::bigint ORDER BY u.name, s.name",
        &[&all, &user],
    )?;
    Ok(rows
        .iter()
        .map(|r| {
            let config: Value = serde_json::from_str(&r.get::<_, String>("config")).unwrap_or(Value::Null);
            let detail = json!({
                "bytes": r.get::<_, i64>("bytes"),
                "files": r.get::<_, i64>("files"),
                "folders": config["folders"].as_array().map_or(0, Vec::len),
                "account": config["service_account"]["client_email"],
                "device": r.get::<_, String>("folder"),
            });
            state_of(r, Some(r.get("owner")), detail)
        })
        .collect())
}

fn mirror_states(c: &mut Client) -> Result<Vec<JobState>, postgres::Error> {
    let rows = c.query(
        "SELECT name, remote, remote_type, options, remote_path, interval_secs, keep_runs, enabled, last_run_at, last_ok_at,
                last_status, last_message, last_files, last_bytes FROM mirrors ORDER BY name",
        &[],
    )?;
    Ok(rows
        .iter()
        .map(|r| {
            let options: Value = serde_json::from_str(&r.get::<_, String>("options")).unwrap_or(Value::Null);
            let mut keys: Vec<&String> = options.as_object().map_or_else(Vec::new, |o| o.keys().collect());
            keys.sort();
            let detail = json!({
                "remote": r.get::<_, String>("remote"),
                "remote_type": r.get::<_, String>("remote_type"),
                "remote_path": r.get::<_, String>("remote_path"),
                "keep_runs": r.get::<_, i64>("keep_runs"),
                "option_keys": keys,
                "files": r.get::<_, Option<i64>>("last_files"),
                "bytes": r.get::<_, Option<i64>>("last_bytes"),
            });
            state_of(r, None, detail)
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
    let row = c
        .query_opt(
            "SELECT s.id, d.folder FROM sources s JOIN devices d ON d.id = s.device_id JOIN users u ON u.id = d.user_id
             WHERE u.name = $1 AND s.name = $2",
            &[&user, &name],
        )?
        .filter(|_| who.is_admin() || who.name == user)
        .ok_or_else(|| ApiError::not_found("No such source"))?;
    Ok((row.get(0), row.get(1)))
}

fn mirror_id(c: &mut Client, name: &str) -> anyhow::Result<i64> {
    Ok(c.query_opt("SELECT id FROM mirrors WHERE name = $1", &[&name])?
        .ok_or_else(|| ApiError::not_found("No such mirror"))?
        .get(0))
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
    let store = state.store.clone();
    run(&state, move |c| {
        let mut tx = c.transaction()?;
        // The source owns a device row (in the workspace of the user who added it), so its files use the
        // same tables as a device's. '!' can never equal a SHA-256 hex digest, so nothing can authenticate as a source.
        let (device_id, folder): (i64, String) = tx
            .query_opt(
                "INSERT INTO devices (name, kind, token_hash, created_at, user_id) VALUES ($1, 'source', '!', $2, $3)
                 ON CONFLICT (user_id, name) DO NOTHING RETURNING id, folder",
                &[&device, &unix_now(), &who.user_id],
            )?
            .map(|r| (r.get(0), r.get(1)))
            .ok_or_else(|| ApiError::conflict(format!("You already have a source named {name}")))?;
        tx.execute(
            "INSERT INTO sources (name, kind, config, interval_secs, device_id, created_at) VALUES ($1, 'gdrive', $2, $3, $4, $5)",
            &[&name, &config.to_string(), &secs, &device_id, &unix_now()],
        )?;
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
    let row = c
        .query_opt(
            "SELECT s.device_id, d.folder FROM sources s JOIN devices d ON d.id = s.device_id WHERE s.id = $1",
            &[&id],
        )?
        .ok_or_else(|| ApiError::not_found("No such source"))?;
    let device = db::Device { id: row.get(0), name: row.get(1) };
    for path in expand(c, &device, "", false)? {
        ingest::mark_deleted(c, store, &device, &path, true)?;
    }
    c.execute("DELETE FROM source_items WHERE source_id = $1", &[&id])?;
    c.execute("DELETE FROM sources WHERE id = $1", &[&id])?;
    db::resolve_alert(c, "source_failed", &device.name)?;
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

/// What `touch` does to a row.
#[derive(Clone, Copy)]
enum Touch {
    RunNow,
    ToggleEnabled,
}

impl Touch {
    fn set(self) -> &'static str {
        match self {
            Touch::RunNow => "run_requested = 1",
            Touch::ToggleEnabled => "enabled = 1 - enabled",
        }
    }
}

/// Flips or requests something on a row of a source or a mirror. `key` tells who to announce it to: the device folder of
/// a source, `mirrors` for a mirror.
fn touch(c: &mut Client, table: Table, id: i64, key: &str, what: Touch) -> anyhow::Result<()> {
    c.execute(&format!("UPDATE {table} SET {} WHERE id = $1", what.set()), &[&id])?;
    db::notify(c, "jobs", key)?;
    Ok(())
}

pub async fn run_source(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path((user, name)): Path<(String, String)>,
) -> Res<Done> {
    who.require_workspace()?;
    run(&state, move |c| {
        let (id, folder) = find_source(c, &who, &user, &name)?;
        touch(c, Table::Sources, id, &folder, Touch::RunNow)
    })
    .await?;
    done()
}

pub async fn toggle_source(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path((user, name)): Path<(String, String)>,
) -> Res<Done> {
    who.require_workspace()?;
    run(&state, move |c| {
        let (id, folder) = find_source(c, &who, &user, &name)?;
        touch(c, Table::Sources, id, &folder, Touch::ToggleEnabled)
    })
    .await?;
    done()
}

pub async fn run_mirror(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(name): Path<String>,
) -> Res<Done> {
    who.require_admin()?;
    run(&state, move |c| {
        let id = mirror_id(c, &name)?;
        touch(c, Table::Mirrors, id, "mirrors", Touch::RunNow)
    })
    .await?;
    done()
}

pub async fn toggle_mirror(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(name): Path<String>,
) -> Res<Done> {
    who.require_admin()?;
    run(&state, move |c| {
        let id = mirror_id(c, &name)?;
        touch(c, Table::Mirrors, id, "mirrors", Touch::ToggleEnabled)
    })
    .await?;
    done()
}

#[derive(Deserialize)]
pub struct NewMirror {
    name: String,
    /// A remote of the rclone config file (`name:path`); when set, the backend, options and path are not used.
    #[serde(default)]
    remote: String,
    #[serde(default)]
    remote_type: String,
    #[serde(default)]
    options: String,
    #[serde(default)]
    remote_path: String,
    #[serde(default = "three_days")]
    interval_hours: f64,
    #[serde(default = "three")]
    keep_runs: i64,
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
    let mut mirror = MirrorSpec {
        name: slug(&b.name)?,
        remote: b.remote,
        remote_type: b.remote_type,
        remote_path: b.remote_path,
        options: parse_options(&b.options)?.into_iter().collect(),
        interval_hours: b.interval_hours,
        keep_runs: b.keep_runs,
    };
    mirror.normalize().map_err(|e| ApiError::bad(format!("{e:#}")))?;
    let secs = hours_to_secs(mirror.interval_hours)?;
    let options = Value::Object(mirror.options.into_iter().collect()).to_string();
    run(&state, move |c| {
        let inserted = c.execute(
            "INSERT INTO mirrors (name, remote, remote_type, options, remote_path, interval_secs, keep_runs, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (name) DO NOTHING",
            &[&mirror.name, &mirror.remote, &mirror.remote_type, &options, &mirror.remote_path, &secs, &mirror.keep_runs, &unix_now()],
        )?;
        if inserted == 0 {
            return Err(ApiError::conflict(format!("A mirror named {} already exists", mirror.name)).into());
        }
        db::notify(c, "jobs", "mirrors")?;
        Ok(())
    })
    .await?;
    done()
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
        c.execute("DELETE FROM mirror_items WHERE mirror_id = $1", &[&id])?;
        touch(c, Table::Mirrors, id, "mirrors", Touch::RunNow)
    })
    .await?;
    done()
}

pub async fn delete_mirror(
    State(state): State<Shared>,
    Extension(who): Extension<Access>,
    Path(name): Path<String>,
) -> Res<Done> {
    who.require_admin()?;
    run(&state, move |c| {
        let id = mirror_id(c, &name)?;
        c.execute("DELETE FROM mirror_items WHERE mirror_id = $1", &[&id])?;
        c.execute("DELETE FROM mirrors WHERE id = $1", &[&id])?;
        db::resolve_alert(c, "mirror_failed", &name)?;
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
