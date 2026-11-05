//! The commands for the folders attached to a folder of the master.

use super::{connect, wake_client};
use crate::config::{self, Config, Mode, NewFolder, Target, MASTER_KEY};
use crate::net;
use crate::sync::state;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

fn synced_error(s: tonic::Status) -> anyhow::Error {
    if s.code() == tonic::Code::Unimplemented {
        anyhow::anyhow!("this master does not support synced folders yet; update it")
    } else {
        anyhow::anyhow!("{}", s.message())
    }
}

/// The synced folders of the user as `<owner>/<name>`, and which of them are their own.
struct SyncedNames {
    all: Vec<String>,
    mine: Vec<String>,
    /// The stable id of each of `all`, in the same order.
    ids: Vec<String>,
    /// The ones the user can read but not change.
    readonly: Vec<String>,
}

impl SyncedNames {
    /// The folder a user means: `<owner>/<name>` as it is, or the bare name of one of their own.
    fn find(&self, name: &str) -> Option<String> {
        if name.contains('/') || name.starts_with(MASTER_KEY) {
            return self.all.iter().find(|n| *n == name).cloned();
        }
        self.mine.iter().find(|n| n.rsplit('/').next() == Some(name)).cloned()
    }

    /// The stable id of the folder `full`, if the master told it.
    fn id_of(&self, full: &str) -> Option<&str> {
        self.all.iter().position(|n| n == full).and_then(|i| self.ids.get(i)).map(String::as_str)
    }
}

async fn synced_names(client: &mut net::Client) -> Result<SyncedNames> {
    use fafnirx_common::pb::ListFoldersRequest;
    let r = client.list_folders(ListFoldersRequest {}).await.map_err(synced_error)?.into_inner();
    Ok(SyncedNames { all: r.names, mine: r.mine, ids: r.ids, readonly: r.readonly })
}

async fn make_folder(client: &mut net::Client, name: &str) -> Result<()> {
    use fafnirx_common::pb::CreateFolderRequest;
    client.create_folder(CreateFolderRequest { name: name.to_string() }).await.map_err(synced_error)?;
    Ok(())
}

/// Prints the synced folders of the user as `<owner>/<name>`, one per line; the user's own are followed by a tab and `own`, those they cannot change by `read-only`.
pub async fn list_available(config_path: PathBuf) -> Result<()> {
    let mut client = connect(&config_path)?.client;
    let names = synced_names(&mut client).await?;
    for name in &names.all {
        if names.mine.contains(name) {
            println!("{name}\town");
        } else if names.readonly.contains(name) {
            println!("{name}\tread-only");
        } else {
            println!("{name}");
        }
    }
    Ok(())
}

pub async fn create_folder(config_path: PathBuf, name: String) -> Result<()> {
    let mut client = connect(&config_path)?.client;
    if let Some(full) = synced_names(&mut client).await?.find(&name) {
        bail!("there is a synced folder called \"{full}\" already");
    }
    make_folder(&mut client, &name).await?;
    let full = synced_names(&mut client).await?.find(&name).unwrap_or(name);
    println!("made the synced folder \"{full}\"");
    Ok(())
}

/// Attaches a local folder to a folder on the master (a synced folder, one of yours, or `master:<path>`): what is in the folder
/// goes to the master and, unless the mode is `up`, what the other computers put there comes here. With `create` the synced
/// folder is made first when it does not exist.
pub async fn sync_folder(config_path: PathBuf, name: String, dir: PathBuf, create: bool, mode: Mode) -> Result<()> {
    mode.check_available()?;
    let mut client = connect(&config_path)?.client;
    let mut names = synced_names(&mut client).await?;
    let full = match names.find(&name) {
        Some(full) => full,
        None if name.starts_with(MASTER_KEY) => bail!("there is no master folder called \"{name}\"; an administrator makes one in the web app"),
        None if create && !name.contains('/') => {
            make_folder(&mut client, &name).await?;
            names = synced_names(&mut client).await?;
            names.find(&name).context("the synced folder was not made")?
        }
        None => bail!("there is no folder called \"{name}\"; `fafnirx folder create {name}` makes it"),
    };
    if let Some(warning) = crate::service::disk_access::warning_for(&config_path, &dir) {
        eprintln!("warning: {warning}");
    }
    let synced = config::add_folder_to_file(
        &config_path,
        NewFolder { dir: &dir, target: Target::Folder, name: Some(&full), id: names.id_of(&full), mode },
    )?;
    println!("{} is synced with \"{}\" ({})", synced.path.display(), synced.name, mode.name());
    if names.readonly.contains(&full) {
        println!("note: \"{full}\" is shared with you to read only: what is in it comes here, and nothing is sent");
    }
    wake_client(&config_path);
    Ok(())
}

