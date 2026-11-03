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
mod tests {
    use super::*;

    #[test]
    fn a_folder_lives_until_it_is_dropped_and_each_has_its_own_name() {
        let parent = tempfile::tempdir().unwrap();
        let (a, b) = (Scratch::new(parent.path()).unwrap(), Scratch::new(parent.path()).unwrap());
        assert_ne!(a.path(), b.path());
        std::fs::write(a.path().join("half"), "x").unwrap();
        let gone = a.path().to_path_buf();
        drop(a);
        assert!(!gone.exists() && b.path().is_dir());
    }

    #[test]
    fn an_old_folder_at_the_same_place_is_emptied() {
        let parent = tempfile::tempdir().unwrap();
        let place = parent.path().join("work");
        std::fs::create_dir_all(&place).unwrap();
        std::fs::write(place.join("old"), "x").unwrap();
        assert!(!Scratch::new_at(place).unwrap().path().join("old").exists());
    }
}
