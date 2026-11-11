//! Doing something that needs an administrator (root) from a process that is not one. The system-wide client belongs to the
//! computer, not to a user, so installing it, changing its config and replacing its program take more than a user has.

use anyhow::{Context, Result};
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::{Command, Stdio};

/// How something is run as root by whoever is not.
#[derive(Debug, PartialEq)]
pub enum Way {
    /// This process is root.
    Direct,
    /// `sudo`, asking in the terminal this was started in.
    Sudo,
    /// `pkexec`, asking in a window (polkit).
    Pkexec,
    /// Neither can ask: the person is told the command.
    Manual,
}

/// What decides how it is done.
pub struct Facts {
    pub root: bool,
    /// Standard input is a terminal, so a password can be typed.
    pub terminal: bool,
    /// A graphical session (X11 or Wayland) is there, so a window can ask.
    pub graphical: bool,
    pub sudo: bool,
    pub pkexec: bool,
}

/// Whoever is at the keyboard is asked where they are: in the terminal if there is one, else in a window.
pub fn choose(f: &Facts) -> Way {
    if f.root {
        Way::Direct
    } else if f.terminal && f.sudo {
        Way::Sudo
    } else if f.graphical && f.pkexec {
        Way::Pkexec
    } else {
        Way::Manual
    }
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

/// Whether this process is root (Linux, macOS) or an administrator with its rights (Windows).
pub fn is_admin() -> bool {
    if cfg!(windows) {
        // Only an elevated administrator may list the sessions of the computer.
        Command::new("net")
            .arg("session")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    } else {
        user_id().as_deref() == Some("0")
    }
}

/// The id of the user of this process (Linux, macOS).
pub fn user_id() -> Option<String> {
    let out = Command::new("id").arg("-u").output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn facts() -> Facts {
    use std::io::IsTerminal;
    Facts {
        root: is_admin(),
        terminal: std::io::stdin().is_terminal(),
        graphical: std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some(),
        sudo: on_path("sudo"),
        pkexec: on_path("pkexec"),
    }
}

/// The command line that runs `program` with `args` as root, and how: `Err` with what to run by hand when nothing can ask.
fn as_root(program: &OsStr, args: &[OsString]) -> Result<(OsString, Vec<OsString>), String> {
    let mut line = vec![program.to_os_string()];
    line.extend(args.iter().cloned());
    match choose(&facts()) {
        Way::Direct => Ok((line.remove(0), line)),
        // The device token of a scripted install travels in the environment, which sudo would drop.
        Way::Sudo if std::env::var_os("FAFNIRX_TOKEN").is_some() => {
            line.insert(0, "--preserve-env=FAFNIRX_TOKEN".into());
            Ok(("sudo".into(), line))
        }
        Way::Sudo => Ok(("sudo".into(), line)),
        Way::Pkexec => Ok(("pkexec".into(), line)),
        Way::Manual => Err(by_hand(&line)),
    }
}

/// The command to type: the program by its name (it is on the path), the config only when it is not the default one, and every
/// word that has a space in it quoted for the shell.
fn by_hand(line: &[OsString]) -> String {
    let default = crate::config::default_config_path();
    let mut words: Vec<String> = Vec::new();
    let mut rest = line.iter().skip(1).peekable();
    while let Some(word) = rest.next() {
        if word == "--config" && rest.peek().is_some_and(|path| Path::new(path) == default) {
            rest.next();
        } else {
            words.push(word.to_string_lossy().into_owned());
        }
    }
    let program = Path::new(&line[0]).file_name().unwrap_or(&line[0]).to_string_lossy().into_owned();
    std::iter::once("sudo".to_string())
        .chain(std::iter::once(program))
        .chain(words)
        .map(|w| if w.contains([' ', '\t', '\'', '"']) { format!("'{}'", w.replace('\'', r"'\''")) } else { w })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs `program args` as root and waits for it. Linux and macOS: `sudo` or `pkexec` ask where the person is.
pub fn run(program: &str, args: &[&str]) -> Result<()> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let (program, args) = as_root(OsStr::new(program), &args).map_err(|by_hand| anyhow::anyhow!("this needs root and nothing here can ask for it: run `{by_hand}`"))?;
    super::run_checked(&program, &args)
}

/// Quotes a word for a Windows command line.
fn quote(word: &OsStr) -> String {
    let word = word.to_string_lossy();
    if !word.is_empty() && !word.contains([' ', '\t', '"']) {
        return word.into_owned();
    }
    format!("\"{}\"", word.replace('"', "\\\""))
}

/// Runs this program again with `args` as an administrator, waits, and returns its exit code. Windows asks with the usual
/// consent window (UAC); elsewhere `sudo` or `pkexec`.
pub fn rerun(args: &[OsString]) -> Result<i32> {
    let exe = std::env::current_exe()?;
    if cfg!(windows) {
        let list = args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ").replace('\'', "''");
        let script = format!(
            "$p = Start-Process -FilePath '{}' -ArgumentList '{list}' -Verb RunAs -Wait -PassThru; exit $p.ExitCode",
            exe.display().to_string().replace('\'', "''")
        );
        let status = super::powershell(&script).stdin(Stdio::null()).status().context("ask for administrator rights")?;
        return Ok(status.code().unwrap_or(1));
    }
    let (program, line) = as_root(exe.as_os_str(), args).map_err(|by_hand| anyhow::anyhow!("this needs root and nothing here can ask for it: run `{by_hand}`"))?;
    let mut cmd = Command::new(&program);
    cmd.args(&line);
    // pkexec clears the environment, so the token of a scripted install goes in on standard input, where `read_token`
    // looks next (an argument would be visible to the other users of the computer).
    let token = std::env::var("FAFNIRX_TOKEN").ok().filter(|t| !t.trim().is_empty() && program == "pkexec");
    let Some(token) = token else {
        return Ok(cmd.status().context("run as root")?.code().unwrap_or(1));
    };
    let mut child = cmd.stdin(Stdio::piped()).spawn().context("run as root")?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        // A run that does not enroll never reads it; the pipe closing then is no error.
        let _ = writeln!(stdin, "{}", token.trim());
    }
    Ok(child.wait().context("run as root")?.code().unwrap_or(1))
}

#[cfg(test)]
#[path = "../../tests/unit/service/elevate.rs"]
mod tests;
