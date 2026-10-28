//! The commands for shared folders.

use super::{connect, wake_client};
use crate::config::{self, Config};
use crate::net;
use crate::sync::state;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

fn shared_error(s: tonic::Status) -> anyhow::Error {
    if s.code() == tonic::Code::Unimplemented {
        anyhow::anyhow!("this master does not support shared folders yet; update it")
    } else {
        anyhow::anyhow!("{}", s.message())
    }
}

/// The shared folders of the user as `<owner>/<name>`, and which of them are their own.
struct SharedNames {
    all: Vec<String>,
    mine: Vec<String>,
}

impl SharedNames {
    /// The folder a user means: `<owner>/<name>` as it is, or the bare name of one of their own.
    fn find(&self, name: &str) -> Option<String> {
        if name.contains('/') {
            return self.all.iter().find(|n| *n == name).cloned();
        }
        self.mine.iter().find(|n| n.rsplit('/').next() == Some(name)).cloned()
    }
}

async fn shared_names(client: &mut net::Client) -> Result<SharedNames> {
    use fafnirx_common::pb::ListSharedRequest;
    let r = client.list_shared(ListSharedRequest {}).await.map_err(shared_error)?.into_inner();
    Ok(SharedNames { all: r.names, mine: r.mine })
}

async fn create_shared(client: &mut net::Client, name: &str) -> Result<()> {
    use fafnirx_common::pb::CreateSharedRequest;
    client.create_shared(CreateSharedRequest { name: name.to_string() }).await.map_err(shared_error)?;
    Ok(())
}

/// Prints the shared folders of the user as `<owner>/<name>`, one per line; the user's own are followed by a tab and `own`.
pub async fn shared_list(config_path: PathBuf) -> Result<()> {
    let mut client = connect(&config_path)?.client;
    let names = shared_names(&mut client).await?;
    for name in &names.all {
        if names.mine.contains(name) {
            println!("{name}\town");
        } else {
            println!("{name}");
        }
    }
    Ok(())
}

pub async fn shared_create(config_path: PathBuf, name: String) -> Result<()> {
    let mut client = connect(&config_path)?.client;
    if let Some(full) = shared_names(&mut client).await?.find(&name) {
        bail!("there is a shared folder called \"{full}\" already");
    }
    create_shared(&mut client, &name).await?;
    let full = shared_names(&mut client).await?.find(&name).unwrap_or(name);
    println!("made the shared folder \"{full}\"");
    Ok(())
}

/// Attaches a local folder to a shared folder: what is in the folder goes to the master and what the other computers
/// put there comes here. With `create` the shared folder is made first when it does not exist.
pub async fn shared_attach(config_path: PathBuf, name: String, dir: PathBuf, create: bool) -> Result<()> {
    let mut client = connect(&config_path)?.client;
    let full = match shared_names(&mut client).await?.find(&name) {
        Some(full) => full,
        None if create && !name.contains('/') => {
            create_shared(&mut client, &name).await?;
            shared_names(&mut client).await?.find(&name).context("the shared folder was not made")?
        }
        None => bail!("there is no shared folder called \"{name}\"; `fafnirx shared create {name}` makes it"),
    };
    let shared = config::attach_shared_to_file(&config_path, &full, &dir)?;
    println!("{} is shared as \"{}\"", shared.path.display(), shared.name);
    wake_client(&config_path);
    Ok(())
}

pub fn shared_detach(config_path: &Path, name: &str) -> Result<()> {
    let full = config::detach_shared_from_file(config_path, name)?;
    println!("\"{full}\" is no longer synced here; the files stay in the folder and on the master");
    Ok(())
}

/// Keeps the files of a shared folder whose deletion on the master is held back, instead of deleting them here. They are sent
/// again at the next cycle. Only what other computers deleted is ever held: deletions in the web app are applied at once.
pub fn shared_keep(config_path: &Path, name: &str) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let dir = cfg.state_dir(config_path);
    let shared = cfg.shared_named(name)?;
    let state = state::State::open(&crate::shared::state_file(&dir, &shared.name))?;
    let Some((files, _)) = state.pull_hold()? else {
        println!("no deletions of the master are held for \"{}\"", shared.name);
        return Ok(());
    };
    state.request_keep()?;
    wake_client(config_path);
    println!(
        "keeping the {files} files of \"{}\" that were deleted on the master: they stay here, and are sent again at the next cycle",
        shared.name
    );
    Ok(())
}
