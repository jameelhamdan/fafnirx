//! The long running client: heartbeat, watcher, periodic scan and update checks.

pub mod requests;
mod status;
mod tasks;

pub use status::*;
use tasks::*;

use crate::config::credentials;
use crate::config::{control_dir, Config};
use crate::net::{self, Client};
use crate::sync::state::State;
use crate::sync::throttle::Limiter;
use crate::sync::{Engine, Report, SyncError};
use crate::update;
use anyhow::Result;
use fafnirx_common::{pb, unix_now, LockExt};
use notify::{RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

const HEARTBEAT_SECS: u64 = 20;
const PROGRESS_HEARTBEAT_SECS: u64 = 3;
const UPDATE_CHECK_SECS: u64 = 6 * 3600;
/// How soon files that failed are tried again.
const RETRY_SECS: u64 = 600;
const DEBOUNCE: Duration = Duration::from_secs(2);

/// The master refused this client because another device is using the same device: it must not send anything.
#[derive(Debug)]
pub struct InUse(pub String);

impl std::fmt::Display for InUse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InUse {}

pub async fn send_heartbeat(client: &mut Client, cfg: &Config, errors: &Mutex<Vec<String>>, status: &Status) -> Result<String> {
    let drained: Vec<String> = std::mem::take(&mut *errors.locked());
    let (total, free) = cfg.folders.first().and_then(|f| fafnirx_common::disk_usage(&f.path)).unwrap_or((0, 0));
    let req = pb::HeartbeatRequest {
        hostname: gethostname::gethostname().to_string_lossy().into_owned(),
        os: std::env::consts::OS.to_string(),
        client_version: update::VERSION.to_string(),
        disk_total: total,
        disk_free: free,
        errors: drained.clone(),
        phase: if status.syncing { status.activity.phase.clone() } else { String::new() },
        current_file: status.activity.current.clone(),
        files_done: status.activity.done as u32,
        files_total: status.activity.total as u32,
        bytes_done: status.activity.done_bytes,
        bytes_total: status.activity.total_bytes,
        speed_bps: status.activity.speed_bps,
        folders: status
            .synced
            .iter()
            .map(|s| pb::FolderState {
                name: s.name.clone(),
                files: s.files as u64,
                bytes: s.bytes,
                last_cycle: s.last_cycle,
                last_error: s.last_error.chars().take(300).collect(),
                held: s.held as u32,
                failing: s.failing as u32,
                mode: s.mode.clone(),
            })
            .collect(),
    };
    match tokio::time::timeout(Duration::from_secs(15), client.heartbeat(req)).await {
        Ok(Ok(resp)) => Ok(resp.into_inner().device_name),
        other => {
            // Keep the errors for the next beat.
            if let Ok(mut e) = errors.lock() {
                let mut restored = drained;
                restored.append(&mut e);
                restored.truncate(50);
                *e = restored;
            }
            match other {
                Ok(Err(s)) if s.code() == tonic::Code::PermissionDenied => Err(InUse(s.message().to_string()).into()),
                Ok(Err(s)) => Err(anyhow::anyhow!("{:?}: {}", s.code(), s.message())),
                _ => Err(anyhow::anyhow!("timed out")),
            }
        }
    }
}

pub async fn run(config_path: PathBuf) -> Result<()> {
    let cfg = Config::load(&config_path)?;
    let state_dir = cfg.state_dir(&config_path);
    std::fs::create_dir_all(&state_dir)?;
    std::fs::create_dir_all(control_dir(&state_dir))?;
    if !crate::service::instance::claim(&state_dir, "daemon", 20)? {
        println!("fafnirx is already running for {}", config_path.display());
        return Ok(());
    }
    crate::service::ensure_login_start(&config_path);
    let result = serve(config_path, cfg, state_dir.clone()).await;
    if let Err(e) = &result {
        StatusFile::new(status_file(&state_dir)).edit(|s| s.fatal = format!("{e:#}"));
    }
    result
}

async fn serve(config_path: PathBuf, mut cfg: Config, state_dir: PathBuf) -> Result<()> {
    let token = credentials::load(&cfg.server, &config_path)?;
    let client = net::client(&cfg, &token, &crate::service::instance::client_id(&state_dir))?;
    let identity = Identity::load(&cfg, &config_path)?;
    let marker = paused_marker(&state_dir);

    let hub = Hub {
        status: Arc::new(StatusFile::new(status_file(&state_dir))),
        wake: Arc::new(Notify::new()),
        online: Arc::new(AtomicBool::new(false)),
        errors: Arc::new(Mutex::new(Vec::new())),
    };
    hub.status.edit(|s| {
        s.message = "starting".into();
        s.server = cfg.server.clone();
    });
    let _ = std::fs::remove_file(stop_marker(&state_dir));

    let mut engine = Engine::new(
        client.clone(),
        State::open(&state_dir.join("state.db"))?,
        Arc::new(Limiter::new(cfg.bandwidth_limit_kbps)),
        hub.errors.clone(),
        marker.clone(),
    );
    engine.exclude = Some(state_dir.clone());
    hub.status.edit(|s| s.initial_sync = !engine.state.initial_sync_done());
    spawn_control(hub.clone(), engine.activity.clone(), engine.sent.clone(), state_dir.clone());
    spawn_heartbeat(hub.clone(), client.clone(), config_path.clone(), identity);
    spawn_synced_watch(hub.clone(), client.clone(), config_path.clone());
    spawn_update_requests(state_dir.clone());
    requests::spawn(config_path.clone(), state_dir.clone(), hub.wake.clone());
    spawn_updater(config_path.clone(), state_dir.clone());

    let mut watched: Vec<PathBuf> = Vec::new();
    let mut attached: Vec<crate::synced::Attached> = Vec::new();
    let mut _watcher: Option<notify::RecommendedWatcher> = None;
    let mut delay = Duration::from_secs(1);
    // When the master's folders were last asked for, and whether that gave a list (without one nothing is called gone).
    let (mut listed_at, mut listed): (Option<std::time::Instant>, bool) = (None, false);
    tracing::info!("fafnirx {} running, config {}", update::VERSION, config_path.display());
    loop {
        tokio::select! {
            _ = hub.wake.notified() => {}
            _ = tokio::time::sleep(delay) => {}
        }
        if marker.exists() {
            delay = Duration::from_secs(5);
            continue;
        }
        if !hub.online.load(Ordering::SeqCst) {
            // The heartbeat wakes us when the master is reachable again.
            delay = Duration::from_secs(30);
            continue;
        }
        // Every cycle (so "Sync now" too) looks at what the master has, before the config is read: a rename it reports is followed
        // first, and a folder that is gone from the list is then really gone.
        // Not more than every few seconds: a busy folder wakes a cycle for each change.
        if listed_at.is_none_or(|at| at.elapsed() >= LIST_EVERY) {
            listed = refresh_master_folders(&hub, &mut client.clone(), &config_path).await;
            listed_at = Some(std::time::Instant::now());
        }
        match Config::load(&config_path) {
            Ok(c) => cfg = c,
            Err(e) => hub.status.edit(|s| s.last_error = format!("config: {e:#}")),
        }
        if listed {
            let gone = crate::synced::deleted_and_empty(cfg.attached_folders(), &hub.status.snapshot().synced_on_master);
            hub.status.edit(|s| s.deleted_on_master = gone);
        }
        let folders: Vec<PathBuf> = cfg.paths().map(|(_, p)| p.to_path_buf()).collect();
        if cfg.watch && folders != watched {
            _watcher = spawn_watcher(&folders, state_dir.clone(), hub.wake.clone());
            watched = folders;
        } else if !cfg.watch {
            _watcher = None;
            watched.clear();
        }

        hub.status.edit(|s| s.syncing = true);
        let mut result = engine.cycle(&cfg).await;
        sync_attached(
            &mut attached,
            &Session { cfg: &cfg, token: &token, state_dir: &state_dir, daemon: &hub },
            &mut result,
        )
        .await;
        engine.set_activity("idle", "", 0, 0);
        let (mut synced_files, mut synced_bytes) = engine.state.totals().unwrap_or((0, 0));
        let mut failing = engine.state.pending_failures().unwrap_or(0);
        let synced_status: Vec<SyncedStatus> = attached.iter().map(|a| a.status()).collect();
        for s in &synced_status {
            synced_files += s.files;
            synced_bytes += s.bytes;
            failing += s.failing as i64;
        }
        let initial_done = engine.state.initial_sync_done();
        delay = next_delay(&cfg, failing > 0);
        if cfg.attached_folders().next().is_some() {
            // The master tells a running client when another device changed a synced folder (`spawn_synced_watch`); this is
            // for when that call is not there (the call failed) or a message was lost.
            delay = delay.min(SYNCED_POLL);
        }
        hub.status.edit(|s| {
            s.synced = synced_status;
            s.syncing = false;
            s.activity = Activity::default();
            s.folders = cfg.folders.len();
            s.synced_files = synced_files;
            s.synced_bytes = synced_bytes;
            s.failing = failing as usize;
            s.initial_sync = !initial_done;
            s.next_scan = unix_now() + delay.as_secs() as i64;
        });
        record_cycle(&hub, result);
    }
}

/// The least time between two looks at the master's list of folders in the main loop.
const LIST_EVERY: Duration = Duration::from_secs(10);

/// How often a device with synced folders asks the master what the others changed.
const SYNCED_POLL: Duration = Duration::from_secs(60);

/// What a sync cycle works with: the configuration, the device token, the state folder and the hub of the daemon.
#[derive(Clone, Copy)]
struct Session<'a> {
    cfg: &'a Config,
    token: &'a str,
    state_dir: &'a Path,
    daemon: &'a Hub,
}

