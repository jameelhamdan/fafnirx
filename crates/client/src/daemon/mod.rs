//! The long running client: heartbeat, watcher, periodic scan and update checks.

mod status;

pub use status::*;

use crate::config::credentials;
use crate::config::{control_dir, Config};
use crate::net::{self, Client};
use crate::sync::state::State;
use crate::sync::throttle::Limiter;
use crate::sync::{Engine, Report, SyncError};
use crate::update;
use anyhow::Result;
use fafnirx_common::{pb, unix_now};
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

/// The master refused this client because another computer is using the same device: it must not send anything.
#[derive(Debug)]
pub struct InUse(pub String);

impl std::fmt::Display for InUse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InUse {}

pub async fn send_heartbeat(
    client: &mut Client,
    cfg: &Config,
    errors: &Mutex<Vec<String>>,
    status: &Status,
) -> Result<String> {
    let drained: Vec<String> = errors.lock().map(|mut e| std::mem::take(&mut *e)).unwrap_or_default();
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
        shared: status
            .shared
            .iter()
            .map(|s| pb::SharedState {
                name: s.name.clone(),
                files: s.files as u64,
                bytes: s.bytes,
                last_cycle: s.last_cycle,
                last_error: s.last_error.chars().take(300).collect(),
                held: s.held as u32,
                failing: s.failing as u32,
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

/// `state_dir` is the client's own folder: its status file changes all the time and must not wake a cycle.
fn spawn_watcher(folders: &[PathBuf], state_dir: PathBuf, wake: Arc<Notify>) -> Option<notify::RecommendedWatcher> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    // Events name paths as the watched folder spells them, which may be a symlinked or canonical form of `state_dir`.
    let state_canonical = state_dir.canonicalize().unwrap_or_else(|_| state_dir.clone());
    let mut watcher = match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        // An event without paths (an overflow, a rescan) says "something may have changed": it always wakes a cycle.
        let own = |p: &PathBuf| p.starts_with(&state_dir) || p.starts_with(&state_canonical);
        if res.is_ok_and(|e| e.paths.is_empty() || !e.paths.iter().all(own)) {
            let _ = tx.send(());
        }
    }) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("cannot start file watcher: {e}");
            return None;
        }
    };
    for f in folders {
        if let Err(e) = watcher.watch(f, RecursiveMode::Recursive) {
            tracing::warn!("cannot watch {}: {e}", f.display());
        }
    }
    // Wait for the burst of events to settle before waking the sync loop.
    tokio::spawn(async move {
        while rx.recv().await.is_some() {
            tokio::time::sleep(DEBOUNCE).await;
            while rx.try_recv().is_ok() {}
            wake.notify_one();
        }
    });
    Some(watcher)
}

/// State shared by the daemon's background tasks.
#[derive(Clone)]
struct Shared {
    status: Arc<StatusFile>,
    /// Wakes the main loop: a watched file changed, the master came back, or the tray asked for a sync.
    wake: Arc<Notify>,
    online: Arc<AtomicBool>,
    /// Problems to report with the next heartbeat.
    errors: Arc<Mutex<Vec<String>>>,
}

/// What the daemon connects with. `enroll` can change it while the daemon runs.
#[derive(PartialEq)]
struct Identity {
    server: String,
    token: String,
}

impl Identity {
    fn load(cfg: &Config, config_path: &Path) -> Result<Self> {
        Ok(Self {
            server: cfg.server.clone(),
            token: credentials::load(&cfg.server, config_path, cfg.token.as_deref())?,
        })
    }
}

/// Ends the client to be started again, to run a new version or pick up a new enrollment.
fn restart() -> ! {
    // The new process needs the instance lock.
    crate::service::instance::release("daemon");
    crate::service::exit_to_be_restarted()
}

