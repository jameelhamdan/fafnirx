#[cfg(unix)]
#[test]
fn nothing_is_written_through_a_link_in_the_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (root, outside) = (dir.path().join("synced"), dir.path().join("outside"));
    std::fs::create_dir_all(root.join("real")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    assert!(super::make_parent(&root, &root.join("real/new/a.txt")).is_ok());
    let e = super::make_parent(&root, &root.join("link/a.txt")).unwrap_err();
    assert!(format!("{e:?}").contains("is a link"), "{e:?}");
    assert!(super::make_parent(&root, &root.join("link/deep/a.txt")).is_err(), "nor below it");
    assert!(!outside.join("deep").exists(), "nothing was made outside");
}

use super::*;
use crate::config::Mode;
use crate::sync::tests::{serve, Calls};

#[test]
fn dates_are_civil_dates() {
    assert_eq!(civil_date(0), "1970-01-01");
    assert_eq!(civil_date(951_782_400), "2000-02-29");
    assert_eq!(civil_date(1_793_059_200), "2026-10-27");
}

#[test]
fn conflicts_get_a_name_of_their_own_next_to_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.txt");
    let first = conflict_name(&file, "laptop", 0);
    assert_eq!(first.file_name().unwrap(), "notes (conflict laptop 1970-01-01).txt");
    std::fs::write(&first, b"x").unwrap();
    assert_eq!(conflict_name(&file, "laptop", 0).file_name().unwrap(), "notes (conflict laptop 1970-01-01 1).txt");
    let bare = dir.path().join("Makefile");
    assert_eq!(conflict_name(&bare, "my pc", 0).file_name().unwrap(), "Makefile (conflict my-pc 1970-01-01)");
}

#[test]
fn a_read_only_share_stops_that_folder_not_the_client() {
    let read_only = SyncError::from(tonic::Status::failed_precondition("\"bob/family\" is shared with you to read only"));
    assert!(matches!(read_only, SyncError::Refused(_)), "a problem with the folder, recorded against no file");
    let in_use = SyncError::from(tonic::Status::permission_denied("device in use: another computer is connected"));
    assert!(matches!(in_use, SyncError::Offline(_)), "a problem with the device: nothing can sync");
}

/// Changes of a synced folder as the master lists them.
mod master {
    use super::*;
    use fafnirx_common::{root_hash, CHUNK_SIZE};

    pub fn root_of(data: &[u8]) -> Vec<u8> {
        assert!(data.len() < CHUNK_SIZE);
        let chunks = if data.is_empty() { vec![] } else { vec![chunk_hash(data)] };
        root_hash(&chunks, data.len() as u64).to_vec()
    }

    pub fn live(path: &str, data: &[u8]) -> pb::Change {
        pb::Change {
            path: path.into(),
            deleted: false,
            root_hash: root_of(data),
            size: data.len() as u64,
            mtime: 1,
            mode: 0,
            from_web: false,
        }
    }

    pub fn gone(path: &str) -> pb::Change {
        pb::Change { path: path.into(), deleted: true, ..Default::default() }
    }

    pub fn gone_in_the_web_app(path: &str) -> pb::Change {
        pb::Change { from_web: true, ..gone(path) }
    }
}

struct Rig {
    attached: Attached,
    cfg: Config,
    held: Arc<Mutex<Calls>>,
    folder: tempfile::TempDir,
    _state: tempfile::TempDir,
}

impl Rig {
    async fn new() -> Self {
        let held = Arc::new(Mutex::new(Calls::default()));
        let url = serve(held.clone()).await;
        let (folder, state) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let synced = Folder {
            name: "alice/family".into(),
            path: folder.path().to_path_buf(),
            target: crate::config::Target::Folder,
            id: None,
            mode: Mode::Keep,
            ask_timeout_hours: 24,
        };
        let mut cfg = Config::new(url);
        cfg.folders.push(synced.clone());
        let attached = Attached::open(&cfg, "token", state.path(), &synced, Arc::new(Mutex::new(Vec::new()))).unwrap();
        Self { attached, cfg, held, folder, _state: state }
    }

    /// Puts a file on the master and lists it as changed.
    fn remote_has(&self, path: &str, data: &[u8]) {
        let mut h = self.held.lock().unwrap();
        h.files.insert(path.into(), data.to_vec());
        h.changes = vec![master::live(path, data)];
    }

    fn remote_deleted(&self, path: &str) {
        let mut h = self.held.lock().unwrap();
        h.files.remove(path);
        h.changes = vec![master::gone(path)];
    }

    async fn cycle(&mut self) -> Report {
        self.attached.cycle(&self.cfg).await.expect("the cycle works")
    }

    fn local(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.folder.path().join(path)).ok()
    }

    /// The names at the top of the folder, sorted.
    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.folder.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// The names in the folder that are conflict copies.
    fn conflicts(&self) -> Vec<String> {
        self.names().into_iter().filter(|n| n.contains("(conflict ")).collect()
    }
}

