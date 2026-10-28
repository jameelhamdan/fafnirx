//! What part of the tree a mirror holds: a folder named like the files are stored, `<user>/<device>/<folder>` or
//! `_master/<folder>` (see `0014_mirror_scope.sql`), and the choices the web app offers to pick one.

use crate::platform::store::MASTER_FOLDER;
use anyhow::{bail, Result};

/// A scope as stored: no slash at either end, no empty, `.` or `..` part, nothing a path cannot hold.
pub fn normalize(scope: &str) -> Result<String> {
    let scope = scope.trim().trim_matches('/');
    for part in scope.split('/').filter(|_| !scope.is_empty()) {
        if part.is_empty() || part == "." || part == ".." || part.contains(['\\', '\0']) {
            bail!("the folder to mirror {scope:?} is not a path like user/device/folder");
        }
    }
    Ok(scope.to_string())
}

/// A device, a local folder, a source or a shared folder, as far as the choices care.
pub struct Place {
    pub id: i64,
    /// `<user>/<name>`, or `_master`.
    pub folder: String,
    pub kind: String,
}

/// One folder that can be chosen next, below the one being looked at.
#[derive(serde::Serialize, Debug, PartialEq)]
pub struct Choice {
    pub name: String,
    /// `user`, `master`, a device kind (`device`, `local`, `source`, `shared`) or `folder`.
    pub kind: String,
}

fn choice(name: &str, kind: &str) -> Choice {
    Choice { name: name.to_string(), kind: kind.to_string() }
}

/// Whether some device holds files at or below `scope`, or `scope` is above a device (a user): a scope that cannot match
/// anything is a typing mistake.
pub fn exists(places: &[Place], scope: &str) -> bool {
    scope.is_empty()
        || places.iter().any(|p| {
            p.folder == scope
                || p.folder.starts_with(&format!("{scope}/"))
                || scope.starts_with(&format!("{}/", p.folder))
        })
}

/// What can be chosen inside `path` (empty for the top): the master files and the users, a user's devices and folders, then
/// the folders inside one. `below` lists the folders directly inside a folder of a device, given as `a/b/` or empty.
pub fn choices(places: &[Place], path: &str, mut below: impl FnMut(i64, &str) -> Vec<String>) -> Vec<Choice> {
    if path.is_empty() {
        let mut users: Vec<&str> =
            places.iter().filter(|p| p.folder != MASTER_FOLDER).filter_map(|p| p.folder.split('/').next()).collect();
        users.sort_unstable();
        users.dedup();
        let master = places.iter().any(|p| p.folder == MASTER_FOLDER).then(|| choice(MASTER_FOLDER, "master"));
        return master.into_iter().chain(users.into_iter().map(|u| choice(u, "user"))).collect();
    }
    if let Some((place, rest)) =
        places.iter().find_map(|p| path.strip_prefix(&p.folder).and_then(|r| rest_of(r)).map(|r| (p, r)))
    {
        let prefix = if rest.is_empty() { String::new() } else { format!("{rest}/") };
        return below(place.id, &prefix).iter().map(|name| choice(name, "folder")).collect();
    }
    let owned = format!("{path}/");
    let mut inside: Vec<Choice> =
        places.iter().filter_map(|p| p.folder.strip_prefix(&owned).map(|name| choice(name, &p.kind))).collect();
    inside.sort_by(|a, b| a.name.cmp(&b.name));
    inside
}

/// What is left of a path after a device's folder: `""` for the folder itself, `"a/b"` below it, nothing for another path.
fn rest_of(after_folder: &str) -> Option<&str> {
    if after_folder.is_empty() {
        Some("")
    } else {
        after_folder.strip_prefix('/')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(id: i64, folder: &str, kind: &str) -> Place {
        Place { id, folder: folder.into(), kind: kind.into() }
    }

    fn places() -> Vec<Place> {
        vec![
            place(0, "_master", "master"),
            place(1, "ana/laptop", "device"),
            place(2, "ana/family", "shared"),
            place(3, "bo/phone", "device"),
        ]
    }

    #[test]
    fn a_scope_is_a_clean_path() {
        assert_eq!(normalize(" /ana/laptop/ ").unwrap(), "ana/laptop");
        assert_eq!(normalize("").unwrap(), "");
        assert_eq!(normalize("/").unwrap(), "");
        for bad in ["a//b", "a/../b", "./a", "a\\b", "a/\0"] {
            assert!(normalize(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_scope_must_be_somewhere_in_the_tree() {
        let p = places();
        assert!(exists(&p, "") && exists(&p, "_master") && exists(&p, "_master/Photos"));
        assert!(exists(&p, "ana") && exists(&p, "ana/laptop") && exists(&p, "ana/laptop/Docs/2024"));
        assert!(!exists(&p, "cy") && !exists(&p, "ana/tablet") && !exists(&p, "an"));
    }

    #[test]
    fn the_top_offers_the_master_files_and_the_users() {
        let top = choices(&places(), "", |_, _| unreachable!());
        assert_eq!(top, [choice("_master", "master"), choice("ana", "user"), choice("bo", "user")]);
    }

    #[test]
    fn a_user_offers_the_devices_and_shared_folders() {
        let inside = choices(&places(), "ana", |_, _| unreachable!());
        assert_eq!(inside, [choice("family", "shared"), choice("laptop", "device")]);
    }

    #[test]
    fn a_device_offers_its_folders_and_the_master_files_theirs() {
        let mut asked = Vec::new();
        let mut below = |id: i64, prefix: &str| {
            asked.push((id, prefix.to_string()));
            vec!["Docs".to_string()]
        };
        assert_eq!(choices(&places(), "ana/laptop", &mut below), [choice("Docs", "folder")]);
        choices(&places(), "ana/laptop/Docs/2024", &mut below);
        choices(&places(), "_master", &mut below);
        assert_eq!(asked, [(1, "".into()), (1, "Docs/2024/".into()), (0, "".into())]);
    }

    #[test]
    fn a_name_that_only_starts_like_a_device_is_not_inside_it() {
        // `ana/laptop2` is not below `ana/laptop`.
        assert!(choices(&places(), "ana/laptop2", |_, _| unreachable!()).is_empty());
    }
}