/// Keeps the files of a synced folder whose deletion on the master is held back, instead of deleting them here. They are sent
/// again at the next cycle. Only what other computers deleted is ever held: deletions in the web app are applied at once.
pub fn keep_held(config_path: &Path, name: &str) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let dir = cfg.state_dir(config_path);
    let synced = cfg.attached_named(name)?;
    let state = state::State::open(&crate::synced::state_file(&dir, synced.key()))?;
    let Some((files, _)) = state.pull_hold()? else {
        println!("no deletions of the master are held for \"{}\"", synced.name);
        return Ok(());
    };
    state.request_keep()?;
    wake_client(config_path);
    println!(
        "keeping the {files} files of \"{}\" that were deleted on the master: they stay here, and are sent again at the next cycle",
        synced.name
    );
    Ok(())
}

/// Shows the mode of a synced folder, or sets it (and with `ask` how long a file waits for an answer).
pub fn set_mode(config_path: &Path, name: &str, mode: Option<Mode>, ask_timeout_hours: Option<u64>) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let synced = cfg.attached_named(name)?;
    if mode.is_none() && ask_timeout_hours.is_none() {
        println!("{}: {}, ask timeout {} hours", synced.name, synced.mode.name(), synced.ask_timeout_hours);
        return Ok(());
    }
    let mode = mode.unwrap_or(synced.mode);
    mode.check_available()?;
    let full = config::set_mode_in_file(config_path, name, mode, ask_timeout_hours)?;
    println!("\"{full}\" is synced \"{}\"", mode.name());
    wake_client(config_path);
    Ok(())
}

/// Answers for the files of a synced folder that wait for a decision. The client applies the answer at its next cycle.
pub fn resolve_waiting(config_path: &Path, name: &str, keep: &str, path: Option<&str>) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let dir = cfg.state_dir(config_path);
    let synced = cfg.attached_named(name)?;
    let state = state::State::open(&crate::synced::state_file(&dir, synced.key()))?;
    let answered = state.answer_pending(path, keep)?;
    if answered == 0 {
        println!("no file of \"{}\" is waiting for a decision", synced.name);
        return Ok(());
    }
    wake_client(config_path);
    println!("{answered} files of \"{}\": keeping {keep}", synced.name);
    Ok(())
}

/// Adds a folder of this computer with a target: `computer` (this computer's own namespace on the master, upload only), a master
/// folder (`master:<path>`), or a folder of the user (`<owner>/<name>`, or the name of one of their own).
pub async fn folder_add(config_path: PathBuf, paths: Vec<PathBuf>, target: String, name: Option<String>, create: bool, mode: Option<Mode>) -> Result<()> {
    if target == "computer" {
        if mode.is_some_and(|m| m != Mode::Up) {
            bail!("a folder collected into this computer's own namespace is uploaded only; pick a master or user folder as the target to sync both ways");
        }
        return super::add_folders(&config_path, &paths, name.as_deref());
    }
    if name.is_some() {
        bail!("--name only applies to the target \"computer\"; the target names the folder it is synced with");
    }
    let [path] = <[PathBuf; 1]>::try_from(paths).map_err(|_| anyhow::anyhow!("one folder at a time can be synced with \"{target}\""))?;
    sync_folder(config_path, target, path, create, mode.unwrap_or_default()).await
}
