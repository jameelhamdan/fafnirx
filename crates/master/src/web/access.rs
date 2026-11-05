//! Who is asking and what they may reach.
//!
//! Every file belongs to a device, and a device belongs to a user or, for a master folder, to nobody. The folder of a
//! device (`<user>/<device>`, or `master:<name>`) is how the API names it, so the rules are about folders:
//!
//! |          | own devices | master folders | other users' devices | users, settings, mirrors, sources |
//! |----------|-------------|----------------|----------------------|-----------------------------------|
//! | admin    | read, write | read, write    | read, write          | yes                               |
//! | normal   | read, write | read, write    | no                   | no                                |
//! | readonly | none        | read           | no                   | no                                |
//!
//! On top of that, a shared folder whose owner added the user is theirs to read, and to write when the owner allowed it
//! (never for a read-only account). The folders a user can use, and whether they may change each, are decided in one place, the
//! query `folders_for_user`; this only looks the folder up in what it gave.

use super::ApiError;
use crate::repo::Viewer;
use axum::http::StatusCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Admin,
    Normal,
    Readonly,
}

impl Role {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "admin" => Some(Self::Admin),
            "normal" => Some(Self::Normal),
            "readonly" => Some(Self::Readonly),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Normal => "normal",
            Self::Readonly => "readonly",
        }
    }
}

/// The user behind a request, as the guard resolved it from the session.
#[derive(Clone, Debug)]
pub struct Access {
    pub user_id: i64,
    pub name: String,
    pub role: Role,
    /// The folders this user can use (their own, those shared with them, the master folders), and whether they may change each.
    pub folders: Vec<(String, bool)>,
}

fn forbidden(what: &str) -> ApiError {
    ApiError(StatusCode::FORBIDDEN, what.into())
}

impl Access {
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }

    /// Whether the folder of a device is one this user may look at.
    pub fn can_see(&self, folder: &str) -> bool {
        self.is_admin() || self.folders.iter().any(|(f, _)| f == folder) || (self.role == Role::Normal && self.owns(folder))
    }

    /// Whether the folder of a device is one this user may add, change or delete files in.
    pub fn can_write(&self, folder: &str) -> bool {
        match (self.role, self.folders.iter().find(|(f, _)| f == folder)) {
            (Role::Readonly, _) => false,
            (Role::Admin, _) => true,
            (_, Some((_, may_write))) => *may_write,
            (_, None) => self.can_see(folder),
        }
    }

    /// Whether the folder is in the workspace of this user (`<name>/<device>`).
    pub fn owns(&self, folder: &str) -> bool {
        folder.split_once('/').is_some_and(|(user, _)| user == self.name)
    }

    pub fn require_admin(&self) -> Result<(), ApiError> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(forbidden("only an administrator can do this"))
        }
    }

    /// For reading: a device that is not visible does not exist, so it cannot be probed for.
    pub fn check_see(&self, folder: &str) -> Result<(), ApiError> {
        if self.can_see(folder) {
            Ok(())
        } else {
            Err(ApiError::not_found("no such device"))
        }
    }

    /// For changing: what the user can see but not change is refused as such.
    pub fn check_write(&self, folder: &str) -> Result<(), ApiError> {
        self.check_see(folder)?;
        if self.can_write(folder) {
            Ok(())
        } else {
            Err(forbidden("your account can only read here"))
        }
    }

    /// Whether this user has a workspace of their own (devices, albums, tags, favorites).
    pub fn has_workspace(&self) -> bool {
        self.role != Role::Readonly
    }

    pub fn require_workspace(&self) -> Result<(), ApiError> {
        if self.has_workspace() {
            Ok(())
        } else {
            Err(forbidden("your account can only read the master files"))
        }
    }

    /// Who is looking, for the queries that list files.
    pub fn viewer(&self) -> Viewer {
        let (all, scope_user) = self.scope_args();
        Viewer { uid: self.user_id, all, scope_user }
    }

    /// The two values the queries take to scope devices: whether everything is visible, and the user whose devices and shared
    /// folders are (a read-only account owns no devices, so this only reaches the folders it was added to).
    pub fn scope_args(&self) -> (bool, i64) {
        match self.role {
            Role::Admin => (true, -1),
            _ => (false, self.user_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access(role: Role) -> Access {
        // What `folders_for_user` gives bob: a master folder, his own folder and a folder alice shared with him to read.
        let writes = role != Role::Readonly;
        Access {
            user_id: 7,
            name: "bob".into(),
            role,
            // (a read-only account has no folders of its own)
            folders: [("master:Photos", true), ("bob/laptop", writes), ("alice/family", false)]
                .into_iter()
                .filter(|(f, _)| writes || *f != "bob/laptop")
                .map(|(f, w)| (f.to_string(), w && writes))
                .collect(),
        }
    }

    #[test]
    fn what_each_role_can_reach() {
        let (admin, normal, readonly) = (access(Role::Admin), access(Role::Normal), access(Role::Readonly));
        for (folder, seen_by, written_by) in [
            ("master:Photos", [true, true, true], [true, true, false]),
            ("bob/laptop", [true, true, false], [true, true, false]),
            ("alice/laptop", [true, false, false], [true, false, false]),
            ("bobby/laptop", [true, false, false], [true, false, false]),
            // Shared with bob, read-only: seen by him whatever his role, changed by nobody but an administrator.
            ("alice/family", [true, true, true], [true, false, false]),
        ] {
            let who = [&admin, &normal, &readonly];
            for i in 0..3 {
                assert_eq!(who[i].can_see(folder), seen_by[i], "{folder} seen by {:?}", who[i].role);
                assert_eq!(who[i].can_write(folder), written_by[i], "{folder} written by {:?}", who[i].role);
            }
        }
        assert!(!readonly.has_workspace() && normal.has_workspace() && admin.is_admin() && !normal.is_admin());

        // The owner can allow writing, to a normal account only.
        let mut allowed = access(Role::Normal);
        allowed.folders = vec![("alice/family".into(), true)];
        assert!(allowed.can_write("alice/family") && !allowed.can_write("alice/other"));
        let mut read_only_account = access(Role::Readonly);
        read_only_account.folders = vec![("alice/family".into(), true)];
        assert!(read_only_account.can_see("alice/family") && !read_only_account.can_write("alice/family"));
    }
}
