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

/// Which installation of the client this is: made the first time it is asked for and kept in the state directory. It tells the
/// master apart two computers that use one device token (a config copied to a second machine has no state directory yet, so
/// it gets an id of its own). It is not a secret and proves nothing; it only keeps two clients from working as one device.
pub fn client_id(state_dir: &Path) -> String {
    let path = state_dir.join("instance-id");
    if let Some(known) = std::fs::read_to_string(&path)
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return known;
    }
    use sha2::{Digest, Sha256};
    use std::hash::{BuildHasher, Hasher};
    // The operating system seeds `RandomState` differently for every one, which is all the randomness this needs.
    let mut hasher = Sha256::new();
    for _ in 0..4 {
        hasher.update(std::collections::hash_map::RandomState::new().build_hasher().finish().to_le_bytes());
    }
    hasher.update(
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()).to_le_bytes(),
    );
    hasher.update(std::process::id().to_le_bytes());
    let id = hex::encode(&hasher.finalize()[..16]);
    let _ = std::fs::create_dir_all(state_dir);
    let _ = std::fs::write(&path, &id);
    id
}

/// Whether another process holds the lock called `name` (the daemon or the tray of this state directory is running).
pub fn is_held(state_dir: &Path, name: &str) -> bool {
    let opened = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(state_dir.join(format!("{name}.lock")));
    match opened {
        // Taking it proves nobody holds it; closing the file gives it back.
        Ok(file) => matches!(file.try_lock(), Err(std::fs::TryLockError::WouldBlock)),
        Err(_) => false,
    }
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
    fn a_held_lock_is_seen_from_outside_and_a_free_one_is_not_taken() {
        // A name of its own: the locks a process holds are kept by name, and the other tests run at the same time.
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_held(dir.path(), "probe"), "nobody holds it");
        assert!(!is_held(dir.path(), "probe"), "and looking did not take it");
        assert!(claim(dir.path(), "probe", 1).unwrap());
        assert!(is_held(dir.path(), "probe"));
        release("probe");
        assert!(!is_held(dir.path(), "probe"), "free again");
    }

    #[test]
    fn an_installation_keeps_its_id_and_another_gets_its_own() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let first = client_id(a.path());
        assert_eq!(first.len(), 32);
        assert_eq!(client_id(a.path()), first, "kept across runs");
        assert_ne!(client_id(b.path()), first, "a second state directory is a second installation");
    }

    #[test]
    fn different_state_directories_do_not_collide() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        assert!(claim(a.path(), "tray", 1).unwrap());
        assert!(claim(b.path(), "tray", 1).unwrap());
        release("tray");
    }
}
