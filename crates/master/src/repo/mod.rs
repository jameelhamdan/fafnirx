//! The repositories: the one place that knows how the master's data is stored. Every function here is a query (typed,
//! generated from `queries/*.sql`, see `docs/development.md`) or a small group of them, and speaks in the master's
//! own terms (a device, an alert, a setting) rather than in rows and columns. Handlers, the gRPC service and the
//! background jobs call these and contain no SQL; the rules (who may do what, what a rename means) stay with them.
//!
//! Functions take `&mut impl postgres::GenericClient`, so they run the same on a pooled connection and inside a
//! transaction. Anything that must be atomic is a transaction opened by the caller (the application layer).

pub mod alerts;
pub mod devices;
pub mod sessions;
pub mod settings;
pub mod users;
