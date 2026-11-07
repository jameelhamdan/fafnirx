//! The client as a program that runs by itself, for every user where the OS allows it. Linux (a systemd unit) and macOS (a
//! LaunchDaemon): a service of the computer that starts at boot, is started again when it fails, and is managed by the operating
//! system like any other program (`unix`). Windows: the client of the user, started when they log in (`windows`). The tray of each
//! user is started when they log in.
//!
//! The config, token and state of a service are in the folders of the computer (`config::system_data_dir`), not in a profile, and
//! it runs with the rights to read the folders of every user, so only an administrator may change them (`prepare_dirs`).

pub mod disk_access;
pub mod elevate;
pub mod instance;
#[cfg(not(windows))]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(not(windows))]
use unix as os;
#[cfg(windows)]
use windows as os;

#[cfg(windows)]
pub use os::spawn_detached;
#[cfg(feature = "tray")]
pub use os::start;
// `uninstall` removes the service (or the entries that start the client at login) and the trays; what was backed up, the
// config and the state stay.
pub use os::{exit_to_be_restarted, installed, remove_program, restart, uninstall};

use crate::config::{control_dir, is_system_config, system_config_path, system_data_dir, system_state_dir};
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

/// Started when the user who installed the client logs in (Windows).
#[cfg(windows)]
const WINDOWS_USER_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// This program's file as it was when the process started (`main` asks for it first). An update replaces the file while this
/// process runs: on Windows the running image is then renamed, and `current_exe` reports that name, which is not a program to
/// start. Everything that starts the client again after an update runs this path, which is the new version.
pub fn program() -> std::path::PathBuf {
    static PROGRAM: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    PROGRAM.get_or_init(|| std::env::current_exe().unwrap_or_else(|_| "fafnirx".into())).clone()
}

/// A PowerShell that reads no profile and asks nothing: the one way this program runs a script.
pub fn powershell(script: &str) -> Command {
    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    command
}

/// Runs a command and shows it, or only shows it for a dry run.
fn run(dry: bool, cmd: &str, args: &[&str]) -> Result<()> {
    println!("$ {cmd} {}", args.join(" "));
    if dry {
        return Ok(());
    }
    run_checked(cmd, args)
}

/// Runs a program and waits for it: an error when it cannot be started or does not succeed.
fn run_checked(program: impl AsRef<std::ffi::OsStr>, args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>) -> Result<()> {
    let program = program.as_ref();
    let status = Command::new(program)
        .args(args)
        .status()
        .with_context(|| format!("run {}", program.to_string_lossy()))?;
    if !status.success() {
        bail!("{} failed with {status}", program.to_string_lossy());
    }
    Ok(())
}

/// Who may do what in the folders of the system-wide install. The service and the administrators may do everything. Every user
/// may read the config (the tray shows it) and write in the control folder (pause, update, restart and the status are there);
/// the token and the databases are for the service and the administrators.
pub fn prepare_dirs(config_path: &Path) -> Result<()> {
    if !is_system_config(config_path) {
        return Ok(());
    }
    let config_dir = system_config_path().parent().map(Path::to_path_buf).unwrap_or_else(system_data_dir);
    let (data, state) = (system_data_dir(), system_state_dir());
    let control = control_dir(&state);
    for dir in [&config_dir, &data, &state, &control] {
        std::fs::create_dir_all(dir).with_context(|| format!("make {}", dir.display()))?;
    }
    os::set_dir_modes(&config_dir, &data, &state, &control)
}

/// Installs the client (a service of the computer on Linux and macOS, the client of the user on Windows) and removes the older
/// versions of it that are left on the computer, so that only this one remains.
pub fn install(config_path: &Path, dry: bool) -> Result<()> {
    let exe = std::env::current_exe()?;
    os::install_service(config_path, &fafnirx_common::plain_path(&exe), dry)?;
    if !dry {
        if let Ok(exe) = std::env::current_exe() {
            crate::update::purge::announce(&fafnirx_common::plain_path(&exe));
        }
    }
    Ok(())
}

/// Starts the client in the background without a service (an install of one user, or a build run by hand).
pub fn start_background(config_path: &Path) -> Result<()> {
    let mut command = Command::new(program());
    command.arg("--config").arg(config_path).args(["run", "--log-file"]);
    if cfg!(windows) {
        command.arg("--hidden");
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command.spawn().context("start the client")?;
    Ok(())
}

/// Ends a client that runs by itself and starts it again.
pub fn restart_background(config_path: &Path, state_dir: &Path) -> Result<()> {
    if instance::is_held(state_dir, "daemon") {
        std::fs::write(crate::daemon::stop_marker(state_dir), b"").context("ask the client to stop")?;
        let started = std::time::Instant::now();
        while instance::is_held(state_dir, "daemon") && started.elapsed() < std::time::Duration::from_secs(30) {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    }
    start_background(config_path)
}

/// Ends the client the way it was asked to: it stays stopped (the "stop" of the tray).
pub fn exit_clean() -> ! {
    std::process::exit(0)
}
