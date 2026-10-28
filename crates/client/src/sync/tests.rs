//! The engine against a master that only records what it is told: what a cycle sends, and what it leaves alone.

use super::*;
use crate::net;
use fafnirx_common::pb::collector_server::{Collector, CollectorServer};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use tonic::{Request, Response, Status, Streaming};

/// What the recording master was told, and what it answers: the paths it lists as changed and the content it serves for a shared folder.
#[derive(Default)]
pub(crate) struct Calls {
    /// Paths committed by the client, with the path each was renamed from (empty if none).
    pub commits: Vec<(String, String)>,
    pub deleted: Vec<String>,
    /// Paths for which the master says it already holds the content.
    pub already: Vec<String>,
    /// What a shared folder lists as changed.
    pub changes: Vec<pb::Change>,
    /// The listing is every live path, as when the computer was away longer than deletions are remembered.
    pub complete: bool,
    /// The content a shared folder serves.
    pub files: HashMap<String, Vec<u8>>,
}

pub(crate) struct Recorder(pub Arc<Mutex<Calls>>);

/// Serves a Recorder on a free port and returns its address.
pub(crate) async fn serve(calls: Arc<Mutex<Calls>>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(CollectorServer::new(Recorder(calls)))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener)),
    );
    format!("http://{addr}")
}

#[tonic::async_trait]
impl Collector for Recorder {
    async fn identify(&self, _: Request<pb::IdentifyRequest>) -> Result<Response<pb::IdentifyResponse>, Status> {
        Ok(Response::new(pb::IdentifyResponse { master_id: "test-master".into() }))
    }
    async fn heartbeat(&self, _: Request<pb::HeartbeatRequest>) -> Result<Response<pb::HeartbeatResponse>, Status> {
        Ok(Response::new(pb::HeartbeatResponse::default()))
    }
    async fn negotiate(&self, req: Request<pb::Manifest>) -> Result<Response<pb::NegotiateResponse>, Status> {
        let already_current = self.0.lock().unwrap().already.contains(&req.get_ref().path);
        let missing = if already_current { vec![] } else { (0..req.get_ref().chunk_hashes.len() as u32).collect() };
        Ok(Response::new(pb::NegotiateResponse { missing, already_current }))
    }
    async fn upload_chunks(&self, req: Request<Streaming<pb::Chunk>>) -> Result<Response<pb::UploadSummary>, Status> {
        let mut stream = req.into_inner();
        let mut stored = 0;
        while stream.message().await?.is_some() {
            stored += 1;
        }
        Ok(Response::new(pb::UploadSummary { stored }))
    }
    async fn commit(&self, req: Request<pb::CommitRequest>) -> Result<Response<pb::CommitResponse>, Status> {
        let r = req.into_inner();
        let path = r.manifest.map(|m| m.path).unwrap_or_default();
        self.0.lock().unwrap().commits.push((path, r.renamed_from));
        Ok(Response::new(pb::CommitResponse { version_id: 1 }))
    }
    async fn mark_deleted(&self, req: Request<pb::DeleteRequest>) -> Result<Response<pb::DeleteResponse>, Status> {
        self.0.lock().unwrap().deleted.push(req.into_inner().path);
        Ok(Response::new(pb::DeleteResponse {}))
    }
    async fn get_update_info(&self, _: Request<pb::UpdateInfoRequest>) -> Result<Response<pb::UpdateInfo>, Status> {
        Err(Status::unimplemented("not under test"))
    }
    type DownloadUpdateStream = tokio_stream::Empty<Result<pb::UpdateChunk, Status>>;
    async fn download_update(
        &self,
        _: Request<pb::UpdateInfoRequest>,
    ) -> Result<Response<Self::DownloadUpdateStream>, Status> {
        Err(Status::unimplemented("not under test"))
    }
    async fn list_shared(&self, _: Request<pb::ListSharedRequest>) -> Result<Response<pb::ListSharedResponse>, Status> {
        Err(Status::unimplemented("not under test"))
    }
    async fn create_shared(
        &self,
        _: Request<pb::CreateSharedRequest>,
    ) -> Result<Response<pb::CreateSharedResponse>, Status> {
        Err(Status::unimplemented("not under test"))
    }
    type ListChangesStream = tokio_stream::Iter<std::vec::IntoIter<Result<pb::ChangeEvent, Status>>>;
    async fn list_changes(
        &self,
        _: Request<pb::ListChangesRequest>,
    ) -> Result<Response<Self::ListChangesStream>, Status> {
        use pb::change_event::Item;
        let calls = self.0.lock().unwrap();
        let mut events = vec![Ok(pb::ChangeEvent { item: Some(Item::Cursor(42)) })];
        if calls.complete {
            events.push(Ok(pb::ChangeEvent { item: Some(Item::Complete(true)) }));
        }
        for change in &calls.changes {
            events.push(Ok(pb::ChangeEvent { item: Some(Item::Change(change.clone())) }));
        }
        Ok(Response::new(tokio_stream::iter(events)))
    }
    type WatchSharedStream = tokio_stream::Empty<Result<pb::SharedChanged, Status>>;
    async fn watch_shared(
        &self,
        _: Request<pb::WatchSharedRequest>,
    ) -> Result<Response<Self::WatchSharedStream>, Status> {
        Err(Status::unimplemented("not under test"))
    }
    type DownloadSharedStream = tokio_stream::Iter<std::vec::IntoIter<Result<pb::FileData, Status>>>;
    async fn download_shared(
        &self,
        r: Request<pb::DownloadSharedRequest>,
    ) -> Result<Response<Self::DownloadSharedStream>, Status> {
        let data = self.0.lock().unwrap().files.get(&r.get_ref().path).cloned();
        let data = data.ok_or_else(|| Status::not_found("no such file"))?;
        Ok(Response::new(tokio_stream::iter(vec![Ok(pb::FileData { data })])))
    }
}

