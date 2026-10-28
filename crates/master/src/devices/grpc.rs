use crate::files::ingest;
use crate::platform::db::{self, Db, Device};
use crate::platform::store::Store;
use crate::repo::{alerts, devices, files, settings};
use fafnirx_common::pb::collector_server::Collector;
use fafnirx_common::pb::*;
use fafnirx_common::{chunk_count, chunk_hash, clean_remote_path, root_hash, to_hash, unix_now, CHUNK_SIZE, HASH_LEN};
use postgres::Client;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

pub struct Service {
    pub db: Db,
    pub store: Arc<Store>,
    pub updates_dir: PathBuf,
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

fn bearer<T>(req: &Request<T>) -> Result<String, Status> {
    req.metadata()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned)
        .ok_or_else(|| Status::unauthenticated("missing device token"))
}

/// Who is calling: the token (which device) and the instance (which installation of the client is using it).
#[derive(Clone)]
pub struct Caller {
    token: String,
    instance: String,
    ip: Option<String>,
    /// The shared folder the call acts on instead of the device, named in the `x-fafnirx-shared` metadata.
    shared: Option<String>,
}

/// The instance of a caller that does not name one: all such callers count as one computer.
const UNNAMED_INSTANCE: &str = "unnamed";

/// The address a request comes from: the connection's, or behind a trusted proxy the one the proxy saw.
fn address_of<T>(req: &Request<T>, trusted: &crate::platform::proxy::Trusted) -> Option<String> {
    let forwarded = req.metadata().get("x-forwarded-for").and_then(|v| v.to_str().ok());
    req.remote_addr().map(|a| trusted.client_ip(a.ip(), forwarded).to_string())
}

fn caller_of<T>(req: &Request<T>, trusted: &crate::platform::proxy::Trusted) -> Result<Caller, Status> {
    let instance = req
        .metadata()
        .get("x-fafnirx-instance")
        .and_then(|v| v.to_str().ok())
        .map(|v| clip(v, 64))
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| UNNAMED_INSTANCE.into());
    let shared = req
        .metadata()
        .get("x-fafnirx-shared")
        .and_then(|v| v.to_str().ok())
        .map(|v| clip(v, 130))
        .filter(|v| !v.is_empty());
    Ok(Caller { token: bearer(req)?, instance, ip: address_of(req, trusted), shared })
}

/// What the call works on: the device, or the shared folder it names. The owner of a shared folder reaches it through any
/// of their devices; another user only when the owner added them, and to change it only when they may write.
fn acting(conn: &mut Client, who: &Caller, write: bool) -> Result<Device, Status> {
    let (dev, user) = authenticate_user(conn, who)?;
    let Some(key) = &who.shared else { return Ok(dev) };
    let found = devices::shared_folder(conn, user, key)
        .map_err(internal)?
        .ok_or_else(|| Status::not_found(format!("no shared folder called \"{key}\"")))?;
    if write && !found.can_write {
        // Not PermissionDenied, which is about the device (in use by another computer) and stops the whole client.
        return Err(Status::failed_precondition(format!("\"{key}\" is shared with you to read only")));
    }
    Ok(Device { id: found.id, name: found.folder })
}

