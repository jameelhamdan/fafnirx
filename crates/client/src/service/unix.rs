//! The service on Linux (a systemd unit) and macOS (a LaunchDaemon), both started at boot, with the tray of each user started at
//! their login.

use super::{disk_access, elevate, run};
use crate::config::system_config_path;
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

const SYSTEMD_UNIT: &str = "/etc/systemd/system/fafnirx.service";
const LINUX_TRAY: &str = "/etc/xdg/autostart/fafnirx-tray.desktop";
const MAC_DAEMON: &str = "/Library/LaunchDaemons/io.fafnirx.client.plist";
const MAC_TRAY: &str = "/Library/LaunchAgents/io.fafnirx.tray.plist";
const MAC_TARGET: &str = "system/io.fafnirx.client";

/// The systemd unit of the client. It is restarted when it exits with an error (a crash, or `daemon::restart` after an
/// update), and left alone when it exits cleanly: that is the "stop".
pub fn systemd_unit(exe: &Path, config: &Path) -> String {
    format!(
        "[Unit]\nDescription=fafnirx client\nAfter=network-online.target\nWants=network-online.target\n\n\
         [Service]\nExecStart=\"{}\" --config \"{}\" run --log-file\nRestart=on-failure\nRestartSec=10\n\n\
         [Install]\nWantedBy=multi-user.target\n",
        exe.display(),
        config.display()
    )
}

/// A launchd job that runs `fafnirx --config <config> <command...>`, and restarts it when it exits with an error if
/// `restart_on_failure` (a clean exit is the "stop").
fn launchd_plist(label: &str, exe: &Path, config: &Path, command: &[&str], restart_on_failure: bool) -> String {
    let command: String = command.iter().map(|word| format!("<string>{word}</string>")).collect();
    let keep_alive = if restart_on_failure { "<dict><key>SuccessfulExit</key><false/></dict>" } else { "<false/>" };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\"><dict>\n\
         <key>Label</key><string>{label}</string>\n\
         <key>ProgramArguments</key><array><string>{}</string><string>--config</string><string>{}</string>{command}</array>\n\
         <key>RunAtLoad</key><true/>\n<key>KeepAlive</key>{keep_alive}\n\
         </dict></plist>\n",
        exe.display(),
        config.display()
    )
}

/// The LaunchDaemon of the client itself.
pub fn client_plist(exe: &Path, config: &Path) -> String {
    launchd_plist("io.fafnirx.client", exe, config, &["run", "--log-file"], true)
}

/// The LaunchAgent of the tray icon, which is not restarted when the user quits it. It belongs in the graphical session only (a
/// menu-bar icon has nowhere to go in an SSH login), and what it prints is kept for when it does not appear.
pub fn tray_plist(exe: &Path, config: &Path) -> String {
    let extra = "<key>LimitLoadToSessionType</key><string>Aqua</string>\n<key>ProcessType</key><string>Interactive</string>\n\
                 <key>StandardErrorPath</key><string>/tmp/fafnirx-tray.log</string>\n</dict></plist>\n";
    launchd_plist("io.fafnirx.tray", exe, config, &["tray"], false).replace("</dict></plist>\n", extra)
}

pub fn tray_desktop_entry(exe: &Path, config: &Path) -> String {
    format!(
        "[Desktop Entry]
Type=Application
Name=fafnirx tray
Exec=\"{}\" --config \"{}\" tray
X-GNOME-Autostart-enabled=true
",
        exe.display(),
        config.display()
    )
}

fn write(dry: bool, path: &Path, content: &str) -> Result<()> {
    println!("write {}", path.display());
    if dry {
        println!("{content}");
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

/// The service runs with the rights to read every user's folders, so the program file it starts must not be one a user can
/// replace: owned by root and not writable by others, in a folder that is too. `FAFNIRX_ALLOW_UNPROTECTED_EXE` lifts this for a build on the machine it was made on.
fn check_exe_is_protected(exe: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    if std::env::var_os("FAFNIRX_ALLOW_UNPROTECTED_EXE").is_some() {
        return Ok(());
    }
    for path in [Some(exe), exe.parent()].into_iter().flatten() {
        let meta = std::fs::metadata(path)?;
        if meta.uid() != 0 || meta.mode() & 0o022 != 0 {
            bail!(
                "{} is not owned by root or can be changed by others: the service runs as root, so its program must be \
                 in a place that users cannot change. Put it in /usr/local/bin with `sudo install`",
                path.display()
            );
        }
    }
    Ok(())
}

/// Sets who may do what in the folders of the system-wide install (see `service::prepare_dirs`).
pub fn set_dir_modes(config_dir: &Path, data: &Path, state: &Path, control: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = |dir: &Path, mode: u32| std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode));
    // Users pass through the folder of the data to reach the control folder, which anyone may write in but not remove
    // another's files from; the state is the service's.
    mode(config_dir, 0o755)?;
    mode(data, 0o755)?;
    mode(state, 0o700)?;
    mode(control, 0o1777)?;
    Ok(())
}

/// Whether the service is installed on this device.
pub fn installed() -> bool {
    Path::new(if cfg!(target_os = "macos") { MAC_DAEMON } else { SYSTEMD_UNIT }).exists()
}

#[cfg(feature = "tray")]
pub fn start() -> Result<()> {
    if cfg!(target_os = "macos") {
        elevate::run("launchctl", &["load", "-w", MAC_DAEMON])?;
        elevate::run("launchctl", &["kickstart", MAC_TARGET])
    } else {
        elevate::run("systemctl", &["start", "fafnirx.service"])
    }
}

