//! Things the tray starts: editors, file managers, the browser, the client itself.

use super::text::clip;
use crate::{service, update};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// This program's file as it was when the tray started. After an update the file at that path is the new version, while the
/// operating system may report the running (old) image under another name, so everything the tray starts uses this.
pub fn exe() -> PathBuf {
    static EXE: OnceLock<PathBuf> = OnceLock::new();
    EXE.get_or_init(|| std::env::current_exe().unwrap_or_else(|_| PathBuf::from("fafnirx"))).clone()
}

/// Keeps a helper from flashing a console window on Windows.
fn quiet(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd.stdin(Stdio::null())
}

fn spawn_quiet(cmd: &mut Command) {
    let _ = quiet(cmd).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

/// Opens a file in a text editor, or a folder in the file manager.
pub fn open_path(path: &Path, as_text: bool) {
    if cfg!(windows) {
        spawn_quiet(Command::new(if as_text { "notepad.exe" } else { "explorer.exe" }).arg(path));
    } else if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        if as_text {
            c.arg("-t");
        }
        spawn_quiet(c.arg(path));
    } else {
        spawn_quiet(Command::new("xdg-open").arg(path));
    }
}

pub fn open_url(url: &str) {
    if cfg!(windows) {
        spawn_quiet(Command::new("cmd").args(["/C", "start", ""]).arg(url));
    } else if cfg!(target_os = "macos") {
        spawn_quiet(Command::new("open").arg(url));
    } else {
        spawn_quiet(Command::new("xdg-open").arg(url));
    }
}

/// Starts afnirx <subcommand> (of the program file as it is now) in the background; on Windows it detaches itself from the console.
fn start(config_path: &Path, subcommand: &[&str]) {
    let mut c = Command::new(exe());
    c.arg("--config").arg(config_path).args(subcommand);
    if cfg!(windows) {
        c.arg("--hidden");
    }
    spawn_quiet(&mut c);
}