/// How long a device that is not heard from keeps its credentials to itself. After that another computer using the
/// same token takes the device over (a reinstalled or replaced computer), and the first one is the one refused.
fn lease_secs() -> i64 {
    std::env::var("FAFNIRX_INSTANCE_LEASE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(90)
}

/// Finds the device of the token and makes sure the caller is the one computer using it.
///
/// A device is used by one installation of the client at a time: the first to connect holds it, and while it is
/// heard from (heartbeats, and everything it uploads, set `last_seen`) another is refused and an alert is raised for the
/// owner, so that two computers with one token cannot send two views of the same folders. One decision, in one
/// statement, so two computers connecting at once cannot both win.
fn authenticate_user(conn: &mut Client, who: &Caller) -> Result<(Device, i64), Status> {
    // The device is its folder (`<user>/<name>`). A device of a disabled user is refused like a revoked one.
    let found = devices::find_by_token(conn, &db::hash_token(&who.token))
        .map_err(internal)?
        .ok_or_else(|| Status::unauthenticated("unknown or revoked device token"))?;
    let (id, folder, user) = (found.id, found.folder, found.user_id);
    let holder = found.instance;
    if holder.as_deref() == Some(who.instance.as_str()) {
        return Ok((Device { id, name: folder }, user));
    }
    let now = unix_now();
    if devices::take_over(conn, id, &who.instance, now, now - lease_secs()).map_err(internal)? {
        if holder.is_some() {
            tracing::info!("{folder}: taken over by another computer, the first one went quiet");
            let _ = alerts::resolve(conn, "device_conflict", &folder);
        }
        return Ok((Device { id, name: folder }, user));
    }
    let (host, ip) = (found.hostname, found.ip);
    let holder_is = match (host, ip) {
        (Some(h), Some(i)) => format!("{h}, {i}"),
        (Some(h), None) => h,
        (None, Some(i)) => i,
        (None, None) => "another computer".into(),
    };
    let caller_is = who.ip.as_deref().unwrap_or("unknown address");
    let _ = alerts::upsert(
        conn,
        "device_conflict",
        &folder,
        &format!(
            "A second computer ({caller_is}) is using the credentials of this device while {holder_is} is connected. \
             It was refused. If the credentials leaked, rotate the token; to move the device to the other computer, release it."
        ),
    );
    Err(Status::permission_denied(format!(
        "device in use: another computer ({holder_is}) is connected as this device. This one sends nothing until \
         that one goes quiet or the device is released in the web app"
    )))
}

fn authenticate(conn: &mut Client, who: &Caller) -> Result<Device, Status> {
    Ok(authenticate_user(conn, who)?.0)
}

fn internal<E: std::fmt::Display>(e: E) -> Status {
    tracing::error!("internal error: {e}");
    Status::internal("internal error")
}

fn invalid<E: std::fmt::Display>(e: E) -> Status {
    Status::invalid_argument(e.to_string())
}

fn check_manifest(m: &Manifest) -> Result<Checked, Status> {
    let path = clean_remote_path(&m.path).map_err(invalid)?;
    if chunk_count(m.size) != m.chunk_hashes.len() as u64 {
        return Err(Status::invalid_argument("chunk count does not match size"));
    }
    let chunks = m.chunk_hashes.iter().map(|h| to_hash(h)).collect::<Result<Vec<_>, _>>().map_err(invalid)?;
    let root = to_hash(&m.root_hash).map_err(invalid)?;
    if root_hash(&chunks, m.size) != root {
        return Err(Status::invalid_argument("root hash does not match chunk hashes"));
    }
    Ok(Checked { path, size: m.size, root, chunks })
}

/// A file as a stream of messages of 256 KiB, each made by `wrap`.
fn stream_file<T: Send + 'static>(
    mut file: tokio::fs::File,
    wrap: fn(Vec<u8>) -> T,
) -> ReceiverStream<Result<T, Status>> {
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

/// The caller acting on the shared folder a request names (the calls that name it in the message, not in the metadata).
fn on_shared_folder(who: Caller, shared: String) -> Result<Caller, Status> {
    if shared.is_empty() {
        return Err(Status::invalid_argument("name the shared folder"));
    }
    Ok(Caller { shared: Some(shared), ..who })
}

/// A computer that holds back deletions waits for a person: an alert on its device says so until it is dealt with, and it
/// closes by itself when the computer stops reporting any (or stops reporting at all, see the minute job).
fn alert_held_deletions(c: &mut Client, device: &str, shared: &[devices::SharedState]) -> anyhow::Result<()> {
    let held: Vec<String> =
        shared.iter().filter(|s| s.held > 0).map(|s| format!("{} ({} files)", s.name, s.held)).collect();
    if held.is_empty() {
        alerts::resolve(c, "deletions_held", device)?;
        return Ok(());
    }
    let message = format!(
        "Deletions are held back here for {}. `fafnirx release-deletes` applies them; \
         `fafnirx shared keep <name>` keeps the files instead.",
        held.join(", ")
    );
    alerts::raise(c, "deletions_held", device, &message)?;
    Ok(())
}

fn clip(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect()
}

impl Service {
    /// Runs `f` against the database. A `Status` returned from `f` (via `?`) reaches the client as is;
    /// any other error is logged and reported as internal.
    async fn blocking<T, F>(&self, f: F) -> Result<T, Status>
    where
        T: Send + 'static,
        F: FnOnce(&mut Client) -> anyhow::Result<T> + Send + 'static,
    {
        db::with(&self.db, f).await.map_err(|e| match e.downcast::<Status>() {
            Ok(status) => status,
            // A refusal the web app would also give (a shared folder cannot hold that path): the client is told which.
            Err(e) => match e.downcast::<crate::web::ApiError>() {
                Ok(refused) if refused.0 == axum::http::StatusCode::CONFLICT => Status::already_exists(refused.1),
                Ok(refused) if refused.0 == axum::http::StatusCode::BAD_REQUEST => Status::invalid_argument(refused.1),
                Ok(other) => internal(other.1),
                Err(e) => internal(e),
            },
        })
    }

    /// Authenticates a device and returns its id.
    async fn check_token(&self, who: Caller) -> Result<i64, Status> {
        self.blocking(move |c| Ok(authenticate(c, &who)?.id)).await
    }

    fn update_dir(&self, platform: &str) -> Result<PathBuf, Status> {
        if platform.is_empty() || !platform.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return Err(Status::invalid_argument("invalid platform"));
        }
        Ok(self.updates_dir.join(platform))
    }
}

