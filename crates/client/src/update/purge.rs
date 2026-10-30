//! Only one version of the client may live on a computer. A new install or update can leave an older program behind: in the place
//! an earlier setup used, in `/usr/bin` when the new one went to `/usr/local/bin`, or in a folder of the `PATH` that comes before
//! the new one, so that typing `fafnirx` runs the old client. After an install and whenever the client starts, the programs
//! found are asked their version, and those older than the one running are stopped and removed.
//!
//! What is looked at is limited to the places a setup puts the program and the folders of the `PATH`; only a program that
//! answers `--version` as `fafnirx <version>` counts, and only an older one is removed (a newer or equal one may be somebody's
//! own build, and is left alone).

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
const PROGRAM: &str = "fafnirx.exe";
#[cfg(not(windows))]
const PROGRAM: &str = "fafnirx";

/// How long a program gets to say its version before it is taken for something else.
const ASK_TIMEOUT: Duration = Duration::from_secs(5);

/// What was done.
#[derive(Debug, Default)]
pub struct Report {
    /// Older versions that were removed, with the version.
    pub removed: Vec<(PathBuf, String)>,
    /// Older versions that could not be removed, and why.
    pub failed: Vec<(PathBuf, String)>,
}

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
    let plain =
        |path: &Path| fafnirx_common::plain_path(&std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
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
    (name == "fafnirx" && version.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
        .then(|| version.to_string())
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

/// Ends the processes that run `program`, so that it can be removed.
fn stop_running(program: &Path) {
    #[cfg(windows)]
    {
        let script = format!(
            "Get-Process -Name fafnirx -ErrorAction SilentlyContinue | Where-Object {{ $_.Path -eq '{}' }} | Stop-Process -Force",
            program.display().to_string().replace('\'', "''")
        );
        let _ = Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]).output();
    }
    #[cfg(target_os = "linux")]
    for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        if std::fs::read_link(entry.path().join("exe")).is_ok_and(|exe| exe == program) {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
    }
    #[cfg(target_os = "macos")]
    if let Ok(list) = Command::new("ps").args(["-axo", "pid=,comm="]).output() {
        for line in String::from_utf8_lossy(&list.stdout).lines() {
            let line = line.trim_start();
            if let Some((pid, command)) = line.split_once(' ') {
                if Path::new(command.trim()) == program {
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
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && !cfg!(windows) => {
            crate::service::elevate::run("rm", &["-f", &program.to_string_lossy()])
        }
        Err(e) => bail!(e),
    }
}

/// Stops and removes every older copy of the client besides the program at `current`. Nothing is removed when it is not clear
/// that the file is a client of an older version. A copy that a package manager put there is removed with it only when
/// `packages` is set (at an install, which asked for the change), never by a client that merely started.
pub fn older_versions(current: &Path, packages: bool) -> Report {
    let mut report = Report::default();
    for (program, version) in older_copies(current) {
        if !packages && package_of(&program).is_some() {
            continue;
        }
        stop_running(&program);
        match remove(&program).with_context(|| format!("remove {} (version {version})", program.display())) {
            Ok(()) => report.removed.push((program, version)),
            Err(e) => report.failed.push((program, format!("{e:#}"))),
        }
    }
    report
}

/// Does it and says what happened: for after an install (`packages`), and for the start of the client.
pub fn announce(current: &Path, packages: bool) {
    let report = older_versions(current, packages);
    for (program, version) in &report.removed {
        tracing::info!("removed the older version {version} of the client at {}", program.display());
        println!("removed the older version {version} at {}", program.display());
    }
    for (program, why) in &report.failed {
        tracing::warn!("an older version of the client is still at {}: {why}", program.display());
        eprintln!("an older version of the client is still at {}: {why}", program.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_what_fafnirx_says() {
        assert_eq!(parse_version("fafnirx 3.13.0\n").as_deref(), Some("3.13.0"));
        assert_eq!(parse_version("fafnirx 3.13"), Some("3.13".into()));
        assert_eq!(parse_version("fafnirx"), None);
        assert_eq!(parse_version("rclone v1.75.2"), None, "another program");
        assert_eq!(parse_version("fafnirx-tool 3.0.0"), None);
        assert_eq!(parse_version("fafnirx beta"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn only_older_versions_are_chosen() {
        let found = vec![
            (PathBuf::from("/a"), "3.12.9".to_string()),
            (PathBuf::from("/b"), "3.13.0".to_string()),
            (PathBuf::from("/c"), "3.14.0".to_string()),
            (PathBuf::from("/d"), "2.0".to_string()),
        ];
        let older: Vec<_> = older_than("3.13.0", found).into_iter().map(|(p, _)| p).collect();
        assert_eq!(older, [PathBuf::from("/a"), PathBuf::from("/d")], "the same and the newer ones stay");
    }

    #[test]
    fn programs_are_found_once_and_the_running_one_is_not_one_of_them() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        for d in [&a, &b] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join(PROGRAM), b"x").unwrap();
        }
        let empty = dir.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        let dirs = vec![a.clone(), b.clone(), a.clone(), empty, dir.path().join("missing")];
        let found = programs_in(&dirs, &a.join(PROGRAM));
        assert_eq!(
            found,
            [fafnirx_common::plain_path(&std::fs::canonicalize(b.join(PROGRAM)).unwrap())],
            "not the running one, not twice"
        );
        assert_eq!(programs_in(&dirs, &dir.path().join("elsewhere")).len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn a_program_that_is_not_the_client_is_not_asked_twice_nor_trusted() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, body: &str| {
            let path = dir.path().join(name);
            std::fs::write(&path, body).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let old = write("old", "#!/bin/sh\necho 'fafnirx 1.2.3'\n");
        let other = write("other", "#!/bin/sh\necho 'something 9.9.9'\n");
        let failing = write("failing", "#!/bin/sh\necho 'fafnirx 1.0.0'\nexit 1\n");
        assert_eq!(version_of(&old).as_deref(), Some("1.2.3"));
        assert_eq!(version_of(&other), None);
        assert_eq!(version_of(&failing), None);
        assert_eq!(version_of(&dir.path().join("missing")), None);
    }

    #[cfg(unix)]
    #[test]
    fn an_older_copy_is_removed_and_the_running_one_stays() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let make = |name: &str, version: &str| {
            std::fs::create_dir_all(dir.path().join(name)).unwrap();
            let path = dir.path().join(name).join(PROGRAM);
            std::fs::write(&path, format!("#!/bin/sh\necho 'fafnirx {version}'\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let (old, newer_copy, current) =
            (make("old", "0.0.1"), make("newer", "99.0.0"), make("current", super::super::VERSION));
        // The same steps as `older_versions`, with the folders given.
        let folders: Vec<_> = ["old", "newer", "current"].iter().map(|n| dir.path().join(n)).collect();
        let found: Vec<_> =
            programs_in(&folders, &current).into_iter().filter_map(|p| version_of(&p).map(|v| (p, v))).collect();
        let older = older_than(super::super::VERSION, found);
        assert_eq!(older.len(), 1);
        remove(&older[0].0).unwrap();
        assert!(!old.exists() && newer_copy.exists() && current.exists());
    }
}
