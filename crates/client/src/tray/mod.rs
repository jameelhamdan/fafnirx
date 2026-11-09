//! Tray icon: a hard drive whose colour shows the state of the running client. Its activity light
//! blinks while the client is syncing. The menu shows what is connected and what is happening, and
//! controls the client. A separate process from `fafnirx run`: it reads `status.json` and talks to the
//! daemon through marker files in the state directory (pause, sync now, stop).

mod dialogs;
mod icon;
mod open;
mod text;

pub use open::ensure_tray;

use crate::config::{control_dir, Config, Folder, Mode};
use crate::daemon::{self, requests::Request};
use anyhow::{Context, Result};
use dialogs::*;
use fafnirx_common::{unix_now, LockExt};
use icon::{icon, Light};
use open::{asked_to_restart, check_for_updates, install_update, open_path, open_url, restart_if_updated, restart_tray, start_client, Updates};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use text::{describe, view, Info, View};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};

const IDLE_REFRESH: Duration = Duration::from_secs(2);
const BLINK: Duration = Duration::from_millis(450);
/// How often the tray looks whether the program file on disk is a newer version than the one it runs.
const UPDATE_LOOKUP: Duration = Duration::from_secs(60);

#[derive(Clone)]
enum Action {
    Quit,
    TogglePause,
    SyncNow,
    CheckUpdates,
    ToggleClient,
    /// Open the page of System Settings where Full Disk Access is given (macOS).
    AllowDiskAccess,
    OpenFile(PathBuf),
    OpenFolder(PathBuf),
    OpenUrl(String),
    AddFolders,
    /// Stop syncing a folder of this computer, collected or attached to a folder of the master.
    RemoveFolder(String),
    /// Sync a local folder, to be chosen, with this existing synced folder.
    AttachSynced(String),
    /// Make a synced folder from a local folder, to be chosen.
    NewSynced,
    /// What the synced folder does with a file changed here and on the master.
    SetMode(String, Mode),
    /// Ask again about the files of this synced folder that wait for a decision.
    ResolveConflicts(String),
}

type Actions = Vec<(MenuId, Action)>;

/// A menu entry whose text changes. The menu is only touched when the text differs, because setting
/// it needlessly can make an open menu flicker.
struct Line {
    item: MenuItem,
    shown: String,
}

impl Line {
    fn new(text: &str, enabled: bool) -> Self {
        Self { item: MenuItem::new(text, enabled, None), shown: text.to_string() }
    }

    fn set(&mut self, text: &str) {
        if self.shown != text {
            self.item.set_text(text);
            self.shown = text.to_string();
        }
    }
}

/// The whole menu: information lines, controls and the folder list.
/// What the menus show besides the status: whether the client is paused or stopped, and the update line.
#[derive(Clone, Copy)]
struct Shown<'a> {
    paused: bool,
    stopped: bool,
    update_text: &'a str,
    available: Option<&'a str>,
}

struct Menus {
    menu: Menu,
    title: Line,
    server: Line,
    device: Line,
    now: Line,
    last_sync: Line,
    on_server: Line,
    session: Line,
    error: Line,
    pause: Line,
    updates: Line,
    /// "Check for updates", which becomes "Update to <version>" once a check found one.
    check: Line,
    client: Line,
    /// macOS: where Full Disk Access is given; it asks for it when the client lacks it.
    access: Line,
    folders: Submenu,
    add_folders: MenuItem,
    /// Actions of the fixed items. The folder entries come and go, so their actions are kept apart.
    actions: Actions,
    folder_actions: Actions,
    /// The line of each folder attached to a folder of the master that says how it is doing.
    synced_states: Vec<(String, Line)>,
    /// The folders and the folders of the master that are not attached, as the submenu shows them.
    shown_folders: (Vec<Folder>, Vec<(String, bool)>),
}