#[tokio::test]
async fn a_new_file_on_the_master_is_received_and_not_sent_back() {
    let mut rig = Rig::new().await;
    rig.remote_has("docs/a.txt", b"hello");
    let report = rig.cycle().await;
    assert_eq!((report.downloaded, report.conflicts, report.failed), (1, 0, 0));
    assert_eq!(rig.local("docs/a.txt").as_deref(), Some("hello"), "in its folder, below the root");
    assert!(rig.held.lock().unwrap().commits.is_empty(), "what came from the master is not sent back to it");
    // The same listing again changes nothing: the file is already what the master has.
    assert_eq!(rig.cycle().await.downloaded, 0);
    assert!(std::fs::read_dir(rig.folder.path().join("docs")).unwrap().count() == 1, "no part file is left");
}

#[tokio::test]
async fn a_folder_synced_up_only_sends_and_receives_nothing() {
    let mut rig = Rig::new().await;
    rig.attached.synced.mode = Mode::Up;
    rig.remote_has("from-master.txt", b"theirs");
    std::fs::write(rig.folder.path().join("mine.txt"), "mine").unwrap();
    let report = rig.cycle().await;
    assert_eq!(report.downloaded, 0, "nothing of the master's comes down");
    assert!(rig.local("from-master.txt").is_none());
    assert!(rig.sent("mine.txt"), "what is here goes up");
}

#[tokio::test]
async fn a_file_changed_on_both_sides_keeps_both() {
    let mut rig = Rig::new().await;
    std::fs::write(rig.folder.path().join("b.txt"), "mine").unwrap();
    rig.remote_has("b.txt", b"theirs");
    let report = rig.cycle().await;
    assert_eq!((report.downloaded, report.conflicts), (1, 1));
    assert_eq!(rig.local("b.txt").as_deref(), Some("theirs"), "the master's version has the name");
    let conflicts = rig.conflicts();
    assert_eq!(conflicts.len(), 1, "{conflicts:?}");
    assert!(conflicts[0].starts_with("b (conflict "), "{}", conflicts[0]);
    assert_eq!(rig.local(&conflicts[0]).as_deref(), Some("mine"), "ours is kept under its own name");
    assert!(
        rig.held.lock().unwrap().commits.iter().any(|(path, _)| *path == conflicts[0]),
        "and sent, like any new file"
    );
}

impl Rig {
    fn with_policy(mut self, mode: Mode) -> Self {
        self.attached.synced.mode = mode;
        self
    }

    /// The versions of this computer that were put aside, by name.
    fn aside(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.folder.path().join(CONFLICT_DIR))
            .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        names.sort();
        names
    }

    fn sent(&self, path: &str) -> bool {
        self.held.lock().unwrap().commits.iter().any(|(p, _)| p == path)
    }

    /// The master lists nothing new (a cursor that went past the last change).
    fn nothing_new(&self) {
        self.held.lock().unwrap().changes.clear();
    }
}

#[tokio::test]
async fn theirs_keeps_the_masters_version_and_puts_ours_aside_unsent() {
    let mut rig = Rig::new().await.with_policy(Mode::Both);
    std::fs::write(rig.folder.path().join("b.txt"), "mine").unwrap();
    rig.remote_has("b.txt", b"theirs");
    let report = rig.cycle().await;
    assert_eq!((report.downloaded, report.conflicts), (1, 1));
    assert_eq!(rig.local("b.txt").as_deref(), Some("theirs"));
    assert!(rig.conflicts().is_empty(), "no conflict copy among the files");
    let aside = rig.aside();
    assert_eq!(aside.len(), 1, "{aside:?}");
    assert_eq!(
        std::fs::read_to_string(rig.folder.path().join(CONFLICT_DIR).join(&aside[0])).unwrap(),
        "mine",
        "ours is not lost"
    );
    rig.nothing_new();
    rig.cycle().await;
    assert!(rig.held.lock().unwrap().commits.is_empty(), "what was put aside is never sent");
}

#[tokio::test]
async fn a_file_edited_here_and_deleted_there_follows_the_policy() {
    let mut rig = Rig::new().await.with_policy(Mode::Both);
    rig.remote_has("d.txt", b"one");
    rig.cycle().await;
    std::fs::write(rig.folder.path().join("d.txt"), "edited here").unwrap();
    rig.remote_deleted("d.txt");
    rig.cycle().await;
    assert!(rig.local("d.txt").is_none(), "the deletion applies");
    let aside = rig.aside();
    assert_eq!(aside.len(), 1, "{aside:?}");
    assert!(!rig.sent("d.txt") && rig.conflicts().is_empty());
}

#[cfg(any(windows, target_os = "macos", feature = "tray"))]
mod ask;

