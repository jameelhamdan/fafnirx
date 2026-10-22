//! Devices: the clients that send files. `grpc` is the device-facing API (upload only); `api` is the admin
//! side (list, add, rotate, revoke, settings).

pub mod api;
pub mod grpc;
