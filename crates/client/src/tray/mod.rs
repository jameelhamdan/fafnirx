//! Tray icon: a hard drive whose colour shows the state of the running client. Its activity light
//! blinks while the client is syncing. The menu shows what is connected and what is happening, and
//! controls the client. A separate process from `fafnirx run`: it reads `status.json` and talks to the
//! daemon through marker files in the state directory (pause, sync now, stop).

mod icon;
mod open;
mod text;

use crate::config::{self, Config, Folder, Shared};
use crate::daemon;
use anyhow::{Context, Result};
use fafnirx_common::unix_now;
use icon::{icon, Light};
use open::{
    check_for_updates, install_update, look_up_shared, open_path, open_url, restart_if_updated, run_cli, start_client,
    Updates,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use text::{describe, view, Info, View};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};

const IDLE_REFRESH: Duration = Duration::from_secs(2);
const BLINK: Duration = Duration::from_millis(450);
const SHARED_LOOKUP: Duration = Duration::from_secs(60);

#[derive(Clone)]
enum Action {
    Quit,
    TogglePause,
    SyncNow,
    CheckUpdates,
    ToggleClient,
    OpenFile(PathBuf),
    OpenFolder(PathBuf),
    OpenUrl(String),
    AddFolders,
    RemoveFolder(String),
    /// Sync a local folder, to be chosen, with this existing shared folder.
    AttachShared(String),
    /// Make a shared folder from a local folder, to be chosen.
    NewShared,
    DetachShared(String),
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
    folders: Submenu,
    add_folders: MenuItem,
    /// Actions of the fixed items. The folder entries come and go, so their actions are kept apart.
    actions: Actions,
    folder_actions: Actions,
    shown_folders: Vec<Folder>,
    shared: Submenu,
    shared_actions: Actions,
    /// The line of each attached folder that says how it is doing.
    shared_states: Vec<(String, Line)>,
    /// The attached folders and the shared folders of the master that are not attached, as the submenu shows them.
    shown_shared: (Vec<Shared>, Vec<String>),
}

impl Menus {
    fn build(config_path: &Path, state_dir: &Path, cfg: &Config) -> Result<Self> {
        let info = |text| Line::new(text, false);
        let control = |text| Line::new(text, true);
        let (title, server, device) = (info("fafnirx"), info("Server"), info("Device"));
        let (now, last_sync, on_server, session, error) = (info("..."), info(""), info(""), info(""), info(""));
        let (pause, sync, check) = (control("Pause"), control("Sync now"), control("Check for updates"));
        let updates = info("Updates: not checked");
        let downloads = control("Downloads on GitHub...");
        let client = control("Stop client");
        let quit = control("Quit tray");
        let add_folders = MenuItem::new("Add folders...", true, None);
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
            (downloads.item.id().clone(), Action::OpenUrl(crate::release::releases_page())),
            (client.item.id().clone(), Action::ToggleClient),
            (quit.item.id().clone(), Action::Quit),
            (add_folders.id().clone(), Action::AddFolders),
            (open[0].item.id().clone(), Action::OpenFile(config_path.to_path_buf())),
            (open[1].item.id().clone(), Action::OpenFile(state_dir.join("fafnirx.log"))),
            (open[2].item.id().clone(), Action::OpenFolder(state_dir.to_path_buf())),
            (open[3].item.id().clone(), Action::OpenUrl(cfg.dashboard_url())),
        ];

