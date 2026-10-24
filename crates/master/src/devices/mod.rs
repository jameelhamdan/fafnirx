//! Devices: the clients that send files. `grpc` is the device-facing API (upload only); `api` is the web app
//! side (list, add, rotate, revoke, settings), `totals` the file and byte counts kept on each device row.

pub mod api;
pub mod grpc;
pub mod totals;
