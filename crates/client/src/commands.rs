//! The one-shot CLI commands: everything `fafnirx <command>` does except running the daemon.

use crate::config::{self, Config};
use crate::{credentials, daemon, net, state, sync, throttle, update};
use anyhow::{bail, Context, Result};
use fafnirx_common::unix_now;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub fn remove_folder(config_path: &Path, name: &str) -> Result<()> {
    config::remove_folder_from_file(config_path, name)?;
    println!("no longer collecting \"{name}\"; what is already on the orchestrator stays there");
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

pub async fn enroll(config_path: PathBuf, server: String, token: String, ca: PathBuf) -> Result<()> {
    let token = token.trim().to_string();
    if token.is_empty() {
        bail!("empty token");
    }
    let pem = std::fs::read_to_string(&ca).with_context(|| format!("read {}", ca.display()))?;
    if !pem.contains("BEGIN CERTIFICATE") {
        bail!("{} is not a PEM certificate", ca.display());
    }
    let dir = config_path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    std::fs::create_dir_all(&dir)?;
    let ca_path = dir.join("ca.pem");
    std::fs::write(&ca_path, pem)?;

    // Re-enrolling keeps folders and settings; a config that cannot be read is an error, not something to overwrite.
    let mut cfg =
        if config_path.exists() { Config::load(&config_path)? } else { Config::new(server.clone(), ca_path.clone()) };
    cfg.server = server.trim_end_matches('/').to_string();
    cfg.ca_cert = ca_path;
    cfg.token = None;
    cfg.validate()?;
    cfg.save(&config_path)?;
    let where_ = credentials::store(&cfg.server, &config_path, &token)?;
    println!("config written to {}", config_path.display());
    println!("token stored in the {where_}");

    let errors = Mutex::new(Vec::new());
    let mut client = net::client(&cfg, &token)?;
    match daemon::send_heartbeat(&mut client, &cfg, &errors).await {
        Ok(name) => println!("connected to {} as device \"{name}\"", cfg.server),
        Err(e) => println!("warning: could not reach {} yet: {e:#}", cfg.server),
    }
    println!("next: fafnirx add-folder <path> [<path>...], then fafnirx run");
    Ok(())
}

pub fn add_folders(config_path: &Path, paths: &[PathBuf], name: Option<&str>) -> Result<()> {
    if paths.len() > 1 && name.is_some() {
        bail!("--name only works with a single folder");
    }
    let mut failed = 0;
    for path in paths {
        match config::add_folder_to_file(config_path, path, name) {
            Ok(f) => println!("collecting {} as \"{}\"", f.path.display(), f.name),
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
    let Some(s) = daemon::read_status(&dir) else {
        println!("not running (no status yet)");
        return Ok(());
    };
    let age = unix_now() - s.updated;
    if age > 120 {
        println!("not running (last seen {age}s ago)");
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
    let client = net::client(&cfg, &token)?;
    let mut engine = sync::Engine::new(
        client,
        state::State::open(&dir.join("state.db"))?,
        Arc::new(throttle::Limiter::new(cfg.bandwidth_limit_kbps)),
        Arc::new(Mutex::new(Vec::new())),
        daemon::paused_marker(&dir),
    );
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
    let mut client = net::client(&cfg, &token)?;
    if update::check_and_apply(&mut client, &cfg, &dir).await? {
        println!("updated; start fafnirx again");
    } else {
        println!("already up to date ({})", update::VERSION);
    }
    Ok(())
}
