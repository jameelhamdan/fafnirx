//! The client as a service of the computer, for every user: it starts at boot, is started again when it fails, and is managed
//! by the operating system like any other program. Windows: a service (`services.msc`, Task Manager) of the system account.
//! Linux: a systemd unit. macOS: a LaunchDaemon. The tray of each user is started when they log in.
//!
//! Its config, token and state are in the folders of the computer (`config::system_data_dir`), not in a profile, and it runs
//! with the rights to read the folders of every user, so only an administrator may change them (`prepare_dirs`).

pub mod disk_access;
pub mod elevate;
pub mod instance;
#[cfg(windows)]
pub mod windows;

use crate::config::{
    control_dir, is_system_config, same_path, system_config_path, system_data_dir, system_state_dir, Config,
};
use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

const SYSTEMD_UNIT: &str = "/etc/systemd/system/fafnirx.service";
const LINUX_TRAY: &str = "/etc/xdg/autostart/fafnirx-tray.desktop";
const MAC_DAEMON: &str = "/Library/LaunchDaemons/io.fafnirx.client.plist";
const MAC_TRAY: &str = "/Library/LaunchAgents/io.fafnirx.tray.plist";
#[cfg(not(windows))]
const MAC_TARGET: &str = "system/io.fafnirx.client";
/// Started for every user who logs in (a service of the computer, installed by an older version).
const WINDOWS_RUN_KEY: &str = r"HKLM\Software\Microsoft\Windows\CurrentVersion\Run";
/// Started when the user who installed the client logs in.
const WINDOWS_USER_RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

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

/// The LaunchAgent of the tray icon, which is not restarted when the user quits it.
pub fn tray_plist(exe: &Path, config: &Path) -> String {
    launchd_plist("io.fafnirx.tray", exe, config, &["tray"], false)
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

/// Windows: starts this executable again with `args`, detached from the console.
#[cfg(windows)]
pub fn spawn_detached(args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    Command::new(std::env::current_exe()?).args(args).creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS).spawn()?;
    Ok(())
}

