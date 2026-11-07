//! The client on Windows is no service of the computer: it is the client of the user who installed it, started when they log in
//! (`HKCU\...\Run`), needing no administrator, with its config, token and state in their profile.

use super::{restart_background, run, WINDOWS_USER_RUN_KEY};
use crate::config::Config;
use anyhow::{bail, Context, Result};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;

/// Starts this executable again with `args`, detached from the console.
pub fn spawn_detached(args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>) -> std::io::Result<()> {
    Command::new(super::program())
        .args(args)
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()?;
    Ok(())
}

/// The command lines that start the client and its tray when the user logs in.
fn user_commands(exe: &Path, config: &Path) -> (String, String) {
    let base = format!("\"{}\" --config \"{}\"", exe.display(), config.display());
    (format!("{base} run --log-file --hidden"), format!("{base} tray --hidden"))
}

/// The client of the user who runs this. It starts when they log in, needs no administrator, and keeps its config, token
/// and state in their profile.
pub fn install_service(config: &Path, exe: &Path, dry: bool) -> Result<()> {
    if !dry && !config.exists() {
        bail!("there is no config yet: run `fafnirx install --server <address>` first");
    }
    let (client, tray) = user_commands(exe, config);
    run(dry, "reg", &["add", WINDOWS_USER_RUN_KEY, "/v", "fafnirx", "/t", "REG_SZ", "/d", &client, "/f"])?;
    if cfg!(feature = "tray") {
        run(dry, "reg", &["add", WINDOWS_USER_RUN_KEY, "/v", "fafnirx-tray", "/t", "REG_SZ", "/d", &tray, "/f"])?;
    }
    if !dry {
        let state = Config::load(config)?.state_dir(config);
        restart_background(config, &state)?;
        #[cfg(feature = "tray")]
        {
            // One that already runs for this config keeps its lock, and this one ends.
            let _ = Command::new(exe).arg("--config").arg(config).args(["tray", "--hidden"]).spawn();
        }
    }
    println!("the client runs in the background now and starts when you log in");
    Ok(())
}

/// Removes the entries that start the client and the tray at login.
pub fn uninstall(config_path: &Path, dry: bool) -> Result<()> {
    for name in ["fafnirx", "fafnirx-tray"] {
        let _ = run(dry, "reg", &["delete", WINDOWS_USER_RUN_KEY, "/v", name, "/f"]);
    }
    if !dry {
        if let Ok(cfg) = Config::load(config_path) {
            let _ = std::fs::write(crate::daemon::stop_marker(&cfg.state_dir(config_path)), b"");
        }
    }
    println!("A tray that is running stays until it is closed.");
    Ok(())
}

/// Windows has no permissions to set: the folders of a user are theirs.
pub fn set_dir_modes(_config_dir: &Path, _data: &Path, _state: &Path, _control: &Path) -> Result<()> {
    Ok(())
}

/// The client of a Windows user is no service of the computer.
pub fn installed() -> bool {
    false
}

#[cfg(feature = "tray")]
pub fn start() -> Result<()> {
    bail!("there is no service of the computer on Windows: the client starts when you log in")
}

pub fn restart() -> Result<()> {
    bail!("there is no service of the computer on Windows: use `fafnirx restart`")
}

/// No service manager starts a client that ends with an error here, so it starts itself.
pub fn exit_to_be_restarted() -> ! {
    let _ = spawn_detached(std::env::args_os().skip(1));
    std::process::exit(0)
}

/// Removes the program and its folder once this process has ended: a running program cannot delete its own file.
pub fn remove_program(program: &Path) -> Result<()> {
    let dir = program.parent().map(|d| format!(" & rmdir \"{}\"", d.display())).unwrap_or_default();
    let line = format!("ping -n 3 127.0.0.1 >nul & del /f /q \"{}\"{dir}", program.display());
    Command::new("cmd")
        .args(["/c", &line])
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .context("remove the program")?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/service/windows.rs"]
mod tests;