impl Menus {
    fn build(config_path: &Path, state_dir: &Path, cfg: &Config) -> Result<Self> {
        let info = |text| Line::new(text, false);
        let control = |text| Line::new(text, true);
        let (title, server, device) = (info("fafnirx"), info("Server"), info("Device"));
        let (now, last_sync, on_server, session, error) = (info("..."), info(""), info(""), info(""), info(""));
        let (pause, sync, check) = (control("Pause"), control("Sync now"), control("Check for updates"));
        let updates = info("Updates: not checked");
        let client = control("Stop client");
        let access = control("Full Disk Access settings...");
        let quit = control("Quit tray");
        let add_folders = MenuItem::new("Collect folders...", true, None);
        let open = [
            Line::new("Open config file", true),
            Line::new("Open log file", true),
            Line::new("Open client data folder", true),
            Line::new("Open web dashboard", true),
        ];

        let actions: Actions = vec![
            (pause.item.id().clone(), Action::TogglePause),
            (sync.item.id().clone(), Action::SyncNow),
            (check.item.id().clone(), Action::CheckUpdates),
            (client.item.id().clone(), Action::ToggleClient),
            (quit.item.id().clone(), Action::Quit),
            (access.item.id().clone(), Action::AllowDiskAccess),
            (add_folders.id().clone(), Action::AddFolders),
            (open[0].item.id().clone(), Action::OpenFile(config_path.to_path_buf())),
            (open[1].item.id().clone(), Action::OpenFile(daemon::log_file(state_dir))),
            (open[2].item.id().clone(), Action::OpenFolder(control_dir(state_dir))),
            (open[3].item.id().clone(), Action::OpenUrl(cfg.dashboard_url())),
        ];

        let folders = Submenu::new("Folders", true);
        let (folder_actions, synced_states) = fill_folders_menu(&folders, &add_folders, &cfg.folders, &[]);
        let sep = PredefinedMenuItem::separator;
        let menu = Menu::new();
        menu.append_items(&[
            &title.item,
            &server.item,
            &device.item,
            &sep(),
            &now.item,
            &last_sync.item,
            &on_server.item,
            &session.item,
            &error.item,
            &sep(),
            &pause.item,
            &sync.item,
            &check.item,
            &updates.item,
            &sep(),
            &folders,
            &sep(),
            &open[0].item,
            &open[1].item,
            &open[2].item,
            &open[3].item,
        ])?;
        if cfg!(target_os = "macos") {
            menu.append(&access.item)?;
        }
        menu.append_items(&[&sep(), &client.item, &quit.item])?;
        Ok(Self {
            menu,
            title,
            server,
            device,
            now,
            last_sync,
            on_server,
            session,
            error,
            pause,
            updates,
            check,
            client,
            access,
            folders,
            add_folders,
            actions,
            folder_actions,
            synced_states,
            shown_folders: (cfg.folders.clone(), Vec::new()),
        })
    }

    /// Shows the current state.
    fn show(&mut self, info: &Info, shown: &Shown) {
        let Shown { paused, stopped, update_text, available } = *shown;
        match available {
            Some(version) => self.check.set(&format!("Update to {version}")),
            None => self.check.set("Check for updates"),
        }
        self.title.set(&info.title);
        self.server.set(&info.server);
        self.device.set(&info.device);
        self.now.set(&info.now);
        self.last_sync.set(&info.last_sync);
        self.on_server.set(&info.on_server);
        self.session.set(&info.session);
        self.error.set(&info.error);
        self.pause.set(if paused { "Resume" } else { "Pause" });
        self.client.set(if stopped { "Start client" } else { "Stop client" });
        self.updates.set(update_text);
    }

    /// The entry for Full Disk Access says so when a folder is blocked for want of it.
    fn show_access(&mut self, blocked: Option<&Path>) {
        match blocked {
            Some(p) => self.access.set(&format!("Allow Full Disk Access (needed for {})...", p.display())),
            None => self.access.set("Full Disk Access settings..."),
        }
    }

    /// Rebuilds the folder list when the config or the master's list of folders changed (by the tray, the CLI or by hand).
    fn sync_folders(&mut self, folders: &[Folder], on_master: &[(String, bool)]) {
        let available: Vec<(String, bool)> = on_master
            .iter()
            .filter(|(n, _)| !folders.iter().any(|f| !f.is_computer() && &f.name == n))
            .cloned()
            .collect();
        if (folders, available.as_slice()) != (self.shown_folders.0.as_slice(), self.shown_folders.1.as_slice()) {
            (self.folder_actions, self.synced_states) = fill_folders_menu(&self.folders, &self.add_folders, folders, &available);
            self.shown_folders = (folders.to_vec(), available);
        }
    }

