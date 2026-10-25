//! Installs the client so it starts when the user logs in / the machine comes up.
//! Linux: systemd user unit. macOS: LaunchAgent. Windows: per-user Run registry key.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const WINDOWS_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// The systemd unit of the client. It is restarted when it exits with an error (a crash, or `daemon::restart` after an
/// update), and left alone when it exits cleanly: that is the tray's "stop".
pub fn systemd_unit(exe: &Path, config: &Path) -> String {
    format!(
        "[Unit]\nDescription=fafnirx client\nAfter=network-online.target\nWants=network-online.target\n\n\
         [Service]\nExecStart=\"{}\" --config \"{}\" run --log-file\nRestart=on-failure\nRestartSec=10\n\n\
         [Install]\nWantedBy=default.target\n",
        exe.display(),
        config.display()
    )
}

/// A LaunchAgent that runs `fafnirx --config <config> <command...>` at login, and restarts it when it exits with an error
/// if `restart_on_failure` (a clean exit is the tray's "stop").
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

/// The LaunchAgent of the client itself.
pub fn client_plist(exe: &Path, config: &Path) -> String {
    launchd_plist("io.fafnirx.client", exe, config, &["run", "--log-file"], true)
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

/// The LaunchAgent of the tray icon, which is not restarted when the user quits it.
pub fn tray_plist(exe: &Path, config: &Path) -> String {
    launchd_plist("io.fafnirx.tray", exe, config, &["tray"], false)
}

/// Windows: starts this executable again with `args`, detached from the console.
#[cfg(windows)]
pub fn spawn_detached(args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    Command::new(std::env::current_exe()?).args(args).creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS).spawn()?;
    Ok(())
}

fn run(dry: bool, cmd: &str, args: &[&str]) -> Result<()> {
    println!("$ {cmd} {}", args.join(" "));
    if dry {
        return Ok(());
    }
    let status = Command::new(cmd).args(args).status().with_context(|| format!("run {cmd}"))?;
    if !status.success() {
        bail!("{cmd} failed with {status}");
    }
    Ok(())
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

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

pub fn install(config: &Path, dry: bool) -> Result<()> {
    let exe = std::env::current_exe()?;
    let config = std::fs::canonicalize(config).unwrap_or_else(|_| config.to_path_buf());
    // Windows: drop the verbatim prefix, so this is the same path (and state directory) as typing it by hand.
    let config = fafnirx_common::plain_path(&config);
    if cfg!(target_os = "linux") {
        let unit = home().join(".config/systemd/user/fafnirx.service");
        write(dry, &unit, &systemd_unit(&exe, &config))?;
        run(dry, "systemctl", &["--user", "daemon-reload"])?;
        run(dry, "systemctl", &["--user", "enable", "--now", "fafnirx.service"])?;
        println!("To keep it running without an open login session: loginctl enable-linger $USER");
        if cfg!(feature = "tray") {
            write(dry, &home().join(".config/autostart/fafnirx-tray.desktop"), &tray_desktop_entry(&exe, &config))?;
        }
    } else if cfg!(target_os = "macos") {
        let plist = home().join("Library/LaunchAgents/io.fafnirx.client.plist");
        write(dry, &plist, &client_plist(&exe, &config))?;
        run(dry, "launchctl", &["load", "-w", &plist.to_string_lossy()])?;
        if cfg!(feature = "tray") {
            let tray = home().join("Library/LaunchAgents/io.fafnirx.tray.plist");
            write(dry, &tray, &tray_plist(&exe, &config))?;
            run(dry, "launchctl", &["load", "-w", &tray.to_string_lossy()])?;
        }
    } else if cfg!(windows) {
        // The per-user Run key starts the client at logon without administrator rights.
        let command = format!("\"{}\" --config \"{}\" run --log-file --hidden", exe.display(), config.display());
        run(dry, "reg", &["add", WINDOWS_RUN_KEY, "/v", "fafnirx", "/t", "REG_SZ", "/d", &command, "/f"])?;
        let tray = format!("\"{}\" --config \"{}\" tray --hidden", exe.display(), config.display());
        if cfg!(feature = "tray") {
            run(dry, "reg", &["add", WINDOWS_RUN_KEY, "/v", "fafnirx-tray", "/t", "REG_SZ", "/d", &tray, "/f"])?;
        }
        if !dry {
            // `--hidden` detaches it from this console.
            Command::new(&exe).arg("--config").arg(&config).args(["run", "--log-file", "--hidden"]).status()?;
            if cfg!(feature = "tray") {
                Command::new(&exe).arg("--config").arg(&config).args(["tray", "--hidden"]).status()?;
            }
        }
    } else {
        bail!("service installation is not supported on this OS");
    }
    Ok(())
}

pub fn uninstall(dry: bool) -> Result<()> {
    if cfg!(target_os = "linux") {
        let _ = run(dry, "systemctl", &["--user", "disable", "--now", "fafnirx.service"]);
        let unit = home().join(".config/systemd/user/fafnirx.service");
        println!("remove {}", unit.display());
        if !dry {
            let _ = std::fs::remove_file(unit);
            let _ = std::fs::remove_file(home().join(".config/autostart/fafnirx-tray.desktop"));
        }
        let _ = run(dry, "systemctl", &["--user", "daemon-reload"]);
    } else if cfg!(target_os = "macos") {
        let plist = home().join("Library/LaunchAgents/io.fafnirx.client.plist");
        let _ = run(dry, "launchctl", &["unload", "-w", &plist.to_string_lossy()]);
        println!("remove {}", plist.display());
        let tray = home().join("Library/LaunchAgents/io.fafnirx.tray.plist");
        let _ = run(dry, "launchctl", &["unload", "-w", &tray.to_string_lossy()]);
        if !dry {
            let _ = std::fs::remove_file(plist);
            let _ = std::fs::remove_file(tray);
        }
    } else if cfg!(windows) {
        run(dry, "reg", &["delete", WINDOWS_RUN_KEY, "/v", "fafnirx", "/f"])?;
        let _ = run(dry, "reg", &["delete", WINDOWS_RUN_KEY, "/v", "fafnirx-tray", "/f"]);
        println!("A running client keeps running until you stop it: taskkill /IM fafnirx.exe");
    } else {
        bail!("service installation is not supported on this OS");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_files_reference_the_binary_and_config() {
        let exe = Path::new("/usr/local/bin/fafnirx");
        let cfg = Path::new("/home/u/.config/fafnirx/config.toml");
        let unit = systemd_unit(exe, cfg);
        assert!(unit.contains(
            "ExecStart=\"/usr/local/bin/fafnirx\" --config \"/home/u/.config/fafnirx/config.toml\" run --log-file"
        ));
        assert!(unit.contains("Restart=on-failure"));
        let plist = client_plist(exe, cfg);
        assert!(
            plist.contains("<string>/usr/local/bin/fafnirx</string>")
                && plist.contains("<key>SuccessfulExit</key><false/>")
        );
        let tray = tray_plist(exe, cfg);
        assert!(
            tray.contains("io.fafnirx.tray")
                && tray.contains("<string>tray</string>")
                && !tray.contains("<string>run</string>")
                && tray.contains("<key>KeepAlive</key><false/>")
        );
        assert!(
            tray_desktop_entry(exe, cfg).contains("fafnirx\" --config \"/home/u/.config/fafnirx/config.toml\" tray")
        );
    }
}
