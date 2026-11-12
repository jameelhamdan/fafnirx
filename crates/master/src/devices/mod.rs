//! Devices: the devices and phones that send files. `grpc` is the device-facing API (uploads, and the shared folders of the
//! user); `api` is the web app side (list, add, rotate, revoke, settings).

pub mod api;
pub mod call;
pub mod grpc;
