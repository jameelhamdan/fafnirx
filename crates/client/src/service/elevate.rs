//! Doing something that needs an administrator (root) from a process that is not one. The system-wide client belongs to the
//! computer, not to a user, so installing it, changing its config and replacing its program take more than a user has.

use anyhow::{bail, Context, Result};
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
        Command::new("id").arg("-u").output().is_ok_and(|o| o.stdout.trim_ascii() == b"0")
    }
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
    let (program, args) = as_root(OsStr::new(program), &args)
        .map_err(|by_hand| anyhow::anyhow!("this needs root and nothing here can ask for it: run `{by_hand}`"))?;
    let status =
        Command::new(&program).args(&args).status().with_context(|| format!("run {}", program.to_string_lossy()))?;
    if !status.success() {
        bail!("{} failed with {status}", program.to_string_lossy());
    }
    Ok(())
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
        let status = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .stdin(Stdio::null())
            .status()
            .context("ask for administrator rights")?;
        return Ok(status.code().unwrap_or(1));
    }
    let (program, line) = as_root(exe.as_os_str(), args)
        .map_err(|by_hand| anyhow::anyhow!("this needs root and nothing here can ask for it: run `{by_hand}`"))?;
    Ok(Command::new(&program).args(&line).status().context("run as root")?.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_package_is_installed_by_whoever_can_ask_for_permission_where_they_are() {
        let f = |root, terminal, graphical, sudo, pkexec| choose(&Facts { root, terminal, graphical, sudo, pkexec });
        assert_eq!(f(true, false, false, false, false), Way::Direct, "root needs nobody's permission");
        assert_eq!(f(false, true, true, true, true), Way::Sudo, "at a terminal, the terminal asks");
        assert_eq!(f(false, false, true, true, true), Way::Pkexec, "from the tray, a window asks");
        assert_eq!(f(false, true, false, false, true), Way::Manual, "a terminal without sudo, and no window: told");
        assert_eq!(f(false, false, false, true, true), Way::Manual, "nothing can ask");
        assert_eq!(f(false, false, true, true, false), Way::Manual, "no polkit");
    }

    #[test]
    fn the_command_to_type_is_short_and_safe_for_the_shell() {
        let line = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();
        let default = crate::config::default_config_path();
        let default = default.to_string_lossy();
        assert_eq!(
            by_hand(&line(&["/usr/local/bin/fafnirx", "--config", &default, "remove-folder", "Documents"])),
            "sudo fafnirx remove-folder Documents",
            "the program by its name, the usual config left out"
        );
        assert_eq!(
            by_hand(&line(&["/usr/local/bin/fafnirx", "--config", "/Library/Application Support/x.toml", "folders"])),
            "sudo fafnirx --config '/Library/Application Support/x.toml' folders",
            "another config is kept, quoted"
        );
        assert_eq!(by_hand(&line(&["fafnirx", "add-folder", "/a/it's"])), r"sudo fafnirx add-folder '/a/it'\''s'");
    }

    #[test]
    fn words_are_quoted_for_a_windows_command_line() {
        assert_eq!(quote(OsStr::new("run")), "run");
        assert_eq!(quote(OsStr::new(r"C:\Program Files\fafnirx")), r#""C:\Program Files\fafnirx""#);
        assert_eq!(quote(OsStr::new(r#"a "b""#)), r#""a \"b\"""#);
        assert_eq!(quote(OsStr::new("")), "\"\"");
    }
}
