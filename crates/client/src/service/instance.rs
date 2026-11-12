//! Only one daemon and one tray run per state directory: each takes an exclusive lock on a file there,
//! for as long as it lives. The operating system releases the lock when the process ends, however it ends.

use anyhow::Result;
use fafnirx_common::LockExt;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

/// The locks this process holds, by name.
static HELD: Mutex<Vec<(String, std::fs::File)>> = Mutex::new(Vec::new());

/// Takes the lock called `name` (a file `<name>.lock` in `state_dir`), waiting `attempts` half-seconds for
/// a process that is just exiting (a restart starts the new one before the old one is gone). Returns false
/// when another process holds it.
pub fn claim(state_dir: &Path, name: &str, attempts: u32) -> Result<bool> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(state_dir.join(format!("{name}.lock")))?;
    for _ in 0..attempts {
        match file.try_lock() {
            Ok(()) => {
                HELD.locked().push((name.to_string(), file));
                return Ok(true);
            }
            Err(std::fs::TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(500)),
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }
    }
    Ok(false)
}

/// The name of the user this process runs for, to tell one tray from another where several users share a device.
#[cfg(feature = "tray")]
pub fn user_name() -> String {
    let name = std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default();
    let name: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    if name.is_empty() {
        "user".into()
    } else {
        name
    }
}

/// Which installation of the client this is: made the first time it is asked for and kept in the state directory. It tells the
/// master apart two devices that use one device token (a config copied to a second machine has no state directory yet, so
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
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
            .to_le_bytes(),
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
        // Taking it proves nobody holds it. It is given back by unlocking, not by closing the file: a process being
        // started at this moment has a copy of the file until it runs its program, and the lock lives as long as any copy.
        Ok(file) => match file.try_lock() {
            Ok(()) => {
                let _ = file.unlock();
                false
            }
            Err(e) => matches!(e, std::fs::TryLockError::WouldBlock),
        },
        Err(_) => false,
    }
}

/// Lets go of a lock early, so that a process this one starts can take it.
pub fn release(name: &str) {
    HELD.locked().retain(|(n, file)| {
        let keep = *n != name;
        if !keep {
            let _ = file.unlock(); // see `is_held`: closing the file may not be enough at once
        }
        keep
    });
}

#[cfg(test)]
#[path = "../../tests/unit/service/instance.rs"]
mod tests;
