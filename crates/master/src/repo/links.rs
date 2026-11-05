//! Links: the sources (inbound) and mirrors (outbound) of the master. Both are a folder tied to some storage that runs on a
//! schedule and remembers what it has copied, so everything they have in common is stored and asked for here; `sources` and `mirrors`
//! only add what is theirs.

use fafnirx_db::client::GenericClient;
use fafnirx_db::client::Params;
use fafnirx_db::queries::links as q;
use postgres::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// A source: a folder of its owner's that a storage is copied into.
    In,
    /// A mirror: a folder, or everything, copied to a storage.
    Out,
}

impl Dir {
    pub fn as_str(self) -> &'static str {
        match self {
            Dir::In => "in",
            Dir::Out => "out",
        }
    }

    pub fn parse(s: &str) -> Dir {
        if s == "in" {
            Dir::In
        } else {
            Dir::Out
        }
    }

    /// What it is called in logs and messages.
    pub fn word(self) -> &'static str {
        match self {
            Dir::In => "source",
            Dir::Out => "mirror",
        }
    }

    /// The kind of alert a failed run raises.
    pub fn alert_kind(self) -> &'static str {
        match self {
            Dir::In => "source_failed",
            Dir::Out => "mirror_failed",
        }
    }
}

/// What a new link is made of.
pub struct New<'a> {
    pub dir: Dir,
    /// The owner of a source.
    pub user_id: Option<i64>,
    pub name: &'a str,
    pub kind: &'a str,
    pub config: &'a str,
    /// The folder a source writes into, or the one a mirror holds.
    pub folder_id: Option<i64>,
    pub interval_secs: i64,
}

/// Adds a link, which runs as soon as the scheduler looks. `None` when the owner has one of that name.
pub fn add(c: &mut impl GenericClient, n: &New, now: i64) -> Result<Option<i64>, Error> {
    let params = q::AddLinkParams {
        direction: n.dir.as_str(),
        user_id: n.user_id,
        name: n.name,
        kind: n.kind,
        config: n.config,
        folder_id: n.folder_id,
        interval_secs: n.interval_secs,
        now,
    };
    q::add_link().params(c, &params).opt()
}

/// A source is named by its owner and its name, a mirror by its name.
pub fn id_by_name(c: &mut impl GenericClient, dir: Dir, owner: Option<&str>, name: &str) -> Result<Option<i64>, Error> {
    q::link_id_by_name().bind(c, &dir.as_str(), &name, &owner).opt()
}

/// A link as it is stored, locked for an edit.
pub struct Stored {
    pub name: String,
    pub kind: String,
    pub config: String,
    pub folder_id: Option<i64>,
    pub interval_secs: i64,
    pub user_id: Option<i64>,
    pub running: bool,
}

pub fn for_edit(c: &mut impl GenericClient, id: i64) -> Result<Option<Stored>, Error> {
    Ok(q::link_for_edit().bind(c, &id).opt()?.map(|l| Stored {
        running: l.last_status.as_deref() == Some("running"),
        name: l.name,
        kind: l.kind,
        config: l.config,
        folder_id: l.folder_id,
        interval_secs: l.interval_secs,
        user_id: l.user_id,
    }))
}

pub fn update(c: &mut impl GenericClient, id: i64, s: &Stored) -> Result<(), Error> {
    let params = q::UpdateLinkParams { name: &s.name, config: &s.config, folder_id: s.folder_id, interval_secs: s.interval_secs, id };
    q::update_link().params(c, &params).map(drop)
}

/// Whether the owner (none for a mirror) has another link of this name.
pub fn name_taken(c: &mut impl GenericClient, dir: Dir, user_id: Option<i64>, name: &str, except: i64) -> Result<bool, Error> {
    q::link_name_taken().bind(c, &dir.as_str(), &name, &user_id, &except).one()
}

/// Removes a link and what it remembered.
pub fn delete(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::delete_link().bind(c, &id).map(drop)
}

/// Asks for a run now.
pub fn request_run(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::request_run().bind(c, &id).map(drop)
}

