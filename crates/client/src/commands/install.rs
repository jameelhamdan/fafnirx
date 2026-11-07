//! `fafnirx install` and `fafnirx uninstall`: everything that sets the client up on a computer or takes it away, in one place. The
//! install scripts, the Windows setup, the Debian package and Homebrew only get the program onto the computer (or download it) and
//! call these.

use crate::config::{self, normalize_server};
use crate::service;
use crate::update::purge;
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// What `fafnirx install` was asked.
pub struct Install {
    /// The master: `nas.lan`, `nas.lan:7443` or a URL. Enrolls this computer (the token is asked for, or read from
    /// `FAFNIRX_TOKEN` or standard input); without it an already enrolled computer is only brought up to date.
    pub server: Option<String>,
    /// Folders to collect.
    pub folders: Vec<PathBuf>,
    /// The program is where it belongs already (a package or a setup put it there): do not copy it.
    pub in_place: bool,
    /// Do not install the service (or, on Windows, the start at login).
    pub no_start: bool,
    /// Windows: leave the PATH alone.
    pub no_path: bool,
    pub dry_run: bool,
}

/// Where the program belongs: a place only administrators can change on Linux and macOS (the service runs as root), the
/// profile of the user on Windows.
pub fn installed_program() -> PathBuf {
    if cfg!(windows) {
        let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        base.join("Programs").join("fafnirx").join("fafnirx.exe")
    } else {
        PathBuf::from("/usr/local/bin/fafnirx")
    }
}

/// Puts the program where it belongs, over the one that is there, and stops the copy that runs.
fn place_program(from: &Path, to: &Path, dry: bool) -> Result<()> {
    println!("copy {} to {}", from.display(), to.display());
    if dry {
        return Ok(());
    }
    let dir = to.parent().context("the program has no folder")?;
    std::fs::create_dir_all(dir).with_context(|| format!("make {}", dir.display()))?;
    purge::stop_running(to);
    // A file of its own name first and then a rename: a failed copy leaves the old program, and a running one keeps its file.
    let next = to.with_extension("new");
    std::fs::copy(from, &next).with_context(|| format!("copy to {}", next.display()))?;
    #[cfg(unix)]
    std::fs::set_permissions(&next, std::os::unix::fs::PermissionsExt::from_mode(0o755))?;
    std::fs::rename(&next, to).with_context(|| format!("replace {}", to.display()))?;
    Ok(())
}

pub async fn install(config_path: PathBuf, o: Install) -> Result<()> {
    let current = fafnirx_common::plain_path(&std::env::current_exe()?);
    let dest = installed_program();
    if !o.in_place && !config::same_path(&current, &dest) {
        place_program(&current, &dest, o.dry_run)?;
        if !o.dry_run {
            // The rest is done by the program that is installed: the service and the start at login must run that file.
            let mut again = Command::new(&dest);
            again.arg("--config").arg(&config_path).args(["install", "--in-place"]);
            if let Some(server) = &o.server {
                again.args(["--server", server]);
            }
            for f in &o.folders {
                again.arg("--folder").arg(f);
            }
            if o.no_start {
                again.arg("--no-start");
            }
            if o.no_path {
                again.arg("--no-path");
            }
            let status = again.status().with_context(|| format!("run {}", dest.display()))?;
            if !status.success() {
                bail!("the installed program failed: {status}");
            }
            return Ok(());
        }
    }

    if cfg!(windows) && !o.no_path {
        if let Some(dir) = current.parent() {
            change_path(dir, true, o.dry_run)?;
        }
    }
    match &o.server {
        Some(server) => {
            let server = normalize_server(server);
            if o.dry_run {
                println!("enroll with {server} (asks for the device token)");
            } else {
                super::enroll(config_path.clone(), server, super::read_token()?).await?;
            }
        }
        None if !o.dry_run && !config_path.exists() => {
            println!("the program is installed; to connect this computer to a master run: fafnirx install --server <address>");
            return Ok(());
        }
        None => {}
    }
    for folder in &o.folders {
        if o.dry_run {
            println!("collect {}", folder.display());
        } else {
            super::folder_add(
                config_path.clone(),
                super::FolderAdd { paths: vec![folder.clone()], target: "computer".into(), name: None, create: false, mode: None },
            )
            .await?;
        }
    }
    if o.no_start {
        println!("the client is not installed as a service or started at login (--no-start)");
        return Ok(());
    }
    let was_installed = service::installed();
    service::install(&config_path, o.dry_run)?;
    if was_installed && !o.dry_run {
        // The service that ran before runs the program that was replaced.
        let _ = service::restart();
    }
    Ok(())
}