#[tokio::test]
async fn a_file_deleted_on_the_master_goes_unless_it_was_changed_here() {
    let mut rig = Rig::new().await;
    rig.remote_has("c.txt", b"one");
    rig.cycle().await;
    rig.remote_deleted("c.txt");
    let report = rig.cycle().await;
    assert_eq!(report.deleted, 1);
    assert!(rig.local("c.txt").is_none(), "unchanged here: it goes too");

    rig.remote_has("d.txt", b"one");
    rig.cycle().await;
    std::fs::write(rig.folder.path().join("d.txt"), "edited here after it came").unwrap();
    rig.remote_deleted("d.txt");
    let report = rig.cycle().await;
    assert_eq!((report.deleted, report.conflicts), (0, 1));
    assert!(rig.local("d.txt").is_none());
    let kept = rig.conflicts();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(rig.local(&kept[0]).as_deref(), Some("edited here after it came"), "the edit is not lost");
}

mod names;

#[tokio::test]
async fn what_was_deleted_while_this_computer_was_away_goes_when_the_master_lists_everything() {
    let mut rig = Rig::new().await;
    for (path, text) in [("keep.txt", "1"), ("gone.txt", "2"), ("edited.txt", "3")] {
        rig.remote_has(path, text.as_bytes());
        rig.cycle().await;
    }
    std::fs::write(rig.folder.path().join("edited.txt"), "changed here meanwhile").unwrap();
    {
        // The master no longer remembers the deletions: it lists what is live, and the other two are not in it.
        let mut h = rig.held.lock().unwrap();
        h.complete = true;
        h.changes = vec![master::live("keep.txt", b"1")];
    }
    let report = rig.cycle().await;
    assert_eq!(report.deleted, 1, "the untouched file that is not listed any more is deleted");
    assert_eq!(rig.local("gone.txt"), None);
    assert_eq!(rig.local("keep.txt").as_deref(), Some("1"), "a listed one stays");
    assert_eq!(report.conflicts, 1, "and the one edited here is kept, under another name");
    let kept = rig.conflicts();
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(rig.local(&kept[0]).as_deref(), Some("changed here meanwhile"));
    assert_eq!(rig.local("edited.txt"), None);
}

#[tokio::test]
async fn a_listing_of_changes_alone_never_deletes_what_it_does_not_mention() {
    let mut rig = Rig::new().await;
    rig.remote_has("a.txt", b"1");
    rig.cycle().await;
    rig.remote_has("b.txt", b"2");
    let report = rig.cycle().await;
    assert_eq!((report.deleted, report.downloaded), (0, 1));
    assert_eq!(rig.local("a.txt").as_deref(), Some("1"));
}

#[tokio::test]
async fn the_state_of_a_folder_says_how_it_is_doing() {
    let mut rig = Rig::new().await;
    let before = rig.attached.status();
    assert_eq!((before.files, before.last_cycle, before.last_error.as_str()), (0, 0, ""));
    assert_eq!(before.summary(1_000), "waiting for the first sync");

    rig.remote_has("a.txt", b"hello");
    rig.cycle().await;
    let after = rig.attached.status();
    assert_eq!(
        (after.name.as_str(), after.files, after.bytes, after.held, after.failing),
        ("alice/family", 1, 5, 0, 0)
    );
    assert!(after.last_cycle > 0 && after.last_error.is_empty());
    assert!(
        after.summary(after.last_cycle + 90).starts_with("in step, 1 files, synced 1m ago"),
        "{}",
        after.summary(after.last_cycle + 90)
    );

    // A problem of the cycle is told, in the state and in its line.
    rig.remote_has("b.txt", b"x");
    rig.held.lock().unwrap().files.remove("b.txt"); // listed, but the master cannot give it
    rig.cycle().await;
    let failed = rig.attached.status();
    assert!(failed.last_error.contains("b.txt"), "{}", failed.last_error);
    assert!(failed.summary(failed.last_cycle).contains("needs attention") || failed.summary(failed.last_cycle).contains("last problem"));
}

mod held;

#[test]
fn only_a_folder_gone_from_the_master_and_empty_here_can_be_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let folder = |name: &str, files: &[&str]| {
        let path = dir.path().join(name.replace('/', "_"));
        std::fs::create_dir_all(&path).unwrap();
        for f in files {
            std::fs::write(path.join(f), b"x").unwrap();
        }
        Folder {
            path,
            name: name.into(),
            target: crate::config::Target::Folder,
            id: None,
            mode: Mode::Both,
            ask_timeout_hours: 0,
        }
    };
    let (gone, kept, still_there) = (folder("me/gone", &[]), folder("me/kept", &["a.txt"]), folder("me/there", &[]));
    let mut missing = folder("me/missing", &[]);
    std::fs::remove_dir(&missing.path).unwrap();
    missing.path = dir.path().join("not-mounted");
    let on_master = vec![("me/there".to_string(), true)];
    let all = [gone, kept, still_there, missing];
    assert_eq!(deleted_and_empty(all.iter(), &on_master), vec!["me/gone".to_string()]);
}
