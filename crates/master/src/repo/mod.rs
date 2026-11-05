//! The repositories: where the master's data is stored and what is more than one plain query. A function here is a small
//! group of typed queries (generated from `queries/*.sql`, see `docs/development.md`) or one with a mapping around it, and
//! speaks in the master's own terms (a device, an alert, a setting) rather than in rows and columns. A single plain query
//! is called where it is needed (`sql::mirrors::mirror_key().bind(c, &name)`); the rules (who may do what, what a rename
//! means) stay with the handlers, the gRPC service and the background jobs.
//!
//! Functions take `&mut impl fafnirx_db::client::GenericClient`, so they run the same on a pooled connection and inside a
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
pub mod links;
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
