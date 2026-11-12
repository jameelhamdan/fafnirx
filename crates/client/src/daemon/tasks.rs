//! The background tasks of the daemon: the folder watcher, the heartbeat, the tray requests, the synced-folder watch and the
//! update checks, and what they share.

use super::*;

/// `state_dir` is the client's own folder: its status file changes all the time and must not wake a cycle.
pub(super) fn spawn_watcher(folders: &[PathBuf], state_dir: PathBuf, wake: Arc<Notify>) -> Option<notify::RecommendedWatcher> {
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
pub(super) struct Hub {
    pub(super) status: Arc<StatusFile>,
    /// Wakes the main loop: a watched file changed, the master came back, or the tray asked for a sync.
    pub(super) wake: Arc<Notify>,
    pub(super) online: Arc<AtomicBool>,
    /// Problems to report with the next heartbeat.
    pub(super) errors: Arc<Mutex<Vec<String>>>,
}

/// What the daemon connects with. `install --server` can change it while the daemon runs.
#[derive(PartialEq)]
pub(super) struct Identity {
    pub(super) server: String,
    pub(super) token: String,
}

impl Identity {
    pub(super) fn load(cfg: &Config, config_path: &Path) -> Result<Self> {
        Ok(Self { server: cfg.server.clone(), token: credentials::load(&cfg.server, config_path)? })
    }
}

/// Ends the client to be started again, to run a new version or pick up a new enrollment.
pub(super) fn restart() -> ! {
    // The new process needs the instance lock.
    crate::service::instance::release("daemon");
    crate::service::exit_to_be_restarted()
}

/// Handles requests from the tray (sync now, stop) and copies what the engine is doing into the status file.
pub(super) fn spawn_control(hub: Hub, activity: Arc<Mutex<Activity>>, sent: Arc<AtomicU64>, dir: PathBuf) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        let mut last = Activity::default();
        let (mut prev_sent, mut prev_at, mut speed) = (0u64, std::time::Instant::now(), 0f64);
        loop {
            tick.tick().await;
            if std::fs::remove_file(sync_now_marker(&dir)).is_ok() {
                tracing::info!("sync requested");
                hub.wake.notify_one();
            }
            if std::fs::remove_file(stop_marker(&dir)).is_ok() {
                tracing::info!("stop requested, exiting");
                // No status file means "not running" to the tray and to `fafnirx status`.
                let _ = std::fs::remove_file(status_file(&dir));
                crate::service::exit_clean();
            }
            let mut now = activity.locked().clone();
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
                hub.status.edit(|s| s.activity = now);
            }
        }
    });
}

