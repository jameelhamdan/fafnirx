//! Protection against deleting a folder's files on the master by accident.
//!
//! A drive that is not mounted, a folder emptied by mistake or a runaway script makes a large share of a folder's files
//! vanish at once. Such deletes are held back and only sent when the files have stayed gone for the hold time (hours),
//! which is long enough to be sure it was meant. The master keeps the data of deleted files for its retention period in
//! any case; this keeps them in its live tree and web app meanwhile. Small deletes are never held.

use crate::sync::state::Synced;
use std::collections::HashMap;

/// A folder trips the guard when at least this many of its synced files vanished...
pub const MIN_FILES: usize = 10;
/// ...and they are more than this share (percent) of the files synced from the folder.
pub const MAX_SHARE_PERCENT: usize = 20;
/// How long a tripped folder is held, unless the config says otherwise.
pub const DEFAULT_HOLD_HOURS: u64 = 12;

/// What to do with the vanished files of one cycle.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    /// Delete these on the master now.
    pub send: Vec<Synced>,
    /// Folder name -> (files held, when the hold began).
    pub held: HashMap<String, (usize, i64)>,
}

/// Which folder a synced path belongs to.
pub type Grouping = fn(&str) -> &str;

/// The folder is the first path component (the collected folders).
pub fn folder_of(path: &str) -> &str {
    path.split('/').next().unwrap_or("")
}

/// All paths are one folder with no name of its own (a synced folder, whose files are at the top).
pub fn at_the_top(_: &str) -> &str {
    ""
}

/// Whether `vanished` of the `synced` files of a folder are too many to delete without waiting.
pub fn tripped(vanished: usize, synced: usize) -> bool {
    vanished >= MIN_FILES && vanished * 100 > synced * MAX_SHARE_PERCENT
}

/// What the guard knows besides the vanished files.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    /// Files synced per folder, including the vanished ones.
    pub synced: &'a HashMap<String, usize>,
    /// When each folder's current hold began (from a previous cycle).
    pub holds: &'a HashMap<String, i64>,
    pub now: i64,
    /// How long a hold lasts.
    pub hold_secs: i64,
    /// The user asked for held deletes to be applied now.
    pub release: bool,
}

/// Decides, folder by folder, what is deleted now and what is held. `group` says which folder a path belongs to.
pub fn plan(group: Grouping, vanished: Vec<Synced>, ctx: &Context) -> Plan {
    let Context { synced, holds, now, hold_secs, release } = *ctx;
    let mut count: HashMap<String, usize> = HashMap::new();
    for v in &vanished {
        *count.entry(group(&v.path).to_string()).or_default() += 1;
    }
    let mut out = Plan::default();
    for (folder, n) in &count {
        if tripped(*n, synced.get(folder).copied().unwrap_or(*n)) {
            let since = holds.get(folder).copied().unwrap_or(now);
            if !release && now - since < hold_secs {
                out.held.insert(folder.clone(), (*n, since));
            }
        }
    }
    for v in vanished {
        if !out.held.contains_key(group(&v.path)) {
            out.send.push(v);
        }
    }
    out
}

#[cfg(test)]
#[path = "../../tests/unit/sync/guard.rs"]
mod tests;
