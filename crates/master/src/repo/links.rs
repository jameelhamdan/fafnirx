//! Links: the sources (inbound) and mirrors (outbound) of the master. Both are a folder tied to some storage that runs on a
//! schedule and remembers what it has copied, so everything they have in common is stored and asked for here; `sources` and `mirrors`
//! only add what is theirs.

use fafnirx_db::client::GenericClient;
use fafnirx_db::client::Params;
use fafnirx_db::queries::jobs as jq;
use fafnirx_db::queries::links as q;
use postgres::Error;
use uuid::Uuid;

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

text_enum! {
    /// How the last run of a link went. The column is text; `Running` is set by the scheduler when it claims the link.
    pub enum RunStatus {
        Running = "running",
        Ok = "ok",
        /// It finished, with problems on some files.
        Partial = "partial",
        Failed = "failed",
    }
}

text_enum! {
    /// What a mirror holds: the files as they are, or an encrypted copy.
    #[derive(Default, serde::Deserialize)]
    #[serde(rename_all = "lowercase")]
    pub enum MirrorKind {
        #[default]
        Replica = "replica",
        Backup = "backup",
    }
}

/// How a source fetches: a storage read through rclone, or a kind a module reads (the module lists `fetch.<kind>`).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(from = "String")]
pub enum SourceKind {
    #[default]
    Rclone,
    Module(String),
}

impl From<String> for SourceKind {
    fn from(s: String) -> Self {
        if s == "rclone" {
            SourceKind::Rclone
        } else {
            SourceKind::Module(s)
        }
    }
}

impl SourceKind {
    pub fn as_str(&self) -> &str {
        match self {
            SourceKind::Rclone => "rclone",
            SourceKind::Module(k) => k,
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
    /// The device whose folders a mirror holds (a mirror holds a folder, a device, or everything).
    pub device_id: Option<i64>,
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
        device_id: n.device_id,
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
    pub device_id: Option<i64>,
    pub interval_secs: i64,
    pub user_id: Option<i64>,
    pub running: bool,
}

impl Stored {
    pub fn source_kind(&self) -> SourceKind {
        SourceKind::from(self.kind.clone())
    }
}

pub fn for_edit(c: &mut impl GenericClient, id: i64) -> Result<Option<Stored>, Error> {
    Ok(q::link_for_edit().bind(c, &id).opt()?.map(|l| Stored {
        running: l.last_status.as_deref().and_then(RunStatus::parse) == Some(RunStatus::Running),
        name: l.name,
        kind: l.kind,
        config: l.config,
        folder_id: l.folder_id,
        device_id: l.device_id,
        interval_secs: l.interval_secs,
        user_id: l.user_id,
    }))
}

pub fn update(c: &mut impl GenericClient, id: i64, s: &Stored) -> Result<(), Error> {
    let params = q::UpdateLinkParams {
        name: &s.name,
        config: &s.config,
        folder_id: s.folder_id,
        device_id: s.device_id,
        interval_secs: s.interval_secs,
        id,
    };
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
pub fn schedule(c: &mut impl GenericClient, now: i64) -> Result<Option<i64>, Error> {
    q::schedule_link().bind(c, &now).opt()
}

/// Takes the next queued run (a job of kind `source` or `mirror`); its link is running from now on. Returns the job and the link.
pub fn claim_job(c: &mut impl GenericClient, now: i64) -> Result<Option<(Uuid, i64)>, Error> {
    let Some(job) = jq::claim_link_job().bind(c, &now).opt()? else { return Ok(None) };
    let Some(link) = job.link_id else { return Ok(None) };
    q::start_link().bind(c, &now, &link)?;
    Ok(Some((job.id, link)))
}

/// A run ended: its job is done (with what it found) or failed (with why).
pub fn finish_job(c: &mut impl GenericClient, job: Uuid, status: RunStatus, message: &str, now: i64) -> Result<(), Error> {
    let failed = status == RunStatus::Failed;
    let (state, error, note) = (
        if failed { "failed" } else { "done" },
        failed.then_some(message),
        (!failed && !message.is_empty()).then_some(message),
    );
    jq::finish_link_job().bind(c, &state, &now, &error, &note, &job).map(drop)
}

/// A link that is due, or about to be removed.
#[derive(Clone)]
pub struct Due {
    pub id: i64,
    pub dir: Dir,
    pub kind: String,
    pub name: String,
    pub config: String,
    pub folder_id: Option<i64>,
    pub device_id: Option<i64>,
    /// The key of the folder or device it is about; empty for everything.
    pub scope: String,
    pub owner: Option<String>,
}

impl Due {
    /// How a source fetches (the link is a source).
    pub fn source_kind(&self) -> SourceKind {
        SourceKind::from(self.kind.clone())
    }

    /// What a mirror holds (the link is a mirror).
    pub fn mirror_kind(&self) -> MirrorKind {
        MirrorKind::parse(&self.kind).unwrap_or_default()
    }

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
        id,
        dir: Dir::parse(&l.direction),
        kind: l.kind,
        name: l.name,
        config: l.config,
        folder_id: l.folder_id,
        device_id: l.device_id,
        scope: l.scope,
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
pub fn fail_interrupted(c: &mut impl GenericClient, now: i64) -> Result<(), Error> {
    q::fail_interrupted_links().bind(c).map(drop)?;
    jq::fail_interrupted_link_jobs().bind(c, &now).map(drop)
}

/// A file a link has copied: the device and path it is at, the remote's id for it (a source's) and the version that was copied
/// (the marker of a source's, the hash of a mirror's).
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub folder_id: i64,
    pub path: String,
    pub remote_id: String,
    pub version: String,
}

pub fn items(c: &mut impl GenericClient, id: i64) -> Result<Vec<Item>, Error> {
    Ok(q::link_items()
        .bind(c, &id)
        .all()?
        .into_iter()
        .map(|i| Item { folder_id: i.folder_id, path: i.path, remote_id: i.remote_id, version: i.version })
        .collect())
}

/// Remembers a file by its place.
pub fn remember(c: &mut impl GenericClient, id: i64, folder_id: i64, path: &str, version: &str) -> Result<(), Error> {
    q::remember_item()
        .params(c, &q::RememberItemParams { link_id: id, folder_id, path, version })
        .map(drop)
}

/// Remembers a file by the remote's id for it, which stays when it is renamed. Brings back a file that had been removed in the
/// web app, since it changed at the source.
pub fn remember_remote(c: &mut impl GenericClient, item: &RemoteItem) -> Result<(), Error> {
    let params = q::RememberRemoteItemParams {
        link_id: item.link_id,
        folder_id: item.folder_id,
        path: item.path,
        remote_id: item.remote_id,
        version: item.version,
    };
    q::remember_remote_item().params(c, &params).map(drop)
}

/// A file of a source by the remote's id for it: the link, the device and path it was stored at, and the remote's version of it.
pub struct RemoteItem<'a> {
    pub link_id: i64,
    pub folder_id: i64,
    pub path: &'a str,
    pub remote_id: &'a str,
    pub version: &'a str,
}

pub fn forget(c: &mut impl GenericClient, id: i64, folder_id: i64, path: &str) -> Result<(), Error> {
    q::forget_item().params(c, &q::ForgetItemParams { link_id: id, folder_id, path }).map(drop)
}

pub fn forget_remote(c: &mut impl GenericClient, id: i64, remote_id: &str) -> Result<(), Error> {
    q::forget_remote_item().bind(c, &id, &remote_id).map(drop)
}

/// Forgets everything it copied, so that its next run copies it all again.
pub fn forget_items(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::forget_items().bind(c, &id).map(drop)
}
