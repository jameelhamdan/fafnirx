//! Third-party sources (inbound) and mirrors (outbound): their configuration. The worker role runs them;
//! this only stores their settings and asks for runs (`run_requested`). Credentials are write-only:
//! they are never sent back to the browser.

use crate::devices::api::NAME_RULE;
use crate::files::browse::expand;
use crate::files::ingest;
use crate::platform::db;
use crate::web::{done, run, ApiError, Done, Res, Shared};
use axum::extract::{Path, State};
use axum::Json;
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

fn state_of(r: &postgres::Row, detail: Value) -> JobState {
    let status: Option<String> = r.get("last_status");
    JobState {
        name: r.get("name"),
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

fn source_states(c: &mut Client) -> Result<Vec<JobState>, postgres::Error> {
    let rows = c.query(
        "SELECT s.name, s.config, s.interval_secs, s.enabled, s.last_run_at, s.last_ok_at, s.last_status, s.last_message,
                COALESCE(SUM(v.size), 0)::BIGINT AS bytes, COUNT(v.id) AS files, s.device_id
         FROM sources s LEFT JOIN files f ON f.device_id = s.device_id AND f.deleted_at IS NULL
         LEFT JOIN versions v ON v.id = f.current_version_id GROUP BY s.id ORDER BY s.name",
        &[],
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
                "device": format!("{SOURCE_PREFIX}{}", r.get::<_, String>("name")),
            });
            state_of(r, detail)
        })
        .collect())
}

fn mirror_states(c: &mut Client) -> Result<Vec<JobState>, postgres::Error> {
    let rows = c.query(
        "SELECT name, remote_type, options, remote_path, interval_secs, keep_runs, enabled, last_run_at, last_ok_at,
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
                "remote_type": r.get::<_, String>("remote_type"),
                "remote_path": r.get::<_, String>("remote_path"),
                "keep_runs": r.get::<_, i64>("keep_runs"),
                "option_keys": keys,
                "files": r.get::<_, Option<i64>>("last_files"),
                "bytes": r.get::<_, Option<i64>>("last_bytes"),
            });
            state_of(r, detail)
        })
        .collect())
}

pub fn job_states(c: &mut Client) -> Result<(Vec<JobState>, Vec<JobState>), postgres::Error> {
    Ok((source_states(c)?, mirror_states(c)?))
}

pub async fn sources(State(state): State<Shared>) -> Res<Vec<JobState>> {
    Ok(Json(run(&state, |c| Ok(source_states(c)?)).await?))
}

pub async fn mirrors(State(state): State<Shared>) -> Res<Vec<JobState>> {
    Ok(Json(run(&state, |c| Ok(mirror_states(c)?)).await?))
}

fn valid_folder_id(id: &str) -> bool {
    (10..=100).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The id after `marker` in `text`, if any: letters, digits, `_` and `-`.
fn id_after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let rest = &text[text.find(marker)? + marker.len()..];
    let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).unwrap_or(rest.len());
    Some(&rest[..end]).filter(|id| !id.is_empty())
}