struct Rig {
    engine: Engine,
    cfg: Config,
    calls: Arc<Mutex<Calls>>,
    folder: tempfile::TempDir,
    _state: tempfile::TempDir,
}

impl Rig {
    async fn new() -> Self {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let url = serve(calls.clone()).await;
        let (folder, state) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let mut cfg = Config::new(url);
        cfg.folders.push(Folder { path: folder.path().to_path_buf(), name: "docs".into() });
        let engine = Engine::new(
            net::client(&cfg, "token", "instance").unwrap(),
            State::open(&state.path().join("state.db")).unwrap(),
            Arc::new(Limiter::new(0)),
            Arc::new(Mutex::new(Vec::new())),
            state.path().join("paused"),
        );
        Self { engine, cfg, calls, folder, _state: state }
    }

    fn put(&self, name: &str, content: &str) {
        fs::write(self.folder.path().join(name), content).unwrap();
    }

    fn files(&self, n: usize) {
        (0..n).for_each(|i| self.put(&format!("f{i}.txt"), &format!("content {i}")));
    }

    async fn cycle(&mut self) -> Report {
        self.engine.cycle(&self.cfg).await.expect("the cycle runs")
    }

    fn deleted(&self) -> Vec<String> {
        let mut d = self.calls.lock().unwrap().deleted.clone();
        d.sort();
        d
    }

    fn errors(&self) -> Vec<String> {
        self.engine.errors.lock().unwrap().clone()
    }
}

/// Every file under `dir` with its content, modification time and permissions.
fn snapshot(dir: &Path) -> BTreeMap<String, (Vec<u8>, std::time::SystemTime, bool)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let meta = fs::metadata(&path).unwrap();
                let rel = path.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/");
                out.insert(rel, (fs::read(&path).unwrap(), meta.modified().unwrap(), meta.permissions().readonly()));
            }
        }
    }
    out
}

