//! Things the tray starts: editors, file managers, the browser, the client itself.

use super::text::clip;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// Runs a helper without flashing a console window on Windows.
fn spawn_quiet(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let _ = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
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

pub fn start_client(config_path: &Path) {
    if let Ok(exe) = std::env::current_exe() {
        let mut c = Command::new(exe);
        c.arg("--config").arg(config_path).args(["run", "--log-file"]);
        if cfg!(windows) {
            c.arg("--hidden"); // the client detaches itself from the console
        }
        spawn_quiet(&mut c);
    }
}

/// Runs `fafnirx update` in the background and keeps its one-line result in `result`.
pub fn check_for_updates(config_path: PathBuf, result: Arc<Mutex<String>>) {
    let say = move |text: String| {
        if let Ok(mut r) = result.lock() {
            *r = clip(&text);
        }
    };
    say("Updates: checking...".into());
    std::thread::spawn(move || {
        let run = std::env::current_exe()
            .and_then(|exe| Command::new(exe).arg("--config").arg(&config_path).arg("update").output());
        say(match run {
            Ok(o) => {
                let out = String::from_utf8_lossy(if o.status.success() { &o.stdout } else { &o.stderr }).to_string();
                let line = out.lines().find(|l| !l.trim().is_empty()).unwrap_or("no answer").trim().to_string();
                if o.status.success() {
                    format!("Updates: {line}")
                } else {
                    format!("Updates: failed: {line}")
                }
            }
            Err(e) => format!("Updates: failed: {e}"),
        });
    });
}
