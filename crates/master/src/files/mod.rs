//! Files: how content gets onto the master and how it is read back. `ingest` commits versions, `livetree`
//! watches `files/` for changes made outside the master, `browse` is the admin file browser, `transfer`
//! the downloads and chunked uploads of the admin API, `zip` the streamed archives.

pub mod browse;
pub mod ingest;
pub mod livetree;
pub mod transfer;
pub mod zip;
