//! fafnirx-fetch: the fetch module of fafnirx. It reads what a source offers (addresses of files, and feeds that link to files) over
//! HTTP(S) and writes one file at a time for the master, which keeps the database and commits the files.
//!
//! A container next to the master, which puts its own login in front of it and holds the token it asks for. The service, the
//! environment it reads (token, address, data folder, `FAFNIRX_FETCH_JOBS`, `FAFNIRX_FETCH_IDLE_SECS`) and the contract with the
//! master are the module kit's (`fafnirx-module-kit`, docs/plugins.md, docs/fetch.md).

mod module;
mod settings;
mod web;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let common = fafnirx_module_kit::start("fafnirx-fetch", "FETCH", "0.0.0.0:8091")?;
    let idle = common.idle;
    fafnirx_module_kit::serve(common, module::Web { idle }).await
}