        let folders = Submenu::new("Folders", true);
        let folder_actions = fill_folders_menu(&folders, &add_folders, &cfg.folders);
        let shared = Submenu::new("Shared folders", true);
        let (shared_actions, shared_states) = fill_shared_menu(&shared, &cfg.shared, &[]);
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
            &downloads.item,
            &sep(),
            &folders,
            &shared,
            &sep(),
            &open[0].item,
            &open[1].item,
            &open[2].item,
            &open[3].item,
            &sep(),
            &client.item,
            &quit.item,
        ])?;
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
            folders,
            add_folders,
            actions,
            folder_actions,
            shown_folders: cfg.folders.clone(),
            shared,
            shared_actions,
            shared_states,
            shown_shared: (cfg.shared.clone(), Vec::new()),
        })
    }

    /// Shows the current state.
    fn show(&mut self, info: &Info, paused: bool, stopped: bool, update_text: &str, available: Option<&str>) {
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

    /// Rebuilds the folder list when the config file changed (by the tray, the CLI or by hand).
    fn sync_folders(&mut self, folders: &[Folder]) {
        if folders != self.shown_folders {
            self.folder_actions = fill_folders_menu(&self.folders, &self.add_folders, folders);
            self.shown_folders = folders.to_vec();
        }
    }

    /// Rebuilds the shared folders when the config or the master's list of them changed.
    fn sync_shared(&mut self, attached: &[Shared], on_master: &[String]) {
        let available: Vec<String> =
            on_master.iter().filter(|n| !attached.iter().any(|s| &s.name == *n)).cloned().collect();
        if (attached, available.as_slice()) != (self.shown_shared.0.as_slice(), self.shown_shared.1.as_slice()) {
            (self.shared_actions, self.shared_states) = fill_shared_menu(&self.shared, attached, &available);
            self.shown_shared = (attached.to_vec(), available);
        }
    }

    /// Says how each attached shared folder is doing, as the client last wrote it.
    fn show_shared_state(&mut self, status: Option<&daemon::Status>, now: i64) {
        for (name, line) in &mut self.shared_states {
            let text = match status.and_then(|s| s.shared.iter().find(|f| f.name == *name)) {
                Some(f) => f.summary(now),
                None => "waiting for the client".to_string(),
            };
            line.set(&text);
        }
    }

    fn action(&self, id: &MenuId) -> Option<Action> {
        self.actions
            .iter()
            .chain(&self.folder_actions)
            .chain(&self.shared_actions)
            .find(|(i, _)| i == id)
            .map(|(_, a)| a.clone())
    }
}

/// Fills the "Folders" submenu: one entry per collected folder, then "Add folders...".
fn fill_folders_menu(menu: &Submenu, add_item: &MenuItem, folders: &[Folder]) -> Actions {
    for _ in 0..menu.items().len() {
        menu.remove_at(0);
    }
    let mut actions = Actions::new();
    if folders.is_empty() {
        let _ = menu.append(&MenuItem::new("No folders yet", false, None));
    }
    for f in folders {
        let path = MenuItem::new(f.path.display().to_string(), false, None);
        let open = MenuItem::new("Open folder", true, None);
        let stop = MenuItem::new("Stop collecting", true, None);
        actions.push((open.id().clone(), Action::OpenFolder(f.path.clone())));
        actions.push((stop.id().clone(), Action::RemoveFolder(f.name.clone())));
        if let Ok(sub) = Submenu::with_items(&f.name, true, &[&path, &PredefinedMenuItem::separator(), &open, &stop]) {
            let _ = menu.append(&sub);
        }
    }
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(add_item);
    actions
}

/// Fills the "Shared folders" submenu: the folders synced here (⇄), the shared folders of the master that are not
/// synced here yet, and the way to make a new one.
fn fill_shared_menu(menu: &Submenu, attached: &[Shared], available: &[String]) -> (Actions, Vec<(String, Line)>) {
    for _ in 0..menu.items().len() {
        menu.remove_at(0);
    }
    let mut actions = Actions::new();
    // One line per attached folder that says how it is doing; `show_shared_state` keeps it current.
    let mut states = Vec::new();
    for s in attached {
        let state = Line::new("waiting for the client", false);
        let path = MenuItem::new(s.path.display().to_string(), false, None);
        let open = MenuItem::new("Open folder", true, None);
        let stop = MenuItem::new("Stop syncing here", true, None);
        actions.push((open.id().clone(), Action::OpenFolder(s.path.clone())));
        actions.push((stop.id().clone(), Action::DetachShared(s.name.clone())));
        let title = format!("{} \u{21c4}", s.name);
        let items: [&dyn tray_icon::menu::IsMenuItem; 5] =
            [&state.item, &path, &PredefinedMenuItem::separator(), &open, &stop];
        if let Ok(sub) = Submenu::with_items(&title, true, &items) {
            let _ = menu.append(&sub);
        }
        states.push((s.name.clone(), state));
    }
    if !attached.is_empty() && !available.is_empty() {
        let _ = menu.append(&PredefinedMenuItem::separator());
    }
    for name in available {
        let item = MenuItem::new(format!("Sync \"{name}\" with a folder..."), true, None);
        actions.push((item.id().clone(), Action::AttachShared(name.clone())));
        let _ = menu.append(&item);
    }
    if !attached.is_empty() || !available.is_empty() {
        let _ = menu.append(&PredefinedMenuItem::separator());
    }
    let new = MenuItem::new("New shared folder from a folder...", true, None);
    actions.push((new.id().clone(), Action::NewShared));
    let _ = menu.append(&new);
    (actions, states)
}

