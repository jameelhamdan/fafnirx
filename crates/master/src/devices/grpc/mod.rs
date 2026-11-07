mod caller;

use self::caller::{address_of, authenticate, authenticate_user, caller_of, on_folder, place, Caller, Place};
use crate::devices::call::{internal, CallError};
use crate::files::ingest;
use crate::platform::db::{self, Db};
use crate::platform::store::Store;
use crate::repo::{alerts, devices, files, settings};
use fafnirx_common::pb::collector_server::Collector;
use fafnirx_common::pb::*;
use fafnirx_common::{chunk_count, chunk_hash, clean_remote_path, root_hash, to_hash, unix_now, CHUNK_SIZE, HASH_LEN};
use postgres::Client;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

pub struct Service {
    pub db: Db,
    pub store: Arc<Store>,
    /// The proxies whose word about a device's address is believed (`FAFNIRX_TRUSTED_PROXY`).
    pub trusted: crate::platform::proxy::Trusted,
    /// What the database announced (`{"topic":..,"key":..}`), the same stream the web app listens to.
    pub events: tokio::sync::broadcast::Sender<String>,
}

/// How long a burst of changes to a shared folder is allowed to gather before computers are told: an upload of a thousand
/// files is one message.
const WATCH_SETTLE: std::time::Duration = std::time::Duration::from_millis(500);

/// Whether a caller whose list of changes ends at `since` may have missed deletions, so that it needs the whole list: it asks
/// from the start, or the master has since removed deletions that are older than `since` (retention).
fn deletions_may_be_missing(c: &mut Client, since: i64) -> bool {
    since == 0 || since < settings::get_i64(c, settings::DELETIONS_KNOWN_FROM, 0)
}

/// The folder an announcement is about, if it says files were written or deleted (empty when it does not name one).
fn changed_folder(payload: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    (value["topic"] == "files").then(|| value["key"].as_str().unwrap_or_default().to_string())
}

struct Checked {
    path: String,
    size: u64,
    root: [u8; HASH_LEN],
    chunks: Vec<[u8; HASH_LEN]>,
}

fn invalid<E: std::fmt::Display>(e: E) -> Status {
    Status::invalid_argument(e.to_string())
}

fn check_manifest(m: &Manifest) -> Result<Checked, CallError> {
    let path = clean_remote_path(&m.path).map_err(invalid)?;
    if chunk_count(m.size) != m.chunk_hashes.len() as u64 {
        return Err(Status::invalid_argument("chunk count does not match size").into());
    }
    let chunks = m.chunk_hashes.iter().map(|h| to_hash(h)).collect::<Result<Vec<_>, _>>().map_err(invalid)?;
    let root = to_hash(&m.root_hash).map_err(invalid)?;
    if root_hash(&chunks, m.size) != root {
        return Err(Status::invalid_argument("root hash does not match chunk hashes").into());
    }
    Ok(Checked { path, size: m.size, root, chunks })
}

/// A file as a stream of messages of 256 KiB, each made by `wrap`.
fn stream_file<T: Send + 'static>(mut file: tokio::fs::File, wrap: fn(Vec<u8>) -> T) -> ReceiverStream<Result<T, Status>> {
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    tokio::spawn(async move {
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            match file.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(Ok(wrap(buf[..n].to_vec()))).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(internal(e))).await;
                    break;
                }
            }
        }
    });
    ReceiverStream::new(rx)
}

/// A computer that holds back deletions waits for a person: an alert on its device says so until it is dealt with, and it
/// closes by itself when the computer stops reporting any (or stops reporting at all, see the minute job).
fn alert_held_deletions(c: &mut Client, device: &str, states: &[devices::FolderState]) -> anyhow::Result<()> {
    let held: Vec<String> = states.iter().filter(|s| s.held > 0).map(|s| format!("{} ({} files)", s.name, s.held)).collect();
    if held.is_empty() {
        alerts::resolve(c, "deletions_held", device)?;
        return Ok(());
    }
    let message = format!(
        "Deletions are held back here for {}. `fafnirx release-deletes` applies them; \
         `fafnirx folder keep <name>` keeps the files instead.",
        held.join(", ")
    );
    alerts::raise(c, "deletions_held", device, &message)?;
    Ok(())
}

