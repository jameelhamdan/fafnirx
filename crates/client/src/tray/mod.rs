//! Tray icon: a hard drive whose colour shows the state of the running client. Its activity light
//! blinks while the client is syncing. The menu shows what is connected and what is happening, and
//! controls the client. A separate process from `fafnirx run`: it reads `status.json` and talks to the
//! daemon through marker files in the state directory (pause, sync now, stop).

mod icon;
mod open;
mod text;

use crate::config::{self, Config, Folder};
use crate::daemon;
use anyhow::{Context, Result};
use fafnirx_common::unix_now;
use icon::{icon, Light};
use open::{check_for_updates, open_path, open_url, start_client};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use text::{describe, view, Info, View};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{TrayIcon, TrayIconBuilder};

const IDLE_REFRESH: Duration = Duration::from_secs(2);
const BLINK: Duration = Duration::from_millis(450);

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
    client: Line,
    folders: Submenu,
    add_folders: MenuItem,
    /// Actions of the fixed items. The folder entries come and go, so their actions are kept apart.
    actions: Actions,
    folder_actions: Actions,
    shown_folders: Vec<(String, PathBuf)>,
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
            client,
            folders,
            add_folders,
            actions,
            folder_actions,
            shown_folders: folder_list(&cfg.folders),
        })
    }

    /// Shows the current state.
    fn show(&mut self, info: &Info, paused: bool, stopped: bool, update_text: &str) {
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
        let list = folder_list(folders);
        if list != self.shown_folders {
            self.folder_actions = fill_folders_menu(&self.folders, &self.add_folders, folders);
            self.shown_folders = list;
        }
    }

    fn action(&self, id: &MenuId) -> Option<Action> {
        self.actions.iter().chain(&self.folder_actions).find(|(i, _)| i == id).map(|(_, a)| a.clone())
    }
}

fn folder_list(folders: &[Folder]) -> Vec<(String, PathBuf)> {
    folders.iter().map(|f| (f.name.clone(), f.path.clone())).collect()
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
    let ask = message(
        rfd::MessageLevel::Warning,
        format!("Stop collecting \"{name}\"?\n\nFiles already uploaded stay on the server."),
    );
    if ask.set_buttons(rfd::MessageButtons::YesNo).show() != rfd::MessageDialogResult::Yes {
        return;
    }
    if let Err(e) = config::remove_folder_from_file(config_path, name) {
        message(rfd::MessageLevel::Error, format!("{e:#}")).show();
    }
}

/// Everything an action needs to do its work.
struct Env {
    config_path: PathBuf,
    state_dir: PathBuf,
    update_result: Arc<Mutex<String>>,
}

impl Env {
    fn touch(&self, marker: PathBuf) {
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
            Action::TogglePause => self.touch(pause_marker),
            Action::SyncNow => self.touch(daemon::sync_now_marker(&self.state_dir)),
            Action::CheckUpdates => check_for_updates(self.config_path.clone(), self.update_result.clone()),
            Action::ToggleClient if client_stopped => start_client(&self.config_path),
            Action::ToggleClient => self.touch(daemon::stop_marker(&self.state_dir)),
            Action::OpenFile(p) => open_path(&p, true),
            Action::OpenFolder(p) => open_path(&p, false),
            Action::OpenUrl(u) => open_url(&u),
            Action::AddFolders => {
                if add_folders_dialog(&self.config_path) {
                    self.touch(daemon::sync_now_marker(&self.state_dir));
                }
            }
            Action::RemoveFolder(name) => remove_folder_dialog(&self.config_path, &name),
        }
        true
    }
}

pub fn run(config_path: PathBuf) -> Result<()> {
    let cfg = Config::load(&config_path)?;
    let state_dir = cfg.state_dir(&config_path);
    std::fs::create_dir_all(&state_dir)?;
    let mut menus = Menus::build(&config_path, &state_dir, &cfg)?;
    let env = Env {
        config_path: config_path.clone(),
        state_dir: state_dir.clone(),
        update_result: Arc::new(Mutex::new("Updates: not checked".to_string())),
    };
    let marker = daemon::paused_marker(&state_dir);

    let event_loop = EventLoopBuilder::new().build();
    let mut tray: Option<TrayIcon> = None;
    let mut shown: Option<(Light, bool)> = None;
    let mut current = View { light: Light::Stopped, text: String::new(), syncing: false };
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

        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            if let Some(action) = menus.action(&ev.id) {
                if !env.perform(action, current.light == Light::Stopped) {
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
            if let Ok(c) = Config::load(&config_path) {
                menus.sync_folders(&c.folders);
            }
            let updates = env.update_result.lock().map(|u| u.clone()).unwrap_or_default();
            menus.show(&info, paused, current.light == Light::Stopped, &updates);
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
