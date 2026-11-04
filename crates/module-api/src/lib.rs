//! What the master and a module agree on. A module is a separate service (its own dependencies) that adds one kind of
//! feature to fafnirx. The master reaches it by an address and a token, asks it for its [`Manifest`], and from then on uses
//! the capabilities the manifest lists. Without a module the master simply does not have the feature. The rules are
//! written down in docs/modules.md.
//!
//! A module is a container the administrator runs next to the master. It announces itself (`POST /api/plugins/register` with the shared
//! secret `FAFNIRX_PLUGIN_SECRET`); the master never starts or installs one.
//!
//! Every request a module gets carries the shared token as `Authorization: Bearer`, except `GET /health`. The master has
//! already checked the login and the user's right to the file; a module has no users of its own.

use serde::{Deserialize, Serialize};

/// The version of this contract. A module whose manifest says another number is not used (and the master says so in its
/// log), so a module and a master of different generations never half-work together.
pub const CONTRACT: u32 = 1;

/// The version of the contract crates, which move with the master. A module built against them needs a master of this version or
/// newer (`Manifest::core`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Whether a master of version `running` is at least `needed` (`major.minor.patch`; a text that is not one is not satisfied).
pub fn core_satisfied(needed: &str, running: &str) -> bool {
    let numbers = |v: &str| -> Option<[u64; 3]> {
        let mut parts = v.split(['.', '-', '+']).take(3).map(|p| p.parse::<u64>().ok());
        Some([parts.next()??, parts.next()??, parts.next()??])
    };
    matches!((numbers(needed), numbers(running)), (Some(n), Some(r)) if r >= n)
}

/// `GET /manifest`: what the module is and what it can do.
pub const MANIFEST_PATH: &str = "/manifest";
/// `GET /health`: `200` without the token, for container health checks.
pub const HEALTH_PATH: &str = "/health";
/// What the web app may reach: `GET /api/m/<module>/<file id>/<rest>` becomes `GET /files/<rest>?p=<place in the file tree>&h=<hex
/// content hash>`, after the master has checked the login and the user's right to the file. Nothing else of a module is
/// reachable from the browser.
pub const FILES_PREFIX: &str = "/files";

/// What the web app may reach of a module's own screens: `GET /api/m/<module>/ui/<rest>` becomes `GET /ui/<rest>` after the
/// master has checked the login. Static files only, the same for everybody: the script and style the web app loads.
pub const UI_PREFIX: &str = "/ui";
/// The version of the web app's side of the contract (what a module's script may use of the app, see `frontend/src/sdk`).
/// A script built for another number is not loaded. The web app's `sdk/runtime.ts` says the same number (a test checks it).
pub const WEB_SDK: u32 = 2;

/// What a module can do for the master. The manifest lists the ones it provides; a request for anything else is a bug.
pub mod capability {
    /// `GET /probe?p&h`: reads a video file and answers with a [`Probe`](super::Probe).
    pub const PROBE_VIDEO: &str = "probe.video";
    /// `GET /poster?p&h&max`: one frame of a file as a JPEG no larger than `max` pixels. Videos, and the pictures (HEIC,
    /// AVIF) the master cannot decode itself.
    pub const POSTER: &str = "poster";
    /// `GET /status`: what the module is working on, for the queue page: a JSON object with `active` and `failed` lists of
    /// tasks, each with a `path` (the file it is about, as in the `p` query), and whatever else the module's own part of the
    /// web app reads. The master leaves out the tasks of files the viewing user cannot see.
    pub const STATUS: &str = "status";
}

/// `GET /manifest`. Made with [`Manifest::new`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Manifest {
    /// The same id the master has the module under (`video`). It is also the id the web
    /// app's module of the same name is registered with.
    pub id: String,
    pub version: String,
    /// [`CONTRACT`] as the module was built against it.
    pub contract: u32,
    /// The oldest master it works with: the [`VERSION`] of the crates it was built against. A master older than this does not use it
    /// and says why.
    pub core: String,
    /// What it provides: the [`capability`] names.
    pub capabilities: Vec<String>,
    /// Its part of the web app, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<Ui>,
    /// What is missing for it to do its work (a program it needs, a folder it cannot write), in words for the administrator.
    /// A module that answers with some is running but not set up; the administrator's page says so.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setup: Vec<String>,
}

