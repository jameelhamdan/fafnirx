//! What a module that reads sources shares with every other one. The module says what it reads (`Fetcher`: its kind of source, how to
//! check its settings, how to list and fetch); the kit is the rest: the environment (`start`), the service the master talks to
//! (`serve`), what is being done (`Tasks`), a folder for work in hand (`Scratch`), and the small tools such a module needs: running a
//! program (`process`), reading settings (`settings`), feeds (`xml`), names of files (`names`) and HTTP (`http`). The contract itself
//! is `fafnirx-module-api` and `fafnirx-fetch-api`; docs/plugins.md describes it.

mod env;
mod fetch;
pub mod http;
pub mod names;
pub mod process;
mod scratch;
pub mod settings;
mod tasks;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod ui;
pub mod xml;

pub use env::{number, run, serve, start, var, Announce, Common};
pub use fetch::{router, Fetcher, Info, Readiness};
pub use names::sanitize_name;
pub use scratch::Scratch;
pub use tasks::Tasks;