    /// Says how each attached synced folder is doing, as the client last wrote it.
    fn show_synced_state(&mut self, status: Option<&daemon::Status>, now: i64) {
        for (name, line) in &mut self.synced_states {
            let text = match status.and_then(|s| s.synced.iter().find(|f| f.name == *name)) {
                Some(f) => f.summary(now),
                None => "waiting for the client".to_string(),
            };
            line.set(&text);
        }
    }

    fn action(&self, id: &MenuId) -> Option<Action> {
        self.actions.iter().chain(&self.folder_actions).find(|(i, _)| i == id).map(|(_, a)| a.clone())
    }
}

/// Fills the "Folders" submenu: one entry per folder of this computer (⇄ for one synced with a folder of the master), the folders of
/// the master that are not synced here yet, and the ways to add one.
fn fill_folders_menu(menu: &Submenu, add_item: &MenuItem, folders: &[Folder], available: &[(String, bool)]) -> (Actions, Vec<(String, Line)>) {
    for _ in 0..menu.items().len() {
        menu.remove_at(0);
    }
    let mut actions = Actions::new();
    // One line per attached folder that says how it is doing; `show_synced_state` keeps it current.
    let mut states = Vec::new();
    if folders.is_empty() {
        let _ = menu.append(&MenuItem::new("No folders yet", false, None));
    }
    for f in folders {
        let path = MenuItem::new(f.path.display().to_string(), false, None);
        let open = MenuItem::new("Open folder", true, None);
        let stop = MenuItem::new(if f.is_computer() { "Stop collecting" } else { "Stop syncing here" }, true, None);
        actions.push((open.id().clone(), Action::OpenFolder(f.path.clone())));
        actions.push((stop.id().clone(), Action::RemoveFolder(f.name.clone())));
        let sub = if f.is_computer() {
            Submenu::with_items(&f.name, true, &[&path, &PredefinedMenuItem::separator(), &open, &stop])
        } else {
            let state = Line::new("waiting for the client", false);
            let (conflicts, conflict_actions) = conflict_menu(f);
            actions.extend(conflict_actions);
            let items: [&dyn tray_icon::menu::IsMenuItem; 6] = [&state.item, &path, &PredefinedMenuItem::separator(), &open, &conflicts, &stop];
            let sub = Submenu::with_items(format!("{} \u{21c4}", f.name), true, &items);
            states.push((f.name.clone(), state));
            sub
        };
        if let Ok(sub) = sub {
            let _ = menu.append(&sub);
        }
    }
    let _ = menu.append(&PredefinedMenuItem::separator());
    for (name, read_only) in available {
        let item = MenuItem::new(
            format!("Sync \"{name}\" with a folder...{}", if *read_only { " (read only)" } else { "" }),
            true,
            None,
        );
        actions.push((item.id().clone(), Action::AttachSynced(name.clone())));
        let _ = menu.append(&item);
    }
    let new = MenuItem::new("New folder from a local folder...", true, None);
    actions.push((new.id().clone(), Action::NewSynced));
    let _ = menu.append(&new);
    let _ = menu.append(add_item);
    (actions, states)
}

/// The mode of this folder, one entry checked, and (with `ask`) the way to be asked again about what waits.
fn conflict_menu(synced: &Folder) -> (Submenu, Actions) {
    let mut actions = Actions::new();
    let sub = Submenu::new("How it syncs", true);
    let choices = [
        (Mode::Both, "Both ways (the server's wins a clash; mine is put aside)"),
        (Mode::Up, "Send only (nothing comes back)"),
        (Mode::Keep, "Both ways, keep both on a clash"),
        (Mode::Ask, "Both ways, ask me on a clash"),
    ];
    for (mode, text) in choices {
        if mode == Mode::Ask && !Mode::ask_available() {
            continue;
        }
        let item = CheckMenuItem::new(text, true, synced.mode == mode, None);
        actions.push((item.id().clone(), Action::SetMode(synced.name.clone(), mode)));
        let _ = sub.append(&item);
    }
    if synced.mode == Mode::Ask {
        let _ = sub.append(&PredefinedMenuItem::separator());
        let again = MenuItem::new("Choose for the files that wait...", true, None);
        actions.push((again.id().clone(), Action::ResolveConflicts(synced.name.clone())));
        let _ = sub.append(&again);
    }
    (sub, actions)
}

/// Everything an action needs to do its work.
struct Env {
    config_path: PathBuf,
    state_dir: PathBuf,
    updates: Updates,
    /// The synced folders the master lists for this user, as last looked up.
    on_master: Arc<Mutex<Vec<(String, bool)>>>,
}

