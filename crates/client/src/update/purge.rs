//! Only one version of the client may live on a computer. A new install or update can leave an older program behind: in the place
//! an earlier setup used, in `/usr/bin` when the new one went to `/usr/local/bin`, or in a folder of the `PATH` that comes before
//! the new one, so that typing `fafnirx` runs the old client. After an install, the programs found are asked their version, and
//! those older than the one running are stopped and removed.
//!
//! What is looked at is limited to the places a setup puts the program and the folders of the `PATH`; only a program that
//! answers `--version` as `fafnirx <version>` counts, and only an older one is removed (a newer or equal one may be somebody's
//! own build, and is left alone).

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PROGRAM: &str = if cfg!(windows) { "fafnirx.exe" } else { "fafnirx" };

/// How long a program gets to say its version before it is taken for something else.
const ASK_TIMEOUT: Duration = Duration::from_secs(5);

/// Where setups put the program: the folders of every install this client has had, besides the one in use.
fn install_dirs() -> Vec<PathBuf> {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let mut dirs = Vec::new();
    if cfg!(windows) {
        for (name, below) in [
            ("LOCALAPPDATA", &["Programs", "fafnirx"][..]),
            ("LOCALAPPDATA", &["fafnirx"][..]),
            ("ProgramFiles", &["fafnirx"][..]),
            ("ProgramFiles(x86)", &["fafnirx"][..]),
        ] {
            dirs.extend(var(name).map(|root| below.iter().fold(root, |p, part| p.join(part))));
        }
    } else {
        dirs.extend(["/usr/local/bin", "/usr/bin", "/opt/fafnirx", "/opt/fafnirx/bin"].map(PathBuf::from));
        dirs.extend(var("HOME").map(|home| home.join(".local").join("bin")));
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs
}

/// The programs named `fafnirx` in `dirs` (each once), other than `current`.
fn programs_in(dirs: &[PathBuf], current: &Path) -> Vec<PathBuf> {
    // Windows gives such paths a `\?\` prefix, which a process list does not have.
    let plain = |path: &Path| fafnirx_common::plain_path(&std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
    let current = plain(current);
    let mut found: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        let Ok(path) = std::fs::canonicalize(dir.join(PROGRAM)).map(|p| fafnirx_common::plain_path(&p)) else {
            continue;
        };
        if path != current && path.is_file() && !found.contains(&path) {
            found.push(path);
        }
    }
    found
}

/// The version in what `fafnirx --version` prints: `fafnirx 3.13.0`.
fn parse_version(output: &str) -> Option<String> {
    let mut words = output.split_whitespace();
    let (name, version) = (words.next()?, words.next()?);
    (name == "fafnirx" && super::parse(version).is_some()).then(|| version.to_string())
}

/// Asks a program its version. Nothing but a client that answers in time and in the words above gives one.
fn version_of(program: &Path) -> Option<String> {
    let mut child = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(status) if status.success() => break,
            Some(_) => return None,
            None if started.elapsed() > ASK_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
    parse_version(&out)
}

/// Of the `(program, version)` found, the ones older than `current`.
fn older_than(current: &str, found: Vec<(PathBuf, String)>) -> Vec<(PathBuf, String)> {
    found.into_iter().filter(|(_, version)| super::newer(current, version)).collect()
}

/// The older copies of the client that are installed besides `current`.
fn older_copies(current: &Path) -> Vec<(PathBuf, String)> {
    let found = programs_in(&install_dirs(), current)
        .into_iter()
        .filter_map(|program| version_of(&program).map(|version| (program, version)))
        .collect();
    older_than(super::VERSION, found)
}

/// Ends the processes that run `program`, so that it can be removed. Not this one.
pub fn stop_running(program: &Path) {
    let me = std::process::id();
    #[cfg(windows)]
    {
        let script = format!(
            "Get-Process -Name fafnirx -ErrorAction SilentlyContinue | Where-Object {{ $_.Path -eq '{}' -and $_.Id -ne {me} }} | Stop-Process -Force",
            program.display().to_string().replace('\'', "''")
        );
        let _ = crate::service::powershell(&script).output();
    }
    #[cfg(target_os = "linux")]
    for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        if pid != me && std::fs::read_link(entry.path().join("exe")).is_ok_and(|exe| exe == program) {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
    }
    #[cfg(target_os = "macos")]
    if let Ok(list) = Command::new("ps").args(["-axo", "pid=,comm="]).output() {
        for line in String::from_utf8_lossy(&list.stdout).lines() {
            let line = line.trim_start();
            if let Some((pid, command)) = line.split_once(' ') {
                if pid != me.to_string() && Path::new(command.trim()) == program {
                    let _ = Command::new("kill").arg(pid).status();
                }
            }
        }
    }
    // Let them exit before the file is touched.
    std::thread::sleep(Duration::from_millis(300));
}

/// The package that owns `program` (dpkg), when a package manager put it there.
fn package_of(program: &Path) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let out = Command::new("dpkg").arg("-S").arg(program).stderr(Stdio::null()).output().ok()?;
    let line = String::from_utf8_lossy(&out.stdout);
    out.status.success().then(|| line.split(':').next().map(|p| p.trim().to_string())).flatten()
}

fn remove(program: &Path) -> Result<()> {
    if let Some(package) = package_of(program) {
        return crate::service::elevate::run("dpkg", &["-r", &package]);
    }
    match std::fs::remove_file(program) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && !cfg!(windows) => crate::service::elevate::run("rm", &["-f", &program.to_string_lossy()]),
        Err(e) => bail!(e),
    }
}

/// After an install: stops and removes every older copy of the client besides the program at `current`, and says what happened.
/// Nothing is removed when it is not clear that the file is a client of an older version.
pub fn announce(current: &Path) {
    for (program, version) in older_copies(current) {
        stop_running(&program);
        match remove(&program).with_context(|| format!("remove {} (version {version})", program.display())) {
            Ok(()) => {
                tracing::info!("removed the older version {version} of the client at {}", program.display());
                println!("removed the older version {version} at {}", program.display());
            }
            Err(e) => {
                tracing::warn!("an older version of the client is still at {}: {e:#}", program.display());
                eprintln!("an older version of the client is still at {}: {e:#}", program.display());
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/update/purge.rs"]
mod tests;