#[tonic::async_trait]
impl Collector for Service {
    async fn identify(&self, req: Request<IdentifyRequest>) -> Result<Response<IdentifyResponse>, Status> {
        let who = caller_of(&req, &self.trusted)?;
        let master_id = self
            .blocking(move |c| {
                authenticate(c, &who)?;
                Ok(settings::get(c, "master_id")?.unwrap_or_default())
            })
            .await?;
        Ok(Response::new(IdentifyResponse { master_id }))
    }

    async fn heartbeat(&self, req: Request<HeartbeatRequest>) -> Result<Response<HeartbeatResponse>, Status> {
        let who = caller_of(&req, &self.trusted)?;
        let ip = address_of(&req, &self.trusted);
        let hb = req.into_inner();
        let name = self
            .blocking(move |c| {
                let (dev, owner) = authenticate_user(c, &who)?;
                let ip = ip.as_deref();
                let info = devices::Heartbeat {
                    hostname: &clip(&hb.hostname, 100),
                    os: &clip(&hb.os, 20),
                    ip,
                    client_version: &clip(&hb.client_version, 30),
                    disk_total: hb.disk_total as i64,
                    disk_free: hb.disk_free as i64,
                };
                devices::record_heartbeat(c, dev.id, &info, unix_now())?;
                // Only folders this computer's user can use: a name is the caller's word, and the owner of a folder is shown it.
                let usable: Vec<String> = devices::shared_folders(c, owner)?.into_iter().map(|s| s.folder).collect();
                let shared_state: Vec<devices::SharedState> = hb
                    .shared
                    .iter()
                    .filter(|s| usable.contains(&s.name))
                    .take(50)
                    .map(|s| devices::SharedState {
                        name: clip(&s.name, 130),
                        files: s.files as i64,
                        bytes: s.bytes as i64,
                        last_cycle: s.last_cycle,
                        last_error: clip(&s.last_error, 300),
                        held: i64::from(s.held),
                        failing: i64::from(s.failing),
                        updated_at: 0,
                    })
                    .collect();
                devices::save_shared_state(c, dev.id, &shared_state, unix_now())?;
                alert_held_deletions(c, &dev.name, &shared_state)?;
                // A conflict that has not come back for ten minutes is over.
                if alerts::resolve_if_quiet(c, "device_conflict", &dev.name, unix_now() - 600)? {
                    db::notify(c, "alerts", &dev.name)?;
                }
                if hb.phase.is_empty() || hb.phase == "idle" {
                    // Going from busy to idle means the sync is done: the totals shown for the device are brought up to date.
                    if devices::clear_progress(c, dev.id)? {
                        crate::devices::totals::refresh(c, dev.id)?;
                    }
                } else {
                    let progress = devices::Progress {
                        phase: &clip(&hb.phase, 20),
                        current_file: &clip(&hb.current_file, 300),
                        files_done: hb.files_done as i64,
                        files_total: hb.files_total as i64,
                        bytes_done: hb.bytes_done as i64,
                        bytes_total: hb.bytes_total as i64,
                        speed_bps: hb.speed_bps as i64,
                    };
                    devices::save_progress(c, dev.id, &progress, unix_now())?;
                }
                db::notify(c, "devices", &dev.name)?;
                if !hb.errors.is_empty() {
                    let msg = hb.errors.iter().take(5).map(|e| clip(e, 300)).collect::<Vec<_>>().join("\n");
                    alerts::upsert(c, "upload_error", &dev.name, &msg)?;
                }
                alerts::resolve(c, "device_offline", &dev.name)?;
                Ok(dev.name)
            })
            .await?;
        Ok(Response::new(HeartbeatResponse { device_name: name, server_time: unix_now() }))
    }

