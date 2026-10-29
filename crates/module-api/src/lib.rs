//! What the master and a module agree on. A module is a separate service (its own dependencies) that adds one kind of
//! feature to fafnirx. The master reaches it by an address and a token, asks it for its [`Manifest`], and from then on uses
//! the capabilities the manifest lists. Without a module the master simply does not have the feature. The rules are
//! written down in docs/modules.md.
//!
//! A module gets there in one of two ways, and the contract is the same: as a service somebody runs and names with
//! `FAFNIRX_MODULE_<ID>_URL` and `FAFNIRX_MODULE_<ID>_TOKEN` (a container), or as a *plugin*, a bundle (`plugin.json` and
//! what it runs) in `<data>/plugins/<id>/` that the master starts itself, on a loopback port, with a token of its own.
//!
//! Every request a module gets carries the shared token as `Authorization: Bearer`, except `GET /health`. The master has
//! already checked the login and the user's right to the file; a module has no users of its own.

use serde::{Deserialize, Serialize};

/// The version of this contract. A module whose manifest says another number is not used (and the master says so in its
/// log), so a module and a master of different generations never half-work together.
pub const CONTRACT: u32 = 1;

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
pub const WEB_SDK: u32 = 1;
/// The file that makes a folder of `<data>/plugins/` a plugin.
pub const PLUGIN_FILE: &str = "plugin.json";

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

/// `GET /manifest`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// The same id the master has the module under (`video` for `FAFNIRX_MODULE_VIDEO_URL`). It is also the id the web
    /// app's module of the same name is registered with.
    pub id: String,
    pub version: String,
    /// [`CONTRACT`] as the module was built against it.
    pub contract: u32,
    /// What it provides: the [`capability`] names.
    pub capabilities: Vec<String>,
    /// Its part of the web app, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<Ui>,
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
}

impl Manifest {
    pub fn has(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }
}

/// `plugin.json`: what makes a folder a plugin, and how the master starts it. The bundle is made by the module's own build and
/// unpacked into `<data>/plugins/<id>/`; nothing else of it is the master's business.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginFile {
    /// The id of the module, and the name of the folder.
    pub id: String,
    pub version: String,
    /// [`CONTRACT`] as the module was built against it.
    pub contract: u32,
    /// What it was built for (`linux-x86_64`, `linux-aarch64`).
    pub platform: String,
    /// The program, relative to the plugin's folder. It is given the settings below and these variables:
    /// `FAFNIRX_MODULE_TOKEN`, `FAFNIRX_MODULE_LISTEN` (a loopback address) and `FAFNIRX_DATA`.
    pub exec: String,
    /// More settings for it. `{dir}` in a value is the plugin's folder.
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
}

/// Whether `id` can name a module: short, lower-case letters and digits (it is also part of an environment variable).
pub fn valid_id(id: &str) -> bool {
    (1..=24).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

impl PluginFile {
    /// Why this cannot run as the plugin in folder `id` on `platform`, if it cannot.
    pub fn problem(&self, id: &str, platform: &str) -> Option<String> {
        let relative = |p: &str| {
            !p.is_empty()
                && !p.starts_with('/')
                && !p.contains('\\')
                && p.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
        };
        if !valid_id(&self.id) || self.id != id {
            return Some(format!("it is called {:?}, not {id:?}", self.id));
        }
        if self.contract != CONTRACT {
            return Some(format!("it speaks contract {}, this master speaks {CONTRACT}", self.contract));
        }
        if self.platform != platform {
            return Some(format!("it is built for {}, this is {platform}", self.platform));
        }
        if !relative(&self.exec) {
            return Some("its program is not a path inside its folder".into());
        }
        None
    }
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
        let m: Manifest =
            serde_json::from_str(r#"{"id":"video","version":"1.0.0","contract":1,"capabilities":["poster","status"]}"#)
                .unwrap();
        assert!(m.has(capability::POSTER) && m.has(capability::STATUS) && !m.has(capability::PROBE_VIDEO));
        assert!(m.ui.is_none(), "a module without screens says nothing of them");
        let with: Manifest = serde_json::from_str(
            r#"{"id":"video","version":"1","contract":1,"capabilities":[],"ui":{"entry":"index.js","sdk":1}}"#,
        )
        .unwrap();
        assert_eq!(with.ui, Some(Ui { entry: "index.js".into(), css: None, sdk: WEB_SDK }));
    }

    fn plugin(exec: &str) -> PluginFile {
        PluginFile {
            id: "video".into(),
            version: "3.12.0".into(),
            contract: CONTRACT,
            platform: "linux-x86_64".into(),
            exec: exec.into(),
            env: Default::default(),
        }
    }

    #[test]
    fn a_plugin_must_fit_its_folder_this_master_and_this_machine() {
        assert_eq!(plugin("bin/fafnirx-video").problem("video", "linux-x86_64"), None);
        assert!(plugin("bin/x").problem("docs", "linux-x86_64").is_some(), "the folder is called docs");
        assert!(plugin("bin/x").problem("video", "linux-aarch64").is_some(), "another machine");
        let mut other = plugin("bin/x");
        other.contract += 1;
        assert!(other.problem("video", "linux-x86_64").is_some(), "another contract");
        for bad in ["", "/bin/x", "../x", "bin/../../x", "bin//x", "./x", "bin\\x"] {
            assert!(plugin(bad).problem("video", "linux-x86_64").is_some(), "{bad:?}");
        }
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