/// Runs a command whose output nobody needs, and says what it printed when it fails.
#[cfg(windows)]
pub fn run_quiet(cmd: &str, args: &[&str]) -> Result<()> {
    let out = Command::new(cmd).args(args).output().with_context(|| format!("run {cmd}"))?;
    if !out.status.success() {
        bail!(
            "{cmd} {} failed with {}: {}{}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stdout).trim(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Runs a command and shows it, or only shows it for a dry run.
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

// --- who may do what -----------------------------------------------------------------------------------------------------------

/// Whether `path` is below `root` (a path on Windows ignores case).
#[cfg(windows)]
fn is_under(path: &Path, root: &Path) -> bool {
    let (path, root) = (path.to_string_lossy().replace('/', "\\"), root.to_string_lossy().replace('/', "\\"));
    let root = root.trim_end_matches('\\');
    !root.is_empty()
        && path.len() > root.len()
        && path[..root.len()].eq_ignore_ascii_case(root)
        && path[root.len()..].starts_with('\\')
}

/// The service runs with the rights to read every user's folders, so the program file it starts must not be one a user can
/// replace. Windows: under Program Files (where the setup puts it). Linux and macOS: owned by root and not writable by others,
/// in a folder that is too. `FAFNIRX_ALLOW_UNPROTECTED_EXE` lifts this for a build on the machine it was made on.
fn check_exe_is_protected(exe: &Path) -> Result<()> {
    if std::env::var_os("FAFNIRX_ALLOW_UNPROTECTED_EXE").is_some() {
        return Ok(());
    }
    #[cfg(windows)]
    {
        let roots = ["ProgramFiles", "ProgramFiles(x86)", "SystemRoot"]
            .map(|v| std::env::var_os(v).map(std::path::PathBuf::from));
        if !roots.iter().flatten().any(|root| is_under(exe, root)) {
            bail!(
                "{} is not under Program Files: the service runs as the system, so its program must be in a place that users \
                 cannot change. Install it with the setup, or put it there",
                exe.display()
            );
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
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
    }
    Ok(())
}

#[cfg(windows)]
fn icacls(path: &Path, grants: &[&str]) -> Result<()> {
    let path = path.to_string_lossy();
    let mut args = vec![path.as_ref(), "/inheritance:r", "/grant:r"];
    args.extend_from_slice(grants);
    run_quiet("icacls", &args)
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
    #[cfg(windows)]
    {
        const SYSTEM: &str = "*S-1-5-18:(OI)(CI)F";
        const ADMINISTRATORS: &str = "*S-1-5-32-544:(OI)(CI)F";
        icacls(&data, &[SYSTEM, ADMINISTRATORS, "*S-1-5-32-545:(OI)(CI)RX"])?;
        icacls(&state, &[SYSTEM, ADMINISTRATORS])?;
        icacls(&control, &[SYSTEM, ADMINISTRATORS, "*S-1-5-32-545:(OI)(CI)M"])?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |dir: &Path, mode: u32| std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode));
        // Users pass through the folder of the data to reach the control folder, which anyone may write in but not remove
        // another's files from; the state is the service's.
        mode(&config_dir, 0o755)?;
        mode(&data, 0o755)?;
        mode(&state, 0o700)?;
        mode(&control, 0o1777)?;
    }
    Ok(())
}

/// Makes a file of the system-wide install (the token) readable only by the service and the administrators. On Linux and macOS
/// the mode does that when the file is made; Windows needs the access list changed because the config folder is readable.
pub fn keep_private(path: &Path) -> Result<()> {
    #[cfg(windows)]
    if path.parent().is_some_and(|dir| system_config_path().parent().is_some_and(|c| same_path(dir, c))) {
        icacls(path, &["*S-1-5-18:F", "*S-1-5-32-544:F"])?;
    }
    #[cfg(not(windows))]
    let _ = (path, same_path as fn(&Path, &Path) -> bool);
    Ok(())
}

// --- installing and asking ---------------------------------------------------------------------------------------------------

/// Whether the service is installed on this computer.
#[cfg(windows)]
pub fn installed() -> bool {
    windows::installed()
}
#[cfg(not(windows))]
pub fn installed() -> bool {
    Path::new(if cfg!(target_os = "macos") { MAC_DAEMON } else { SYSTEMD_UNIT }).exists()
}

#[cfg(windows)]
pub fn start() -> Result<()> {
    windows::start()
}
#[cfg(not(windows))]
pub fn start() -> Result<()> {
    if cfg!(target_os = "macos") {
        elevate::run("launchctl", &["load", "-w", MAC_DAEMON])?;
        elevate::run("launchctl", &["kickstart", MAC_TARGET])
    } else {
        elevate::run("systemctl", &["start", "fafnirx.service"])
    }
}

/// Stops the service and starts it again, which runs the program file as it is now.
#[cfg(windows)]
pub fn restart() -> Result<()> {
    windows::restart()
}
#[cfg(not(windows))]
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

/// The command lines that start the client and its tray when the user logs in (Windows, a client of one user).
fn windows_user_commands(exe: &Path, config: &Path) -> (String, String) {
    let base = format!("\"{}\" --config \"{}\"", exe.display(), config.display());
    (format!("{base} run --log-file --hidden"), format!("{base} tray --hidden"))
}

/// Windows: the client of the user who runs this. It starts when they log in, needs no administrator, and keeps its config, token
/// and state in their profile. A service of the computer is for the config of the computer (`install`).
fn install_for_user(dry: bool, exe: &Path, config: &Path) -> Result<()> {
    if !dry && !config.exists() {
        bail!("there is no config yet: run `fafnirx enroll --server <url>` first");
    }
    let (client, tray) = windows_user_commands(exe, config);
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

/// Installs the service and the tray of every user, and starts the service; then removes the older versions of the client that
/// are left on the computer, so that only this one remains.
pub fn install(config_path: &Path, dry: bool) -> Result<()> {
    install_service(config_path, dry)?;
    if !dry {
        if let Ok(exe) = std::env::current_exe() {
            crate::update::purge::announce(&fafnirx_common::plain_path(&exe), true);
        }
    }
    Ok(())
}

/// Installs the service and the tray of every user, and starts the service. Needs an administrator; the config of the computer
/// (`fafnirx enroll`) comes first, because the service starts with it. On Windows the client of a user is installed instead,
/// unless the computer has the config of an older version's service.
fn install_service(config_path: &Path, dry: bool) -> Result<()> {
    let exe = std::env::current_exe()?;
    let exe = fafnirx_common::plain_path(&exe);
    if cfg!(windows) && !is_system_config(config_path) {
        return install_for_user(dry, &exe, config_path);
    }
    let config = system_config_path();
    if !dry {
        if !config.exists() {
            bail!("there is no config for the computer yet: run `fafnirx enroll --server <url>` first");
        }
        check_exe_is_protected(&exe)?;
        prepare_dirs(&config)?;
    }
    if cfg!(windows) {
        #[cfg(windows)]
        if !dry {
            windows::install(&exe, &config)?;
        }
        println!("the service fafnirx is installed and starts at boot");
        if cfg!(feature = "tray") {
            let tray = format!("\"{}\" --config \"{}\" tray --hidden", exe.display(), config.display());
            run(dry, "reg", &["add", WINDOWS_RUN_KEY, "/v", "fafnirx-tray", "/t", "REG_SZ", "/d", &tray, "/f"])?;
            // Best effort, as on macOS: without the user's rights (this runs as an administrator), or the tray starts the next time
            // they log in. One that already runs for the computer's config keeps its lock, and this one ends.
            let _ = run(dry, "runas", &["/trustlevel:0x20000", &tray]);
        }
    } else if cfg!(target_os = "linux") {
        write(dry, Path::new(SYSTEMD_UNIT), &systemd_unit(&exe, &config))?;
        run(dry, "systemctl", &["daemon-reload"])?;
        run(dry, "systemctl", &["enable", "--now", "fafnirx.service"])?;
        if cfg!(feature = "tray") {
            write(dry, Path::new(LINUX_TRAY), &tray_desktop_entry(&exe, &config))?;
        }
    } else if cfg!(target_os = "macos") {
        write(dry, Path::new(MAC_DAEMON), &client_plist(&exe, &config))?;
        run(dry, "launchctl", &["load", "-w", MAC_DAEMON])?;
        if cfg!(feature = "tray") {
            write(dry, Path::new(MAC_TRAY), &tray_plist(&exe, &config))?;
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
pub fn uninstall(config_path: &Path, dry: bool) -> Result<()> {
    if cfg!(windows) && !is_system_config(config_path) {
        for name in ["fafnirx", "fafnirx-tray"] {
            let _ = run(dry, "reg", &["delete", WINDOWS_USER_RUN_KEY, "/v", name, "/f"]);
        }
        if !dry {
            if let Ok(cfg) = Config::load(config_path) {
                let _ = std::fs::write(crate::daemon::stop_marker(&cfg.state_dir(config_path)), b"");
            }
        }
        println!("A tray that is running stays until it is closed.");
    } else if cfg!(windows) {
        #[cfg(windows)]
        if !dry {
            windows::uninstall()?;
        }
        let _ = run(dry, "reg", &["delete", WINDOWS_RUN_KEY, "/v", "fafnirx-tray", "/f"]);
        println!("A tray that is running stays until it is closed.");
    } else if cfg!(target_os = "linux") {
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

// --- a client that runs by itself -----------------------------------------------------------------------------------------------

/// Starts the client in the background without a service (an install of one user, or a build run by hand).
pub fn start_background(config_path: &Path) -> Result<()> {
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("--config").arg(config_path).args(["run", "--log-file"]);
    if cfg!(windows) {
        command.arg("--hidden");
    }
    command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
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

// --- ending the process --------------------------------------------------------------------------------------------------------

/// Ends the client the way it was asked to: it stays stopped (the "stop" of the tray).
pub fn exit_clean() -> ! {
    #[cfg(windows)]
    if windows::in_service() {
        windows::exit_clean();
    }
    std::process::exit(0)
}

/// Ends the client to be started again, on a new version or with a new enrollment: the service manager starts it when it
/// ends with an error (systemd, launchd and the Windows service do). Run by hand on Windows nothing does, so it starts itself.
pub fn exit_to_be_restarted() -> ! {
    #[cfg(windows)]
    {
        if windows::in_service() {
            windows::exit_for_restart();
        }
        let _ = spawn_detached(std::env::args_os().skip(1));
        std::process::exit(0)
    }
    #[cfg(not(windows))]
    std::process::exit(75)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_files_reference_the_binary_and_config() {
        let exe = Path::new("/usr/local/bin/fafnirx");
        let cfg = Path::new("/etc/fafnirx/config.toml");
        let unit = systemd_unit(exe, cfg);
        assert!(
            unit.contains("ExecStart=\"/usr/local/bin/fafnirx\" --config \"/etc/fafnirx/config.toml\" run --log-file")
        );
        assert!(
            unit.contains("Restart=on-failure") && unit.contains("WantedBy=multi-user.target"),
            "a system unit, not a user's"
        );
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
        assert!(tray_desktop_entry(exe, cfg).contains("fafnirx\" --config \"/etc/fafnirx/config.toml\" tray"));
    }

    #[test]
    fn the_client_of_a_user_starts_with_their_config_when_they_log_in() {
        let (exe, cfg) = (Path::new("/apps/fafnirx"), Path::new("/me/config.toml"));
        let (client, tray) = windows_user_commands(exe, cfg);
        assert_eq!(client, "\"/apps/fafnirx\" --config \"/me/config.toml\" run --log-file --hidden");
        assert_eq!(tray, "\"/apps/fafnirx\" --config \"/me/config.toml\" tray --hidden");
    }

    #[test]
    fn installing_for_a_user_needs_their_config_and_a_dry_run_changes_nothing() {
        let (exe, cfg) = (Path::new("fafnirx"), Path::new("no-such-dir/config.toml"));
        assert!(install_for_user(false, exe, cfg).is_err(), "enroll comes first");
        install_for_user(true, exe, cfg).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn a_program_is_protected_when_it_is_below_a_folder_of_the_system() {
        let root = Path::new(r"C:\Program Files");
        assert!(is_under(Path::new(r"C:\Program Files\fafnirx\fafnirx.exe"), root));
        assert!(is_under(Path::new(r"c:\program files\fafnirx\fafnirx.exe"), root), "ignoring case");
        assert!(
            !is_under(Path::new(r"C:\Program Files (x86)\fafnirx.exe"), root),
            "another folder with the same start"
        );
        assert!(!is_under(Path::new(r"C:\Users\me\fafnirx.exe"), root));
        assert!(!is_under(Path::new(r"C:\Program Files"), root), "the folder itself is not below it");
        assert!(!is_under(Path::new(r"C:\x"), Path::new("")), "no root is no match");
    }
}