    async fn negotiate(&self, req: Request<Manifest>) -> Result<Response<NegotiateResponse>, Status> {
        let who = caller_of(&req, &self.trusted)?;
        let m = check_manifest(req.get_ref())?;
        let store = self.store.clone();
        self.blocking(move |c| {
            let dev = acting(c, &who, true)?;
            let current = files::live_content(c, dev.id, &m.path)?.map(|(root, _)| root);
            // A backup device is told it is up to date for a path deleted in the web app, whatever it holds now. A shared
            // folder is not: its computers delete such a file themselves, so a file there is a new one.
            let hidden = who.shared.is_none() && files::deleted_in_web_app(c, dev.id, &m.path)?;
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
        let who = caller_of(&req, &self.trusted)?;
        let device_id = self.blocking(move |c| Ok(acting(c, &who, true)?.id)).await?;
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
        let who = caller_of(&req, &self.trusted)?;
        let r = req.into_inner();
        let m = check_manifest(r.manifest.as_ref().ok_or_else(|| Status::invalid_argument("missing manifest"))?)?;
        let renamed_from =
            if r.renamed_from.is_empty() { None } else { Some(clean_remote_path(&r.renamed_from).map_err(invalid)?) };
        let store = self.store.clone();
        let version_id = self
            .blocking(move |c| {
                let dev = acting(c, &who, true)?;
                if who.shared.is_none() && files::deleted_in_web_app(c, dev.id, &m.path)? {
                    return Ok(0);
                }
                // Content the device already has under another path is taken from there; otherwise the
                // upload must be complete and intact.
                let staged = match files::live_path_with_root(c, dev.id, &m.root, renamed_from.as_deref())? {
                    Some(from) => store.copy_live_to_temp(&dev.name, &from, &m.root).map_err(|e| {
                        Status::failed_precondition(format!("the file this one is copied from changed: {e:#}"))
                    })?,
                    None => store.finish_upload(dev.id, &m.root, m.size).map_err(|e| {
                        Status::failed_precondition(format!("the upload of this file is incomplete: {e:#}"))
                    })?,
                };
                ingest::commit_version(
                    c,
                    &store,
                    &staged,
                    &ingest::NewVersion {
                        device: &dev,
                        path: &m.path,
                        root: m.root,
                        size: m.size,
                        mtime: r.mtime,
                        mode: r.mode,
                        renamed_from: renamed_from.as_deref(),
                    },
                )
            })
            .await?;
        Ok(Response::new(CommitResponse { version_id }))
    }

    async fn mark_deleted(&self, req: Request<DeleteRequest>) -> Result<Response<DeleteResponse>, Status> {
        let who = caller_of(&req, &self.trusted)?;
        let path = clean_remote_path(&req.get_ref().path).map_err(invalid)?;
        let store = self.store.clone();
        self.blocking(move |c| {
            let dev = acting(c, &who, true)?;
            ingest::mark_deleted(c, &store, &dev, &path, false)?;
            Ok(())
        })
        .await?;
        Ok(Response::new(DeleteResponse {}))
    }

    async fn get_update_info(&self, req: Request<UpdateInfoRequest>) -> Result<Response<UpdateInfo>, Status> {
        let who = caller_of(&req, &self.trusted)?;
        self.check_token(who).await?;
        let dir = self.update_dir(&req.get_ref().platform)?;
        let (Ok(version), Ok(signature)) =
            (std::fs::read_to_string(dir.join("version")), std::fs::read_to_string(dir.join("signature")))
        else {
            return Ok(Response::new(UpdateInfo { available: false, ..Default::default() }));
        };
        let bin = dir.join("bin");
        let data = tokio::fs::read(&bin).await.map_err(|_| Status::not_found("update binary missing"))?;
        let sig = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, signature.trim())
            .map_err(|_| Status::internal("bad signature file"))?;
        Ok(Response::new(UpdateInfo {
            available: true,
            version: version.trim().to_string(),
            sha256: Sha256::digest(&data).to_vec(),
            signature: sig,
            size: data.len() as u64,
        }))
    }

    type DownloadUpdateStream = ReceiverStream<Result<UpdateChunk, Status>>;

    async fn download_update(
        &self,
        req: Request<UpdateInfoRequest>,
    ) -> Result<Response<Self::DownloadUpdateStream>, Status> {
        let who = caller_of(&req, &self.trusted)?;
        self.check_token(who).await?;
        let bin = self.update_dir(&req.get_ref().platform)?.join("bin");
        let file = tokio::fs::File::open(&bin).await.map_err(|_| Status::not_found("no update"))?;
        Ok(Response::new(stream_file(file, |data| UpdateChunk { data })))
    }

    async fn list_shared(&self, req: Request<ListSharedRequest>) -> Result<Response<ListSharedResponse>, Status> {
        let who = caller_of(&req, &self.trusted)?;
        let (names, mine) = self
            .blocking(move |c| {
                let (_, owner) = authenticate_user(c, &who)?;
                let all = devices::shared_folders(c, owner)?;
                let mine = all.iter().filter(|s| s.mine).map(|s| s.folder.clone()).collect::<Vec<_>>();
                Ok((all.into_iter().map(|s| s.folder).collect::<Vec<_>>(), mine))
            })
            .await?;
        Ok(Response::new(ListSharedResponse { names, mine }))
    }

    async fn create_shared(&self, req: Request<CreateSharedRequest>) -> Result<Response<CreateSharedResponse>, Status> {
        let who = caller_of(&req, &self.trusted)?;
        let name = req.into_inner().name;
        let made = self
            .blocking(move |c| {
                let (_, owner) = authenticate_user(c, &who)?;
                // Making one that is there already is not an error.
                if devices::shared_folder(c, owner, &name)?.is_some_and(|s| s.mine) {
                    return Ok(None);
                }
                Ok(Some(super::shared::make(c, owner, &name)?))
            })
            .await?;
        if let Some(folder) = made {
            // The folder exists from the start so that a network share shows it.
            let _ = std::fs::create_dir_all(self.store.live_dir(&folder));
        }
        Ok(Response::new(CreateSharedResponse {}))
    }

    type ListChangesStream = tokio_stream::Iter<std::vec::IntoIter<Result<ChangeEvent, Status>>>;

    async fn list_changes(
        &self,
        req: Request<ListChangesRequest>,
    ) -> Result<Response<Self::ListChangesStream>, Status> {
        let who = caller_of(&req, &self.trusted)?;
        let r = req.into_inner();
        let who = on_shared_folder(who, r.shared)?;
        // Taken before the query and rewound a little, so that a write racing with it is listed by the next call at the latest.
        let cursor = (unix_now() - 2).max(0);
        let since = r.since.max(0);
        let (changes, complete) = self
            .blocking(move |c| {
                let dev = acting(c, &who, false)?;
                let complete = deletions_may_be_missing(c, since);
                // A caller that may have missed deletions is sent everything that is live, to compare with what it has.
                Ok((files::changes_since(c, dev.id, if complete { 0 } else { since })?, complete))
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

    type WatchSharedStream = ReceiverStream<Result<SharedChanged, Status>>;

    async fn watch_shared(
        &self,
        req: Request<WatchSharedRequest>,
    ) -> Result<Response<Self::WatchSharedStream>, Status> {
        let who = caller_of(&req, &self.trusted)?;
        let asked = req.into_inner().shared;
        // The names the caller may use, with the folder each one is.
        let watched: Vec<(String, String)> = self
            .blocking(move |c| {
                let (_, user) = authenticate_user(c, &who)?;
                let mut found = Vec::new();
                for name in asked {
                    if let Some(s) = devices::shared_folder(c, user, &name)? {
                        found.push((name, s.folder));
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
                if tx.send(Ok(SharedChanged { shared: name.clone() })).await.is_err() {
                    return;
                }
            }
            loop {
                let mut folders = std::collections::HashSet::new();
                match announced.recv().await {
                    Ok(payload) => folders.extend(changed_folder(&payload)),
                    // Fell behind: every folder may have changed.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        folders.extend(watched.iter().map(|(_, folder)| folder.clone()))
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
                // An announcement without a folder (a delete of several files) may be about any of them.
                let about = |folders: &std::collections::HashSet<String>, folder: &String| {
                    folders.contains(folder) || folders.contains("")
                };
                if !watched.iter().any(|(_, folder)| about(&folders, folder)) {
                    continue;
                }
                // The rest of the burst, so that it is one message.
                tokio::time::sleep(WATCH_SETTLE).await;
                while let Ok(payload) = announced.try_recv() {
                    folders.extend(changed_folder(&payload));
                }
                for (name, folder) in &watched {
                    if about(&folders, folder) && tx.send(Ok(SharedChanged { shared: name.clone() })).await.is_err() {
                        return;
                    }
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type DownloadSharedStream = ReceiverStream<Result<FileData, Status>>;

    async fn download_shared(
        &self,
        req: Request<DownloadSharedRequest>,
    ) -> Result<Response<Self::DownloadSharedStream>, Status> {
        let who = caller_of(&req, &self.trusted)?;
        let r = req.into_inner();
        let who = on_shared_folder(who, r.shared)?;
        let path = clean_remote_path(&r.path).map_err(invalid)?;
        let store = self.store.clone();
        let file_path = {
            let path = path.clone();
            self.blocking(move |c| {
                let dev = acting(c, &who, false)?;
                if files::live_content(c, dev.id, &path)?.is_none() {
                    return Err(Status::not_found("no such file").into());
                }
                store.live_path(&dev.name, &path)
            })
            .await?
        };
        let file = tokio::fs::File::open(&file_path).await.map_err(|_| Status::not_found("the file is not stored"))?;
        Ok(Response::new(stream_file(file, |data| FileData { data })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::db::testing::{db_with_device, open_alerts};

    fn caller(instance: &str) -> Caller {
        Caller { token: "secret".into(), instance: instance.into(), ip: Some("10.0.0.9".into()), shared: None }
    }

    /// What a computer says about a shared folder.
    fn said(name: &str, files: i64, held: i64, error: &str) -> devices::SharedState {
        devices::SharedState {
            name: name.into(),
            files,
            bytes: files * 10,
            last_cycle: 100,
            last_error: error.into(),
            held,
            failing: 0,
            updated_at: 0,
        }
    }

    fn add_user(c: &mut Client, name: &str) -> i64 {
        let insert =
            "INSERT INTO users (name, role, password_hash, created_at) VALUES ($1, 'normal', '!', 0) RETURNING id";
        c.query_one(insert, &[&name]).unwrap().get(0)
    }

    #[test]
    fn a_shared_folder_is_its_owners_until_the_owner_adds_someone() {
        let db = setup();
        let mut conn = db.get().unwrap();
        let c: &mut Client = &mut conn;
        let (_, owner) = authenticate_user(c, &caller("first")).unwrap();
        let folder = devices::create_shared(c, owner, "family", 0).unwrap().expect("made");
        let on = |who: Caller, key: &str| Caller { shared: Some(key.into()), ..who };

        let found = acting(c, &on(caller("first"), "family"), true).expect("the owner needs no setup");
        assert_eq!(found.name, folder);
        let by_folder = acting(c, &on(caller("first"), &folder), true).expect("and can name it in full too");
        assert_eq!(by_folder.id, found.id);
        assert_eq!(acting(c, &on(caller("first"), "nope"), false).unwrap_err().code(), tonic::Code::NotFound);

        // Another user, with a device of their own.
        let bob = add_user(c, "bob");
        devices::insert(c, bob, "phone", "device", &db::hash_token("bobtoken"), 0).unwrap();
        let bobs = Caller { token: "bobtoken".into(), instance: "b".into(), ip: None, shared: None };
        for key in [folder.as_str(), "family"] {
            let refused = acting(c, &on(bobs.clone(), key), false);
            assert_eq!(refused.unwrap_err().code(), tonic::Code::NotFound, "{key} is not shared with bob");
        }
        assert!(devices::shared_folders(c, bob).unwrap().is_empty());

        // The owner adds bob to read.
        c.execute(
            "INSERT INTO shared_members (folder_id, user_id, can_write, created_at) VALUES ($1, $2, FALSE, 0)",
            &[&found.id, &bob],
        )
        .unwrap();
        let theirs = devices::shared_folders(c, bob).unwrap();
        assert_eq!(theirs.iter().map(|s| s.folder.as_str()).collect::<Vec<_>>(), [folder.as_str()]);
        assert!(!theirs[0].mine, "it is not bob's own");
        assert_eq!(acting(c, &on(bobs.clone(), &folder), false).unwrap().id, found.id);
        let read_only = acting(c, &on(bobs.clone(), &folder), true).unwrap_err();
        assert_eq!(read_only.code(), tonic::Code::FailedPrecondition);
        assert_eq!(
            acting(c, &on(bobs.clone(), "family"), false).unwrap_err().code(),
            tonic::Code::NotFound,
            "bob's own namespace has no \"family\""
        );

        c.execute("UPDATE shared_members SET can_write = TRUE", &[]).unwrap();
        acting(c, &on(bobs.clone(), &folder), true).expect("now bob may change it");

        // A read-only account never writes, whatever it was given before it was demoted.
        c.execute("UPDATE users SET role = 'readonly' WHERE id = $1", &[&bob]).unwrap();
        assert_eq!(acting(c, &on(bobs.clone(), &folder), true).unwrap_err().code(), tonic::Code::FailedPrecondition);
        acting(c, &on(bobs.clone(), &folder), false).expect("but still reads");

        // And nobody is let into the folder of an owner who was disabled.
        c.execute("UPDATE users SET disabled_at = 1 WHERE id = $1", &[&owner]).unwrap();
        assert_eq!(acting(c, &on(bobs.clone(), &folder), false).unwrap_err().code(), tonic::Code::NotFound);
    }

    #[test]
    fn a_computer_that_may_have_missed_deletions_is_sent_the_whole_list() {
        let db = setup();
        let mut conn = db.get().unwrap();
        let c: &mut Client = &mut conn;
        assert!(deletions_may_be_missing(c, 0), "asking from the start");
        assert!(!deletions_may_be_missing(c, 1_000), "retention has removed nothing yet");
        settings::set(c, settings::DELETIONS_KNOWN_FROM, "5000").unwrap();
        assert!(deletions_may_be_missing(c, 4_999), "away since before deletions were removed");
        assert!(!deletions_may_be_missing(c, 5_000), "away since the time they are all known from");
        assert!(!deletions_may_be_missing(c, 9_000));
    }

    #[test]
    fn what_a_computer_says_about_its_shared_folders_replaces_what_it_said_before() {
        let db = setup();
        let mut conn = db.get().unwrap();
        let c: &mut Client = &mut conn;
        let device = authenticate(c, &caller("first")).unwrap().id;
        devices::save_shared_state(c, device, &[said("alice/family", 3, 0, ""), said("alice/work", 1, 0, "boom")], 500)
            .unwrap();
        let first = devices::shared_state_of_device(c, device).unwrap();
        assert_eq!(first.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["alice/family", "alice/work"]);
        assert_eq!((first[0].files, first[1].last_error.as_str(), first[0].updated_at), (3, "boom", 500));
        // The next beat names only one: the other is no longer attached.
        devices::save_shared_state(c, device, &[said("alice/family", 4, 0, "")], 600).unwrap();
        let second = devices::shared_state_of_device(c, device).unwrap();
        assert_eq!((second.len(), second[0].files), (1, 4));
        // The owner of a folder sees every computer with it attached, another user only their own.
        let owner: i64 = authenticate_user(c, &caller("first")).unwrap().1;
        assert_eq!(devices::shared_state_of_folder(c, "alice/family", true, -1).unwrap().len(), 1);
        assert_eq!(devices::shared_state_of_folder(c, "alice/family", false, owner).unwrap().len(), 1);
        assert!(devices::shared_state_of_folder(c, "alice/family", false, owner + 1).unwrap().is_empty());
        devices::save_shared_state(c, device, &[], 700).unwrap();
        assert!(
            devices::shared_state_of_device(c, device).unwrap().is_empty(),
            "a computer with none attached says nothing"
        );
    }

    #[test]
    fn what_a_user_sees_of_the_folders_is_one_rule_of_the_database() {
        let db = setup();
        let mut conn = db.get().unwrap();
        let c: &mut Client = &mut conn;
        let (_, owner) = authenticate_user(c, &caller("first")).unwrap();
        let family = devices::create_shared(c, owner, "family", 0).unwrap().expect("made");
        let (member, stranger) = (add_user(c, "member"), add_user(c, "stranger"));
        let folder_id: i64 = c.query_one("SELECT id FROM devices WHERE folder = $1", &[&family]).unwrap().get(0);
        devices::add_shared_member(c, folder_id, member, false, 0).unwrap();
        let sees = |c: &mut Client, which: &str, uid: i64, everything: bool| -> bool {
            c.query_one(
                "SELECT visible(d, $2, $3) FROM devices d WHERE d.folder = $1 OR d.kind = $1",
                &[&which, &uid, &everything],
            )
            .unwrap()
            .get(0)
        };
        let own_device = authenticate(c, &caller("first")).unwrap().name;
        for (who, uid, everything, family_seen, device_seen) in [
            ("its owner", owner, false, true, true),
            ("a member", member, false, true, false),
            ("a stranger", stranger, false, false, false),
            ("an administrator", stranger, true, true, true),
        ] {
            assert_eq!(sees(c, &family, uid, everything), family_seen, "{who} and the shared folder");
            assert_eq!(sees(c, &own_device, uid, everything), device_seen, "{who} and the owner's computer");
            assert!(sees(c, "master", uid, everything), "{who} and the master files");
        }
        c.execute("UPDATE users SET disabled_at = 1 WHERE id = $1", &[&owner]).unwrap();
        assert!(!sees(c, &family, member, false), "a member loses the folder of a disabled owner");
    }

    #[test]
    fn held_deletions_raise_an_alert_that_closes_when_they_are_dealt_with() {
        let db = setup();
        let mut conn = db.get().unwrap();
        let c: &mut Client = &mut conn;
        let open = |c: &mut Client| -> Vec<(String, String, String)> {
            alerts::open(c)
                .unwrap()
                .into_iter()
                .filter(|a| a.kind == "deletions_held")
                .map(|a| (a.kind, a.key, a.message))
                .collect()
        };
        alert_held_deletions(c, "laptop", &[said("alice/family", 20, 0, "")]).unwrap();
        assert!(open(c).is_empty(), "nothing held, nothing to say");
        alert_held_deletions(c, "laptop", &[said("alice/family", 20, 12, ""), said("alice/work", 20, 3, "")]).unwrap();
        let raised = open(c);
        assert_eq!(raised.len(), 1, "one alert for the computer");
        assert_eq!(raised[0].1, "laptop");
        assert!(raised[0].2.contains("alice/family (12 files)") && raised[0].2.contains("alice/work (3 files)"));
        alert_held_deletions(c, "laptop", &[said("alice/family", 20, 12, "")]).unwrap();
        assert_eq!(open(c).len(), 1, "still one, updated");
        alert_held_deletions(c, "laptop", &[said("alice/family", 20, 0, "")]).unwrap();
        assert!(open(c).is_empty(), "closed once nothing is held");
    }

    #[test]
    fn a_shared_folder_lists_what_was_written_and_deleted_since() {
        let db = setup();
        let mut conn = db.get().unwrap();
        let c: &mut Client = &mut conn;
        let (_, owner) = authenticate_user(c, &caller("first")).unwrap();
        devices::create_shared(c, owner, "family", 0).unwrap();
        let id = devices::shared_folders(c, owner).unwrap()[0].id;

        let root = [7u8; 32];
        for (path, at) in [("a.txt", 100), ("b.txt", 200)] {
            let file = files::ensure(c, id, path).unwrap();
            let v = files::insert_version(
                c,
                &files::NewVersion { file_id: file, root: &root, size: 3, mtime: 5, mode: 0o644, created_at: at },
            )
            .unwrap();
            files::make_current(c, file, v).unwrap();
        }
        let paths = |c: &mut Client, since| -> Vec<(String, bool)> {
            files::changes_since(c, id, since).unwrap().into_iter().map(|x| (x.path, x.root.is_some())).collect()
        };
        assert_eq!(paths(c, 0), [("a.txt".into(), true), ("b.txt".into(), true)], "everything from the start");
        assert_eq!(paths(c, 150), [("b.txt".into(), true)], "only what is newer");
        assert!(paths(c, 300).is_empty());

        files::delete_path(c, id, "a.txt", 400, false).unwrap();
        assert_eq!(paths(c, 300), [("a.txt".into(), false)], "a deletion is a change, without content");
        let all = files::changes_since(c, id, 0).unwrap();
        let b = all.iter().find(|x| x.path == "b.txt").unwrap();
        assert_eq!((b.root, b.size, b.mtime, b.mode), (Some(root), 3, 5, 0o644));
    }

    /// A database with a device whose token is "secret".
    fn setup() -> Db {
        let db = db_with_device();
        db.get().unwrap().execute("UPDATE devices SET token_hash = $1", &[&db::hash_token("secret")]).unwrap();
        db
    }

    #[test]
    fn only_one_computer_uses_a_device_at_a_time() {
        let db = setup();
        let mut c = db.get().unwrap();
        authenticate(&mut c, &caller("first")).expect("the first to connect holds the device");
        authenticate(&mut c, &caller("first")).expect("and keeps it");
        let refused = authenticate(&mut c, &caller("second")).expect_err("a second computer is refused");
        assert_eq!(refused.code(), tonic::Code::PermissionDenied);
        assert!(refused.message().starts_with("device in use"), "{}", refused.message());
        assert_eq!(open_alerts(&mut c), 1, "and the owner is told");
        authenticate(&mut c, &caller("second")).expect_err("still refused");
        assert_eq!(open_alerts(&mut c), 1, "once, not once per attempt");
        authenticate(&mut c, &caller("first")).expect("the first is not disturbed");
        let wrong = Caller { token: "other".into(), ..caller("first") };
        assert_eq!(authenticate(&mut c, &wrong).unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn a_computer_that_went_quiet_is_replaced() {
        let db = setup();
        let mut c = db.get().unwrap();
        authenticate(&mut c, &caller("old")).unwrap();
        authenticate(&mut c, &caller("new")).unwrap_err();
        // The first has not been heard from for longer than the lease.
        c.execute("UPDATE devices SET last_seen = $1", &[&(unix_now() - lease_secs() - 5)]).unwrap();
        authenticate(&mut c, &caller("new")).expect("the new computer takes the device over");
        assert_eq!(open_alerts(&mut c), 0, "the conflict is over");
        let back =
            authenticate(&mut c, &caller("old")).expect_err("and the old one, when it returns, is the one refused");
        assert_eq!(back.code(), tonic::Code::PermissionDenied);
    }

    #[test]
    fn a_released_device_goes_to_the_next_computer() {
        let db = setup();
        let mut c = db.get().unwrap();
        authenticate(&mut c, &caller("old")).unwrap();
        c.execute("UPDATE devices SET instance_id = NULL", &[]).unwrap();
        authenticate(&mut c, &caller("new")).expect("whoever connects next holds it");
        authenticate(&mut c, &caller("old")).expect_err("the old one is refused now");
    }

    #[test]
    fn callers_that_do_not_name_an_instance_count_as_one() {
        let db = setup();
        let mut c = db.get().unwrap();
        authenticate(&mut c, &caller(UNNAMED_INSTANCE)).unwrap();
        authenticate(&mut c, &caller(UNNAMED_INSTANCE)).unwrap();
        authenticate(&mut c, &caller("new")).expect_err("but a caller that names one does not share it");
    }
}
