//! Changes to the folders that other programs ask the service for. In a system-wide install the config is the administrators'
//! and the service runs as root, while the tray runs as a user and cannot ask for a password. So the tray leaves a request in
//! the control folder, the service applies it for the user who left it, and says how it went in an answer file.
//!
//! The control folder is writable by every user, so the service trusts nothing in a request but its owner (the user who made
//! the file) and only changes folders that belong to that user.

#[cfg(feature = "tray")]
use super::status::{read_status, Status};
use crate::commands;
use crate::config::{self, control_dir, Config};
use anyhow::{bail, Context, Result};
#[cfg(feature = "tray")]
use fafnirx_common::unix_now;
use serde::{Deserialize, Serialize};
#[cfg(any(feature = "tray", test))]
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
#[cfg(feature = "tray")]
use std::time::SystemTime;
use tokio::sync::Notify;

/// A request is a small file; anything larger is not one.
const MAX_REQUEST_BYTES: u64 = 64 << 10;
/// Answers nobody collected are removed after this long (a user cannot remove the service's files from the synced folder).
const ANSWER_KEEP: Duration = Duration::from_secs(300);
/// How long the one who asks waits for the answer.
#[cfg(feature = "tray")]
const ANSWER_WAIT: Duration = Duration::from_secs(60);

/// What the tray asks for; the same as the commands of the same name.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Request {
    AddFolder {
        paths: Vec<PathBuf>,
    },
    RemoveFolder {
        name: String,
    },
    AttachSynced {
        name: String,
        path: PathBuf,
        create: bool,
    },
    SetMode {
        name: String,
        mode: crate::config::Mode,
    },
    /// `keep`: theirs, mine or both; `path` None: every file that waits.
    Resolve {
        name: String,
        keep: String,
        path: Option<String>,
    },
}

impl Request {
    /// The command line that does the same.
    #[cfg(any(feature = "tray", test))]
    pub fn args(&self) -> Vec<OsString> {
        match self {
            Self::AddFolder { paths } => ["folder".into(), "add".into()]
                .into_iter()
                .chain(paths.iter().map(|p| p.as_os_str().to_owned()))
                .collect(),
            Self::RemoveFolder { name } => vec!["folder".into(), "remove".into(), name.into()],
            Self::AttachSynced { name, path, create } => {
                let mut args = vec!["folder".into(), "add".into(), path.as_os_str().to_owned(), "--target".into(), name.into()];
                if *create {
                    args.push("--create".into());
                }
                args
            }
            Self::SetMode { name, mode } => vec!["folder".into(), "mode".into(), name.into(), mode.name().into()],
            Self::Resolve { name, keep, path } => {
                let mut args: Vec<OsString> = vec!["folder".into(), "resolve".into(), name.into(), keep.into()];
                // After `--`: a file name that begins with a dash is not an option.
                if let Some(path) = path {
                    args.extend(["--".into(), path.into()]);
                }
                args
            }
        }
    }
}

#[cfg(any(feature = "tray", test))]
fn request_file(control: &Path, id: &str) -> PathBuf {
    control.join(format!("request.{id}"))
}

fn answer_file(control: &Path, id: &str) -> PathBuf {
    control.join(format!("answer.{id}"))
}

/// The answer file starts with `ok` or `error`, then the text.
#[cfg(any(feature = "tray", test))]
fn parse_answer(text: &str) -> Result<String> {
    match text.split_once('\n') {
        Some(("ok", rest)) => Ok(rest.trim().to_string()),
        Some(("error", rest)) => bail!("{}", rest.trim()),
        _ => bail!("the client's answer could not be read"),
    }
}

