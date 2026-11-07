//! A folder for one piece of work, removed when it is dropped, so that a failed or abandoned fetch leaves nothing behind.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct Scratch(PathBuf);

impl Scratch {
    /// A new folder with a name of its own below `parent`.
    pub fn new(parent: &Path) -> anyhow::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        Self::new_at(parent.join(format!("{stamp:x}-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))))
    }

    /// An empty folder at exactly this place.
    pub fn new_at(path: PathBuf) -> anyhow::Result<Self> {
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
#[path = "../tests/unit/scratch.rs"]
mod tests;