impl Env {
    /// Leaves a marker file for the running client to find.
    fn touch(marker: PathBuf) {
        let _ = std::fs::write(marker, b"");
    }

    /// Returns false when the tray should quit.
    fn perform(&self, action: Action, client_stopped: bool) -> bool {
        let pause_marker = daemon::paused_marker(&self.state_dir);
        match action {
            Action::Quit => return false,
            Action::TogglePause if pause_marker.exists() => {
                let _ = std::fs::remove_file(pause_marker);
            }
            Action::TogglePause => Self::touch(pause_marker),
            Action::SyncNow => Self::touch(daemon::sync_now_marker(&self.state_dir)),
            // The entry checks; once a check found a newer version it installs that one and starts everything again on it.
            Action::CheckUpdates => match self.updates.available() {
                Some(version) => install_update(self.config_path.clone(), version, self.updates.clone()),
                None => check_for_updates(self.config_path.clone(), self.updates.clone()),
            },
            Action::ToggleClient if client_stopped => start_client(&self.config_path),
            Action::ToggleClient => Self::touch(daemon::stop_marker(&self.state_dir)),
            Action::OpenFile(p) => open_path(&p, true),
            Action::OpenFolder(p) => open_path(&p, false),
            Action::OpenUrl(u) => open_url(&u),
            Action::AllowDiskAccess => crate::service::disk_access::open_settings(),
            Action::AddFolders => {
                if add_folders_dialog(&self.config_path) {
                    Self::touch(daemon::sync_now_marker(&self.state_dir));
                }
            }
            Action::RemoveFolder(name) => remove_folder_dialog(&self.config_path, &name),
            Action::AttachSynced(name) => {
                let read_only = daemon::read_status(&self.state_dir).is_some_and(|s| s.synced_readonly.contains(&name));
                attach_existing_synced(&self.config_path, &name, read_only);
            }
            Action::NewSynced => {
                let known = self.on_master.locked().clone();
                new_synced(&self.config_path, &known, owner_of(daemon::read_status(&self.state_dir).as_ref()));
            }
            Action::SetMode(name, mode) => {
                if let Err(e) = cli_change(&self.config_path, &Request::SetMode { name, mode }) {
                    message(rfd::MessageLevel::Error, format!("{e:#}")).show();
                }
            }
            Action::ResolveConflicts(name) => {
                if let Ok(mut asked) = ASKED.lock() {
                    asked.retain(|(folder, _)| *folder != name);
                }
                let waiting = daemon::read_status(&self.state_dir).is_some_and(|s| s.synced.iter().any(|f| f.name == name && !f.conflicts.is_empty()));
                if !waiting {
                    message(rfd::MessageLevel::Info, format!("No file of \"{name}\" is waiting for a choice.")).show();
                }
            }
        }
        true
    }
}

