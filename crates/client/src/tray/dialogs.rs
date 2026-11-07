//! The questions and confirmations a person answers in a window: conflicts, disk access, adding, attaching and removing folders.
//! They run apart from the menu, which must go on working.

use super::open::change_folders;
use super::text::{self};
use crate::config::Config;
use crate::daemon;
use crate::daemon::requests::Request;
use crate::service::disk_access;
use fafnirx_common::unix_now;
use fafnirx_common::LockExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Mutex;

/// Files already asked about (folder, path), so that a question is shown once; forgotten when the file is no longer waiting.
pub(super) static ASKED: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Shows the question for every file that waits and was not asked about yet. It runs apart from the menu, which must go on working.
pub(super) fn ask_about_conflicts(config_path: &Path, status: Option<&daemon::Status>) {
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
pub(super) fn resolve_conflicts(config_path: &Path, files: Vec<(String, daemon::ConflictInfo)>) {
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

/// What the person chose for one file: `theirs` or `both`; `None` to decide later (closing the window).
pub(super) fn ask_which(folder: &str, file: &daemon::ConflictInfo) -> Option<&'static str> {
    use rfd::{MessageButtons, MessageDialogResult::Custom};
    const THEIRS: &str = "Keep the server's";
    const LATER: &str = "Decide later";
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
        .set_buttons(MessageButtons::YesNoCancelCustom(THEIRS.into(), BOTH.into(), LATER.into()))
        .show();
    match first {
        Custom(c) if c == THEIRS => Some("theirs"),
        Custom(c) if c == BOTH => Some("both"),
        // Where the system cannot show the button texts it shows Yes / No / Cancel, in the order of the buttons above.
        rfd::MessageDialogResult::Yes => Some("theirs"),
        rfd::MessageDialogResult::No => Some("both"),
        _ => None,
    }
}

/// A name the master accepts for a synced folder, made from the name of a directory.
pub(super) fn synced_name_of(dir: &Path) -> Option<String> {
    let raw = dir.file_name()?.to_string_lossy().to_lowercase();
    let name: String = raw.chars().map(|c| if c.is_ascii_alphanumeric() || "._-".contains(c) { c } else { '-' }).collect();
    let name = name.trim_matches(['-', '.', '_']).chars().take(63).collect::<String>();
    fafnirx_common::valid_device_name(&name).then_some(name)
}

/// Asks for a folder and syncs it with the synced folder `name`, made first when `create` is set.
pub(super) fn attach_synced_dialog(config_path: &Path, name: &str, dir: &Path, create: bool) {
    warn_before_attaching(config_path, &[dir.to_path_buf()]);
    let request = Request::AttachSynced { name: name.into(), path: dir.into(), create };
    if let Err(e) = change_folders(config_path, &request) {
        message(rfd::MessageLevel::Error, format!("Could not sync \"{name}\":\n\n{e}")).show();
    }
}

/// The first folder of `config` that the client cannot reach for want of Full Disk Access (macOS).
pub(super) fn blocked_folder(status: Option<&daemon::Status>, config: &Config) -> Option<PathBuf> {
    let paths = config.folders.iter().map(|f| f.path.as_path());
    disk_access::first_blocked(status.and_then(|s| s.full_disk_access), paths).map(Path::to_path_buf)
}

/// Tells the user once, when the tray finds a folder that is blocked, and offers to open the settings. It runs apart from the
/// menu, which must go on working.
pub(super) fn ask_for_disk_access(blocked: &Path) {
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
pub(super) fn warn_before_attaching(config_path: &Path, dirs: &[PathBuf]) {
    let Some(warning) = dirs.iter().find_map(|d| disk_access::warning_for(config_path, d)) else { return };
    if ask_yes_no(rfd::MessageLevel::Warning, format!("{warning}\n\nOpen the settings now?")) {
        disk_access::open_settings();
    }
}

pub(super) fn ask_yes_no(level: rfd::MessageLevel, text: String) -> bool {
    message(level, text).set_buttons(rfd::MessageButtons::YesNo).show() == rfd::MessageDialogResult::Yes
}

/// Asks, and only when the answer is yes runs change, saying so when it fails.
pub(super) fn confirm_then(text: String, change: impl FnOnce() -> anyhow::Result<()>) {
    if ask_yes_no(rfd::MessageLevel::Warning, text) {
        if let Err(e) = change() {
            message(rfd::MessageLevel::Error, format!("{e:#}")).show();
        }
    }
}

/// Syncs a folder chosen by the user with an existing synced folder.
pub(super) fn attach_existing_synced(config_path: &Path, name: &str, read_only: bool) {
    let Some(dir) = rfd::FileDialog::new().set_title(format!("Choose the folder to sync with \"{name}\"")).pick_folder() else {
        return;
    };
    let rule = if read_only {
        "It is shared with you to read only: its files come to this folder, and nothing of this folder is sent."
    } else {
        "Files from both are merged: what is only on one side is copied to the other, and if a file differs on both sides the \
         server's is kept and yours is put aside (the menu of the folder changes that)."
    };
    let text = format!("Sync {} with the synced folder \"{name}\"?\n\n{rule}", dir.display());
    if ask_yes_no(rfd::MessageLevel::Info, text) {
        attach_synced_dialog(config_path, name, &dir, false);
    }
}

/// A synced folder as people read it, with the user it belongs to in front: `ana/family`. Without the user (not known before
/// the client first reached the master) it is the bare name.
pub(super) fn with_owner(owner: &str, name: &str) -> String {
    if owner.is_empty() || name.contains('/') {
        name.to_string()
    } else {
        format!("{owner}/{name}")
    }
}

/// The user this computer belongs to, from the name of the device (`<user>/<device>`) the client last got from the master.
pub(super) fn owner_of(status: Option<&daemon::Status>) -> &str {
    status.and_then(|s| s.device.split_once('/')).map_or("", |(owner, _)| owner)
}

/// Makes a synced folder named after a folder chosen by the user, and syncs that folder with it. `owner` is the user, to
/// show the folder the way the master names it.
pub(super) fn new_synced(config_path: &Path, on_master: &[(String, bool)], owner: &str) {
    let Some(dir) = rfd::FileDialog::new().set_title("Choose the folder to sync").pick_folder() else { return };
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

pub(super) fn message(level: rfd::MessageLevel, text: String) -> rfd::MessageDialog {
    rfd::MessageDialog::new().set_level(level).set_title("fafnirx").set_description(text)
}

/// Asks which folders to collect (several can be picked at once) and adds them to the config.
/// Returns whether the dialog was used.
pub(super) fn add_folders_dialog(config_path: &Path) -> bool {
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

pub(super) fn cli_change(config_path: &Path, request: &Request) -> anyhow::Result<()> {
    change_folders(config_path, request).map(drop).map_err(anyhow::Error::msg)
}

pub(super) fn remove_folder_dialog(config_path: &Path, name: &str) {
    let text = format!("Stop syncing \"{name}\" on this computer?\n\nNothing is deleted: the files stay in the folder and on the server.");
    confirm_then(text, || cli_change(config_path, &Request::RemoveFolder { name: name.into() }));
}
