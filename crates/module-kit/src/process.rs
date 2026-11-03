//! Runs a program for a source and watches it. A download takes as long as it takes, so there is no limit on the total time;
//! what is not allowed is standing still: a program that neither prints nor makes its files grow for `idle` is killed.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// How often a quiet program is looked at.
const LOOK_EVERY: Duration = Duration::from_secs(2);
/// What the program said last is what says why it failed.
const STDERR_KEEP: usize = 64 << 10;
/// A listing of a big source is one line per file: this is room for tens of thousands of them.
const STDOUT_KEEP: usize = 128 << 20;

/// Reads a pipe to its end, noting every time something arrives, and keeps the last `cap` bytes of it.
async fn drain<R: tokio::io::AsyncRead + Unpin>(mut pipe: R, alive: Arc<(Instant, AtomicU64)>, cap: usize) -> Vec<u8> {
    let (mut kept, mut buf) = (Vec::new(), vec![0u8; 16 << 10]);
    loop {
        match pipe.read(&mut buf).await {
            Ok(0) | Err(_) => return kept,
            Ok(n) => {
                alive.1.store(alive.0.elapsed().as_millis() as u64, Ordering::Relaxed);
                kept.extend_from_slice(&buf[..n]);
                if kept.len() > cap {
                    kept.drain(..kept.len() - cap);
                }
            }
        }
    }
}

/// The bytes of the files below `dir`, folders within folders included: a file that grows is a download that goes on.
fn size_of(dir: &Path) -> u64 {
    fn walk(dir: &Path, depth: usize) -> u64 {
        let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
        entries
            .flatten()
            .map(|e| match e.metadata() {
                Ok(m) if m.is_dir() && depth < 8 => walk(&e.path(), depth + 1),
                Ok(m) if m.is_file() => m.len(),
                _ => 0,
            })
            .sum()
    }
    walk(dir, 0)
}

/// The program with only what it needs of the environment: no token or setting of the module reaches it.
pub fn command(program: &Path, args: Vec<String>, home: &Path) -> Command {
    let mut c = Command::new(program);
    c.env_clear().args(args).env("HOME", home).env("TMPDIR", home).env("LANG", "C.UTF-8");
    for keep in ["PATH", "SYSTEMROOT", "SSL_CERT_FILE", "SSL_CERT_DIR", "HTTPS_PROXY", "HTTP_PROXY", "NO_PROXY"] {
        if let Some(v) = std::env::var_os(keep) {
            c.env(keep, v);
        }
    }
    c
}

/// What a program wrote, and whether it ended well.
#[derive(Debug)]
pub struct Output {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

impl Output {
    /// The output if the program ended well, else its last words as the error.
    pub fn ok(self) -> Result<Self> {
        if self.success {
            Ok(self)
        } else {
            bail!("{}", last_words(&self.stderr, "it exited with an error"))
        }
    }
}

/// Runs `cmd` to its end. It fails with a message of its own when the program stood still for `idle` (output on either
/// pipe, or growth of the files in `watch`, counts as moving). A program that ends with an error is not an error here: the
/// caller decides (`Output::ok`), since a listing that is partly good is still a listing.
pub async fn run(mut cmd: Command, idle: Duration, watch: Option<&Path>) -> Result<Output> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = cmd.spawn().context("start the program")?;
    let alive = Arc::new((Instant::now(), AtomicU64::new(0)));
    let stdout = tokio::spawn(drain(child.stdout.take().expect("piped"), alive.clone(), STDOUT_KEEP));
    let stderr = tokio::spawn(drain(child.stderr.take().expect("piped"), alive.clone(), STDERR_KEEP));
    let mut seen = watch.map(size_of).unwrap_or(0);
    let status = loop {
        tokio::select! {
            status = child.wait() => break status.context("wait for the program")?,
            _ = tokio::time::sleep(LOOK_EVERY) => {
                let now = alive.0.elapsed().as_millis() as u64;
                if let Some(dir) = watch {
                    let size = size_of(dir);
                    if size != seen {
                        seen = size;
                        alive.1.store(now, Ordering::Relaxed);
                    }
                }
                let quiet = now.saturating_sub(alive.1.load(Ordering::Relaxed));
                if quiet >= idle.as_millis() as u64 {
                    let _ = child.kill().await;
                    bail!("stopped: no progress for {} seconds", idle.as_secs());
                }
            }
        }
    };
    let stdout = String::from_utf8_lossy(&stdout.await.unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&stderr.await.unwrap_or_default()).into_owned();
    Ok(Output { stdout, stderr, success: status.success() })
}

/// What a program complained about: the last line that says `ERROR` (in any bracket), else its last line, or `fallback`.
fn last_words(stderr: &str, fallback: &str) -> String {
    let lines: Vec<&str> = stderr.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let tail = lines.iter().rev().find(|l| l.contains("ERROR")).or(lines.last());
    tail.map(|l| l.chars().take(400).collect())
        .unwrap_or_else(|| format!("the program failed: {fallback}"))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.arg("-c").arg(script);
        c
    }

    #[tokio::test]
    async fn a_program_that_succeeds_is_ok() {
        let out = run(sh("echo one; echo two >&2"), Duration::from_secs(30), None).await.unwrap().ok().unwrap();
        assert_eq!((out.stdout.trim(), out.stderr.trim()), ("one", "two"));
    }

    #[tokio::test]
    async fn a_failure_is_its_last_error_line() {
        let out = run(
            sh("echo '02/11 19:00:00 [ERROR] gave up' >&2; echo 'trailing' >&2; echo partial; exit 1"),
            Duration::from_secs(30),
            None,
        )
        .await
        .unwrap();
        assert!(!out.success && out.stdout.trim() == "partial", "a failure still gives what was written");
        assert_eq!(out.ok().err().unwrap().to_string(), "02/11 19:00:00 [ERROR] gave up");
        let silent = run(sh("exit 3"), Duration::from_secs(30), None).await.unwrap().ok().err().unwrap();
        assert!(silent.to_string().starts_with("the program failed"), "{silent}");
    }

    #[tokio::test]
    async fn a_program_that_stands_still_is_killed_and_one_that_prints_is_not() {
        let started = Instant::now();
        let e = run(sh("sleep 30"), Duration::from_secs(1), None).await.unwrap_err();
        assert!(e.to_string().contains("no progress"), "{e}");
        assert!(started.elapsed() < Duration::from_secs(10), "it was killed, not waited for");
        let chatty = run(sh("for i in 1 2 3 4; do echo $i; sleep 0.6; done"), Duration::from_secs(2), None).await;
        assert!(chatty.is_ok(), "output counts as progress even when the whole run is longer than idle");
    }

    #[tokio::test]
    async fn a_growing_file_counts_as_progress() {
        let dir = tempfile::tempdir().unwrap();
        let script = format!("for i in 1 2 3 4 5; do echo x >> {}/part; sleep 0.8; done", dir.path().display());
        let out = run(sh(&script), Duration::from_secs(2), Some(dir.path())).await;
        assert!(out.is_ok(), "{:?}", out.err());
    }

    #[tokio::test]
    async fn the_end_of_a_long_complaint_is_kept() {
        let e = run(
            sh("i=0; while [ $i -lt 20000 ]; do echo 'noise noise noise noise noise noise' >&2; i=$((i+1)); done; echo 'ERROR: the end' >&2; exit 1"),
            Duration::from_secs(30),
            None,
        )
        .await
        .unwrap()
        .ok()
        .unwrap_err();
        assert_eq!(e.to_string(), "ERROR: the end");
    }
}
