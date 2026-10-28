//! The one-shot CLI commands: everything `fafnirx <command>` does except running the daemon.

use crate::config::{self, Config, Shared};
use crate::{credentials, daemon, net, state, sync, throttle, update};
use anyhow::{bail, Context, Result};
use fafnirx_common::unix_now;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub fn remove_folder(config_path: &Path, name: &str) -> Result<()> {
    config::remove_folder_from_file(config_path, name)?;
    println!("no longer collecting \"{name}\"; what is already on the master stays there");
    Ok(())
}

pub fn list_folders(config_path: &Path) -> Result<()> {
    let cfg = Config::load(config_path)?;
    if cfg.folders.is_empty() && cfg.shared.is_empty() {
        println!("no folders yet: fafnirx add-folder <path>");
    }
    for f in &cfg.folders {
        println!("{}\t{}", f.name, f.path.display());
    }
    for s in &cfg.shared {
        println!("{}\t{}\t(shared, both ways)", s.name, s.path.display());
    }
    Ok(())
}

/// The device token from FAFNIRX_TOKEN, a hidden prompt on a terminal, or the first line of standard input.
pub fn read_token() -> Result<String> {
    use std::io::IsTerminal;
    let token = match std::env::var("FAFNIRX_TOKEN") {
        Ok(t) if !t.trim().is_empty() => t,
        _ if std::io::stdin().is_terminal() => rpassword::prompt_password("Device token: ")?,
        _ => {
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            line
        }
    };
    Ok(token.trim().to_string())
}

pub async fn enroll(config_path: PathBuf, server: String, token: String, start_service: bool) -> Result<()> {
    let token = token.trim().to_string();
    if token.is_empty() {
        bail!("empty token");
    }
    // Re-enrolling keeps folders and settings; a config that cannot be read is an error, not something to overwrite.
    let mut cfg = if config_path.exists() { Config::load(&config_path)? } else { Config::new(server.clone()) };
    cfg.server = server.trim_end_matches('/').to_string();
    cfg.token = None;
    cfg.validate()?;
    cfg.save(&config_path)?;
    if cfg.is_plain_over_internet() {
        println!("warning: {} is plain http on a public address; the token and your files can be read on the way. Use https (a TLS proxy or a Cloudflare tunnel).", cfg.server);
    }
    let where_ = credentials::store(&cfg.server, &config_path, &token)?;
    println!("config written to {}", config_path.display());
    println!("token stored in the {where_}");

    let errors = Mutex::new(Vec::new());
    let mut client = net::client(&cfg, &token, &crate::instance::client_id(&cfg.state_dir(&config_path)))?;
    match daemon::send_heartbeat(&mut client, &cfg, &errors, &daemon::Status::default()).await {
        Ok(name) => println!("connected to {} as device \"{name}\"", cfg.server),
        Err(e) => println!("warning: could not reach {} yet: {e:#}", cfg.server),
    }
    if start_service {
        // Enrolling is the last step of setting a device up, so it also makes the client run from now on.
        match crate::service::install(&config_path, false) {
            Ok(()) => println!("the client runs in the background now and starts at every login"),
            Err(e) => println!(
                "warning: could not start the client as a service ({e:#}); run `fafnirx install-service` to retry"
            ),
        }
    }
    println!("next: fafnirx add-folder <path> [<path>...]; it collects them as soon as they are added");
    Ok(())
}

/// Asks a running client to look at its folders now, not at its next scheduled scan. Without a running
/// client the request waits and starts a cycle as soon as one runs.
fn wake_client(config_path: &Path) {
    let Ok(cfg) = Config::load(config_path) else { return };
    let dir = cfg.state_dir(config_path);
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::write(daemon::sync_now_marker(&dir), b"");
    }
}

pub fn add_folders(config_path: &Path, paths: &[PathBuf], name: Option<&str>) -> Result<()> {
    if paths.len() > 1 && name.is_some() {
        bail!("--name only works with a single folder");
    }
    let mut failed = 0;
    for path in paths {
        match config::add_folder_to_file(config_path, path, name) {
            Ok(f) => {
                println!("collecting {} as \"{}\"", f.path.display(), f.name);
                wake_client(config_path);
            }
            Err(e) => {
                failed += 1;
                eprintln!("{}: {e:#}", path.display());
            }
        }
    }
    if failed > 0 {
        bail!("{failed} of {} folders were not added", paths.len());
    }
    Ok(())
}