#[tokio::test]
async fn new_changed_renamed_and_deleted_files_reach_the_master() {
    let mut rig = Rig::new().await;
    rig.files(3);
    let first = rig.cycle().await;
    assert_eq!((first.uploaded, first.renamed, first.deleted, first.failed), (3, 0, 0, 0));
    assert_eq!(rig.cycle().await, Report::default(), "nothing changed, nothing is sent");

    rig.put("f0.txt", "a different, longer content");
    assert_eq!(rig.cycle().await.uploaded, 1, "a changed file is sent again");

    fs::rename(rig.folder.path().join("f1.txt"), rig.folder.path().join("moved.txt")).unwrap();
    let renamed = rig.cycle().await;
    assert_eq!((renamed.renamed, renamed.uploaded, renamed.deleted), (1, 0, 0), "a rename is a rename");
    assert!(rig.calls.lock().unwrap().commits.contains(&("docs/moved.txt".into(), "docs/f1.txt".into())));

    fs::remove_file(rig.folder.path().join("f2.txt")).unwrap();
    assert_eq!(rig.cycle().await.deleted, 1);
    assert_eq!(rig.deleted(), ["docs/f2.txt"], "a file deleted on the device is deleted on the master");
}

#[tokio::test]
async fn a_folder_that_cannot_be_read_deletes_nothing() {
    let mut rig = Rig::new().await;
    rig.files(3);
    rig.cycle().await;
    rig.cfg.folders[0].path = rig.folder.path().join("unmounted");
    let report = rig.cycle().await;
    assert_eq!(report.deleted, 0);
    assert!(rig.deleted().is_empty(), "an unreadable folder is not an empty one");
    assert!(!rig.errors().is_empty(), "and it is reported");
}

#[tokio::test]
async fn the_client_never_changes_the_files_it_collects() {
    let mut rig = Rig::new().await;
    fs::create_dir(rig.folder.path().join("sub")).unwrap();
    rig.files(30);
    rig.put("sub/deep.txt", "deep");
    let before = snapshot(rig.folder.path());
    rig.cycle().await;
    fs::remove_file(rig.folder.path().join("f3.txt")).unwrap();
    let mut expected = before.clone();
    expected.remove("f3.txt");
    rig.cycle().await;
    assert_eq!(snapshot(rig.folder.path()), expected, "uploading and deleting on the master touch nothing here");
    // And when the master refuses everything (nothing listens any more) the files are as they were.
    rig.cfg.server = "http://127.0.0.1:1".into();
    rig.engine.client = net::client(&rig.cfg, "token", "instance").unwrap();
    rig.put("new.txt", "x");
    expected.insert("new.txt".into(), snapshot(rig.folder.path()).remove("new.txt").unwrap());
    assert!(rig.engine.cycle(&rig.cfg).await.is_err());
    assert_eq!(snapshot(rig.folder.path()), expected);
}

#[tokio::test]
async fn a_mass_deletion_is_held_and_sent_only_when_it_was_meant() {
    let mut rig = Rig::new().await;
    rig.files(40);
    rig.cycle().await;

    // An accident: three quarters of the folder is gone.
    for i in 0..30 {
        fs::remove_file(rig.folder.path().join(format!("f{i}.txt"))).unwrap();
    }
    assert_eq!(rig.cycle().await.deleted, 0);
    assert!(rig.deleted().is_empty(), "held back, not sent");
    assert_eq!(rig.engine.state.holds().unwrap()["docs"].0, 30);
    assert_eq!(rig.errors().len(), 1, "the owner is told, once");
    rig.cycle().await;
    assert_eq!(rig.errors().len(), 1, "not once per cycle");

    // The drive comes back: the hold ends and nothing was deleted.
    for i in 0..30 {
        rig.put(&format!("f{i}.txt"), &format!("content {i}"));
    }
    rig.cycle().await;
    assert!(rig.engine.state.holds().unwrap().is_empty());
    assert!(rig.deleted().is_empty());

    // Gone again, and this time it stays gone: after the hold time it is sent.
    for i in 0..30 {
        fs::remove_file(rig.folder.path().join(format!("f{i}.txt"))).unwrap();
    }
    rig.cycle().await;
    assert!(rig.deleted().is_empty());
    let long_ago = unix_now() - (rig.cfg.delete_hold_hours as i64 * 3600) - 1;
    rig.engine.state.set_holds(&HashMap::from([("docs".to_string(), (30, long_ago))])).unwrap();
    assert_eq!(rig.cycle().await.deleted, 30, "it was meant");
    assert_eq!(rig.deleted().len(), 30);
    assert!(rig.engine.state.holds().unwrap().is_empty());
}

