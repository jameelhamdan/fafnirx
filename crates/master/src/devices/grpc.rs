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
pub struct Caller {
    token: String,
    instance: String,
    ip: Option<String>,
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
    Ok(Caller { token: bearer(req)?, instance, ip: address_of(req, trusted) })
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
fn authenticate(conn: &mut Client, who: &Caller) -> Result<Device, Status> {
    // The device is its folder (`<user>/<name>`). A device of a disabled user is refused like a revoked one.
    let found = devices::find_by_token(conn, &db::hash_token(&who.token))
        .map_err(internal)?
        .ok_or_else(|| Status::unauthenticated("unknown or revoked device token"))?;
    let (id, folder) = (found.id, found.folder);
    let holder = found.instance;
    if holder.as_deref() == Some(who.instance.as_str()) {
        return Ok(Device { id, name: folder });
    }
    let now = unix_now();
    if devices::take_over(conn, id, &who.instance, now, now - lease_secs()).map_err(internal)? {
        if holder.is_some() {
            tracing::info!("{folder}: taken over by another computer, the first one went quiet");
            let _ = alerts::resolve(conn, "device_conflict", &folder);
        }
        return Ok(Device { id, name: folder });
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
        db::with(&self.db, f).await.map_err(|e| e.downcast::<Status>().unwrap_or_else(internal))
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
                let dev = authenticate(c, &who)?;
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
            let dev = authenticate(c, &who)?;
            let current = files::live_content(c, dev.id, &m.path)?.map(|(root, _)| root);
            // Deleted in the web app: the device is told it is up to date, whatever it holds now.
            if current == Some(m.root) || files::deleted_in_web_app(c, dev.id, &m.path)? {
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
        let device_id = self.check_token(who).await?;
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
                let dev = authenticate(c, &who)?;
                if files::deleted_in_web_app(c, dev.id, &m.path)? {
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
            let dev = authenticate(c, &who)?;
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
        let mut file = tokio::fs::File::open(&bin).await.map_err(|_| Status::not_found("no update"))?;
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        tokio::spawn(async move {
            let mut buf = vec![0u8; 256 * 1024];
            loop {
                match file.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(Ok(UpdateChunk { data: buf[..n].to_vec() })).await.is_err() {
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
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::db::testing::{db_with_device, open_alerts};

    fn caller(instance: &str) -> Caller {
        Caller { token: "secret".into(), instance: instance.into(), ip: Some("10.0.0.9".into()) }
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
