//! fafnirx: the client. Collects folders from this device to the master.

mod commands;
mod config;
mod daemon;
mod net;
mod service;
mod sync;
mod synced;
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
    /// Folders of this computer and where each one goes: this computer's own namespace on the master, a master folder, or a folder of yours
    Folder(FolderCmd),
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
    /// Set this computer up: put the program where it belongs, connect it to the master, collect folders, and run the client as a
    /// service of the computer (Linux, macOS) or at login (Windows). The device token is asked for (hidden), or read from standard
    /// input or FAFNIRX_TOKEN; it is not an argument because other users of the machine can see arguments.
    Install {
        /// The master: nas.lan, nas.lan:7443 or a URL. Without it an enrolled computer is only brought up to date
        #[arg(long)]
        server: Option<String>,
        /// A folder to collect (repeat for more)
        #[arg(long = "folder")]
        folders: Vec<PathBuf>,
        /// The program is where it belongs already (a package or a setup put it there): do not copy it
        #[arg(long)]
        in_place: bool,
        /// Do not install the service or the start at login
        #[arg(long)]
        no_start: bool,
        /// Windows: do not add the program to the PATH
        #[arg(long)]
        no_path: bool,
        /// Say what would be done, and do nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Take this computer's client away: the service, the start at login, and the program. What was backed up stays on the master
    Uninstall {
        /// Also remove the config, the token and the state of this client
        #[arg(long)]
        purge: bool,
        /// Leave the program file (a package removes its own)
        #[arg(long)]
        keep_binary: bool,
        /// Say what would be done, and do nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Restart the client (the service, when it is installed) and the tray, which run the program file as it is now
    Restart,
}

impl Cmd {
    /// Whether the command changes the config or the token, or reads the token, which a system-wide install keeps from users.
    fn needs_admin(&self) -> bool {
        match self {
            Cmd::Install { dry_run, .. } | Cmd::Uninstall { dry_run, .. } => !dry_run,
            Cmd::Sync | Cmd::ReleaseDeletes => true,
            // Listing what this computer has changes and reads nothing secret; every other thing a folder does does.
            Cmd::Folder(FolderCmd { action: FolderAction::List { available: false } }) => false,
            Cmd::Folder(_) => true,
            _ => false,
        }
    }
}

#[derive(clap::Args)]
struct FolderCmd {
    #[command(subcommand)]
    action: FolderAction,
}

#[derive(Subcommand)]
enum FolderAction {
    /// Add a folder: where it goes is its target
    Add {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// computer (this computer's own namespace, upload only), master:<folder> (a subfolder of the master files that an
        /// administrator made), or <owner>/<folder> / the name of a folder of yours
        #[arg(long, default_value = "computer")]
        target: String,
        /// Name in this computer's namespace (only with the target computer; default: the folder name)
        #[arg(long)]
        name: Option<String>,
        /// both (default; the server's version wins a clash), up (send only), keep or ask: see `folder mode`
        #[arg(long)]
        mode: Option<config::Mode>,
        /// Make the folder first when there is none of that name (your own synced folders)
        #[arg(long)]
        create: bool,
    },
    /// List the folders of this computer and where each one goes; with --available, the folders on the master that can be added
    List {
        #[arg(long)]
        available: bool,
    },
    /// Make a folder of yours on the master that your computers and other users can sync (add it with `folder add`)
    Create { name: String },
    /// Show or set how a synced folder syncs: both (both ways; the master's version wins a file changed on both sides and this
    /// computer's is put aside), up (send only; nothing comes back), keep (both ways; both versions are kept) or ask (both ways; a
    /// question on screen)
    Mode {
        name: String,
        mode: Option<config::Mode>,
        /// With ask: hours a file waits for an answer before both versions are kept (0: for ever; default 24)
        #[arg(long)]
        ask_timeout_hours: Option<u64>,
    },
    /// Say which version of the files waiting for a decision (mode `ask`) is kept
    Resolve {
        name: String,
        #[arg(value_parser = ["theirs", "both"])]
        keep: String,
        /// Only this file (a path in the folder); all that wait when not given
        path: Option<String>,
    },
    /// Keep the files whose deletion on the master is held back (instead of `release-deletes`, which deletes them here)
    Keep { name: String },
    /// Stop syncing a folder on this computer (nothing is deleted): a collected one or a synced one
    Remove { name: String },
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
    // Before anything can replace the program (an update): see `service::program`.
    service::program();
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
    let system = config::is_system_config(&config_path) || (!cfg!(windows) && matches!(cli.cmd, Cmd::Install { .. } | Cmd::Uninstall { .. }));
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
        Cmd::Folder(FolderCmd { action }) => match action {
            FolderAction::Add { paths, target, name, mode, create } => tokio_main(commands::folder_add(config_path, paths, target, name, create, mode)),
            FolderAction::List { available: false } => commands::list_folders(&config_path),
            FolderAction::List { available: true } => tokio_main(commands::list_available(config_path)),
            FolderAction::Create { name } => tokio_main(commands::create_folder(config_path, name)),
            FolderAction::Resolve { name, keep, path } => commands::resolve_waiting(&config_path, &name, &keep, path.as_deref()),
            FolderAction::Keep { name } => commands::keep_held(&config_path, &name),
            FolderAction::Mode { name, mode, ask_timeout_hours } => commands::set_mode(&config_path, &name, mode, ask_timeout_hours),
            FolderAction::Remove { name } => commands::remove_folder(&config_path, &name),
        },
        Cmd::Status { json } => commands::status(&config_path, json),
        Cmd::ReleaseDeletes => commands::release_deletes(&config_path),
        Cmd::Pause => commands::set_paused(&config_path, true),
        Cmd::Resume => commands::set_paused(&config_path, false),
        Cmd::Install { server, folders, in_place, no_start, no_path, dry_run } => {
            init_logging(None);
            tokio_main(commands::install(
                config_path,
                commands::Install { server, folders, in_place, no_start, no_path, dry_run },
            ))
        }
        Cmd::Uninstall { purge, keep_binary, dry_run } => commands::uninstall(&config_path, purge, keep_binary, dry_run),
        Cmd::Restart => {
            init_logging(None);
            tokio_main(commands::restart(config_path))
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
