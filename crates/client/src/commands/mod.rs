//! The one-shot CLI commands: everything `fafnirx <command>` does except running the daemon.

mod folder;
mod install;
mod status;

pub use folder::{create_folder, folder_add, keep_held, list_available, resolve_waiting, set_mode, sync_folder};
pub use install::{install, uninstall, Install};
pub use status::{release_deletes, status};

use crate::config::credentials;
use crate::config::{self, control_dir, Config, Mode, NewFolder, Target};
use crate::sync::{state, throttle};
use crate::{daemon, net, service, sync, update};
use anyhow::{bail, Context, Result};
use fafnirx_common::unix_now;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Stops syncing a folder of this computer, collected or attached to a folder of the master. Nothing is deleted.
pub fn remove_folder(config_path: &Path, name: &str) -> Result<()> {
    let full = config::remove_folder_from_file(config_path, name)?;
    println!("\"{full}\" is no longer synced here; the files stay in the folder and on the master");
    Ok(())
}

pub fn list_folders(config_path: &Path) -> Result<()> {
    let cfg = Config::load(config_path)?;
    if cfg.folders.is_empty() {
        println!("no folders yet: fafnirx folder add <path>");
    }
    for f in &cfg.folders {
        if f.is_computer() {
            println!("{}\t{}", f.name, f.path.display());
        } else {
            println!("{}\t{}\t(synced {})", f.name, f.path.display(), f.mode.name());
        }
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

/// Connects this computer to the master: writes the config, keeps the token, and says hello. `fafnirx install` calls it.
pub async fn enroll(config_path: PathBuf, server: String, token: String) -> Result<()> {
    let token = token.trim().to_string();
    if token.is_empty() {
        bail!("empty token");
    }
    // The folders of a system-wide install are made with their permissions before anything is written to them.
    service::prepare_dirs(&config_path)?;
    // Re-enrolling keeps folders and settings; a config that cannot be read is an error, not something to overwrite.
    let mut cfg = if config_path.exists() { Config::load(&config_path)? } else { Config::new(server.clone()) };
    cfg.server = config::normalize_server(&server);
    cfg.validate()?;
    cfg.save(&config_path)?;
    if cfg.is_plain_over_internet() {
        println!(
            "warning: {} is plain http on a public address; the token and your files can be read on the way. Use https (a TLS proxy or a Cloudflare tunnel).",
            cfg.server
        );
    }
    let where_ = credentials::store(&cfg.server, &config_path, &token)?;
    println!("config written to {}", config_path.display());
    println!("token stored in the {where_}");

    let errors = Mutex::new(Vec::new());
    let mut client = net::client(&cfg, &token, &crate::service::instance::client_id(&cfg.state_dir(&config_path)))?;
    match daemon::send_heartbeat(&mut client, &cfg, &errors, &daemon::Status::default()).await {
        Ok(name) => println!("connected to {} as device \"{name}\"", cfg.server),
        Err(e) => println!("warning: could not reach {} yet: {e:#}", cfg.server),
    }
    Ok(())
}

/// Asks a running client to look at its folders now, not at its next scheduled scan. Without a running
/// client the request waits and starts a cycle as soon as one runs.
fn wake_client(config_path: &Path) {
    let Ok(cfg) = Config::load(config_path) else { return };
    let dir = cfg.state_dir(config_path);
    if std::fs::create_dir_all(control_dir(&dir)).is_ok() {
        let _ = std::fs::write(daemon::sync_now_marker(&dir), b"");
    }
}

pub fn add_folders(config_path: &Path, paths: &[PathBuf], name: Option<&str>) -> Result<()> {
    if paths.len() > 1 && name.is_some() {
        bail!("--name only works with a single folder");
    }
    let mut failed = 0;
    for path in paths {
        if let Some(warning) = crate::service::disk_access::warning_for(config_path, path) {
            eprintln!("warning: {warning}");
        }
        match config::add_folder_to_file(config_path, NewFolder { dir: path, target: Target::Computer, name, id: None, mode: Mode::Up }) {
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
    let token = credentials::load(&cfg.server, config_path)?;
    let client = net::client(&cfg, &token, &crate::service::instance::client_id(&state_dir))?;
    Ok(Connection { cfg, state_dir, token, client })
}

pub fn set_paused(config_path: &Path, paused: bool) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let dir = cfg.state_dir(config_path);
    std::fs::create_dir_all(control_dir(&dir))?;
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
    std::fs::create_dir_all(control_dir(&dir))?;
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
    for synced in cfg.attached_folders() {
        let mut attached = crate::synced::Attached::open(&cfg, &token, &dir, synced, Arc::new(Mutex::new(Vec::new())))?;
        match attached.cycle(&cfg).await {
            Ok(r) => report.add(&r),
            Err(e) => bail!("sync of the synced folder {} failed: {e}", synced.name),
        }
    }
    println!("{report}, {} bytes sent", report.bytes);
    Ok(())
}

/// Installs the newest version and starts the client again on it, or with `check_only` only says whether there is one:
/// `available <version>` or `current <version>` (the tray reads these two).
pub async fn update_now(config_path: PathBuf, check_only: bool) -> Result<()> {
    let cfg = Config::load(&config_path)?;
    let dir = cfg.state_dir(&config_path);
    if check_only {
        match update::check().await? {
            Some(version) => println!("available {version}"),
            None => println!("current {}", update::VERSION),
        }
        return Ok(());
    }
    if config::is_system_config(&config_path) && service::installed() {
        return update_service(&dir).await;
    }
    if update::check_and_apply(&dir, true).await? {
        println!("updated; starting the client again");
        service::restart_background(&config_path, &dir)?;
    } else {
        println!("already up to date ({})", update::VERSION);
    }
    Ok(())
}

/// Asks the service to update itself, which only it may (the program is where users cannot change it), and waits until it
/// runs again on the new version.
async fn update_service(dir: &Path) -> Result<()> {
    let result = daemon::update_result_file(dir);
    // The answer starts with this, which no earlier request had.
    let nonce = format!("{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis());
    std::fs::write(daemon::update_marker(dir), &nonce).context("ask the service to update")?;
    println!("asked the service to update");
    let first = format!("{nonce}\n");
    let answer = wait_for(Duration::from_secs(180), || {
        std::fs::read_to_string(&result).ok().and_then(|t| t.strip_prefix(&first).map(str::to_string))
    })
    .await
    .context("the service did not answer in 3 minutes: is it running? (`fafnirx status`)")?;
    let answer = answer.trim();
    if let Some(version) = answer.strip_prefix("updated ") {
        let running = wait_for(Duration::from_secs(90), || {
            daemon::read_status(dir).filter(|s| s.version == version && s.online && unix_now() - s.updated < 30)
        })
        .await;
        match running {
            Some(_) => println!("updated to {version}; the service is running again"),
            None => bail!("updated to {version}, but the service is not back yet: check `fafnirx status`"),
        }
    } else if let Some(version) = answer.strip_prefix("current ") {
        println!("already up to date ({version})");
    } else {
        bail!("the service could not update: {}", answer.strip_prefix("failed: ").unwrap_or(answer));
    }
    Ok(())
}

/// Calls `check` every half second until it returns something, or `limit` has passed.
async fn wait_for<T>(limit: Duration, mut check: impl FnMut() -> Option<T>) -> Option<T> {
    let started = std::time::Instant::now();
    loop {
        if let Some(found) = check() {
            return Some(found);
        }
        if started.elapsed() > limit {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Restarts what runs of the client: the client (the service when it is installed, which runs the program file as it is now,
/// else the client that runs by itself) and the trays, which start over or are started if this desktop has none.
fn restart_trays(config_path: &Path, state_dir: &Path) {
    let _ = std::fs::write(daemon::restart_tray_marker(state_dir), b"");
    #[cfg(feature = "tray")]
    if crate::tray::ensure_tray(config_path, &control_dir(state_dir)) {
        println!("tray started");
    }
    #[cfg(not(feature = "tray"))]
    let _ = config_path;
}

/// Restarts the client and the trays.
pub async fn restart(config_path: PathBuf) -> Result<()> {
    let cfg = Config::load(&config_path)?;
    let dir = cfg.state_dir(&config_path);
    if config::is_system_config(&config_path) && service::installed() {
        // The new process is the one with another id: a status that is only newer may be the last write of the old one.
        let before = daemon::read_status(&dir).map(|s| s.pid);
        service::restart()?;
        let running = wait_for(Duration::from_secs(60), || {
            daemon::read_status(&dir).filter(|s| s.online && Some(s.pid) != before && s.fatal.is_empty())
        })
        .await;
        match running {
            Some(s) => println!("restarted: fafnirx {} is running", s.version),
            None => bail!("restarted, but the client is not online yet: check `fafnirx status`"),
        }
        restart_trays(&config_path, &dir);
        return Ok(());
    }
    service::restart_background(&config_path, &dir)?;
    restart_trays(&config_path, &dir);
    println!("restarted");
    Ok(())
}
