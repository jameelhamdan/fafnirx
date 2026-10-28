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

/// All paths are one folder with no name of its own (a shared folder, whose files are at the top).
pub fn at_the_top(_: &str) -> &str {
    ""
}

/// Whether `vanished` of the `synced` files of a folder are too many to delete without waiting.
pub fn tripped(vanished: usize, synced: usize) -> bool {
    vanished >= MIN_FILES && vanished * 100 > synced * MAX_SHARE_PERCENT
}

/// Decides, folder by folder, what is deleted now and what is held.
///
/// - `synced`: files synced per folder, including the vanished ones.
/// - `holds`: when each folder's current hold began (from a previous cycle).
/// - `release`: the user asked for held deletes to be applied now.
/// - `group`: which folder a path belongs to.
pub fn plan(
    group: Grouping,
    vanished: Vec<Synced>,
    synced: &HashMap<String, usize>,
    holds: &HashMap<String, i64>,
    now: i64,
    hold_secs: i64,
    release: bool,
) -> Plan {
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
mod tests {
    use super::*;

    const HOUR: i64 = 3600;

    /// A device with one folder per name, as most tests have it.
    fn plan(
        vanished: Vec<Synced>,
        synced: &HashMap<String, usize>,
        holds: &HashMap<String, i64>,
        now: i64,
        hold_secs: i64,
        release: bool,
    ) -> Plan {
        super::plan(folder_of, vanished, synced, holds, now, hold_secs, release)
    }

    #[test]
    fn a_shared_folder_is_one_folder_whatever_its_subfolders() {
        let mut gone: Vec<Synced> = Vec::new();
        for dir in ["a", "b", "c"] {
            gone.extend(self::gone(dir, 4));
        }
        let all = totals(&[("", 20)]);
        let top = super::plan(at_the_top, gone.clone(), &all, &HashMap::new(), 0, 12 * HOUR, false);
        assert_eq!((top.send.len(), top.held[""]), (0, (12, 0)), "12 of 20 files together is a large part");
        let by_name = super::plan(
            folder_of,
            gone,
            &totals(&[("a", 20), ("b", 20), ("c", 20)]),
            &HashMap::new(),
            0,
            12 * HOUR,
            false,
        );
        assert_eq!(by_name.send.len(), 12, "counted per name, each is below the minimum");
    }

    fn gone(folder: &str, n: usize) -> Vec<Synced> {
        (0..n)
            .map(|i| Synced { path: format!("{folder}/f{i}"), size: 1, mtime_ns: 1, root_hash: vec![0; 32] })
            .collect()
    }

    fn totals(pairs: &[(&str, usize)]) -> HashMap<String, usize> {
        pairs.iter().map(|(f, n)| (f.to_string(), *n)).collect()
    }

    #[test]
    fn small_deletes_go_through() {
        let p = plan(gone("docs", 9), &totals(&[("docs", 9)]), &HashMap::new(), 0, 12 * HOUR, false);
        assert_eq!((p.send.len(), p.held.len()), (9, 0), "below the minimum, even if it is everything");
        let p = plan(gone("docs", 10), &totals(&[("docs", 1000)]), &HashMap::new(), 0, 12 * HOUR, false);
        assert_eq!((p.send.len(), p.held.len()), (10, 0), "a small share of a big folder");
    }

    #[test]
    fn a_large_share_is_held_from_the_first_cycle() {
        let p = plan(gone("docs", 30), &totals(&[("docs", 100)]), &HashMap::new(), 500, 12 * HOUR, false);
        assert!(p.send.is_empty());
        assert_eq!(p.held["docs"], (30, 500), "30 of 100 files, held since now");
        assert!(!tripped(20, 100), "exactly the share is not more than it");
        assert!(tripped(21, 100));
    }

    #[test]
    fn only_the_tripped_folder_is_held() {
        let mut v = gone("docs", 50);
        v.extend(gone("photos", 11));
        let p = plan(v, &totals(&[("docs", 50), ("photos", 5000)]), &HashMap::new(), 0, 12 * HOUR, false);
        assert_eq!(p.held.keys().collect::<Vec<_>>(), ["docs"]);
        assert_eq!(p.send.len(), 11);
        assert!(p.send.iter().all(|s| s.path.starts_with("photos/")));
    }

    #[test]
    fn deletes_are_sent_once_the_hold_time_has_passed() {
        let holds = totals_i64(&[("docs", 0)]);
        let all = totals(&[("docs", 40)]);
        let p = plan(gone("docs", 40), &all, &holds, 12 * HOUR - 1, 12 * HOUR, false);
        assert_eq!((p.send.len(), p.held["docs"]), (0, (40, 0)), "still waiting, and the clock did not restart");
        let p = plan(gone("docs", 40), &all, &holds, 12 * HOUR, 12 * HOUR, false);
        assert_eq!((p.send.len(), p.held.len()), (40, 0), "long enough: it was meant");
    }

    #[test]
    fn the_user_can_release_the_deletes() {
        let p = plan(gone("docs", 40), &totals(&[("docs", 40)]), &totals_i64(&[("docs", 0)]), 60, 12 * HOUR, true);
        assert_eq!((p.send.len(), p.held.len()), (40, 0));
    }

    #[test]
    fn files_that_come_back_end_the_hold() {
        // Nothing vanished any more: no plan, so the caller's stored holds are replaced by nothing.
        let p = plan(Vec::new(), &totals(&[("docs", 40)]), &totals_i64(&[("docs", 0)]), 100, 12 * HOUR, false);
        assert_eq!(p, Plan::default());
        // Few enough vanished again: they are an ordinary delete, and the old hold does not apply.
        let p = plan(gone("docs", 3), &totals(&[("docs", 40)]), &totals_i64(&[("docs", 0)]), 100, 12 * HOUR, false);
        assert_eq!((p.send.len(), p.held.len()), (3, 0));
    }

    fn totals_i64(pairs: &[(&str, i64)]) -> HashMap<String, i64> {
        pairs.iter().map(|(f, n)| (f.to_string(), *n)).collect()
    }
}