/// A name the master accepts for a shared folder, made from the name of a directory.
fn shared_name_of(dir: &Path) -> Option<String> {
    let raw = dir.file_name()?.to_string_lossy().to_lowercase();
    let name: String =
        raw.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '-' }).collect();
    let name = name.trim_matches(['-', '.', '_']).chars().take(63).collect::<String>();
    fafnirx_common::valid_device_name(&name).then_some(name)
}

/// Asks for a folder and syncs it with the shared folder `name`, made first when `create` is set.
fn attach_shared_dialog(config_path: &Path, name: &str, dir: &Path, create: bool) {
    let mut args = vec!["shared".into(), "attach".into(), name.into(), dir.as_os_str().to_owned()];
    if create {
        args.push("--create".into());
    }
    if let Err(e) = run_cli(config_path, &args) {
        message(rfd::MessageLevel::Error, format!("Could not sync \"{name}\":\n\n{e}")).show();
    }
}

fn ask_yes_no(level: rfd::MessageLevel, text: String) -> bool {
    message(level, text).set_buttons(rfd::MessageButtons::YesNo).show() == rfd::MessageDialogResult::Yes
}

/// Asks, and only when the answer is yes runs change, saying so when it fails.
fn confirm_then(text: String, change: impl FnOnce() -> anyhow::Result<()>) {
    if ask_yes_no(rfd::MessageLevel::Warning, text) {
        if let Err(e) = change() {
            message(rfd::MessageLevel::Error, format!("{e:#}")).show();
        }
    }
}

/// Syncs a folder chosen by the user with an existing shared folder.
fn attach_existing_shared(config_path: &Path, name: &str) {
    let Some(dir) =
        rfd::FileDialog::new().set_title(format!("Choose the folder to sync with \"{name}\"")).pick_folder()
    else {
        return;
    };
    let text = format!(
        "Sync {} with the shared folder \"{name}\"?\n\nFiles from both are merged: what is only on one side is copied to the \
         other, and if a file differs on both sides both are kept.",
        dir.display()
    );
    if ask_yes_no(rfd::MessageLevel::Info, text) {
        attach_shared_dialog(config_path, name, &dir, false);
    }
}

/// Makes a shared folder named after a folder chosen by the user, and syncs that folder with it.
fn new_shared(config_path: &Path, on_master: &[(String, bool)]) {
    let Some(dir) = rfd::FileDialog::new().set_title("Choose the folder to share").pick_folder() else { return };
    let Some(name) = shared_name_of(&dir) else {
        message(
            rfd::MessageLevel::Warning,
            format!(
                "\"{}\" cannot be the name of a shared folder. Use the command line to choose one:\nfafnirx shared attach <name> <folder> --create",
                dir.display()
            ),
        )
        .show();
        return;
    };
    // Only the user's own folders can clash with the name: another user's is `<owner>/<name>`.
    if on_master.iter().any(|(n, own)| *own && n.rsplit('/').next() == Some(name.as_str())) {
        let text = format!(
            "There is a shared folder called \"{name}\" already.\n\nSync {} with it? Files from both are merged.",
            dir.display()
        );
        if ask_yes_no(rfd::MessageLevel::Info, text) {
            attach_shared_dialog(config_path, &name, &dir, false);
        }
    } else if ask_yes_no(
        rfd::MessageLevel::Info,
        format!("Make the shared folder \"{name}\" and keep {} in sync with it?", dir.display()),
    ) {
        attach_shared_dialog(config_path, &name, &dir, true);
    }
}