pub fn run(config_path: PathBuf) -> Result<()> {
    let cfg = Config::load(&config_path)?;
    let state_dir = cfg.state_dir(&config_path);
    let control = control_dir(&state_dir);
    std::fs::create_dir_all(&control)?;
    // A second tray would put a second icon in the notification area.
    // Fix the path of the program before anything replaces it (see `open::exe`).
    open::exe();
    // A tray that is replaced by a new one after an update starts it first, so it may have to wait for the old one to go.
    // One tray for each user who is logged in: in a system-wide install the control folder is synced.
    if !crate::service::instance::claim(&control, &format!("tray-{}", crate::service::instance::user_name()), 20)? {
        println!("the fafnirx tray is already running");
        return Ok(());
    }
    let mut menus = Menus::build(&config_path, &state_dir, &cfg)?;
    let env = Env {
        config_path: config_path.clone(),
        state_dir: state_dir.clone(),
        updates: Updates::new(),
        on_master: Arc::new(Mutex::new(Vec::new())),
    };
    let mut next_lookup = Instant::now();
    let marker = daemon::paused_marker(&state_dir);
    let (restart_marker, started) = (daemon::restart_tray_marker(&state_dir), std::time::SystemTime::now());

    let mut builder = EventLoopBuilder::new();
    // A menu-bar program: no Dock icon and no app menu, and it is not taken for an app that must be brought to the front.
    #[cfg(target_os = "macos")]
    tao::platform::macos::EventLoopBuilderExtMacOS::with_activation_policy(&mut builder, tao::platform::macos::ActivationPolicy::Accessory);
    let event_loop = builder.build();
    let mut tray: Option<TrayIcon> = None;
    let mut shown: Option<(Light, bool)> = None;
    let mut current = View { light: Light::Stopped, text: String::new(), syncing: false, stopped: true };
    let mut led = false;
    let (mut next_read, mut next_blink) = (Instant::now(), Instant::now());

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(100));
        // The icon has to be created once the event loop is running (required on macOS and Linux).
        if let Event::NewEvents(StartCause::Init) = event {
            let created = icon(Light::Stopped, false).and_then(|i| {
                TrayIconBuilder::new()
                    .with_menu(Box::new(menus.menu.clone()))
                    .with_tooltip("fafnirx")
                    .with_icon(i)
                    .build()
                    .context("create tray icon")
            });
            match created {
                Ok(t) => tray = Some(t),
                Err(e) => {
                    eprintln!("tray unavailable: {e:#}");
                    *flow = ControlFlow::Exit;
                    return;
                }
            }
        }

        // A new tray was started (after an update): this one is done.
        if env.updates.restart.load(Ordering::SeqCst) {
            *flow = ControlFlow::Exit;
            return;
        }

        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if let Some(action) = menus.action(&ev.id) {
                if !env.perform(action, current.stopped) {
                    *flow = ControlFlow::Exit;
                }
                next_read = Instant::now();
            }
        }

        let now = Instant::now();
        // Re-read the status often enough to follow a sync, rarely when idle.
        if now >= next_read {
            next_read = now + if current.syncing { BLINK } else { IDLE_REFRESH };
            let paused = marker.exists();
            let status = daemon::read_status(&state_dir);
            let unix = unix_now();
            current = view(status.as_ref(), paused, unix);
            let cfg_now = Config::load(&config_path).ok();
            let blocked = cfg_now.as_ref().and_then(|c| blocked_folder(status.as_ref(), c));
            let mut info = describe(status.as_ref(), &current, unix);
            if let Some(p) = &blocked {
                info.error = text::one_line(&format!("Needs Full Disk Access: {}", p.display()));
                ask_for_disk_access(p);
            }
            if now >= next_lookup {
                next_lookup = now + UPDATE_LOOKUP;
                restart_if_updated(config_path.clone(), env.updates.clone());
            }
            // `fafnirx restart` was run since this tray started.
            if asked_to_restart(&restart_marker, started) && !env.updates.restart.load(Ordering::SeqCst) {
                restart_tray(&config_path, &env.updates);
            }
            // The client asks the master for the synced folders (it holds the token); the tray only reads what it found.
            if let (Some(s), Ok(mut listed)) = (&status, env.on_master.lock()) {
                *listed = s.synced_on_master.clone();
            }
            menus.show_access(blocked.as_deref());
            if let Some(c) = &cfg_now {
                let readonly = status.as_ref().map(|s| s.synced_readonly.as_slice()).unwrap_or_default();
                let on_master: Vec<(String, bool)> = env.on_master.locked().iter().map(|(n, _)| (n.clone(), readonly.contains(n))).collect();
                menus.sync_folders(&c.folders, &on_master);
            }
            menus.show_synced_state(status.as_ref(), unix);
            ask_about_conflicts(&config_path, status.as_ref());
            let update_text = env.updates.text.locked().clone();
            menus.show(
                &info,
                &Shown {
                    paused,
                    stopped: current.stopped,
                    update_text: &update_text,
                    available: env.updates.available().as_deref(),
                },
            );
            if let Some(t) = &tray {
                let tip = if current.syncing { &info.now } else { &current.text };
                let _ = t.set_tooltip(Some(format!("fafnirx: {tip}").chars().take(120).collect::<String>()));
            }
        }
        if now >= next_blink {
            next_blink = now + BLINK;
            led = current.syncing && !led;
        }
        if !current.syncing {
            led = false;
        }
        if let Some(t) = &tray {
            if shown != Some((current.light, led)) {
                if let Ok(i) = icon(current.light, led) {
                    let _ = t.set_icon(Some(i));
                    shown = Some((current.light, led));
                }
            }
        }
    });
}

#[cfg(test)]
#[path = "../../tests/unit/tray/mod.rs"]
mod tests;
