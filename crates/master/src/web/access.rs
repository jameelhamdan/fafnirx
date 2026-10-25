//! Who is asking and what they may reach.
//!
//! Every file belongs to a device, and a device belongs to a user or, for the master files, to nobody. The folder of a
//! device (`<user>/<device>`, or `_master`) is how the API names it, so the rules are about folders:
//!
//! |          | own devices | master files | other users' devices | users, settings, mirrors, sources |
//! |----------|-------------|--------------|----------------------|-----------------------------------|
//! | admin    | read, write | read, write  | read, write          | yes                               |
//! | normal   | read, write | read, write  | no                   | no                                |
//! | readonly | none        | read         | no                   | no                                |

use super::ApiError;
use crate::platform::store::MASTER_FOLDER;
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
        match self.role {
            Role::Admin => true,
            Role::Normal => folder == MASTER_FOLDER || self.owns(folder),
            Role::Readonly => folder == MASTER_FOLDER,
        }
    }

    /// Whether the folder of a device is one this user may add, change or delete files in.
    pub fn can_write(&self, folder: &str) -> bool {
        self.role != Role::Readonly && self.can_see(folder)
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

    /// SQL for "the device `d` is visible to this user", with the user's id in it as a number (it is ours, not input).
    pub fn visible_sql(&self) -> String {
        match self.role {
            Role::Admin => "TRUE".into(),
            Role::Normal => format!("(d.kind = 'master' OR d.user_id = {})", self.user_id),
            Role::Readonly => "d.kind = 'master'".into(),
        }
    }

    /// The two values the queries take to scope devices: whether everything is visible, and the user whose devices are (-1: nobody's).
    pub fn scope_args(&self) -> (bool, i64) {
        match self.role {
            Role::Admin => (true, -1),
            Role::Normal => (false, self.user_id),
            Role::Readonly => (false, -1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn access(role: Role) -> Access {
        Access { user_id: 7, name: "bob".into(), role }
    }

    #[test]
    fn what_each_role_can_reach() {
        let (admin, normal, readonly) = (access(Role::Admin), access(Role::Normal), access(Role::Readonly));
        for (folder, seen_by, written_by) in [
            ("_master", [true, true, true], [true, true, false]),
            ("bob/laptop", [true, true, false], [true, true, false]),
            ("alice/laptop", [true, false, false], [true, false, false]),
            ("bobby/laptop", [true, false, false], [true, false, false]),
        ] {
            let who = [&admin, &normal, &readonly];
            for i in 0..3 {
                assert_eq!(who[i].can_see(folder), seen_by[i], "{folder} seen by {:?}", who[i].role);
                assert_eq!(who[i].can_write(folder), written_by[i], "{folder} written by {:?}", who[i].role);
            }
        }
        assert!(!readonly.has_workspace() && normal.has_workspace() && admin.is_admin() && !normal.is_admin());
    }
}