/// What the one-shot commands that talk to the master need.
struct Connection {
    cfg: Config,
    state_dir: PathBuf,
    token: String,
    client: net::Client,
}

fn connect(config_path: &Path) -> Result<Connection> {
    let cfg = Config::load(config_path)?;
    let state_dir = cfg.state_dir(config_path);
    let token = credentials::load(&cfg.server, config_path, cfg.token.as_deref())?;
    let client = net::client(&cfg, &token, &crate::instance::client_id(&state_dir))?;
    Ok(Connection { cfg, state_dir, token, client })
}

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
    if shared_names(&mut client).await?.find(&name).is_some() {
        bail!("there is a shared folder called \"{name}\" already");
    }
    create_shared(&mut client, &name).await?;
    println!("made the shared folder \"{name}\"");
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
    config::detach_shared_from_file(config_path, name)?;
    println!("\"{name}\" is no longer synced here; the files stay in the folder and on the master");
    Ok(())
}

/// What each shared folder is doing, from its own state database: it is all kept there, so this is true with the client stopped.
fn shared_statuses(cfg: &Config, dir: &Path) -> Vec<daemon::SharedStatus> {
    states_of(cfg, dir)
        .into_iter()
        .filter_map(|(shared, state)| Some(crate::shared::status_of(shared?, &state)))
        .collect()
}

fn print_shared(shared: &[daemon::SharedStatus]) {
    if !shared.is_empty() {
        println!("shared folders:");
        for f in shared {
            println!("  {} ({}): {}", f.name, f.path, f.summary(unix_now()));
        }
    }
}

/// What the client is doing: for people, or with `json` for scripts and monitoring (the status the client keeps, whether it
/// is running, whether it is paused, and the shared folders).
pub fn status(config_path: &Path, json: bool) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let dir = cfg.state_dir(config_path);
    let shared = shared_statuses(&cfg, &dir);
    if json {
        let status = daemon::read_status(&dir);
        let running = status.as_ref().is_some_and(|s| s.fatal.is_empty() && unix_now() - s.updated <= 120);
        let paused = daemon::paused_marker(&dir).exists();
        println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({ "running": running, "paused": paused, "status": status, "shared": shared })
            )?
        );
        return Ok(());
    }
    if daemon::paused_marker(&dir).exists() {
        println!("paused (fafnirx resume to continue)");
    }
    held_lines(&dir, &cfg).iter().for_each(|l| println!("{l}"));
    let version = print_daemon(&dir);
    print_shared(&shared);
    if let Some(version) = version {
        println!("version {version}");
    }
    Ok(())
}

/// What the client says about itself; the version when it is running.
fn print_daemon(dir: &Path) -> Option<String> {
    let Some(s) = daemon::read_status(dir) else {
        println!("not running (no status yet)");
        return None;
    };
    if !s.fatal.is_empty() {
        println!("not running: {}", s.fatal);
        return None;
    }
    let age = unix_now() - s.updated;
    if age > 120 {
        println!("not running (last seen {age}s ago)");
        return None;
    }
    if s.online {
        println!("online");
    } else {
        println!("ERROR: {}", if s.message.is_empty() { "offline" } else { &s.message });
    }
    if s.last_cycle > 0 {
        println!("last cycle {}s ago: {}", unix_now() - s.last_cycle, s.last);
    }
    if !s.last_error.is_empty() {
        println!("last error: {}", s.last_error);
    }
    Some(s.version)
}
/// The state databases of this client: its own, and one for each shared folder attached.
fn states_of<'a>(cfg: &'a Config, dir: &Path) -> Vec<(Option<&'a Shared>, state::State)> {
    let mut states = Vec::new();
    if let Ok(own) = state::State::open(&dir.join("state.db")) {
        states.push((None, own));
    }
    for s in &cfg.shared {
        if let Ok(state) = state::State::open(&crate::shared::state_file(dir, &s.name)) {
            states.push((Some(s), state));
        }
    }
    states
}