/// Asks the service to make a change and waits for its answer. Blocks, as the command it replaces does.
#[cfg(feature = "tray")]
pub fn submit(state_dir: &Path, request: &Request) -> Result<String> {
    if !read_status(state_dir).is_some_and(|s: Status| unix_now() - s.updated < 30) {
        bail!("the client is not running: start it from the menu first");
    }
    let control = control_dir(state_dir);
    let id = format!("{}-{}", std::process::id(), SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_millis());
    // The file appears whole: the service never reads a half-written request.
    fafnirx_common::write_atomic(&request_file(&control, &id), serde_json::to_vec(request)?).context("leave the request for the client")?;
    let answer = answer_file(&control, &id);
    let started = std::time::Instant::now();
    loop {
        if let Ok(text) = std::fs::read_to_string(&answer) {
            let _ = std::fs::remove_file(&answer);
            return parse_answer(&text);
        }
        if started.elapsed() > ANSWER_WAIT {
            let _ = std::fs::remove_file(request_file(&control, &id));
            bail!("the client did not answer in {} seconds: is it running? (`fafnirx status`)", ANSWER_WAIT.as_secs());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Whether the one who asked (`requester`, a user id) may change something that belongs to `owner`: their own, or anything for root.
fn may_change(requester: Option<u32>, owner: Option<u32>) -> bool {
    requester == Some(0) || (requester.is_some() && requester == owner)
}

#[cfg(unix)]
fn owner_of(meta: &std::fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    Some(meta.uid())
}

#[cfg(not(unix))]
fn owner_of(_: &std::fs::Metadata) -> Option<u32> {
    None
}

/// Whether the folder at `path` belongs to the user with this id.
fn belongs_to(requester: Option<u32>, path: &Path) -> bool {
    may_change(requester, std::fs::metadata(path).ok().and_then(|m| owner_of(&m)))
}

/// A folder the requester may collect: it exists, and is theirs.
fn own_directory(path: &Path, mine: &(dyn Fn(&Path) -> bool + Sync)) -> Result<PathBuf> {
    let path = std::fs::canonicalize(path).with_context(|| format!("cannot read {}", path.display()))?;
    if !mine(&path) {
        bail!("{} is not yours; ask an administrator to add it with `sudo fafnirx`", path.display());
    }
    Ok(path)
}

/// Makes the change. `mine` tells whether a folder belongs to whoever asked.
async fn apply(config_path: &Path, request: Request, mine: &(dyn Fn(&Path) -> bool + Sync)) -> Result<String> {
    match request {
        Request::AddFolder { paths } => {
            let (mut added, mut failed) = (0, Vec::new());
            for path in &paths {
                match own_directory(path, mine).and_then(|p| commands::collect_folder(config_path, &p, None)) {
                    Ok(_) => added += 1,
                    Err(e) => failed.push(format!("{}: {e:#}", path.display())),
                }
            }
            if !failed.is_empty() {
                bail!("{added} of {} folders were added:\n{}", paths.len(), failed.join("\n"));
            }
            Ok(format!("{added} folders added"))
        }
        Request::RemoveFolder { name } => {
            let cfg = Config::load(config_path)?;
            let folder = cfg.folder_named(&name).with_context(|| format!("no folder named \"{name}\""))?;
            if !mine(&folder.path) {
                bail!("\"{name}\" is not yours; ask an administrator to remove it with `sudo fafnirx folder remove`");
            }
            config::remove_folder_from_file(config_path, &name)?;
            Ok(String::new())
        }
        Request::AttachSynced { name, path, create } => {
            let path = own_directory(&path, mine)?;
            commands::sync_folder(config_path.to_path_buf(), name, path, create, Default::default()).await?;
            Ok(String::new())
        }
        Request::SetMode { name, mode } => {
            let cfg = Config::load(config_path)?;
            if !mine(&cfg.attached_named(&name)?.path) {
                bail!("\"{name}\" is not yours; ask an administrator to change it with `sudo fafnirx folder mode`");
            }
            commands::change_mode(config_path, &name, mode, None)?;
            Ok(String::new())
        }
        Request::Resolve { name, keep, path } => {
            let cfg = Config::load(config_path)?;
            if !mine(&cfg.attached_named(&name)?.path) {
                bail!("\"{name}\" is not yours; ask an administrator to answer with `sudo fafnirx folder resolve`");
            }
            commands::resolve_waiting(config_path, &name, &keep, path.as_deref())?;
            Ok(String::new())
        }
    }
}

/// Writes an answer where the one who asked can read it. A file or a link made there beforehand is not written through.
fn answer(control: &Path, id: &str, result: &Result<String>) {
    let text = match result {
        Ok(message) => format!("ok\n{message}"),
        Err(e) => format!("error\n{e:#}"),
    };
    let path = answer_file(control, id);
    let _ = std::fs::remove_file(&path);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o644);
    if let Ok(mut file) = options.open(&path) {
        let _ = std::io::Write::write_all(&mut file, text.as_bytes());
    }
}

/// Takes the request file `name` from the control folder, applies it for the user who made it, and answers.
async fn serve_one(config_path: &Path, control: &Path, name: &str) -> bool {
    let path = control.join(name);
    let id = name.trim_start_matches("request.");
    // Not a link, and a plain file of a sensible size: whatever else a user leaves there is not a request.
    let Ok(meta) = std::fs::symlink_metadata(&path) else { return false };
    if !meta.is_file() || meta.len() > MAX_REQUEST_BYTES {
        let _ = std::fs::remove_file(&path);
        return false;
    }
    let requester = owner_of(&meta);
    let request = std::fs::read(&path)
        .context("read the request")
        .and_then(|b| Ok(serde_json::from_slice::<Request>(&b)?));
    let _ = std::fs::remove_file(&path);
    let mine = |folder: &Path| belongs_to(requester, folder);
    let result = match request {
        Ok(request) => {
            tracing::info!("request from user {requester:?}: {request:?}");
            apply(config_path, request, &mine).await
        }
        Err(e) => Err(e.context("not a request")),
    };
    if let Err(e) = &result {
        tracing::warn!("request refused or failed: {e:#}");
    }
    answer(control, id, &result);
    result.is_ok()
}

/// Removes answers that nobody came for.
fn remove_old_answers(control: &Path) {
    let Ok(entries) = std::fs::read_dir(control) else { return };
    for entry in entries.flatten() {
        let old = entry.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
        if entry.file_name().to_string_lossy().starts_with("answer.") && old.is_some_and(|age| age > ANSWER_KEEP) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Answers the requests of the tray. Only a system-wide service has them: for a client of one user the tray changes the config itself.
pub fn spawn(config_path: PathBuf, state_dir: PathBuf, wake: Arc<Notify>) {
    if !cfg!(unix) || !config::is_system_config(&config_path) {
        return;
    }
    tokio::spawn(async move {
        let control = control_dir(&state_dir);
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let names: Vec<String> = std::fs::read_dir(&control)
                .map(|entries| {
                    entries
                        .flatten()
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .filter(|n| {
                            n.strip_prefix("request.")
                                .is_some_and(|id| id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
                        })
                        .collect()
                })
                .unwrap_or_default();
            for name in names {
                if serve_one(&config_path, &control, &name).await {
                    wake.notify_one();
                }
            }
            remove_old_answers(&control);
        }
    });
}

#[cfg(test)]
#[path = "../../tests/unit/daemon/requests.rs"]
mod tests;