fn detach_shared_dialog(config_path: &Path, name: &str) {
    let text = format!("Stop syncing \"{name}\" on this computer?\n\nNothing is deleted: the files stay in the folder and on the server.");
    confirm_then(text, || config::detach_shared_from_file(config_path, name));
}

fn message(level: rfd::MessageLevel, text: String) -> rfd::MessageDialog {
    rfd::MessageDialog::new().set_level(level).set_title("fafnirx").set_description(text)
}

/// Asks which folders to collect (several can be picked at once) and adds them to the config.
/// Returns whether the dialog was used.
fn add_folders_dialog(config_path: &Path) -> bool {
    let Some(picked) = rfd::FileDialog::new().set_title("Choose the folders to collect").pick_folders() else {
        return false;
    };
    let problems: Vec<String> = picked
        .iter()
        .filter_map(|dir| {
            config::add_folder_to_file(config_path, dir, None).err().map(|e| format!("{}\n{e:#}", dir.display()))
        })
        .collect();
    if !problems.is_empty() {
        message(rfd::MessageLevel::Warning, format!("Some folders were not added:\n\n{}", problems.join("\n\n")))
            .show();
    }
    true
}

fn remove_folder_dialog(config_path: &Path, name: &str) {
    let text = format!("Stop collecting \"{name}\"?\n\nFiles already uploaded stay on the server.");
    confirm_then(text, || config::remove_folder_from_file(config_path, name));
}

/// Everything an action needs to do its work.
struct Env {
    config_path: PathBuf,
    state_dir: PathBuf,
    updates: Updates,
    /// The shared folders the master lists for this user, as last looked up.
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
                Some(version) => install_update(
                    self.config_path.clone(),
                    self.state_dir.clone(),
                    version,
                    !client_stopped,
                    self.updates.clone(),
                ),
                None => check_for_updates(self.config_path.clone(), self.updates.clone()),
            },
            Action::ToggleClient if client_stopped => start_client(&self.config_path),
            Action::ToggleClient => Self::touch(daemon::stop_marker(&self.state_dir)),
            Action::OpenFile(p) => open_path(&p, true),
            Action::OpenFolder(p) => open_path(&p, false),
            Action::OpenUrl(u) => open_url(&u),
            Action::AddFolders => {
                if add_folders_dialog(&self.config_path) {
                    Self::touch(daemon::sync_now_marker(&self.state_dir));
                }
            }
            Action::RemoveFolder(name) => remove_folder_dialog(&self.config_path, &name),
            Action::AttachShared(name) => attach_existing_shared(&self.config_path, &name),
            Action::NewShared => {
                let known = self.on_master.lock().map(|l| l.clone()).unwrap_or_default();
                new_shared(&self.config_path, &known);
            }
            Action::DetachShared(name) => detach_shared_dialog(&self.config_path, &name),
        }
        true
    }
}

pub fn run(config_path: PathBuf) -> Result<()> {
    let cfg = Config::load(&config_path)?;
    let state_dir = cfg.state_dir(&config_path);
    std::fs::create_dir_all(&state_dir)?;
    // A second tray would put a second icon in the notification area.
    // Fix the path of the program before anything replaces it (see `open::exe`).
    open::exe();
    // A tray that is replaced by a new one after an update starts it first, so it may have to wait for the old one to go.
    if !crate::instance::claim(&state_dir, "tray", 20)? {
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

    let event_loop = EventLoopBuilder::new().build();
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
            let info = describe(status.as_ref(), &current, unix);
            if now >= next_lookup {
                // The master is asked now and then, not at every refresh: the list changes rarely.
                next_lookup = now + SHARED_LOOKUP;
                look_up_shared(config_path.clone(), env.on_master.clone());
                restart_if_updated(config_path.clone(), env.updates.clone());
            }
            if let Ok(c) = Config::load(&config_path) {
                menus.sync_folders(&c.folders);
                let on_master: Vec<String> =
                    env.on_master.lock().map(|l| l.iter().map(|(n, _)| n.clone()).collect()).unwrap_or_default();
                menus.sync_shared(&c.shared, &on_master);
            }
            menus.show_shared_state(status.as_ref(), unix);
            let update_text = env.updates.text.lock().map(|u| u.clone()).unwrap_or_default();
            menus.show(&info, paused, current.stopped, &update_text, env.updates.available().as_deref());
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