#[tokio::test]
async fn held_deletions_can_be_released_at_once() {
    let mut rig = Rig::new().await;
    rig.files(40);
    rig.cycle().await;
    for i in 0..30 {
        fs::remove_file(rig.folder.path().join(format!("f{i}.txt"))).unwrap();
    }
    rig.cycle().await;
    assert!(rig.deleted().is_empty());
    rig.engine.state.request_release().unwrap();
    assert_eq!(rig.cycle().await.deleted, 30);
}

#[tokio::test]
async fn a_few_deletions_are_never_held() {
    let mut rig = Rig::new().await;
    rig.files(40);
    rig.cycle().await;
    for i in 0..5 {
        fs::remove_file(rig.folder.path().join(format!("f{i}.txt"))).unwrap();
    }
    assert_eq!(rig.cycle().await.deleted, 5);
}

#[tokio::test]
async fn nothing_is_sent_while_paused() {
    let mut rig = Rig::new().await;
    rig.files(2);
    fs::write(&rig.engine.pause_marker, b"").unwrap();
    assert!(matches!(rig.engine.cycle(&rig.cfg).await, Err(SyncError::Paused)));
    assert!(rig.calls.lock().unwrap().commits.is_empty());
}

/// Makes `path` unreadable for the length of the returned guard (a file another program holds open on Windows, no
/// permission elsewhere). Nothing to do for a user who can read everything (root).
#[cfg(windows)]
fn lock(path: &Path) -> Option<fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    fs::OpenOptions::new().read(true).share_mode(0).open(path).ok()
}

/// Gives the file its permissions back when it is dropped.
#[cfg(unix)]
struct Unreadable(PathBuf);

#[cfg(unix)]
impl Drop for Unreadable {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o644));
    }
}

#[cfg(unix)]
fn lock(path: &Path) -> Option<Unreadable> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o000)).ok()?;
    let guard = Unreadable(path.to_path_buf());
    fs::File::open(path).is_err().then_some(guard)
}

#[tokio::test]
async fn a_file_that_is_locked_or_unreadable_is_not_taken_for_deleted() {
    let mut rig = Rig::new().await;
    rig.files(3);
    rig.cycle().await;
    let guard = lock(&rig.folder.path().join("f0.txt"));
    #[cfg(windows)]
    assert!(guard.is_some(), "a file can be locked on Windows");
    if guard.is_none() {
        return; // this user can read everything
    }
    let report = rig.cycle().await;
    assert_eq!(report.deleted, 0);
    assert!(rig.deleted().is_empty(), "it is there, only not readable now");
    drop(guard);
    assert_eq!(rig.cycle().await, Report::default(), "and when it can be read again nothing changed");
}

#[cfg(unix)]
#[tokio::test]
async fn a_directory_that_cannot_be_walked_deletes_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let mut rig = Rig::new().await;
    fs::create_dir(rig.folder.path().join("sub")).unwrap();
    rig.put("sub/a.txt", "a");
    rig.put("top.txt", "t");
    rig.cycle().await;
    let sub = rig.folder.path().join("sub");
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::read_dir(&sub).is_ok() {
        fs::set_permissions(&sub, fs::Permissions::from_mode(0o755)).unwrap();
        return; // root
    }
    fs::remove_file(rig.folder.path().join("top.txt")).unwrap();
    rig.cycle().await;
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(rig.deleted().is_empty(), "nothing is deleted from a folder that could not be read completely");
}

#[tokio::test]
async fn a_rename_onto_a_path_the_master_already_has_still_removes_the_old_path() {
    let mut rig = Rig::new().await;
    rig.files(3);
    rig.cycle().await;
    rig.calls.lock().unwrap().already.push("docs/dup.txt".into());
    fs::rename(rig.folder.path().join("f1.txt"), rig.folder.path().join("dup.txt")).unwrap();
    rig.cycle().await;
    assert_eq!(rig.deleted(), ["docs/f1.txt"], "the old name is gone on the master too");
    assert_eq!(rig.cycle().await, Report::default());
}
