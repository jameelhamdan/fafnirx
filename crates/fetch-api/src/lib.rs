//! What the master and a module that reads sources say to each other (docs/fetch.md). The module owns its kinds of source: it
//! checks the settings a user gave, says how to label the source, lists what it offers and writes one file when asked; the
//! master keeps the database and knows no kind by name. Types only, as in `fafnirx-module-api`
//! (the manifest, the capabilities and everything every module shares).

use serde::{Deserialize, Serialize};

/// The capability a module lists in its manifest for each kind of source it can read (`fetch.web`).
pub fn capability(kind: &str) -> String {
    format!("fetch.{kind}")
}

/// `POST /fetch/check` ([`FetchRequest`] -> [`Checked`]): the module reduces the settings a user gave to what it reads, or answers
/// `400` with a sentence for the user. The master calls it when a source is added.
pub const FETCH_CHECK_PATH: &str = "/fetch/check";

/// `POST /fetch/list` ([`FetchRequest`] -> [`FetchListing`]) and `POST /fetch/get` ([`FetchGet`] -> [`Fetched`]): the
/// master's scheduler asks a module what a source offers and has it write one file. The master keeps the database: it
/// compares the listing with what it has, tells the module which entries to get, and commits the files.
pub const FETCH_LIST_PATH: &str = "/fetch/list";
pub const FETCH_GET_PATH: &str = "/fetch/get";

/// What a fetch call is about: the kind of source (the `fetch.<kind>` capability) and its settings as the user gave them
/// (the module has no database; the master sends them with every call).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchRequest {
    pub kind: String,
    pub config: serde_json::Value,
}

/// One file a source offers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchEntry {
    /// Where it goes in the source's folder, `/`-separated.
    pub path: String,
    /// Changes when the file the source would make changes (a new version, another format); the master fetches again.
    pub marker: String,
    /// Bytes, 0 when not known before the file is made.
    #[serde(default)]
    pub size: u64,
    /// Unix nanoseconds.
    #[serde(default)]
    pub mtime_ns: i64,
    /// Whatever the module needs to make this file; the master hands it back unread.
    #[serde(default)]
    pub reference: serde_json::Value,
}

/// The answer to `POST /fetch/list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchListing {
    pub items: Vec<FetchEntry>,
    /// Entries it could not offer (not available, or not yet); they are tried again next run.
    #[serde(default)]
    pub skipped: usize,
    /// False when the source may offer more than is listed (some of it could not be read this time). The master then deletes
    /// nothing for files that are missing from the listing: absent is not the same as gone.
    #[serde(default = "complete")]
    pub complete: bool,
}

fn complete() -> bool {
    true
}

/// `POST /fetch/get`: write the file of `entry` to `dest`, a path under the master's `tmp` folder (the module may write nowhere
/// else). It answers when the file is complete; the master hashes and commits it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchGet {
    pub kind: String,
    pub config: serde_json::Value,
    pub entry: FetchEntry,
    pub dest: String,
}

/// The answer to `POST /fetch/get`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fetched {
    pub size: u64,
}

/// The answer to `POST /fetch/check`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checked {
    /// The settings as the module will read them: unknown keys dropped, text trimmed.
    pub config: serde_json::Value,
    /// What the Sources page shows of the source (a link, a count).
    pub label: String,
}