/// Starts the client: the service when this computer has one, else the program itself in the background.
pub fn start_client(config_path: &Path) {
    if crate::config::is_system_config(config_path) && service::installed() {
        let _ = service::start();
    } else {
        start(config_path, &["run", "--log-file"]);
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// Whether the job installed by `fafnirx install-service` / the install script exists for this label.
fn launch_agent(label: &str) -> bool {
    let file = format!("{label}.plist");
    cfg!(target_os = "macos")
        && (std::path::Path::new("/Library/LaunchAgents").join(&file).is_file()
            || home().join("Library/LaunchAgents").join(file).is_file())
}

/// Starts a launchd job again on the program file as it is now (`kickstart -k` ends it first if it is running). This is how a
/// background program that is not an app bundle updates itself on macOS: replace the file, let launchd start it, so that the job
/// stays the one launchd supervises and starts at login.
fn kickstart(label: &str) -> bool {
    let Ok(uid) = Command::new("id").arg("-u").output() else { return false };
    let target = format!("gui/{}/{label}", String::from_utf8_lossy(&uid.stdout).trim());
    Command::new("launchctl").args(["kickstart", "-k", &target]).status().is_ok_and(|s| s.success())
}

/// Replaces this tray with a new one and tells it to end. On macOS through its launchd job, elsewhere by starting the new
/// one, which waits for this one to let go of its lock.
fn restart_tray(config_path: &Path, updates: &Updates) {
    // The new tray exists before the old one is told to end, or the process could end first.
    if !(launch_agent("io.fafnirx.tray") && kickstart("io.fafnirx.tray")) {
        start(config_path, &["tray"]); // waits for this one to let go of its lock
    }
    updates.restart.store(true, Ordering::SeqCst);
}

/// Runs `fafnirx <args>` and returns what it printed, or the first line it complained with.
pub fn run_cli(config_path: &Path, args: &[std::ffi::OsString]) -> Result<String, String> {
    let out =
        quiet(Command::new(exe()).arg("--config").arg(config_path).args(args)).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let line = err.lines().map(|l| l.trim()).find(|l| !l.is_empty()).unwrap_or("it failed without saying why");
    Err(line.trim_start_matches("Error: ").to_string())
}

/// A check or an install is going on: pressing the menu entry again meanwhile does nothing.
static UPDATING: AtomicBool = AtomicBool::new(false);

/// What the update entries of the menu show and decide on.
#[derive(Clone)]
pub struct Updates {
    /// The one-line state shown in the menu.
    pub text: Arc<Mutex<String>>,
    /// The version found by the last check, if it is newer than this one: the menu then offers to install it.
    pub available: Arc<Mutex<Option<String>>>,
    /// Set when the tray has to end because a new one took its place.
    pub restart: Arc<AtomicBool>,
}

impl Updates {
    pub fn new() -> Self {
        Self {
            text: Arc::new(Mutex::new("Updates: not checked".to_string())),
            available: Arc::new(Mutex::new(None)),
            restart: Arc::new(AtomicBool::new(false)),
        }
    }

    fn say(&self, text: String) {
        if let Ok(mut r) = self.text.lock() {
            *r = clip(&text);
        }
    }

    pub fn available(&self) -> Option<String> {
        self.available.lock().ok().and_then(|a| a.clone())
    }

    fn set_available(&self, version: Option<String>) {
        if let Ok(mut a) = self.available.lock() {
            *a = version;
        }
    }
}

/// Asks, in the background, whether there is a newer version (`fafnirx update --check`, which downloads nothing). The
/// menu then offers to install it.
pub fn check_for_updates(config_path: PathBuf, updates: Updates) {
    if UPDATING.swap(true, Ordering::SeqCst) {
        return;
    }
    updates.say("Updates: checking...".into());
    std::thread::spawn(move || {
        match run_cli(&config_path, &["update".into(), "--check".into()]) {
            Ok(out) => {
                let line = out.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
                match line.strip_prefix("available ") {
                    Some(version) => {
                        updates.set_available(Some(version.trim().to_string()));
                        updates.say(format!("Updates: {} is available", version.trim()));
                    }
                    None => {
                        updates.set_available(None);
                        updates.say(format!("Updates: up to date ({})", update::VERSION));
                    }
                }
            }
            Err(e) => updates.say(format!("Updates: failed: {e}")),
        }
        UPDATING.store(false, Ordering::SeqCst);
    });
}

/// Installs the version the last check found (`fafnirx update`, which also starts the client again on it), and replaces this
/// tray with a new one. Nothing is downloaded by hand and nothing is left to start.
pub fn install_update(config_path: PathBuf, version: String, updates: Updates) {
    if UPDATING.swap(true, Ordering::SeqCst) {
        return;
    }
    updates.say(format!("Updates: installing {version}..."));
    std::thread::spawn(move || {
        if let Err(e) = run_cli(&config_path, &["update".into()]) {
            updates.say(format!("Updates: could not install {version}: {e}"));
            UPDATING.store(false, Ordering::SeqCst);
            return;
        }
        updates.set_available(None);
        updates.say(format!("Updates: installed {version}, restarting..."));
        restart_tray(&config_path, &updates);
    });
}
/// The version of the program file on disk, which is not this tray's when the client updated itself in the background
/// (`auto_update`) while the tray went on running the old one.
fn installed_version() -> Option<String> {
    let out = quiet(Command::new(exe()).arg("--version")).output().ok()?;
    // `fafnirx 3.3.0`
    String::from_utf8_lossy(&out.stdout).split_whitespace().last().map(str::to_string)
}

/// Replaces this tray with a new one when the program file on disk is a newer version than the one running.
pub fn restart_if_updated(config_path: PathBuf, updates: Updates) {
    std::thread::spawn(move || {
        if installed_version().is_some_and(|v| update::newer(&v, update::VERSION))
            && !updates.restart.load(Ordering::SeqCst)
        {
            restart_tray(&config_path, &updates);
        }
    });
}
