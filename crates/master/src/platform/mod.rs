//! What every app builds on: the PostgreSQL pool, locks, notifications, settings and alerts (`db`, with the numbered `migrations` that make the schema), and the
//! file store on disk (`store`).

pub mod db;
pub mod migrations;
pub mod proxy;
pub mod store;