/// Keeps the master informed and notices when it is reachable again.
pub(super) fn spawn_heartbeat(hub: Hub, mut client: Client, config_path: PathBuf, identity: Identity) {
    tokio::spawn(async move {
        let mut beats = 0u32;
        loop {
            if let Ok(cfg) = Config::load(&config_path) {
                // Re-enrolling changes the token or the server; connections already made keep the old ones.
                if Identity::load(&cfg, &config_path).is_ok_and(|now| now != identity) {
                    tracing::info!("enrollment changed, restarting");
                    restart();
                }
                match send_heartbeat(&mut client, &cfg, &hub.errors, &hub.status.snapshot()).await {
                    Ok(device) => {
                        if !hub.online.swap(true, Ordering::SeqCst) {
                            tracing::info!("connected to {}", cfg.server);
                            hub.wake.notify_one();
                        }
                        // The list of synced folders changes rarely: it is asked for every third beat.
                        if beats.is_multiple_of(3) {
                            refresh_master_folders(&hub, &mut client, &config_path).await;
                        }
                        beats = beats.wrapping_add(1);
                        hub.status.edit(|s| {
                            s.online = true;
                            s.message.clear();
                            s.device = device;
                            s.server = cfg.server.clone();
                            s.bandwidth_limit_kbps = cfg.bandwidth_limit_kbps;
                            s.full_disk_access = crate::service::disk_access::granted();
                        });
                    }
                    Err(e) => {
                        if hub.online.swap(false, Ordering::SeqCst) {
                            tracing::warn!("lost connection: {e:#}");
                        }
                        hub.status.edit(|s| {
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
            let was_syncing = hub.status.snapshot().syncing;
            let wait = if was_syncing { PROGRESS_HEARTBEAT_SECS } else { HEARTBEAT_SECS };
            for _ in 0..wait {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if hub.status.snapshot().syncing != was_syncing {
                    break;
                }
            }
        }
    });
}

/// Asks the master which synced folders this user can use and notes them in the status; false when it could not be asked.
pub(super) async fn refresh_master_folders(hub: &Hub, client: &mut Client, config_path: &Path) -> bool {
    let Ok(listed) = client.list_folders(pb::ListFoldersRequest {}).await else { return false };
    let listed = listed.into_inner();
    let names = listed.names.iter().map(|n| (n.clone(), listed.mine.contains(n))).collect();
    hub.status.edit(|s| {
        s.synced_on_master = names;
        s.synced_readonly = listed.readonly.clone();
    });
    // A folder renamed on the master is renamed here: it is attached by its id.
    let by_id: Vec<(String, String)> = listed.names.iter().cloned().zip(listed.ids.iter().cloned()).collect();
    match crate::config::follow_renames_in_file(config_path, &by_id) {
        Ok(changes) => changes.iter().for_each(|(old, new)| tracing::info!("{old} is now called {new} on the master")),
        Err(e) => tracing::warn!("cannot follow a rename in the config: {e:#}"),
    }
    true
}

/// Keeps a call open that tells when another device changed one of the synced folders attached here, and wakes the sync
/// loop. Without it (a master that does not know the call, a connection that drops) the folders are still asked every minute.
pub(super) fn spawn_synced_watch(hub: Hub, mut client: Client, config_path: PathBuf) {
    let attached = |path: &Path| -> Vec<String> {
        Config::load(path)
            .map(|c| c.attached_folders().map(|s| s.key().to_string()).collect())
            .unwrap_or_default()
    };
    tokio::spawn(async move {
        loop {
            let names = attached(&config_path);
            if !names.is_empty() && hub.online.load(Ordering::SeqCst) {
                match client.watch_folder(pb::WatchFolderRequest { folders: names.clone() }).await {
                    Ok(response) => {
                        let mut stream = response.into_inner();
                        loop {
                            tokio::select! {
                                message = stream.message() => match message {
                                    Ok(Some(_)) => hub.wake.notify_one(),
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
                    Err(e) => tracing::debug!("cannot watch the synced folders: {}", e.message()),
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

/// Checks for a new release now and then.
pub(super) fn spawn_updater(config_path: PathBuf, work_dir: PathBuf) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            if Config::load(&config_path).is_ok_and(|c| c.auto_update) {
                match update::check_and_apply(&work_dir, false).await {
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
pub(super) fn spawn_update_requests(state_dir: PathBuf) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let marker = update_marker(&state_dir);
            // Empty while the request is being written.
            let Some(nonce) = std::fs::read_to_string(&marker).ok().filter(|n| !n.is_empty()) else { continue };
            let _ = std::fs::remove_file(&marker);
            let (answer, restart_now) = match update::check().await {
                Err(e) => (format!("failed: {e:#}"), false),
                Ok(None) => (format!("current {}", update::VERSION), false),
                Ok(Some(version)) => match update::check_and_apply(&state_dir, true).await {
                    Ok(true) => (format!("updated {version}"), true),
                    Ok(false) => (format!("current {}", update::VERSION), false),
                    Err(e) => (format!("failed: {e:#}"), false),
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
pub(super) fn record_cycle(hub: &Hub, result: Result<Report, SyncError>) {
    match result {
        Ok(r) => {
            log_report(&r);
            let last_error = hub.errors.locked().last().cloned().unwrap_or_default();
            hub.status.edit(|s| {
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
            hub.online.store(false, Ordering::SeqCst);
            hub.status.edit(|s| {
                s.online = false;
                s.message = format!("upload interrupted: {m}");
            });
        }
        Err(SyncError::File(m) | SyncError::Refused(m)) => hub.status.edit(|s| s.last_error = m),
    }
}