/// A text from a client as it is stored: without control characters, at most `max` characters.
pub(super) fn sanitize(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

impl Service {
    /// Who is calling.
    fn caller<T>(&self, req: &Request<T>) -> Result<Caller, Status> {
        caller_of(req, &self.trusted)
    }

    /// Runs `f` against the database for the place the call works on (see `place`), and the caller.
    async fn blocking_at<T, F>(&self, who: Caller, write: bool, f: F) -> Result<T, Status>
    where
        T: Send + 'static,
        F: FnOnce(&mut Client, Place, &Caller) -> Result<T, CallError> + Send + 'static,
    {
        self.blocking(move |c| {
            let at = place(c, &who, write)?;
            f(c, at, &who)
        })
        .await
    }

    /// Runs `f` against the database. What `f` fails with is told to the client as `CallError` says; a failure to get a
    /// connection is internal.
    async fn blocking<T, F>(&self, f: F) -> Result<T, Status>
    where
        T: Send + 'static,
        F: FnOnce(&mut Client) -> Result<T, CallError> + Send + 'static,
    {
        db::with(&self.db, move |c| Ok(f(c))).await.map_err(internal)?.map_err(Status::from)
    }
}

#[tonic::async_trait]
impl Collector for Service {
    async fn identify(&self, req: Request<IdentifyRequest>) -> Result<Response<IdentifyResponse>, Status> {
        let who = self.caller(&req)?;
        let master_id = self
            .blocking(move |c| {
                authenticate(c, &who)?;
                Ok(settings::get(c, "master_id")?.unwrap_or_default())
            })
            .await?;
        Ok(Response::new(IdentifyResponse { master_id }))
    }

    async fn heartbeat(&self, req: Request<HeartbeatRequest>) -> Result<Response<HeartbeatResponse>, Status> {
        let who = self.caller(&req)?;
        let ip = address_of(&req, &self.trusted);
        let hb = req.into_inner();
        let name = self
            .blocking(move |c| {
                let (dev, owner) = authenticate_user(c, &who)?;
                let ip = ip.as_deref();
                let info = devices::Heartbeat {
                    hostname: &sanitize(&hb.hostname, 100),
                    os: &sanitize(&hb.os, 20),
                    ip,
                    client_version: &sanitize(&hb.client_version, 30),
                    disk_total: hb.disk_total as i64,
                    disk_free: hb.disk_free as i64,
                };
                devices::record_heartbeat(c, dev.id, &info, unix_now())?;
                // Only folders this computer's user can use: a name is the caller's word, and the owner of a folder is shown it.
                let usable: Vec<String> = devices::folders(c, owner)?.into_iter().map(|f| f.folder).collect();
                let states: Vec<devices::FolderState> = hb
                    .folders
                    .iter()
                    .filter(|s| usable.contains(&s.name))
                    .take(50)
                    .map(|s| devices::FolderState {
                        name: sanitize(&s.name, 130),
                        files: s.files as i64,
                        bytes: s.bytes as i64,
                        last_cycle: s.last_cycle,
                        last_error: sanitize(&s.last_error, 300),
                        held: i64::from(s.held),
                        failing: i64::from(s.failing),
                        mode: sanitize(&s.mode, 10),
                        updated_at: 0,
                    })
                    .collect();
                devices::save_folder_state(c, dev.id, &states, unix_now())?;
                alert_held_deletions(c, &dev.name, &states)?;
                // A conflict that has not come back for ten minutes is over.
                alerts::resolve_if_quiet(c, "device_conflict", &dev.name, unix_now() - 600)?;
                if hb.phase.is_empty() || hb.phase == "idle" {
                    // Going from busy to idle means the sync is done: the totals shown for the device are brought up to date.
                    if devices::clear_progress(c, dev.id)? {
                        crate::devices::totals::refresh(c, dev.id)?;
                    }
                } else {
                    let progress = devices::Progress {
                        phase: &sanitize(&hb.phase, 20),
                        current_file: &sanitize(&hb.current_file, 300),
                        files_done: hb.files_done as i64,
                        files_total: hb.files_total as i64,
                        bytes_done: hb.bytes_done as i64,
                        bytes_total: hb.bytes_total as i64,
                        speed_bps: hb.speed_bps as i64,
                    };
                    devices::save_progress(c, dev.id, &progress, unix_now())?;
                }
                db::notify(c, db::Topic::Devices, &dev.name)?;
                if !hb.errors.is_empty() {
                    let msg = hb.errors.iter().take(5).map(|e| sanitize(e, 300)).collect::<Vec<_>>().join("\n");
                    alerts::upsert(c, "upload_error", &dev.name, &msg)?;
                }
                alerts::resolve(c, "device_offline", &dev.name)?;
                Ok(dev.name)
            })
            .await?;
        Ok(Response::new(HeartbeatResponse { device_name: name, server_time: unix_now() }))
    }

    async fn negotiate(&self, req: Request<Manifest>) -> Result<Response<NegotiateResponse>, Status> {
        let who = self.caller(&req)?;
        let m = check_manifest(req.get_ref())?;
        let store = self.store.clone();
        self.blocking_at(who, true, move |c, at, who| {
            let (dev, path) = (&at.device, m.path.clone());
            let current = files::live_content(c, dev.id, &path)?.map(|(root, _)| root);
            // A backup device is told it is up to date for a path deleted in the web app, whatever it holds now. A shared
            // folder is not: its computers delete such a file themselves, so a file there is a new one.
            let hidden = who.folder.is_none() && files::deleted_in_web_app(c, dev.id, &path)?;
            if current == Some(m.root) || hidden {
                return Ok(Response::new(NegotiateResponse { missing: vec![], already_current: true }));
            }
            // The device already has this content under another path (a rename or a copy): the commit
            // takes it from there, nothing needs to be sent.
            if files::live_path_with_root(c, dev.id, &m.root, None)?.is_some() {
                return Ok(Response::new(NegotiateResponse { missing: vec![], already_current: false }));
            }
            let missing = store.missing_chunks(dev.id, &m.root, &m.chunks);
            Ok(Response::new(NegotiateResponse { missing, already_current: false }))
        })
        .await
    }

    async fn upload_chunks(&self, req: Request<Streaming<Chunk>>) -> Result<Response<UploadSummary>, Status> {
        let who = self.caller(&req)?;
        let device_id = self.blocking_at(who, true, |_, at, _| Ok(at.device.id)).await?;
        let mut stream = req.into_inner();
        let mut stored = 0u32;
        while let Some(chunk) = stream.message().await? {
            let hash = to_hash(&chunk.hash).map_err(invalid)?;
            let root = to_hash(&chunk.root_hash).map_err(invalid)?;
            if chunk.data.is_empty() || chunk.data.len() > CHUNK_SIZE {
                return Err(Status::invalid_argument("bad chunk size"));
            }
            if chunk_hash(&chunk.data) != hash {
                return Err(Status::data_loss("chunk does not match its hash"));
            }
            let store = self.store.clone();
            tokio::task::spawn_blocking(move || store.write_chunk(device_id, &root, chunk.index, &chunk.data))
                .await
                .map_err(internal)?
                .map_err(internal)?;
            stored += 1;
        }
        Ok(Response::new(UploadSummary { stored }))
    }

    async fn commit(&self, req: Request<CommitRequest>) -> Result<Response<CommitResponse>, Status> {
        let who = self.caller(&req)?;
        let r = req.into_inner();
        let m = check_manifest(r.manifest.as_ref().ok_or_else(|| Status::invalid_argument("missing manifest"))?)?;
        let renamed_from = if r.renamed_from.is_empty() {
            None
        } else {
            Some(clean_remote_path(&r.renamed_from).map_err(invalid)?)
        };
        let store = self.store.clone();
        let version_id = self
            .blocking_at(who, true, move |c, at, who| {
                let (dev, path) = (&at.device, m.path.clone());
                if who.folder.is_none() && files::deleted_in_web_app(c, dev.id, &path)? {
                    return Ok(0);
                }
                if at.folder {
                    // A folder takes the rules of the most limited computer for what is synced into it.
                    ingest::check_names(c, dev, &path, renamed_from.as_deref(), true)?;
                }
                // Content the device already has under another path is taken from there; otherwise the
                // upload must be complete and intact.
                let staged = match files::live_path_with_root(c, dev.id, &m.root, renamed_from.as_deref())? {
                    Some(from) => store
                        .copy_live_to_temp(&dev.name, &from, &m.root)
                        .map_err(|e| Status::failed_precondition(format!("the file this one is copied from changed: {e:#}")))?,
                    None => store
                        .finish_upload(dev.id, &m.root, m.size)
                        .map_err(|e| Status::failed_precondition(format!("the upload of this file is incomplete: {e:#}")))?,
                };
                ingest::commit_version(
                    c,
                    &store,
                    &staged,
                    &ingest::NewVersion {
                        device: dev,
                        path: &path,
                        root: m.root,
                        size: m.size,
                        mtime: r.mtime,
                        mode: r.mode,
                        renamed_from: renamed_from.as_deref(),
                    },
                )
                .map_err(CallError::from)
            })
            .await?;
        Ok(Response::new(CommitResponse { version_id }))
    }

    async fn mark_deleted(&self, req: Request<DeleteRequest>) -> Result<Response<DeleteResponse>, Status> {
        let who = self.caller(&req)?;
        let path = clean_remote_path(&req.get_ref().path).map_err(invalid)?;
        let store = self.store.clone();
        self.blocking_at(who, true, move |c, at, _| {
            ingest::mark_deleted(c, &store, &at.device, &path, false)?;
            Ok(())
        })
        .await?;
        Ok(Response::new(DeleteResponse {}))
    }

    async fn list_folders(&self, req: Request<ListFoldersRequest>) -> Result<Response<ListFoldersResponse>, Status> {
        let who = self.caller(&req)?;
        let (names, mine, ids, readonly) = self
            .blocking(move |c| {
                let (_, owner) = authenticate_user(c, &who)?;
                let all = devices::folders(c, owner)?;
                let mine = all.iter().filter(|f| f.mine).map(|f| f.folder.clone()).collect::<Vec<_>>();
                let ids = all.iter().map(|f| f.public_id.to_string()).collect();
                let readonly = all.iter().filter(|f| !f.can_write).map(|f| f.folder.clone()).collect::<Vec<_>>();
                Ok((all.into_iter().map(|f| f.folder).collect::<Vec<_>>(), mine, ids, readonly))
            })
            .await?;
        Ok(Response::new(ListFoldersResponse { names, mine, ids, readonly }))
    }

    async fn create_folder(&self, req: Request<CreateFolderRequest>) -> Result<Response<CreateFolderResponse>, Status> {
        let who = self.caller(&req)?;
        let name = req.into_inner().name;
        let made = self
            .blocking(move |c| {
                let (_, owner) = authenticate_user(c, &who)?;
                // Making one that is there already is not an error.
                if devices::folder(c, owner, &name)?.is_some_and(|f| f.mine) {
                    return Ok(None);
                }
                Ok(Some(super::folders::make(c, owner, &name)?))
            })
            .await?;
        if let Some(folder) = made {
            // The folder exists from the start so that a network share shows it.
            let _ = std::fs::create_dir_all(self.store.live_dir(&folder));
        }
        Ok(Response::new(CreateFolderResponse {}))
    }

    type ListChangesStream = tokio_stream::Iter<std::vec::IntoIter<Result<ChangeEvent, Status>>>;

    async fn list_changes(&self, req: Request<ListChangesRequest>) -> Result<Response<Self::ListChangesStream>, Status> {
        let who = self.caller(&req)?;
        let r = req.into_inner();
        let who = on_folder(who, r.folder)?;
        // Taken before the query and rewound a little, so that a write racing with it is listed by the next call at the latest.
        let cursor = (unix_now() - 2).max(0);
        let since = r.since.max(0);
        let (changes, complete) = self
            .blocking_at(who, false, move |c, at, _| {
                let complete = deletions_may_be_missing(c, since);
                // A caller that may have missed deletions is sent everything that is live, to compare with what it has.
                let changes = files::changes_since(c, at.device.id, if complete { 0 } else { since })?;
                Ok((changes, complete))
            })
            .await?;
        let event = |item| Ok(ChangeEvent { item: Some(item) });
        let mut events = vec![event(change_event::Item::Cursor(cursor))];
        if complete {
            events.push(event(change_event::Item::Complete(true)));
        }
        events.extend(changes.into_iter().map(|ch| {
            let change = Change {
                path: ch.path,
                deleted: ch.root.is_none(),
                root_hash: ch.root.map(|r| r.to_vec()).unwrap_or_default(),
                size: ch.size,
                mtime: ch.mtime,
                mode: ch.mode,
                from_web: ch.from_web,
            };
            event(change_event::Item::Change(change))
        }));
        Ok(Response::new(tokio_stream::iter(events)))
    }

    type WatchFolderStream = ReceiverStream<Result<FolderChanged, Status>>;

    async fn watch_folder(&self, req: Request<WatchFolderRequest>) -> Result<Response<Self::WatchFolderStream>, Status> {
        let who = self.caller(&req)?;
        let asked = req.into_inner().folders;
        // The names the caller may use, with the folder each one is.
        let watched: Vec<(String, String)> = self
            .blocking(move |c| {
                let (_, user) = authenticate_user(c, &who)?;
                let mut found = Vec::new();
                for name in asked {
                    // The caller may name a folder by its stable id; the answer names it the way it was asked.
                    if let Some(f) = devices::folder(c, user, &name)? {
                        found.push((name, f.folder));
                    }
                }
                Ok(found)
            })
            .await?;
        // Subscribed before the first message, so that nothing between the two is missed.
        let mut announced = self.events.subscribe();
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            for (name, _) in &watched {
                if tx.send(Ok(FolderChanged { folder: name.clone() })).await.is_err() {
                    return;
                }
            }
            loop {
                let mut folders = std::collections::HashSet::new();
                match announced.recv().await {
                    Ok(payload) => folders.extend(changed_folder(&payload)),
                    // Fell behind: every folder may have changed.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => folders.extend(watched.iter().map(|(_, folder)| folder.clone())),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
                // An announcement without a folder (a delete of several files) may be about any of them.
                let about = |folders: &std::collections::HashSet<String>, folder: &String| folders.contains(folder) || folders.contains("");
                if !watched.iter().any(|(_, folder)| about(&folders, folder)) {
                    continue;
                }
                // The rest of the burst, so that it is one message.
                tokio::time::sleep(WATCH_SETTLE).await;
                while let Ok(payload) = announced.try_recv() {
                    folders.extend(changed_folder(&payload));
                }
                for (name, folder) in &watched {
                    if about(&folders, folder) && tx.send(Ok(FolderChanged { folder: name.clone() })).await.is_err() {
                        return;
                    }
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type DownloadFolderStream = ReceiverStream<Result<FileData, Status>>;

    async fn download_folder(&self, req: Request<DownloadFolderRequest>) -> Result<Response<Self::DownloadFolderStream>, Status> {
        let who = self.caller(&req)?;
        let r = req.into_inner();
        let who = on_folder(who, r.folder)?;
        let path = clean_remote_path(&r.path).map_err(invalid)?;
        let store = self.store.clone();
        let file_path = {
            let path = path.clone();
            self.blocking_at(who, false, move |c, at, _| {
                if files::live_content(c, at.device.id, &path)?.is_none() {
                    return Err(Status::not_found("no such file").into());
                }
                Ok(store.live_path(&at.device.name, &path)?)
            })
            .await?
        };
        let file = tokio::fs::File::open(&file_path).await.map_err(|_| Status::not_found("the file is not stored"))?;
        Ok(Response::new(stream_file(file, |data| FileData { data })))
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/devices/grpc.rs"]
mod tests;