/// Syncs the synced folders attached to this device, both ways, after the device's own folders. A problem with one
/// folder is shown and the next one goes on; a master that cannot be reached ends the cycle.
async fn sync_attached(attached: &mut Vec<crate::synced::Attached>, on: &Session<'_>, result: &mut Result<Report, SyncError>) {
    let Session { cfg, token, state_dir, daemon } = *on;
    attached.retain(|a| cfg.attached_folders().any(|s| a.is(s)));
    if matches!(result, Err(SyncError::Offline(_) | SyncError::Paused)) {
        return;
    }
    for s in cfg.attached_folders() {
        if attached.iter().any(|a| a.is(s)) {
            continue;
        }
        match crate::synced::Attached::open(cfg, token, state_dir, s, daemon.errors.clone()) {
            Ok(a) => attached.push(a),
            Err(e) => daemon.status.edit(|st| st.last_error = format!("{}: {e:#}", s.name)),
        }
    }
    let readonly = daemon.status.snapshot().synced_readonly;
    for a in attached.iter_mut() {
        let name = a.name().to_string();
        a.set_read_only(readonly.contains(&name));
        match a.cycle(cfg).await {
            Ok(r) => {
                log_report(&r);
                if let Ok(total) = result {
                    total.add(&r);
                }
            }
            Err(SyncError::Paused) => break,
            Err(SyncError::File(m) | SyncError::Refused(m)) => {
                daemon.status.edit(|st| st.last_error = format!("{name}: {m}"));
            }
            Err(offline) => {
                if result.is_ok() {
                    *result = Err(offline);
                }
                break;
            }
        }
    }
}

/// When the next cycle is due without a trigger: the scan interval, or sooner while files are waiting for a retry.
fn next_delay(cfg: &Config, has_failures: bool) -> Duration {
    let scan = if cfg.scan_interval_secs == 0 { 24 * 3600 } else { cfg.scan_interval_secs };
    Duration::from_secs(if has_failures { scan.min(RETRY_SECS) } else { scan })
}

fn log_report(r: &Report) {
    if *r != Report::default() {
        tracing::info!("collected: {r}, {} bytes sent", r.bytes);
    }
}
