use crate::files::ingest;
use crate::platform::db::{self, resolve_alert, upsert_alert, Db, Device};
use crate::platform::store::Store;
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

fn authenticate(conn: &mut Client, token: &str) -> Result<Device, Status> {
    conn.query_opt(
        "SELECT id, name FROM devices WHERE token_hash = $1 AND revoked_at IS NULL AND kind = 'device'",
        &[&db::hash_token(token)],
    )
    .map_err(internal)?
    .map(|r| Device { id: r.get(0), name: r.get(1) })
    .ok_or_else(|| Status::unauthenticated("unknown or revoked device token"))
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

    /// Authenticates a device token and returns the device id.
    async fn check_token(&self, token: String) -> Result<i64, Status> {
        self.blocking(move |c| Ok(authenticate(c, &token)?.id)).await
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
        let token = bearer(&req)?;
        let master_id = self
            .blocking(move |c| {
                authenticate(c, &token)?;
                Ok(c.query_one("SELECT value FROM settings WHERE key = 'master_id'", &[])?.get::<_, String>(0))
            })
            .await?;
        Ok(Response::new(IdentifyResponse { master_id }))
    }

    async fn heartbeat(&self, req: Request<HeartbeatRequest>) -> Result<Response<HeartbeatResponse>, Status> {
        let token = bearer(&req)?;
        let ip = req.remote_addr().map(|a| a.ip().to_string());
        let hb = req.into_inner();
        let name = self
            .blocking(move |c| {
                let dev = authenticate(c, &token)?;
                c.execute(
                    "UPDATE devices SET hostname = $1, os = $2, ip = $3, client_version = $4,
                     disk_total = $5, disk_free = $6, last_seen = $7 WHERE id = $8",
                    &[
                        &clip(&hb.hostname, 100),
                        &clip(&hb.os, 20),
                        &ip,
                        &clip(&hb.client_version, 30),
                        &(hb.disk_total as i64),
                        &(hb.disk_free as i64),
                        &unix_now(),
                        &dev.id,
                    ],
                )?;
                if hb.phase.is_empty() || hb.phase == "idle" {
                    c.execute("DELETE FROM device_activity WHERE device_id = $1", &[&dev.id])?;
                } else {
                    c.execute(
                        "INSERT INTO device_activity (device_id, updated_at, phase, current_file, files_done,
                                                      files_total, bytes_done, bytes_total, speed_bps)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                         ON CONFLICT (device_id) DO UPDATE SET updated_at = EXCLUDED.updated_at,
                           phase = EXCLUDED.phase, current_file = EXCLUDED.current_file,
                           files_done = EXCLUDED.files_done, files_total = EXCLUDED.files_total,
                           bytes_done = EXCLUDED.bytes_done, bytes_total = EXCLUDED.bytes_total,
                           speed_bps = EXCLUDED.speed_bps",
                        &[
                            &dev.id,
                            &unix_now(),
                            &clip(&hb.phase, 20),
                            &clip(&hb.current_file, 300),
                            &(hb.files_done as i64),
                            &(hb.files_total as i64),
                            &(hb.bytes_done as i64),
                            &(hb.bytes_total as i64),
                            &(hb.speed_bps as i64),
                        ],
                    )?;
                }
                db::notify(c, "devices", &dev.name)?;
                if !hb.errors.is_empty() {
                    let msg = hb.errors.iter().take(5).map(|e| clip(e, 300)).collect::<Vec<_>>().join("\n");
                    upsert_alert(c, "upload_error", &dev.name, &msg)?;
                }
                resolve_alert(c, "device_offline", &dev.name)?;
                Ok(dev.name)
            })
            .await?;
        Ok(Response::new(HeartbeatResponse { device_name: name, server_time: unix_now() }))
    }

    async fn negotiate(&self, req: Request<Manifest>) -> Result<Response<NegotiateResponse>, Status> {
        let token = bearer(&req)?;
        let m = check_manifest(req.get_ref())?;
        let store = self.store.clone();
        self.blocking(move |c| {
            let dev = authenticate(c, &token)?;
            let current: Option<Vec<u8>> = c
                .query_opt(
                    "SELECT v.root_hash FROM files f JOIN versions v ON v.id = f.current_version_id
                     WHERE f.device_id = $1 AND f.path = $2 AND f.deleted_at IS NULL",
                    &[&dev.id, &m.path],
                )?
                .map(|r| r.get(0));
            // Deleted in the web app: the device is told it is up to date, whatever it holds now.
            if current.as_deref() == Some(&m.root[..]) || ingest::deleted_by_admin(c, dev.id, &m.path)? {
                return Ok(Response::new(NegotiateResponse { missing: vec![], already_current: true }));
            }
            // The device already has this content under another path (a rename or a copy): the commit
            // takes it from there, nothing needs to be sent.
            if ingest::live_path_with_root(c, dev.id, &m.root, None)?.is_some() {
                return Ok(Response::new(NegotiateResponse { missing: vec![], already_current: false }));
            }
            let missing = store.missing_chunks(dev.id, &m.root, &m.chunks);
            Ok(Response::new(NegotiateResponse { missing, already_current: false }))
        })
        .await
    }

    async fn upload_chunks(&self, req: Request<Streaming<Chunk>>) -> Result<Response<UploadSummary>, Status> {
        let token = bearer(&req)?;
        let device_id = self.check_token(token).await?;
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
        let token = bearer(&req)?;
        let r = req.into_inner();
        let m = check_manifest(r.manifest.as_ref().ok_or_else(|| Status::invalid_argument("missing manifest"))?)?;
        let renamed_from =
            if r.renamed_from.is_empty() { None } else { Some(clean_remote_path(&r.renamed_from).map_err(invalid)?) };
        let store = self.store.clone();
        let version_id = self
            .blocking(move |c| {
                let dev = authenticate(c, &token)?;
                if ingest::deleted_by_admin(c, dev.id, &m.path)? {
                    return Ok(0);
                }
                // Content the device already has under another path is taken from there; otherwise the
                // upload must be complete and intact.
                let staged = match ingest::live_path_with_root(c, dev.id, &m.root, renamed_from.as_deref())? {
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
        let token = bearer(&req)?;
        let path = clean_remote_path(&req.get_ref().path).map_err(invalid)?;
        let store = self.store.clone();
        self.blocking(move |c| {
            let dev = authenticate(c, &token)?;
            ingest::mark_deleted(c, &store, &dev, &path, false)?;
            Ok(())
        })
        .await?;
        Ok(Response::new(DeleteResponse {}))
    }

    async fn get_update_info(&self, req: Request<UpdateInfoRequest>) -> Result<Response<UpdateInfo>, Status> {
        let token = bearer(&req)?;
        self.check_token(token).await?;
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
        let token = bearer(&req)?;
        self.check_token(token).await?;
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
