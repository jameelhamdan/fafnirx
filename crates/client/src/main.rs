//! fafnirx: the client. Collects folders from this device to the master.

mod commands;
mod config;
mod daemon;
mod net;
mod service;
mod shared;
mod sync;
#[cfg(feature = "tray")]
mod tray;
mod update;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::{default_config_path, Config};
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Parser)]
#[command(name = "fafnirx", version, about = "Collects folders from this device to a fafnirx master")]
struct Cli {
    /// Config file (default: per-OS location)
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    /// Waits for Enter at the end (set when a command is run again as administrator in a window of its own)
    #[arg(long, global = true, hide = true)]
    pause: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Connect this device to a master. The device token is asked for (hidden), or read from standard input or
    /// FAFNIRX_TOKEN; it is not an argument because other users of the machine can see arguments.
    Enroll {
        /// Address of the master: http://host:7443 on a trusted network, https://... behind a TLS proxy
        #[arg(long)]
        server: String,
        /// Do not start the client in the background and at every login (the default)
        #[arg(long)]
        no_service: bool,
    },
    /// Add one or more folders to collect
    AddFolder {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Name inside the device namespace (default: the folder name). Only with a single folder.
        #[arg(long)]
        name: Option<String>,
    },
    /// Stop collecting a folder (what is already on the master stays there)
    RemoveFolder {
        name: String,
    },
    /// List the collected folders
    Folders,
    /// Shared folders: folders of your user that any of your computers can attach, synced in both directions
    #[command(subcommand)]
    Shared(SharedCmd),
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
    Status {
        /// Print it as JSON, for scripts and monitoring
        #[arg(long)]
        json: bool,
    },
    /// Send now the deletions that are held back because a large part of a folder's files disappeared
    ReleaseDeletes,
    /// Stop uploading until `resume`
    Pause,
    Resume,
    /// Install the newest release and restart the client on it
    Update {
        /// Only say whether there is one (`available <version>` or `current <version>`), install nothing
        #[arg(long)]
        check: bool,
    },
    /// Install the client as a service of this computer: it starts at boot, for every user, and the tray starts at login
    InstallService {
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove the service (what was backed up, the config and the state stay)
    UninstallService {
        #[arg(long)]
        dry_run: bool,
    },
    /// Restart the client (the service, when it is installed) and the tray, which run the program file as it is now
    Restart,
    /// What the Windows Service Control Manager runs
    #[command(hide = true)]
    Service,
}

impl Cmd {
    /// Whether the command changes the config or the token, or reads the token, which a system-wide install keeps from users.
    fn needs_admin(&self) -> bool {
        match self {
            Cmd::InstallService { dry_run } | Cmd::UninstallService { dry_run } => !dry_run,
            Cmd::Enroll { .. }
            | Cmd::AddFolder { .. }
            | Cmd::RemoveFolder { .. }
            | Cmd::Sync
            | Cmd::ReleaseDeletes
            | Cmd::Shared(_) => true,
            _ => false,
        }
    }
}

#[derive(Subcommand)]
enum SharedCmd {
    /// List the shared folders on the master
    List,
    /// Make a shared folder
    Create { name: String },
    /// Sync a local folder with a shared folder
    Attach {
        name: String,
        path: PathBuf,
        /// Make the shared folder first when there is none of that name
        #[arg(long)]
        create: bool,
    },
    /// Stop syncing a shared folder on this computer (nothing is deleted)
    Detach { name: String },
    /// Keep the files whose deletion on the master is held back (instead of `release-deletes`, which deletes them here)
    Keep { name: String },
}

fn init_logging(file: Option<&std::path::Path>) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    // A background program has no console to print a panic to: it goes to the log, with where it happened.
    std::panic::set_hook(Box::new(|info| tracing::error!("panic: {info}")));
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
    let pause = cli.pause;
    let result = run(cli);
    if pause {
        // An administrator's window of its own closes with the command: it is read first.
        if let Err(e) = &result {
            eprintln!("Error: {e:#}");
        }
        println!("\nPress Enter to close.");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    result
}

fn run(cli: Cli) -> Result<()> {
    let config_path = cli.config.clone().unwrap_or_else(default_config_path);
    // The system-wide install belongs to the computer: what changes it, or reads its token, is done by an administrator.
    let system = config::is_system_config(&config_path)
        || (!cfg!(windows) && matches!(cli.cmd, Cmd::InstallService { .. } | Cmd::UninstallService { .. }));
    if cli.cmd.needs_admin() && system && !service::elevate::is_admin() {
        use std::io::IsTerminal;
        let mut args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
        if std::io::stdin().is_terminal() && !cli.pause {
            args.push("--pause".into());
        }
        std::process::exit(service::elevate::rerun(&args)?);
    }
    match cli.cmd {
        Cmd::Run { log_file, hidden } => {
            if respawn_hidden(hidden, true)? {
                return Ok(());
            }
            let cfg = Config::load(&config_path)?;
            let state_dir = cfg.state_dir(&config_path);
            std::fs::create_dir_all(config::control_dir(&state_dir))?;
            let log = log_file.then(|| daemon::log_file(&state_dir));
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
        Cmd::Update { check } => {
            init_logging(None);
            tokio_main(commands::update_now(config_path, check))
        }
        Cmd::Enroll { server, no_service } => {
            init_logging(None);
            let token = commands::read_token()?;
            tokio_main(commands::enroll(config_path, server, token, !no_service))
        }
        Cmd::AddFolder { paths, name } => commands::add_folders(&config_path, &paths, name.as_deref()),
        Cmd::RemoveFolder { name } => commands::remove_folder(&config_path, &name),
        Cmd::Folders => commands::list_folders(&config_path),
        Cmd::Shared(SharedCmd::List) => tokio_main(commands::shared_list(config_path)),
        Cmd::Shared(SharedCmd::Create { name }) => tokio_main(commands::shared_create(config_path, name)),
        Cmd::Shared(SharedCmd::Attach { name, path, create }) => {
            tokio_main(commands::shared_attach(config_path, name, path, create))
        }
        Cmd::Shared(SharedCmd::Detach { name }) => commands::shared_detach(&config_path, &name),
        Cmd::Shared(SharedCmd::Keep { name }) => commands::shared_keep(&config_path, &name),
        Cmd::Status { json } => commands::status(&config_path, json),
        Cmd::ReleaseDeletes => commands::release_deletes(&config_path),
        Cmd::Pause => commands::set_paused(&config_path, true),
        Cmd::Resume => commands::set_paused(&config_path, false),
        Cmd::InstallService { dry_run } => service::install(&config_path, dry_run),
        Cmd::UninstallService { dry_run } => service::uninstall(&config_path, dry_run),
        Cmd::Restart => {
            init_logging(None);
            tokio_main(commands::restart(config_path))
        }
        Cmd::Service => {
            let cfg = Config::load(&config_path)?;
            let state_dir = cfg.state_dir(&config_path);
            std::fs::create_dir_all(config::control_dir(&state_dir))?;
            init_logging(Some(&daemon::log_file(&state_dir)));
            #[cfg(windows)]
            return service::windows::run(config_path);
            #[cfg(not(windows))]
            anyhow::bail!("the service is run by Windows; use afnirx run to run the client by hand")
        }
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
    // The client waits for the disk and the network: two workers are plenty, and each idle thread costs memory.
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(8)
        .enable_all()
        .build()?
        .block_on(f)
}
