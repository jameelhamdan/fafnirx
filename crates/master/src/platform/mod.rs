//! What every app builds on: the PostgreSQL pool, locks, notifications, settings and alerts (`db`, with the numbered `migrations` that make the schema), and the
//! file store on disk (`store`).

#[macro_use]
mod text_enum;

pub mod db;
pub mod id;
pub mod migrations;
pub mod proxy;
pub mod store;
pub mod tuning;
