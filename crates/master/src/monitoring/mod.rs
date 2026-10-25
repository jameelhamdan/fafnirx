//! Keeping watch: the dashboard and alerts API (`api`), the scheduled jobs (`jobs`: offline and disk alerts, retention,
//! version pruning) and the view of the background work queue (`queue`).

pub mod api;
pub mod jobs;
pub mod queue;
