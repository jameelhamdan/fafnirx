//! fafnirx: the client. Collects folders from this device to the orchestrator.

mod commands;
mod config;
mod credentials;
mod daemon;
mod hash;
mod net;
mod scan;
mod service;
mod state;
mod sync;
mod throttle;
#[cfg(feature = "tray")]
mod tray;
mod update;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::{default_config_path, Config};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Parser)]
#[command(name = "fafnirx", version, about = "Collects folders from this device to a fafnirx orchestrator")]
struct Cli {
    /// Config file (default: per-OS location)
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Connect this device to an orchestrator
    Enroll {
        #[arg(long)]
        server: String,
        /// Device token. Tokens are URL-safe base64 and may start with `-`.
        #[arg(long, allow_hyphen_values = true)]
        token: String,
        /// CA certificate downloaded from the orchestrator web UI
        #[arg(long)]
        ca: PathBuf,
    },
    /// Add one or more folders to collect
    AddFolder {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Name inside the device namespace (default: the folder name). Only with a single folder.
        #[arg(long)]
        name: Option<String>,
    },
    /// Stop collecting a folder (what is already on the orchestrator stays there)
    RemoveFolder {
        name: String,
    },
    /// List the collected folders
    Folders,
    /// Run in the foreground: watch, upload, stay connected
    Run {
        /// Also write the log to a file in the state directory
        #[arg(long)]
        log_file: bool,
        /// Windows: detach from the console
        #[arg(long)]
        hidden: bool,
    },
    /// Show a tray icon with the client status (pause/resume, quit)
    #[cfg(feature = "tray")]
    Tray {
        /// Windows: detach from the console
        #[arg(long)]
        hidden: bool,
    },
    /// Run one collection cycle and exit
    Sync,
    /// Show what the running client is doing
    Status,
    /// Stop uploading until `resume`
    Pause,
    Resume,
    /// Check for a signed update and install it
    Update,
    /// Start automatically at login
    InstallService {
        #[arg(long)]
        dry_run: bool,
    },
    UninstallService {
        #[arg(long)]
        dry_run: bool,
    },
}

fn init_logging(file: Option<&std::path::Path>) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    if let Some(path) = file {
        // Start a fresh file when the old one gets large.
        if std::fs::metadata(path).map(|m| m.len() > 5 << 20).unwrap_or(false) {
            let _ = std::fs::rename(path, path.with_extension("log.old"));
        }
        if let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            builder.with_ansi(false).with_writer(Mutex::new(f)).init();
            return;
        }
    }
    builder.init();
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config_path = cli.config.clone().unwrap_or_else(default_config_path);
    match cli.cmd {
        Cmd::Run { log_file, hidden } => {
            if respawn_hidden(hidden, true)? {
                return Ok(());
            }
            let cfg = Config::load(&config_path)?;
            let state_dir = cfg.state_dir(&config_path);
            std::fs::create_dir_all(&state_dir)?;
            let log = log_file.then(|| state_dir.join("fafnirx.log"));
            init_logging(log.as_deref());
            tokio_main(daemon::run(config_path))
        }
        #[cfg(feature = "tray")]
        Cmd::Tray { hidden } => {
            if respawn_hidden(hidden, false)? {
                return Ok(());
            }
            tray::run(config_path)
        }
        Cmd::Sync => {
            init_logging(None);
            tokio_main(commands::sync_once(config_path))
        }
        Cmd::Update => {
            init_logging(None);
            tokio_main(commands::update_now(config_path))
        }
        Cmd::Enroll { server, token, ca } => {
            init_logging(None);
            tokio_main(commands::enroll(config_path, server, token, ca))
        }
        Cmd::AddFolder { paths, name } => commands::add_folders(&config_path, &paths, name.as_deref()),
        Cmd::RemoveFolder { name } => commands::remove_folder(&config_path, &name),
        Cmd::Folders => commands::list_folders(&config_path),
        Cmd::Status => commands::status(&config_path),
        Cmd::Pause => commands::set_paused(&config_path, true),
        Cmd::Resume => commands::set_paused(&config_path, false),
        Cmd::InstallService { dry_run } => service::install(&config_path, dry_run),
        Cmd::UninstallService { dry_run } => service::uninstall(dry_run),
    }
}

/// Windows: relaunch this command detached from the console. Returns true when the parent should exit.
#[allow(unused_variables)]
fn respawn_hidden(hidden: bool, log_file: bool) -> Result<bool> {
    #[cfg(windows)]
    if hidden {
        let mut args: Vec<String> = std::env::args().skip(1).filter(|a| a != "--hidden").collect();
        if log_file && !args.iter().any(|a| a == "--log-file") {
            args.push("--log-file".into());
        }
        service::spawn_detached(args)?;
        return Ok(true);
    }
    Ok(false)
}

fn tokio_main<F: std::future::Future<Output = Result<()>>>(f: F) -> Result<()> {
    tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_that_starts_with_a_dash_is_a_value_not_a_flag() {
        let args = ["fafnirx", "enroll", "--server", "https://x:7443", "--token", "-xVeqMt4730Es_-", "--ca", "ca.pem"];
        match Cli::try_parse_from(args).unwrap().cmd {
            Cmd::Enroll { token, .. } => assert_eq!(token, "-xVeqMt4730Es_-"),
            _ => panic!("expected enroll"),
        }
    }
}
