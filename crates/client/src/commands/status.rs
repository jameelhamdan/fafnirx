//! What the client is doing, and the deletions it holds back.

use super::wake_client;
use crate::config::{self, Config, Shared};
use crate::daemon;
use crate::sync::state;
use anyhow::Result;
use fafnirx_common::unix_now;
use std::path::Path;

/// Whether the state databases are the caller's to open. In a system-wide install they are the service's alone, so anyone else
/// reads what it publishes (status.json).
fn databases_are_ours(config_path: &Path) -> bool {
    !config::is_system_config(config_path) || crate::service::elevate::is_admin()
}

/// What each shared folder is doing, from its own state database: it is all kept there, so this is true with the client stopped.
fn shared_statuses(config_path: &Path, cfg: &Config, dir: &Path) -> Vec<daemon::SharedStatus> {
    if !databases_are_ours(config_path) {
        return daemon::read_status(dir).map(|s| s.shared).unwrap_or_default();
    }
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
    let shared = shared_statuses(config_path, &cfg, &dir);
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
    if databases_are_ours(config_path) {
        held_lines(&dir, &cfg).iter().for_each(|l| println!("{l}"));
    }
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
    // Needs the databases: for the service, run as administrator (the command asks for it).
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