/// A module's part of the web app: a script (an ES module whose default export is the module, see
/// `frontend/src/sdk/registry.ts`) and optionally a style sheet, both served under [`UI_PREFIX`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ui {
    /// The script, relative to [`UI_PREFIX`] (`index.js`).
    pub entry: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub css: Option<String>,
    /// [`WEB_SDK`] as the script was built against it.
    pub sdk: u32,
    /// The languages the module has translations for: `strings.<lang>.json` under [`UI_PREFIX`], English text -> translation. The
    /// app fetches the one of the language in use and merges it with its own strings.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub strings: Vec<String>,
}

impl Manifest {
    pub fn new(id: impl Into<String>, version: impl Into<String>, capabilities: Vec<String>) -> Self {
        Self {
            id: id.into(),
            version: version.into(),
            contract: CONTRACT,
            core: VERSION.into(),
            capabilities,
            ui: None,
            setup: Vec::new(),
        }
    }

    pub fn ui(mut self, ui: Option<Ui>) -> Self {
        self.ui = ui;
        self
    }

    pub fn setup(mut self, setup: Vec<String>) -> Self {
        self.setup = setup;
        self
    }

    pub fn has(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }
}

/// Whether `id` can name a module: short, lower-case letters and digits (it is also part of an environment variable).
pub fn valid_id(id: &str) -> bool {
    (1..=24).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// What a video file says about itself (`probe.video`). Everything is optional: files differ in what they say.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Probe {
    pub duration_ms: Option<i64>,
    /// The size as it is shown (a sideways video has its sides swapped).
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub codec: Option<String>,
    /// Unix seconds, from the file's own tags.
    pub taken_at: Option<i64>,
    /// (latitude, longitude), from the file's own tags.
    pub position: Option<(f64, f64)>,
}

/// Whether `header` (the value of `Authorization`) is `Bearer <token>`. Compares without stopping at the first difference.
pub fn bearer_matches(header: Option<&str>, token: &str) -> bool {
    let Some(given) = header.and_then(|h| h.strip_prefix("Bearer ")) else { return false };
    given.len() == token.len() && given.bytes().zip(token.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_master_must_be_at_least_the_version_a_module_needs() {
        assert!(core_satisfied("3.20.0", "3.20.0") && core_satisfied("3.20.0", "3.21.1") && core_satisfied("3.20.0", "4.0.0"));
        assert!(!core_satisfied("3.20.0", "3.19.9") && !core_satisfied("3.20.1", "3.20.0"));
        assert!(core_satisfied("3.20.0", "3.20.0-rc1"), "a pre-release counts as its number");
        assert!(
            !core_satisfied("x", "3.20.0") && !core_satisfied("3.20.0", ""),
            "what is not a version is not satisfied"
        );
        let m = Manifest::new("video", "1", vec![]);
        assert!(core_satisfied(&m.core, VERSION) && m.contract == CONTRACT);
    }

    #[test]
    fn the_whole_token_is_compared() {
        assert!(bearer_matches(Some("Bearer secret-token"), "secret-token"));
        for bad in [
            None,
            Some(""),
            Some("secret-token"),
            Some("bearer secret-token"),
            Some("Bearer secret-token-and-more"),
            Some("Bearer secret-toke"),
            Some("Bearer secret-tokeN"),
        ] {
            assert!(!bearer_matches(bad, "secret-token"), "{bad:?}");
        }
    }

    #[test]
    fn a_manifest_says_what_it_provides() {
        let m: Manifest = serde_json::from_str(r#"{"id":"video","version":"1.0.0","contract":1,"core":"3.20.0","capabilities":["poster","status"]}"#).unwrap();
        assert!(m.has(capability::POSTER) && m.has(capability::STATUS) && !m.has(capability::PROBE_VIDEO));
        assert!(m.ui.is_none(), "a module without screens says nothing of them");
        let json = format!(r#"{{"id":"video","version":"1","contract":1,"core":"3.20.0","capabilities":[],"ui":{{"entry":"index.js","sdk":{WEB_SDK}}}}}"#);
        let with: Manifest = serde_json::from_str(&json).unwrap();
        assert_eq!(with.ui, Some(Ui { entry: "index.js".into(), css: None, sdk: WEB_SDK, strings: vec![] }));
    }

    #[test]
    fn ids_are_short_plain_words() {
        for ok in ["video", "docs", "a1"] {
            assert!(valid_id(ok), "{ok}");
        }
        for bad in ["", "Video", "my-docs", "docs/x", "..", &"a".repeat(25)] {
            assert!(!valid_id(bad), "{bad:?}");
        }
    }
}