/// Handles requests from the tray (sync now, stop) and copies what the engine is doing into the status file.
fn spawn_control(shared: Shared, activity: Arc<Mutex<Activity>>, sent: Arc<AtomicU64>, dir: PathBuf) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        let mut last = Activity::default();
        let (mut prev_sent, mut prev_at, mut speed) = (0u64, std::time::Instant::now(), 0f64);
        loop {
            tick.tick().await;
            if std::fs::remove_file(sync_now_marker(&dir)).is_ok() {
                tracing::info!("sync requested");
                shared.wake.notify_one();
            }
            if std::fs::remove_file(stop_marker(&dir)).is_ok() {
                tracing::info!("stop requested, exiting");
                // No status file means "not running" to the tray and to `fafnirx status`.
                let _ = std::fs::remove_file(status_file(&dir));
                crate::service::exit_clean();
            }
            let mut now = activity.lock().map(|a| a.clone()).unwrap_or_default();
            let total_sent = sent.load(Ordering::Relaxed);
            let secs = prev_at.elapsed().as_secs_f64();
            if total_sent < prev_sent {
                speed = 0.0; // a new cycle
            } else if secs > 0.0 {
                // Smoothed over about two seconds, so the number does not jump with every chunk.
                speed = 0.75 * speed + 0.25 * (total_sent - prev_sent) as f64 / secs;
            }
            (prev_sent, prev_at) = (total_sent, std::time::Instant::now());
            if now.phase == "uploading" {
                now.bytes = total_sent;
                now.file_bytes = total_sent.saturating_sub(now.file_start).min(now.file_size);
                now.done_bytes = (now.base_bytes + now.file_bytes).min(now.total_bytes);
                now.speed_bps = speed as u64;
            }
            if now != last {
                last = now.clone();
                shared.status.edit(|s| s.activity = now);
            }
        }
    });
}

/// Keeps the master informed and notices when it is reachable again.
fn spawn_heartbeat(shared: Shared, mut client: Client, config_path: PathBuf, identity: Identity) {
    tokio::spawn(async move {
        let mut beats = 0u32;
        loop {
            if let Ok(cfg) = Config::load(&config_path) {
                // Re-enrolling changes the token or the server; connections already made keep the old ones.
                if Identity::load(&cfg, &config_path).is_ok_and(|now| now != identity) {
                    tracing::info!("enrollment changed, restarting");
                    restart();
                }
                match send_heartbeat(&mut client, &cfg, &shared.errors, &shared.status.snapshot()).await {
                    Ok(device) => {
                        if !shared.online.swap(true, Ordering::SeqCst) {
                            tracing::info!("connected to {}", cfg.server);
                            shared.wake.notify_one();
                        }
                        // The list of shared folders changes rarely: it is asked for every third beat.
                        if beats.is_multiple_of(3) {
                            if let Ok(listed) = client.list_shared(pb::ListSharedRequest {}).await {
                                let listed = listed.into_inner();
                                let names = listed.names.iter().map(|n| (n.clone(), listed.mine.contains(n))).collect();
                                shared.status.edit(|s| s.shared_on_master = names);
                            }
                        }
                        beats = beats.wrapping_add(1);
                        shared.status.edit(|s| {
                            s.online = true;
                            s.message.clear();
                            s.device = device;
                            s.server = cfg.server.clone();
                            s.bandwidth_limit_kbps = cfg.bandwidth_limit_kbps;
                        });
                    }
                    Err(e) => {
                        if shared.online.swap(false, Ordering::SeqCst) {
                            tracing::warn!("lost connection: {e:#}");
                        }
                        shared.status.edit(|s| {
                            s.online = false;
                            s.message = match e.downcast_ref::<InUse>() {
                                Some(in_use) => format!("not sending: {in_use}"),
                                None => format!("cannot reach the master: {e:#}"),
                            };
                        });
                    }
                }
            }
            // While files are going out the master gets a progress update every few seconds.
            let was_syncing = shared.status.snapshot().syncing;
            let wait = if was_syncing { PROGRESS_HEARTBEAT_SECS } else { HEARTBEAT_SECS };
            for _ in 0..wait {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if shared.status.snapshot().syncing != was_syncing {
                    break;
                }
            }
        }
    });
}