/// Stops the service and starts it again, which runs the program file as it is now.
pub fn restart() -> Result<()> {
    if cfg!(target_os = "macos") {
        elevate::run("launchctl", &["kickstart", "-k", MAC_TARGET])
    } else {
        elevate::run("systemctl", &["restart", "fafnirx.service"])
    }
}

/// The user at the console of a Mac, if one is logged in: the tray is started for them.
fn console_user() -> Option<String> {
    let out = Command::new("stat").args(["-f", "%u", "/dev/console"]).output().ok()?;
    let uid = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!uid.is_empty() && uid != "0").then_some(uid)
}

/// Makes sure the tray starts when a user logs in: writes its LaunchAgent (macOS) or autostart entry (Linux) when it is missing or
/// belongs to another program file, and on a Mac loads it for the user at the console. An update replaces only the program file,
/// so a device that got here from a version without the entry would otherwise never have a tray. Only the service (root) does
/// this. Best effort.
pub fn ensure_login_start(config: &Path) {
    if !cfg!(feature = "tray") || !super::elevate::is_admin() || !crate::config::is_system_config(config) {
        return;
    }
    let exe = fafnirx_common::plain_path(&super::program());
    let (path, content) = if cfg!(target_os = "macos") {
        (MAC_TRAY, tray_plist(&exe, config))
    } else {
        (LINUX_TRAY, tray_desktop_entry(&exe, config))
    };
    if std::fs::read_to_string(path).is_ok_and(|current| current == content) {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(Path::new(path).parent().unwrap_or(Path::new("/"))).and_then(|()| std::fs::write(path, content)) {
        tracing::warn!("could not make the tray start when you log in ({path}): {e}");
        return;
    }
    if cfg!(target_os = "macos") {
        if let Some(uid) = console_user() {
            let _ = Command::new("launchctl").args(["asuser", &uid, "launchctl", "load", "-w", MAC_TRAY]).status();
        }
    }
}

/// Installs the service and the tray of every user, and starts the service. Needs an administrator; the config of the device
/// (`fafnirx install --server <address>`) comes first, because the service starts with it.
pub fn install_service(_config_path: &Path, exe: &Path, dry: bool) -> Result<()> {
    let config = system_config_path();
    if !dry {
        if !config.exists() {
            bail!("there is no config for the device yet: run `fafnirx install --server <address>` first");
        }
        check_exe_is_protected(exe)?;
        super::prepare_dirs(&config)?;
    }
    if cfg!(target_os = "linux") {
        write(dry, Path::new(SYSTEMD_UNIT), &systemd_unit(exe, &config))?;
        run(dry, "systemctl", &["daemon-reload"])?;
        run(dry, "systemctl", &["enable", "--now", "fafnirx.service"])?;
        if cfg!(feature = "tray") {
            write(dry, Path::new(LINUX_TRAY), &tray_desktop_entry(exe, &config))?;
        }
    } else if cfg!(target_os = "macos") {
        write(dry, Path::new(MAC_DAEMON), &client_plist(exe, &config))?;
        run(dry, "launchctl", &["load", "-w", MAC_DAEMON])?;
        if cfg!(feature = "tray") {
            write(dry, Path::new(MAC_TRAY), &tray_plist(exe, &config))?;
            if let Some(uid) = console_user() {
                // Best effort: otherwise the tray starts the next time they log in.
                let _ = run(dry, "launchctl", &["asuser", &uid, "launchctl", "load", "-w", MAC_TRAY]);
            }
        }
        println!(
            "macOS only lets the client change files in Documents, Desktop, Downloads, iCloud Drive and on other volumes with Full Disk Access.\nTo sync folders there: {}.",
            disk_access::STEPS
        );
    } else {
        bail!("service installation is not supported on this OS");
    }
    Ok(())
}

/// Removes the service and the trays. What was backed up, the config and the state stay.
pub fn uninstall(_config_path: &Path, dry: bool) -> Result<()> {
    if cfg!(target_os = "linux") {
        let _ = run(dry, "systemctl", &["disable", "--now", "fafnirx.service"]);
        println!("remove {SYSTEMD_UNIT}");
        if !dry {
            let _ = std::fs::remove_file(SYSTEMD_UNIT);
            let _ = std::fs::remove_file(LINUX_TRAY);
        }
        let _ = run(dry, "systemctl", &["daemon-reload"]);
    } else if cfg!(target_os = "macos") {
        let _ = run(dry, "launchctl", &["unload", "-w", MAC_DAEMON]);
        println!("remove {MAC_DAEMON}");
        if let Some(uid) = console_user() {
            let _ = run(dry, "launchctl", &["asuser", &uid, "launchctl", "unload", "-w", MAC_TRAY]);
        }
        if !dry {
            let _ = std::fs::remove_file(MAC_DAEMON);
            let _ = std::fs::remove_file(MAC_TRAY);
        }
    } else {
        bail!("service installation is not supported on this OS");
    }
    Ok(())
}

/// Ends the client to be started again, on a new version or with a new enrollment: the service manager starts it when it
/// ends with an error (systemd and launchd do).
pub fn exit_to_be_restarted() -> ! {
    std::process::exit(75)
}

#[cfg(test)]
#[path = "../../tests/unit/service/unix.rs"]
mod tests;

/// Removes the file of a program that is not running.
pub fn remove_program(program: &Path) -> Result<()> {
    std::fs::remove_file(program).with_context(|| format!("remove {}", program.display()))
}
