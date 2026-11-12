//! Sources (inbound) and mirrors (outbound), the two directions of a link: their configuration. The background scheduler
//! runs them; this only stores their settings and asks for runs. A source belongs to a user and writes into one folder, a
//! mirror to the administrators and holds one folder or everything. Credentials are write-only: they are never sent back to the
//! browser (the encryption key of a backup mirror, in its restore kit, is the one exception).

use crate::integrations::fetch_source;
use crate::platform::db;
use crate::repo::links::{self, Dir, RunStatus, SourceKind};
use crate::web::access::{Access, Ctx};
use crate::web::{act, run, ApiError, App, Done, Res};
use axum::Json;
use fafnirx_db::queries as sql;
use postgres::Client;
use serde::Serialize;
use serde_json::{json, Value};

mod mirror;
mod source;

pub use mirror::*;
pub use source::*;

const INTERVAL_HOURS: (f64, f64) = (0.25, 24.0 * 365.0);
/// Source folders are named after their source.
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
            let kind = SourceKind::from(r.kind.clone());
            let detail = match dir {
                Dir::In => json!({
                    "bytes": r.bytes,
                    "files": r.files,
                    "kind": r.kind,
                    "remote": storage_label(&kind, &config),
                    "folder": r.scope,
                    "config": shown_config(&kind, &config),
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
                        "scope": r.scope,
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
                running: r.last_status.as_deref().and_then(RunStatus::parse) == Some(RunStatus::Running),
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
fn shown_config(kind: &SourceKind, config: &Value) -> Value {
    if *kind == SourceKind::Rclone {
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

pub async fn sources(Ctx { state, who }: Ctx) -> Res<Vec<JobState>> {
    who.require_workspace()?;
    Ok(Json(run(&state, move |c| Ok(states(c, Dir::In, &who)?)).await?))
}

pub async fn mirrors(Ctx { state, who }: Ctx) -> Res<Vec<JobState>> {
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
            let option = line.split('=').next().unwrap_or(line);
            return Err(ApiError::bad(format!("Option \"{option}\": use key=value with lowercase letters, digits and _")));
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
fn storage_label(kind: &SourceKind, config: &Value) -> Value {
    if matches!(kind, SourceKind::Module(_)) {
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

/// The settings of an rclone source as they are stored.
fn rclone_config(remote: String, remote_type: String, remote_path: String, options: std::collections::BTreeMap<String, Value>) -> Value {
    json!({
        "remote": remote,
        "remote_type": remote_type,
        "remote_path": remote_path,
        "options": Value::Object(options.into_iter().collect()).to_string(),
    })
}

/// What `touch_link` does to a source or a mirror.
#[derive(Clone, Copy)]
enum Touch {
    RunNow,
    ToggleEnabled,
}

/// Flips or requests something on a source or a mirror, and announces it to `key`: `<owner>/<name>` of a source, `mirrors` for a mirror.
fn touch(c: &mut Client, id: i64, key: &str, what: Touch) -> anyhow::Result<()> {
    match what {
        Touch::RunNow => links::request_run(c, id)?,
        Touch::ToggleEnabled => links::toggle(c, id)?,
    }
    db::notify(c, db::Topic::Jobs, key)?;
    Ok(())
}

/// Does `what` to the source `<user>/<name>` if the user manages it, or to the mirror `name` (administrators) when no user is given,
/// and announces it (to `<user>/<name>`, or to `mirrors`).
async fn touch_link(state: &App, who: Access, user: Option<String>, name: String, what: Touch) -> Res<Done> {
    if user.is_some() {
        who.require_workspace()?;
    } else {
        who.require_admin()?;
    }
    act(state, move |c| {
        let (id, key) = match &user {
            Some(user) => (find_source(c, &who, user, &name)?, format!("{user}/{name}")),
            None => (mirror_id(c, &name)?, "mirrors".to_string()),
        };
        touch(c, id, &key, what)
    })
    .await
}

#[cfg(test)]
#[path = "../../../tests/unit/integrations/api.rs"]
mod tests;
