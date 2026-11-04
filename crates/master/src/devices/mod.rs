//! Devices: the clients that send files. `grpc` is the device-facing API (uploads, and the shared folders of the user);
//! `api` is the web app side (list, add, rotate, revoke, settings), `folders` the web side of folders and sharing, `totals` the
//! file and byte counts kept on each device row.

pub mod api;
pub mod folders;
pub mod grpc;
pub mod master_folders;
pub mod rename;
pub mod totals;
