//! How the names of a shared folder are spelled on this computer.

use fafnirx_common::fold_path;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// What a folder holds, by name and by folded name, so that a lookup does not read the folder again.
#[derive(Default)]
struct DirNames {
    names: HashSet<String>,
    folded: HashMap<String, Vec<String>>,
}

impl DirNames {
    fn read(dir: &Path) -> Self {
        let mut dir_names = Self::default();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            dir_names.add(entry.file_name().to_string_lossy().into_owned());
        }
        dir_names
    }

    fn add(&mut self, name: String) {
        if self.names.insert(name.clone()) {
            self.folded.entry(fold_path(&name)).or_default().push(name);
        }
    }

    fn remove(&mut self, name: &str) {
        self.names.remove(name);
        if let Some(same) = self.folded.get_mut(&fold_path(name)) {
            same.retain(|n| n != name);
        }
    }

    /// The entry called `name`, spelled as it is here: the name itself if it exists, else one that differs only in case.
    fn find(&self, name: &str) -> Option<String> {
        if self.names.contains(name) {
            return Some(name.to_string());
        }
        self.folded.get(&fold_path(name)).and_then(|same| same.first().cloned())
    }
}

/// How the names of a shared folder are spelled on this computer. Every kind of computer treats names that differ only in
/// case (or Unicode form) as one, and a file system that folds case cannot be asked, so the folders are read. Each is read once
/// and then kept up to date as files are received, moved and deleted: asking the file system per path would read a folder of
/// ten thousand files ten thousand times.
pub(super) struct Spellings {
    root: PathBuf,
    dirs: HashMap<PathBuf, DirNames>,
}

impl Spellings {
    pub(super) fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf(), dirs: HashMap::new() }
    }

    /// Forgets what was read; for the cases (renames of folders, conflict copies) not worth tracking.
    pub(super) fn clear(&mut self) {
        self.dirs.clear();
    }

    pub(super) fn find(&mut self, dir: &Path, name: &str) -> Option<String> {
        self.dirs.entry(dir.to_path_buf()).or_insert_with(|| DirNames::read(dir)).find(name)
    }

    /// `path` was made (and the folders above it, as far as they are known).
    pub(super) fn added(&mut self, path: &Path) {
        let Ok(relative) = path.strip_prefix(&self.root) else { return };
        let mut dir = self.root.clone();
        for part in relative.components() {
            let name = part.as_os_str().to_string_lossy().into_owned();
            if let Some(listing) = self.dirs.get_mut(&dir) {
                listing.add(name.clone());
            }
            dir.push(name);
        }
    }

    /// `path` was removed or moved away.
    pub(super) fn removed(&mut self, path: &Path) {
        let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { return };
        if let Some(listing) = self.dirs.get_mut(dir) {
            listing.remove(&name.to_string_lossy());
        }
    }

    /// An existing path of the folder that is the same name as `remote` but spelled differently: another case, or another
    /// Unicode form, in the file or in a folder above it. The path is returned as it is spelled here.
    pub(super) fn clash_with(&mut self, remote: &str) -> Option<PathBuf> {
        let parts: Vec<&str> = remote.split('/').collect();
        let mut dir = self.root.clone();
        let mut differs = false;
        for (i, part) in parts.iter().enumerate() {
            let Some(found) = self.find(&dir, part) else {
                // Nothing of this name is here, so nothing below it is either.
                dir.extend(&parts[i..]);
                return differs.then_some(dir);
            };
            differs |= found != *part;
            dir.push(found);
        }
        differs.then_some(dir)
    }

    /// Gives the folders above `remote` the spelling `remote` has, renaming a folder here that has the same name in another case.
    pub(super) fn respell_folders(&mut self, remote: &str) -> std::io::Result<()> {
        let mut dir = self.root.clone();
        let parts: Vec<&str> = remote.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            if let Some(found) = self.find(&dir, part).filter(|found| found != part) {
                std::fs::rename(dir.join(&found), dir.join(part))?;
                self.clear();
            }
            dir.push(part);
        }
        Ok(())
    }

    /// Whether `remote` exists here spelled exactly so, in the file and in every folder above it.
    pub(super) fn exists_spelled(&mut self, remote: &str) -> bool {
        let mut dir = self.root.clone();
        for part in remote.split('/') {
            if self.find(&dir, part).as_deref() != Some(part) {
                return false;
            }
            dir.push(part);
        }
        true
    }
}
