//! Only one daemon and one tray run per state directory: each takes an exclusive lock on a file there,
//! for as long as it lives. The operating system releases the lock when the process ends, however it ends.

use anyhow::Result;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

/// The locks this process holds, by name.
static HELD: Mutex<Vec<(&'static str, std::fs::File)>> = Mutex::new(Vec::new());

/// Takes the lock called `name` (a file `<name>.lock` in `state_dir`), waiting `attempts` half-seconds for
/// a process that is just exiting (a restart starts the new one before the old one is gone). Returns false
/// when another process holds it.
pub fn claim(state_dir: &Path, name: &'static str, attempts: u32) -> Result<bool> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(state_dir.join(format!("{name}.lock")))?;
    for _ in 0..attempts {
        match file.try_lock() {
            Ok(()) => {
                HELD.lock().unwrap_or_else(|e| e.into_inner()).push((name, file));
                return Ok(true);
            }
            Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(500)),
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }
    }
    Ok(false)
}

/// Lets go of a lock early, so that a process this one starts can take it.
pub fn release(name: &str) {
    HELD.lock().unwrap_or_else(|e| e.into_inner()).retain(|(n, _)| *n != name);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_process_holds_a_name_per_state_directory() {
        let dir = tempfile::tempdir().unwrap();
        assert!(claim(dir.path(), "daemon", 1).unwrap());
        // A second claim opens the file again and must not get the lock.
        assert!(!claim(dir.path(), "daemon", 1).unwrap(), "already held");
        assert!(claim(dir.path(), "tray", 1).unwrap(), "another name is independent");
        release("daemon");
        assert!(claim(dir.path(), "daemon", 1).unwrap(), "free again once released");
        release("daemon");
        release("tray");
    }

    #[test]
    fn different_state_directories_do_not_collide() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        assert!(claim(a.path(), "tray", 1).unwrap());
        assert!(claim(b.path(), "tray", 1).unwrap());
        release("tray");
    }
}