/// Keeps a call open that tells when another computer changed one of the shared folders attached here, and wakes the sync
/// loop. Without it (a master that does not know the call, a connection that drops) the folders are still asked every minute.
fn spawn_shared_watch(shared: Shared, mut client: Client, config_path: PathBuf) {
    let attached = |path: &Path| -> Vec<String> {
        Config::load(path).map(|c| c.shared.into_iter().map(|s| s.name).collect()).unwrap_or_default()
    };
    tokio::spawn(async move {
        loop {
            let names = attached(&config_path);
            if !names.is_empty() && shared.online.load(Ordering::SeqCst) {
                match client.watch_shared(pb::WatchSharedRequest { shared: names.clone() }).await {
                    Ok(response) => {
                        let mut stream = response.into_inner();
                        loop {
                            tokio::select! {
                                message = stream.message() => match message {
                                    Ok(Some(_)) => shared.wake.notify_one(),
                                    _ => break,
                                },
                                // A folder attached or detached meanwhile: ask for the right ones.
                                _ = tokio::time::sleep(Duration::from_secs(15)) => {
                                    if attached(&config_path) != names {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => tracing::debug!("cannot watch the shared folders: {}", e.message()),
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

/// Checks for a new release now and then.
fn spawn_updater(mut client: Client, config_path: PathBuf, work_dir: PathBuf) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            if let Some(cfg) = Config::load(&config_path).ok().filter(|c| c.auto_update) {
                match update::check_and_apply(&mut client, &cfg, &work_dir, false).await {
                    Ok(true) => {
                        tracing::info!("restarting to run the new version");
                        restart();
                    }
                    Ok(false) => {}
                    Err(e) => tracing::warn!("update check failed: {e:#}"),
                }
            }
            tokio::time::sleep(Duration::from_secs(UPDATE_CHECK_SECS)).await;
        }
    });
}

/// Answers `fafnirx update`: the program file is where only the service may change it, so the service does the update, says how
/// it went in `update_result_file`, and starts again on the new version.
fn spawn_update_requests(mut client: Client, config_path: PathBuf, state_dir: PathBuf) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let marker = update_marker(&state_dir);
            // Empty while the request is being written.
            let Some(nonce) = std::fs::read_to_string(&marker).ok().filter(|n| !n.is_empty()) else { continue };
            let _ = std::fs::remove_file(&marker);
            let (answer, restart_now) = match Config::load(&config_path) {
                Err(e) => (format!("failed: {e:#}"), false),
                Ok(cfg) => match update::check(Some(&mut client), &cfg).await {
                    Err(e) => (format!("failed: {e:#}"), false),
                    Ok(None) => (format!("current {}", update::VERSION), false),
                    Ok(Some(version)) => match update::check_and_apply(&mut client, &cfg, &state_dir, true).await {
                        Ok(true) => (format!("updated {version}"), true),
                        Ok(false) => (format!("current {}", update::VERSION), false),
                        Err(e) => (format!("failed: {e:#}"), false),
                    },
                },
            };
            let _ = std::fs::write(update_result_file(&state_dir), format!("{nonce}\n{answer}"));
            if restart_now {
                tracing::info!("restarting to run the new version");
                restart();
            }
        }
    });
}

/// Writes the outcome of one cycle into the status file.
fn record_cycle(shared: &Shared, result: Result<Report, SyncError>) {
    match result {
        Ok(r) => {
            log_report(&r);
            let last_error = shared.errors.lock().ok().and_then(|e| e.last().cloned()).unwrap_or_default();
            shared.status.edit(|s| {
                s.last_cycle = unix_now();
                s.last = r.clone();
                s.session_files += r.uploaded + r.renamed;
                s.session_bytes += r.bytes;
                // Scan problems (an unreadable folder) are errors even when no single file failed.
                s.last_error = if r.failed > 0 || !last_error.is_empty() { last_error } else { String::new() };
            });
        }
        Err(SyncError::Paused) => {}
        Err(SyncError::Offline(m)) => {
            tracing::warn!("sync interrupted: {m}");
            shared.online.store(false, Ordering::SeqCst);
            shared.status.edit(|s| {
                s.online = false;
                s.message = format!("upload interrupted: {m}");
            });
        }
        Err(SyncError::File(m) | SyncError::Refused(m)) => shared.status.edit(|s| s.last_error = m),
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
    let result = serve(config_path, cfg, state_dir.clone()).await;
    if let Err(e) = &result {
        StatusFile::new(status_file(&state_dir)).edit(|s| s.fatal = format!("{e:#}"));
    }
    result
}

async fn serve(config_path: PathBuf, mut cfg: Config, state_dir: PathBuf) -> Result<()> {
    let token = credentials::load(&cfg.server, &config_path, cfg.token.as_deref())?;
    let client = net::client(&cfg, &token, &crate::service::instance::client_id(&state_dir))?;
    let identity = Identity::load(&cfg, &config_path)?;
    let marker = paused_marker(&state_dir);

    let shared = Shared {
        status: Arc::new(StatusFile::new(status_file(&state_dir))),
        wake: Arc::new(Notify::new()),
        online: Arc::new(AtomicBool::new(false)),
        errors: Arc::new(Mutex::new(Vec::new())),
    };
    shared.status.edit(|s| {
        s.message = "starting".into();
        s.server = cfg.server.clone();
    });
    let _ = std::fs::remove_file(stop_marker(&state_dir));

    let mut engine = Engine::new(
        client.clone(),
        State::open(&state_dir.join("state.db"))?,
        Arc::new(Limiter::new(cfg.bandwidth_limit_kbps)),
        shared.errors.clone(),
        marker.clone(),
    );
    engine.exclude = Some(state_dir.clone());
    shared.status.edit(|s| s.initial_sync = !engine.state.initial_sync_done());
    spawn_control(shared.clone(), engine.activity.clone(), engine.sent.clone(), state_dir.clone());
    spawn_heartbeat(shared.clone(), client.clone(), config_path.clone(), identity);
    spawn_shared_watch(shared.clone(), client.clone(), config_path.clone());
    spawn_update_requests(client.clone(), config_path.clone(), state_dir.clone());
    spawn_updater(client, config_path.clone(), state_dir.clone());

    let mut watched: Vec<PathBuf> = Vec::new();
    let mut attached: Vec<crate::shared::Attached> = Vec::new();
    let mut _watcher: Option<notify::RecommendedWatcher> = None;
    let mut delay = Duration::from_secs(1);
    tracing::info!("fafnirx {} running, config {}", update::VERSION, config_path.display());
    loop {
        tokio::select! {
            _ = shared.wake.notified() => {}
            _ = tokio::time::sleep(delay) => {}
        }
        if marker.exists() {
            delay = Duration::from_secs(5);
            continue;
        }
        if !shared.online.load(Ordering::SeqCst) {
            // The heartbeat wakes us when the master is reachable again.
            delay = Duration::from_secs(30);
            continue;
        }
        match Config::load(&config_path) {
            Ok(c) => cfg = c,
            Err(e) => shared.status.edit(|s| s.last_error = format!("config: {e:#}")),
        }
        let folders: Vec<PathBuf> = cfg.paths().map(|(_, p)| p.to_path_buf()).collect();
        if cfg.watch && folders != watched {
            _watcher = spawn_watcher(&folders, state_dir.clone(), shared.wake.clone());
            watched = folders;
        } else if !cfg.watch {
            _watcher = None;
            watched.clear();
        }

        shared.status.edit(|s| s.syncing = true);
        let mut result = engine.cycle(&cfg).await;
        sync_attached(&mut attached, &cfg, &token, &state_dir, &shared, &mut result).await;
        engine.set_activity("idle", "", 0, 0, 0);
        let (mut synced_files, mut synced_bytes) = engine.state.totals().unwrap_or((0, 0));
        let mut failing = engine.state.pending_failures().unwrap_or(0);
        let shared_status: Vec<SharedStatus> = attached.iter().map(|a| a.status()).collect();
        for s in &shared_status {
            synced_files += s.files;
            synced_bytes += s.bytes;
            failing += s.failing as i64;
        }
        let initial_done = engine.state.initial_sync_done();
        delay = next_delay(&cfg, failing > 0);
        if !cfg.shared.is_empty() {
            // The master tells a running client when another computer changed a shared folder (`spawn_shared_watch`); this is
            // for when that call is not there (an older master) or a message was lost.
            delay = delay.min(SHARED_POLL);
        }
        shared.status.edit(|s| {
            s.shared = shared_status;
            s.syncing = false;
            s.activity = Activity::default();
            s.folders = cfg.folders.len() + cfg.shared.len();
            s.synced_files = synced_files;
            s.synced_bytes = synced_bytes;
            s.failing = failing as usize;
            s.initial_sync = !initial_done;
            s.next_scan = unix_now() + delay.as_secs() as i64;
        });
        record_cycle(&shared, result);
    }
}

/// How often a computer with shared folders asks the master what the others changed.
const SHARED_POLL: Duration = Duration::from_secs(60);

/// Syncs the shared folders attached to this computer, both ways, after the device's own folders. A problem with one
/// folder is shown and the next one goes on; a master that cannot be reached ends the cycle.
async fn sync_attached(
    attached: &mut Vec<crate::shared::Attached>,
    cfg: &Config,
    token: &str,
    state_dir: &Path,
    daemon: &Shared,
    result: &mut Result<Report, SyncError>,
) {
    attached.retain(|a| cfg.shared.iter().any(|s| a.is(s)));
    if matches!(result, Err(SyncError::Offline(_) | SyncError::Paused)) {
        return;
    }
    for s in &cfg.shared {
        if attached.iter().any(|a| a.is(s)) {
            continue;
        }
        match crate::shared::Attached::open(cfg, token, state_dir, s, daemon.errors.clone()) {
            Ok(a) => attached.push(a),
            Err(e) => daemon.status.edit(|st| st.last_error = format!("{}: {e:#}", s.name)),
        }
    }
    for a in attached.iter_mut() {
        let name = a.name().to_string();
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