pub fn uninstall(config_path: &Path, purge_data: bool, keep_binary: bool, dry: bool) -> Result<()> {
    // Read before anything is removed: the server and the state folder come from the config.
    let cfg = config::Config::load(config_path).ok();
    service::uninstall(config_path, dry)?;
    let program = installed_program();
    let current = fafnirx_common::plain_path(&std::env::current_exe()?);
    if !dry {
        // The client and the tray that still run it.
        purge::stop_running(&current);
    }
    if cfg!(windows) {
        // Where the setup put it may be a folder the person chose.
        for dir in [program.parent(), current.parent()].into_iter().flatten() {
            change_path(dir, false, dry)?;
        }
    }
    if !keep_binary && program.exists() {
        remove_program(&program, dry)?;
    }
    if purge_data {
        purge_files(config_path, cfg.as_ref(), dry)?;
    }
    println!("what this computer sent to the master stays there");
    Ok(())
}

fn remove_program(program: &Path, dry: bool) -> Result<()> {
    println!("remove {}", program.display());
    if dry {
        return Ok(());
    }
    purge::stop_running(program);
    crate::service::remove_program(program)
}

/// `--purge`: the config, the token and the state of this client. Only what the client itself made is removed.
fn purge_files(config_path: &Path, cfg: Option<&config::Config>, dry: bool) -> Result<()> {
    let mut paths: Vec<PathBuf> = vec![config_path.to_path_buf(), config_path.with_file_name("token")];
    if let Some(cfg) = cfg {
        if cfg.state_dir.is_none() {
            paths.push(cfg.state_dir(config_path));
            if config::is_system_config(config_path) {
                paths.push(config::control_dir(&cfg.state_dir(config_path)));
            }
        }
        if !dry {
            config::credentials::forget(&cfg.server, config_path);
        }
    }
    for p in paths {
        println!("remove {}", p.display());
        if !dry {
            let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
        }
    }
    if !dry {
        if let Some(dir) = config_path.parent() {
            let _ = std::fs::remove_dir(dir);
        }
    }
    Ok(())
}

// --- the PATH of the user (Windows) --------------------------------------------------------------------------------------------

/// `paths`, a `;` list, with `dir` added; `None` when it is there already (compared without regard to case or a final `\`).
pub fn path_with(paths: &str, dir: &str) -> Option<String> {
    if paths.split(';').any(|p| same_entry(p, dir)) {
        return None;
    }
    Some(if paths.trim_end_matches(';').is_empty() {
        dir.to_string()
    } else {
        format!("{};{dir}", paths.trim_end_matches(';'))
    })
}

/// `paths` without `dir`; `None` when it was not there.
pub fn path_without(paths: &str, dir: &str) -> Option<String> {
    let kept: Vec<&str> = paths.split(';').filter(|p| !same_entry(p, dir)).collect();
    (kept.len() != paths.split(';').count()).then(|| kept.join(";"))
}

fn same_entry(a: &str, b: &str) -> bool {
    let plain = |s: &str| s.trim().trim_end_matches(['\\', '/']).to_lowercase();
    !a.trim().is_empty() && plain(a) == plain(b)
}

/// Adds `dir` to the PATH of the user, or takes it out. The value is read and written as the registry holds it (with `%VARS%`
/// unexpanded and of the kind `REG_EXPAND_SZ`), and the programs that are open are told.
fn change_path(dir: &Path, add: bool, dry: bool) -> Result<()> {
    let dir = dir.display().to_string();
    println!(
        "{} {dir} {} the PATH of the user",
        if add { "add" } else { "remove" },
        if add { "to" } else { "from" }
    );
    if dry {
        return Ok(());
    }
    const READ: &str = "[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment').GetValue('Path','',[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)";
    let out = crate::service::powershell(READ).output().context("read the PATH")?;
    let old = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let Some(new) = (if add { path_with(&old, &dir) } else { path_without(&old, &dir) }) else { return Ok(()) };
    // Setting and then removing a variable is what tells the programs that are open that the environment changed.
    const WRITE: &str = "$k=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment',$true); \
        $k.SetValue('Path',$env:FAFNIRX_NEW_PATH,[Microsoft.Win32.RegistryValueKind]::ExpandString); \
        [Environment]::SetEnvironmentVariable('FAFNIRX_NOTIFY','1','User'); [Environment]::SetEnvironmentVariable('FAFNIRX_NOTIFY',$null,'User')";
    let status = crate::service::powershell(WRITE).env("FAFNIRX_NEW_PATH", new).status().context("write the PATH")?;
    if !status.success() {
        bail!("could not change the PATH: {status}");
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/commands/install.rs"]
mod tests;