/// What `status` says about deletions held back: sent ones, one line per folder, and the master's that are held here.
fn held_lines(dir: &Path, cfg: &Config) -> Vec<String> {
    let mut lines = Vec::new();
    for (shared, state) in states_of(cfg, dir) {
        let mut held: Vec<_> = state.holds().unwrap_or_default().into_iter().collect();
        held.sort();
        for (folder, (files, since)) in held {
            // The files of a shared folder are all at its top: its holds have no folder name of their own.
            let label = shared.map_or(folder, |s| s.name.clone());
            let left = (since + (cfg.delete_hold_hours * 3600) as i64 - unix_now()).max(0);
            lines.push(format!(
                "holding the deletion of {files} files of \"{label}\" for {}h {}m more (fafnirx release-deletes sends it now)",
                left / 3600,
                left % 3600 / 60
            ));
        }
        if let (Some(Shared { name, .. }), Ok(Some((files, since)))) = (shared, state.pull_hold()) {
            lines.push(format!(
                "holding {files} files of \"{name}\" that were deleted on the master, since {} ago: fafnirx release-deletes \
                 deletes them here, fafnirx shared keep {name} keeps them",
                crate::daemon::age(unix_now() - since)
            ));
        }
    }
    lines
}

/// Asks the client to apply the held deletions at its next cycle, which it starts right away: those it sends, and those of
/// the master it holds back.
pub fn release_deletes(config_path: &Path) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let dir = cfg.state_dir(config_path);
    let lines = held_lines(&dir, &cfg);
    if lines.is_empty() {
        println!("no deletions are held");
        return Ok(());
    }
    for (_, state) in states_of(&cfg, &dir) {
        if state.held_files() > 0 {
            state.request_release()?;
        }
    }
    wake_client(config_path);
    println!("the held deletions are applied at the next cycle:");
    lines.iter().for_each(|l| println!("  {l}"));
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

pub fn set_paused(config_path: &Path, paused: bool) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let dir = cfg.state_dir(config_path);
    std::fs::create_dir_all(&dir)?;
    let marker = daemon::paused_marker(&dir);
    if paused {
        std::fs::write(&marker, b"")?;
        println!("paused");
    } else {
        let _ = std::fs::remove_file(&marker);
        println!("resumed");
    }
    Ok(())
}

pub async fn sync_once(config_path: PathBuf) -> Result<()> {
    let Connection { cfg, state_dir: dir, token, client } = connect(&config_path)?;
    std::fs::create_dir_all(&dir)?;
    let mut engine = sync::Engine::new(
        client,
        state::State::open(&dir.join("state.db"))?,
        Arc::new(throttle::Limiter::new(cfg.bandwidth_limit_kbps)),
        Arc::new(Mutex::new(Vec::new())),
        daemon::paused_marker(&dir),
    );
    engine.exclude = Some(dir.clone());
    let mut report = match engine.cycle(&cfg).await {
        Ok(r) => r,
        Err(e) => bail!("sync failed: {e}"),
    };
    for shared in &cfg.shared {
        let mut attached = crate::shared::Attached::open(&cfg, &token, &dir, shared, Arc::new(Mutex::new(Vec::new())))?;
        match attached.cycle(&cfg).await {
            Ok(r) => report.add(&r),
            Err(e) => bail!("sync of the shared folder {} failed: {e}", shared.name),
        }
    }
    println!("{report}, {} bytes sent", report.bytes);
    Ok(())
}

/// Installs the newest version, or with `check_only` only says whether there is one: `available <version>` or
/// `current <version>` (the tray reads these two).
pub async fn update_now(config_path: PathBuf, check_only: bool) -> Result<()> {
    let Connection { cfg, state_dir: dir, mut client, .. } = connect(&config_path)?;
    if check_only {
        match update::check(&mut client, &cfg).await? {
            Some(version) => println!("available {version}"),
            None => println!("current {}", update::VERSION),
        }
        return Ok(());
    }
    if update::check_and_apply(&mut client, &cfg, &dir, true).await? {
        println!("updated; start fafnirx again");
    } else {
        println!("already up to date ({})", update::VERSION);
    }
    Ok(())
}
