//! Devices: the clients that send files. `grpc` is the device-facing API (uploads, and the shared folders of the user);
//! `api` is the web app side (list, add, rotate, revoke, settings), `shared` the web side of shared folders, `totals` the
//! file and byte counts kept on each device row.

pub mod api;
pub mod grpc;
pub mod shared;
pub mod totals;
