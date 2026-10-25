//! The one-shot CLI commands: everything `fafnirx <command>` does except running the daemon.

use crate::config::{self, Config};
use crate::{credentials, daemon, net, state, sync, throttle, update};
use anyhow::{bail, Result};
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
    if cfg.folders.is_empty() {
        println!("no folders yet: fafnirx add-folder <path>");
    }
    for f in &cfg.folders {
        println!("{}\t{}", f.name, f.path.display());
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

pub fn status(config_path: &Path) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let dir = cfg.state_dir(config_path);
    if daemon::paused_marker(&dir).exists() {
        println!("paused (fafnirx resume to continue)");
    }
    held_lines(&dir, &cfg).iter().for_each(|l| println!("{l}"));
    let Some(s) = daemon::read_status(&dir) else {
        println!("not running (no status yet)");
        return Ok(());
    };
    if !s.fatal.is_empty() {
        println!("not running: {}", s.fatal);
        return Ok(());
    }
    let age = unix_now() - s.updated;
    if age > 120 {
        println!("not running (last seen {age}s ago)");
        return Ok(());
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
    println!("version {}", s.version);
    Ok(())
}

/// What `status` says about deletions held back, one line per folder.
fn held_lines(dir: &Path, cfg: &Config) -> Vec<String> {
    let Ok(state) = state::State::open(&dir.join("state.db")) else { return Vec::new() };
    let mut held: Vec<_> = state.holds().unwrap_or_default().into_iter().collect();
    held.sort();
    held.iter()
        .map(|(folder, (files, since))| {
            let left = (since + (cfg.delete_hold_hours * 3600) as i64 - unix_now()).max(0);
            format!(
                "holding the deletion of {files} files of \"{folder}\" for {}h {}m more (fafnirx release-deletes sends it now)",
                left / 3600,
                left % 3600 / 60
            )
        })
        .collect()
}

/// Asks the client to send the held deletions at its next cycle, which it starts right away.
pub fn release_deletes(config_path: &Path) -> Result<()> {
    let cfg = Config::load(config_path)?;
    let dir = cfg.state_dir(config_path);
    let lines = held_lines(&dir, &cfg);
    if lines.is_empty() {
        println!("no deletions are held");
        return Ok(());
    }
    state::State::open(&dir.join("state.db"))?.request_release()?;
    wake_client(config_path);
    println!("the held deletions are sent at the next cycle:");
    lines.iter().for_each(|l| println!("  {l}"));
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
    let cfg = Config::load(&config_path)?;
    let dir = cfg.state_dir(&config_path);
    std::fs::create_dir_all(&dir)?;
    let token = credentials::load(&cfg.server, &config_path, cfg.token.as_deref())?;
    let client = net::client(&cfg, &token, &crate::instance::client_id(&dir))?;
    let mut engine = sync::Engine::new(
        client,
        state::State::open(&dir.join("state.db"))?,
        Arc::new(throttle::Limiter::new(cfg.bandwidth_limit_kbps)),
        Arc::new(Mutex::new(Vec::new())),
        daemon::paused_marker(&dir),
    );
    engine.exclude = Some(dir);
    match engine.cycle(&cfg).await {
        Ok(r) => {
            println!("{r}, {} bytes sent", r.bytes);
            Ok(())
        }
        Err(e) => bail!("sync failed: {e}"),
    }
}

pub async fn update_now(config_path: PathBuf) -> Result<()> {
    let cfg = Config::load(&config_path)?;
    let dir = cfg.state_dir(&config_path);
    let token = credentials::load(&cfg.server, &config_path, cfg.token.as_deref())?;
    let mut client = net::client(&cfg, &token, &crate::instance::client_id(&dir))?;
    if update::check_and_apply(&mut client, &cfg, &dir).await? {
        println!("updated; start fafnirx again");
    } else {
        println!("already up to date ({})", update::VERSION);
    }
    Ok(())
}