/// Drive folder ids, one per line. Pasted folder URLs are reduced to their id.
fn parse_folder_ids(text: &str) -> Result<Vec<String>, ApiError> {
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

fn parse_service_account(text: &str) -> Result<Value, ApiError> {
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

fn hours_to_secs(hours: f64) -> Result<i64, ApiError> {
    if !(INTERVAL_HOURS.0..=INTERVAL_HOURS.1).contains(&hours) {
        return Err(ApiError::bad("Interval: between 0.25 hours and one year"));
    }
    Ok((hours * 3600.0) as i64)
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

pub async fn create_source(State(state): State<Shared>, Json(b): Json<NewSource>) -> Res<Done> {
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
        if tx.query_opt("SELECT 1 FROM sources WHERE name = $1", &[&name])?.is_some() {
            return Err(ApiError::conflict(format!("A source named {name} already exists")).into());
        }
        // The source owns a device row, so its files use the same tables as a device's. '!' can never
        // equal a SHA-256 hex digest, so nothing can authenticate as a source.
        let device_id: i64 = tx
            .query_one(
                "INSERT INTO devices (name, kind, token_hash, created_at) VALUES ($1, 'source', '!', $2)
                 ON CONFLICT (name) DO UPDATE SET name = devices.name WHERE devices.kind = 'source' RETURNING id",
                &[&device, &unix_now()],
            )
            .map_err(|_| ApiError::conflict(format!("A device named {device} already exists")))?
            .get(0);
        tx.execute(
            "INSERT INTO sources (name, kind, config, interval_secs, device_id, created_at) VALUES ($1, 'gdrive', $2, $3, $4, $5)",
            &[&name, &config.to_string(), &secs, &device_id, &unix_now()],
        )?;
        db::notify(&mut tx, "jobs", "sources")?;
        tx.commit()?;
        let _ = std::fs::create_dir_all(store.live_dir(&device));
        Ok(())
    })
    .await?;
    done()
}

pub async fn delete_source(State(state): State<Shared>, Path(name): Path<String>) -> Res<Done> {
    let store = state.store.clone();
    run(&state, move |c| {
        let row = c
            .query_opt("SELECT s.id, s.device_id, d.name FROM sources s JOIN devices d ON d.id = s.device_id WHERE s.name = $1", &[&name])?
            .ok_or_else(|| ApiError::not_found("No such source"))?;
        let (id, device) = (row.get::<_, i64>(0), db::Device { id: row.get(1), name: row.get(2) });
        // Collected files follow the normal delete rule: kept for the retention period, then removed.
        for path in expand(c, &device, "", false)? {
            ingest::mark_deleted(c, &store, &device, &path, true)?;
        }
        c.execute("DELETE FROM source_items WHERE source_id = $1", &[&id])?;
        c.execute("DELETE FROM sources WHERE id = $1", &[&id])?;
        db::resolve_alert(c, "source_failed", &name)?;
        db::notify(c, "jobs", "sources")?;
        Ok(())
    })
    .await?;
    done()
}

/// Flips or requests something on a row of `table` ("sources" or "mirrors", never user input).
fn touch(c: &mut Client, table: &str, name: &str, set: &str) -> anyhow::Result<()> {
    let n = c.execute(&format!("UPDATE {table} SET {set} WHERE name = $1"), &[&name])?;
    if n == 0 {
        return Err(ApiError::not_found(format!("No such {}", table.trim_end_matches('s'))).into());
    }
    db::notify(c, "jobs", table)?;
    Ok(())
}

pub async fn run_source(State(state): State<Shared>, Path(name): Path<String>) -> Res<Done> {
    run(&state, move |c| touch(c, "sources", &name, "run_requested = 1")).await?;
    done()
}

pub async fn toggle_source(State(state): State<Shared>, Path(name): Path<String>) -> Res<Done> {
    run(&state, move |c| touch(c, "sources", &name, "enabled = 1 - enabled")).await?;
    done()
}

pub async fn run_mirror(State(state): State<Shared>, Path(name): Path<String>) -> Res<Done> {
    run(&state, move |c| touch(c, "mirrors", &name, "run_requested = 1")).await?;
    done()
}

pub async fn toggle_mirror(State(state): State<Shared>, Path(name): Path<String>) -> Res<Done> {
    run(&state, move |c| touch(c, "mirrors", &name, "enabled = 1 - enabled")).await?;
    done()
}

#[derive(Deserialize)]
pub struct NewMirror {
    name: String,
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

pub async fn create_mirror(State(state): State<Shared>, Json(b): Json<NewMirror>) -> Res<Done> {
    let name = slug(&b.name)?;
    let remote_type = b.remote_type.trim().to_lowercase();
    if remote_type.is_empty()
        || remote_type.len() > 32
        || !remote_type.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    {
        return Err(ApiError::bad("Type: an rclone backend name such as s3, sftp, webdav, b2"));
    }
    let options = parse_options(&b.options)?;
    let secs = hours_to_secs(b.interval_hours)?;
    if !(1..=100).contains(&b.keep_runs) {
        return Err(ApiError::bad("Versions kept: between 1 and 100 runs"));
    }
    run(&state, move |c| {
        let inserted = c.execute(
            "INSERT INTO mirrors (name, remote_type, options, remote_path, interval_secs, keep_runs, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (name) DO NOTHING",
            &[
                &name,
                &remote_type,
                &Value::Object(options).to_string(),
                &b.remote_path.trim(),
                &secs,
                &b.keep_runs,
                &unix_now(),
            ],
        )?;
        if inserted == 0 {
            return Err(ApiError::conflict(format!("A mirror named {name} already exists")).into());
        }
        db::notify(c, "jobs", "mirrors")?;
        Ok(())
    })
    .await?;
    done()
}

/// Forget what was uploaded: the next run re-sends everything (use after emptying the remote).
pub async fn resync_mirror(State(state): State<Shared>, Path(name): Path<String>) -> Res<Done> {
    run(&state, move |c| {
        c.execute("DELETE FROM mirror_items WHERE mirror_id = (SELECT id FROM mirrors WHERE name = $1)", &[&name])?;
        touch(c, "mirrors", &name, "run_requested = 1")
    })
    .await?;
    done()
}

pub async fn delete_mirror(State(state): State<Shared>, Path(name): Path<String>) -> Res<Done> {
    run(&state, move |c| {
        let id: i64 = c
            .query_opt("SELECT id FROM mirrors WHERE name = $1", &[&name])?
            .ok_or_else(|| ApiError::not_found("No such mirror"))?
            .get(0);
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
