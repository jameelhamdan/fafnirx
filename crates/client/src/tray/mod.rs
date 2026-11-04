//! Tray icon: a hard drive whose colour shows the state of the running client. Its activity light
//! blinks while the client is syncing. The menu shows what is connected and what is happening, and
//! controls the client. A separate process from `fafnirx run`: it reads `status.json` and talks to the
//! daemon through marker files in the state directory (pause, sync now, stop).

mod icon;
mod open;
mod text;

pub use open::ensure_tray;

use crate::config::{control_dir, Config, Folder, Mode, SyncedFolder};
use crate::daemon::{self, requests::Request};
use crate::service::disk_access;
use anyhow::{Context, Result};
use fafnirx_common::{unix_now, LockExt};
use icon::{icon, Light};
use open::{asked_to_restart, change_folders, check_for_updates, install_update, open_path, open_url, restart_if_updated, restart_tray, start_client, Updates};
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
    RemoveFolder(String),
    /// Sync a local folder, to be chosen, with this existing synced folder.
    AttachSynced(String),
    /// Make a synced folder from a local folder, to be chosen.
    NewShared,
    DetachSynced(String),
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
    shown_folders: Vec<Folder>,
    synced: Submenu,
    synced_actions: Actions,
    /// The line of each attached folder that says how it is doing.
    synced_states: Vec<(String, Line)>,
    /// The attached folders and the synced folders of the master that are not attached, as the submenu shows them.
    shown_synced: (Vec<SyncedFolder>, Vec<String>),
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
        let access = control("Full Disk Access settings...");
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
            (downloads.item.id().clone(), Action::OpenUrl(crate::update::release::releases_page())),
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
        let folder_actions = fill_folders_menu(&folders, &add_folders, &cfg.folders);
        let synced = Submenu::new("SyncedFolder folders", true);
        let (synced_actions, synced_states) = fill_synced_menu(&synced, &cfg.synced, &[]);
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
            &synced,
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
            shown_folders: cfg.folders.clone(),
            synced,
            synced_actions,
            synced_states,
            shown_synced: (cfg.synced.clone(), Vec::new()),
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

    /// The entry for Full Disk Access says so when a folder is blocked for want of it.
    fn show_access(&mut self, blocked: Option<&Path>) {
        match blocked {
            Some(p) => self.access.set(&format!("Allow Full Disk Access (needed for {})...", p.display())),
            None => self.access.set("Full Disk Access settings..."),
        }
    }

    /// Rebuilds the folder list when the config file changed (by the tray, the CLI or by hand).
    fn sync_folders(&mut self, folders: &[Folder]) {
        if folders != self.shown_folders {
            self.folder_actions = fill_folders_menu(&self.folders, &self.add_folders, folders);
            self.shown_folders = folders.to_vec();
        }
    }

    /// Rebuilds the synced folders when the config or the master's list of them changed.
    fn sync_synced(&mut self, attached: &[SyncedFolder], on_master: &[String]) {
        let available: Vec<String> = on_master.iter().filter(|n| !attached.iter().any(|s| &s.name == *n)).cloned().collect();
        if (attached, available.as_slice()) != (self.shown_synced.0.as_slice(), self.shown_synced.1.as_slice()) {
            (self.synced_actions, self.synced_states) = fill_synced_menu(&self.synced, attached, &available);
            self.shown_synced = (attached.to_vec(), available);
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
        self.actions
            .iter()
            .chain(&self.folder_actions)
            .chain(&self.synced_actions)
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

/// Fills the "SyncedFolder folders" submenu: the folders synced here (⇄), the synced folders of the master that are not
/// synced here yet, and the way to make a new one.
fn fill_synced_menu(menu: &Submenu, attached: &[SyncedFolder], available: &[String]) -> (Actions, Vec<(String, Line)>) {
    for _ in 0..menu.items().len() {
        menu.remove_at(0);
    }
    let mut actions = Actions::new();
    // One line per attached folder that says how it is doing; `show_synced_state` keeps it current.
    let mut states = Vec::new();
    for s in attached {
        let state = Line::new("waiting for the client", false);
        let path = MenuItem::new(s.path.display().to_string(), false, None);
        let open = MenuItem::new("Open folder", true, None);
        let stop = MenuItem::new("Stop syncing here", true, None);
        actions.push((open.id().clone(), Action::OpenFolder(s.path.clone())));
        actions.push((stop.id().clone(), Action::DetachSynced(s.name.clone())));
        let (conflicts, conflict_actions) = conflict_menu(s);
        actions.extend(conflict_actions);
        let title = format!("{} \u{21c4}", s.name);
        let items: [&dyn tray_icon::menu::IsMenuItem; 6] = [&state.item, &path, &PredefinedMenuItem::separator(), &open, &conflicts, &stop];
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
        actions.push((item.id().clone(), Action::AttachSynced(name.clone())));
        let _ = menu.append(&item);
    }
    if !attached.is_empty() || !available.is_empty() {
        let _ = menu.append(&PredefinedMenuItem::separator());
    }
    let new = MenuItem::new("New folder from a local folder...", true, None);
    actions.push((new.id().clone(), Action::NewShared));
    let _ = menu.append(&new);
    (actions, states)
}

/// The mode of this folder, one entry checked, and (with `ask`) the way to be asked again about what waits.
fn conflict_menu(synced: &SyncedFolder) -> (Submenu, Actions) {
    let mut actions = Actions::new();
    let sub = Submenu::new("How it syncs", true);
    let choices = [
        (Mode::Both, "Both ways (the server's wins a clash; mine is put aside)"),
        (Mode::Up, "Send only (nothing comes back)"),
        (Mode::Mine, "Both ways, mine wins a clash (replaces the server's)"),
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

/// Files already asked about (folder, path), so that a question is shown once; forgotten when the file is no longer waiting.
static ASKED: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Shows the question for every file that waits and was not asked about yet. It runs apart from the menu, which must go on working.
fn ask_about_conflicts(config_path: &Path, status: Option<&daemon::Status>) {
    let mut asked = ASKED.locked();
    // No status (the client is restarting): not a reason to forget what was asked.
    let Some(status) = status else { return };
    let waiting: Vec<(String, daemon::ConflictInfo)> = status.synced.iter().flat_map(|f| f.conflicts.iter().map(|c| (f.name.clone(), c.clone()))).collect();
    asked.retain(|(folder, path)| waiting.iter().any(|(f, c)| f == folder && &c.path == path));
    let new: Vec<_> = waiting.into_iter().filter(|(f, c)| !asked.contains(&(f.clone(), c.path.clone()))).collect();
    if new.is_empty() {
        return;
    }
    asked.extend(new.iter().map(|(f, c)| (f.clone(), c.path.clone())));
    let config_path = config_path.to_path_buf();
    std::thread::spawn(move || resolve_conflicts(&config_path, new));
}

/// Asks about each file, like the dialog of a file manager that finds a file in the place where one is copied. Closing it
/// leaves the file waiting (the menu asks again).
fn resolve_conflicts(config_path: &Path, files: Vec<(String, daemon::ConflictInfo)>) {
    let total = files.len();
    for (n, (folder, file)) in files.into_iter().enumerate() {
        let Some(keep) = ask_which(&folder, &file) else { continue };
        let request = Request::Resolve { name: folder.clone(), keep: keep.into(), path: Some(file.path.clone()) };
        // The same for all that remain: asked once, and only when there is more than one left.
        if let Err(e) = change_folders(config_path, &request) {
            message(rfd::MessageLevel::Error, format!("Could not keep the file:\n\n{e}")).show();
            continue;
        }
        let left = total - n - 1;
        if left > 0
            && ask_yes_no(
                rfd::MessageLevel::Info,
                format!("Do the same for the other {left} files that changed here and on the server?"),
            )
        {
            let all = Request::Resolve { name: folder, keep: keep.into(), path: None };
            if let Err(e) = change_folders(config_path, &all) {
                message(rfd::MessageLevel::Error, format!("Could not keep the files:\n\n{e}")).show();
            }
            return;
        }
    }
}

/// What the person chose for one file: `theirs`, `mine` or `both`; `None` to decide later (closing the window).
fn ask_which(folder: &str, file: &daemon::ConflictInfo) -> Option<&'static str> {
    use rfd::{MessageButtons, MessageDialogResult::Custom};
    const THEIRS: &str = "Keep the server's";
    const MINE: &str = "Keep mine";
    const BOTH: &str = "Keep both";
    let ago = |unix: i64| format!("changed {} ago", daemon::age(unix_now() - unix));
    let server = if file.deleted {
        "deleted".to_string()
    } else {
        format!("{}, {}", text::human_bytes(file.master_size), ago(file.master_mtime))
    };
    let body = format!(
        "\"{}\" in \"{folder}\" was changed on this computer and on the server.\n\nThis computer: {}, {}\nServer: {server}\n\nClosing this window decides later.",
        file.path,
        text::human_bytes(file.local_size),
        ago(file.local_mtime),
    );
    let first = message(rfd::MessageLevel::Warning, body)
        .set_buttons(MessageButtons::YesNoCancelCustom(THEIRS.into(), BOTH.into(), MINE.into()))
        .show();
    match first {
        Custom(c) if c == THEIRS => Some("theirs"),
        Custom(c) if c == BOTH => Some("both"),
        Custom(c) if c == MINE => {
            // Replacing what others did is not one click away.
            let sure = message(
                rfd::MessageLevel::Warning,
                format!("Keep this computer's \"{}\"? It replaces the server's version for every computer.", file.path),
            )
            .set_buttons(MessageButtons::OkCancelCustom("Keep mine".into(), "Decide later".into()))
            .show();
            matches!(sure, Custom(c) if c == MINE).then_some("mine")
        }
        // Where the system cannot show the button texts it shows Yes / No / Cancel, in the order of the buttons above: the
        // first two are the safe ones, and Cancel (the one that replaces the server's) is not taken for a choice.
        rfd::MessageDialogResult::Yes => Some("theirs"),
        rfd::MessageDialogResult::No => Some("both"),
        _ => None,
    }
}

/// A name the master accepts for a synced folder, made from the name of a directory.
fn synced_name_of(dir: &Path) -> Option<String> {
    let raw = dir.file_name()?.to_string_lossy().to_lowercase();
    let name: String = raw.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '-' }).collect();
    let name = name.trim_matches(['-', '.', '_']).chars().take(63).collect::<String>();
    fafnirx_common::valid_device_name(&name).then_some(name)
}

/// Asks for a folder and syncs it with the synced folder `name`, made first when `create` is set.
fn attach_synced_dialog(config_path: &Path, name: &str, dir: &Path, create: bool) {
    warn_before_attaching(config_path, &[dir.to_path_buf()]);
    let request = Request::AttachSynced { name: name.into(), path: dir.into(), create };
    if let Err(e) = change_folders(config_path, &request) {
        message(rfd::MessageLevel::Error, format!("Could not sync \"{name}\":\n\n{e}")).show();
    }
}

/// The first folder of `config` that the client cannot reach for want of Full Disk Access (macOS).
fn blocked_folder(status: Option<&daemon::Status>, config: &Config) -> Option<PathBuf> {
    let paths = config
        .folders
        .iter()
        .map(|f| f.path.as_path())
        .chain(config.synced.iter().map(|s| s.path.as_path()));
    disk_access::first_blocked(status.and_then(|s| s.full_disk_access), paths).map(Path::to_path_buf)
}

/// Tells the user once, when the tray finds a folder that is blocked, and offers to open the settings. It runs apart from the
/// menu, which must go on working.
fn ask_for_disk_access(blocked: &Path) {
    static ASKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if ASKED.swap(true, Ordering::SeqCst) {
        return;
    }
    let text = format!("{}\n\nOpen the settings now?", disk_access::blocked_text(blocked));
    std::thread::spawn(move || {
        if ask_yes_no(rfd::MessageLevel::Warning, text) {
            disk_access::open_settings();
        }
    });
}

/// Before folders are attached: when one is where macOS blocks the client, says so and offers the settings. The folders are
/// attached anyway; they sync once the permission is given and the client restarted.
fn warn_before_attaching(config_path: &Path, dirs: &[PathBuf]) {
    let Some(warning) = dirs.iter().find_map(|d| disk_access::warning_for(config_path, d)) else { return };
    if ask_yes_no(rfd::MessageLevel::Warning, format!("{warning}\n\nOpen the settings now?")) {
        disk_access::open_settings();
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

/// Syncs a folder chosen by the user with an existing synced folder.
fn attach_existing_synced(config_path: &Path, name: &str) {
    let Some(dir) = rfd::FileDialog::new().set_title(format!("Choose the folder to sync with \"{name}\"")).pick_folder() else {
        return;
    };
    let text = format!(
        "Sync {} with the synced folder \"{name}\"?\n\nFiles from both are merged: what is only on one side is copied to the \
         other, and if a file differs on both sides the server's is kept and yours is put aside \
         (the menu of the folder changes that).",
        dir.display()
    );
    if ask_yes_no(rfd::MessageLevel::Info, text) {
        attach_synced_dialog(config_path, name, &dir, false);
    }
}

/// A synced folder as people read it, with the user it belongs to in front: `ana/family`. Without the user (not known before
/// the client first reached the master) it is the bare name.
fn with_owner(owner: &str, name: &str) -> String {
    if owner.is_empty() || name.contains('/') {
        name.to_string()
    } else {
        format!("{owner}/{name}")
    }
}

/// The user this computer belongs to, from the name of the device (`<user>/<device>`) the client last got from the master.
fn owner_of(status: Option<&daemon::Status>) -> &str {
    status.and_then(|s| s.device.split_once('/')).map_or("", |(owner, _)| owner)
}

/// Makes a synced folder named after a folder chosen by the user, and syncs that folder with it. `owner` is the user, to
/// show the folder the way the master names it.
fn new_synced(config_path: &Path, on_master: &[(String, bool)], owner: &str) {
    let Some(dir) = rfd::FileDialog::new().set_title("Choose the folder to share").pick_folder() else { return };
    let Some(name) = synced_name_of(&dir) else {
        message(
            rfd::MessageLevel::Warning,
            format!(
                "\"{}\" cannot be the name of a synced folder. Use the command line to choose one:\nfafnirx folder add <folder> --target <name> --create",
                dir.display()
            ),
        )
        .show();
        return;
    };
    // Only the user's own folders can clash with the name: another user's is `<owner>/<name>`.
    let existing = on_master.iter().find(|(n, own)| *own && n.rsplit('/').next() == Some(name.as_str()));
    if let Some((full, _)) = existing {
        let text = format!(
            "There is a synced folder called \"{full}\" already.\n\nSync {} with it? Files from both are merged.",
            dir.display()
        );
        if ask_yes_no(rfd::MessageLevel::Info, text) {
            attach_synced_dialog(config_path, full, &dir, false);
        }
    } else if ask_yes_no(
        rfd::MessageLevel::Info,
        format!(
            "Make the synced folder \"{}\" and keep {} in sync with it?",
            with_owner(owner, &name),
            dir.display()
        ),
    ) {
        attach_synced_dialog(config_path, &name, &dir, true);
    }
}

fn detach_synced_dialog(config_path: &Path, name: &str) {
    let text = format!("Stop syncing \"{name}\" on this computer?\n\nNothing is deleted: the files stay in the folder and on the server.");
    confirm_then(text, || cli_change(config_path, &Request::DetachSynced { name: name.into() }));
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
    warn_before_attaching(config_path, &picked);
    // The config of a system-wide install is the administrators': the service makes the change for this user.
    if let Err(e) = change_folders(config_path, &Request::AddFolder { paths: picked }) {
        message(rfd::MessageLevel::Warning, format!("Some folders were not added:\n\n{e}")).show();
    }
    true
}

fn cli_change(config_path: &Path, request: &Request) -> anyhow::Result<()> {
    change_folders(config_path, request).map(drop).map_err(anyhow::Error::msg)
}

fn remove_folder_dialog(config_path: &Path, name: &str) {
    let text = format!("Stop collecting \"{name}\"?\n\nFiles already uploaded stay on the server.");
    confirm_then(text, || cli_change(config_path, &Request::RemoveFolder { name: name.into() }));
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
            Action::AttachSynced(name) => attach_existing_synced(&self.config_path, &name),
            Action::NewShared => {
                let known = self.on_master.locked().clone();
                new_synced(&self.config_path, &known, owner_of(daemon::read_status(&self.state_dir).as_ref()));
            }
            Action::DetachSynced(name) => detach_synced_dialog(&self.config_path, &name),
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
            let cfg_now = Config::load(&config_path).ok();
            let blocked = cfg_now.as_ref().and_then(|c| blocked_folder(status.as_ref(), c));
            let mut info = describe(status.as_ref(), &current, unix);
            if let Some(p) = &blocked {
                info.error = text::clip(&format!("Needs Full Disk Access: {}", p.display()));
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
                menus.sync_folders(&c.folders);
                let on_master: Vec<String> = env.on_master.locked().iter().map(|(n, _)| n.clone()).collect();
                menus.sync_synced(&c.synced, &on_master);
            }
            menus.show_synced_state(status.as_ref(), unix);
            ask_about_conflicts(&config_path, status.as_ref());
            let update_text = env.updates.text.locked().clone();
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

#[cfg(test)]
mod tests {
    #[test]
    fn a_tray_starts_over_only_when_the_marker_is_newer_than_it() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("restart-tray");
        let before = std::time::SystemTime::now();
        assert!(!super::asked_to_restart(&marker, before), "no marker, nothing to do");
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&marker, b"").unwrap();
        assert!(super::asked_to_restart(&marker, before), "asked after this tray started");
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(
            !super::asked_to_restart(&marker, std::time::SystemTime::now()),
            "the new tray is younger than the marker"
        );
    }

    use super::*;

    #[test]
    fn a_synced_folder_is_shown_with_its_owner() {
        assert_eq!(with_owner("ana", "family"), "ana/family");
        assert_eq!(with_owner("ana", "bo/trip"), "bo/trip", "already named in full");
        assert_eq!(with_owner("", "family"), "family", "before the master has said who the user is");
    }

    #[test]
    fn the_owner_is_the_user_part_of_the_device_name() {
        let status = |device: &str| daemon::Status { device: device.into(), ..Default::default() };
        assert_eq!(owner_of(Some(&status("ana/laptop"))), "ana");
        assert_eq!(owner_of(Some(&status(""))), "");
        assert_eq!(owner_of(None), "");
    }
}