/// A run did not do it all: asks for another at once, unless it is switched off.
pub fn continue_run(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::continue_run().bind(c, &id).map(drop)
}

/// Switches it on or off.
pub fn toggle(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::toggle_link().bind(c, &id).map(drop)
}

/// Takes the next run that is due; it is running from now on.
pub fn claim(c: &mut impl GenericClient, now: i64) -> Result<Option<i64>, Error> {
    q::claim_link().bind(c, &now).opt()
}

/// A link that is due, or about to be removed.
pub struct Due {
    pub dir: Dir,
    pub kind: String,
    pub name: String,
    pub config: String,
    pub folder_id: Option<i64>,
    pub folder: Option<String>,
    pub owner: Option<String>,
}

impl Due {
    /// What names it in logs, alerts and live events: `<owner>/<name>` of a source, the name of a mirror.
    pub fn key(&self) -> String {
        match &self.owner {
            Some(owner) => format!("{owner}/{}", self.name),
            None => self.name.clone(),
        }
    }
}

pub fn due(c: &mut impl GenericClient, id: i64) -> Result<Option<Due>, Error> {
    Ok(q::link_to_run().bind(c, &id).opt()?.map(|l| Due {
        dir: Dir::parse(&l.direction),
        kind: l.kind,
        name: l.name,
        config: l.config,
        folder_id: l.folder_id,
        folder: l.folder,
        owner: l.owner,
    }))
}

/// Records how a run went; `ok_at` is when it worked, if it did.
pub fn finish(c: &mut impl GenericClient, id: i64, status: &str, message: &str, ok_at: Option<i64>) -> Result<(), Error> {
    q::finish_link().params(c, &q::FinishLinkParams { status, message, ok_at, id }).map(drop)
}

/// How many files and bytes the last run left on a mirror.
pub fn set_totals(c: &mut impl GenericClient, id: i64, files: i64, bytes: i64) -> Result<(), Error> {
    q::set_link_totals().bind(c, &files, &bytes, &id).map(drop)
}

/// Marks the runs that were going on when the master stopped as failed.
pub fn fail_interrupted(c: &mut impl GenericClient) -> Result<(), Error> {
    q::fail_interrupted_links().bind(c).map(drop)
}

/// A file a link has copied: the device and path it is at, the remote's id for it (a source's) and the version that was copied
/// (the marker of a source's, the hash of a mirror's).
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub device_id: i64,
    pub path: String,
    pub remote_id: String,
    pub version: String,
}

pub fn items(c: &mut impl GenericClient, id: i64) -> Result<Vec<Item>, Error> {
    Ok(q::link_items()
        .bind(c, &id)
        .all()?
        .into_iter()
        .map(|i| Item { device_id: i.device_id, path: i.path, remote_id: i.remote_id, version: i.version })
        .collect())
}

/// Remembers a file by its place.
pub fn remember(c: &mut impl GenericClient, id: i64, device_id: i64, path: &str, version: &str) -> Result<(), Error> {
    q::remember_item()
        .params(c, &q::RememberItemParams { link_id: id, device_id, path, version })
        .map(drop)
}

/// Remembers a file by the remote's id for it, which stays when it is renamed. Brings back a file that had been removed in the
/// web app, since it changed at the source.
pub fn remember_remote(c: &mut impl GenericClient, id: i64, device_id: i64, path: &str, remote_id: &str, version: &str) -> Result<(), Error> {
    q::remember_remote_item()
        .params(c, &q::RememberRemoteItemParams { link_id: id, device_id, path, remote_id, version })
        .map(drop)
}

pub fn forget(c: &mut impl GenericClient, id: i64, device_id: i64, path: &str) -> Result<(), Error> {
    q::forget_item().params(c, &q::ForgetItemParams { link_id: id, device_id, path }).map(drop)
}

pub fn forget_remote(c: &mut impl GenericClient, id: i64, remote_id: &str) -> Result<(), Error> {
    q::forget_remote_item().bind(c, &id, &remote_id).map(drop)
}

/// Forgets everything it copied, so that its next run copies it all again.
pub fn forget_items(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::forget_items().bind(c, &id).map(drop)
}
