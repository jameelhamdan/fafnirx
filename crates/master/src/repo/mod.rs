//! The repositories: the one place that knows how the master's data is stored. Every function here is a query (typed,
//! generated from `queries/*.sql`, see `docs/development.md`) or a small group of them, and speaks in the master's
//! own terms (a device, an alert, a setting) rather than in rows and columns. Handlers, the gRPC service and the
//! background jobs call these and contain no SQL; the rules (who may do what, what a rename means) stay with them.
//!
//! Functions take `&mut impl postgres::GenericClient`, so they run the same on a pooled connection and inside a
//! transaction. Anything that must be atomic is a transaction opened by the caller (the application layer).

pub mod albums;
pub mod alerts;
pub mod devices;
pub mod events;
pub mod file_meta;
pub mod file_state;
pub mod files;
pub mod jobs;
pub mod library;
pub mod listing;
pub mod mirrors;
pub mod retention;
pub mod sessions;
pub mod settings;
pub mod snapshot;
pub mod sources;
pub mod tags;
pub mod users;

/// Who is looking, for the queries that list files: which user's say (favorites, tags) is shown, and which devices
/// are visible.
#[derive(Clone, Copy, Debug)]
pub struct Viewer {
    /// The user looking.
    pub uid: i64,
    /// Everything is visible (administrators).
    pub all: bool,
    /// Otherwise the master files and the devices of this user are (-1: of nobody).
    pub scope_user: i64,
}
