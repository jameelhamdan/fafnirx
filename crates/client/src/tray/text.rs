//! The words in the tray menu and tooltip, worked out from the daemon's status file.

use super::icon::Light;
use crate::daemon;

const MAX_LINE: usize = 90;

#[derive(Clone, PartialEq, Debug)]
pub struct View {
    pub light: Light,
    pub text: String,
    /// Uploading or scanning right now: the activity light blinks.
    pub syncing: bool,
}

/// Works out what the icon should say from the status file and the pause marker.
pub fn view(s: Option<&daemon::Status>, paused: bool, now: i64) -> View {
    let mk = |light, text: String, syncing| View { light, text, syncing };
    if paused {
        return mk(Light::Paused, "Paused".into(), false);
    }
    let Some(s) = s else {
        return mk(Light::Stopped, "Not running".into(), false);
    };
    if !s.fatal.is_empty() {
        return mk(Light::Error, format!("Stopped: {}", s.fatal), false);
    }
    if now - s.updated > 120 {
        return mk(Light::Stopped, "Not running".into(), false);
    }
    if !s.online {
        let why = if s.message.is_empty() { "offline" } else { &s.message };
        return mk(Light::Error, format!("Error: {why}"), false);
    }
    if !s.last_error.is_empty() && s.last.failed > 0 {
        return mk(Light::Error, format!("Error: {}", s.last_error), s.syncing);
    }
    let text = match progress(&s.activity) {
        Some(p) if s.syncing => format!("Syncing {p}"),
        _ if s.syncing => "Syncing".into(),
        _ => "Online".into(),
    };
    mk(Light::Ok, text, s.syncing)
}

/// "62%, 12.3 MB/s, 40 s left" while uploading, once the sizes are known.
pub fn progress(a: &daemon::Activity) -> Option<String> {
    if a.phase != "uploading" || a.total_bytes == 0 {
        return None;
    }
    let mut t = format!("{}%", 100 * a.done_bytes / a.total_bytes);
    if a.speed_bps > 0 {
        t.push_str(&format!(", {}/s", human_bytes(a.speed_bps)));
        let left = a.total_bytes.saturating_sub(a.done_bytes) / a.speed_bps;
        if left > 0 {
            t.push_str(&format!(", {} left", span(left as i64)));
        }
    }
    Some(t)
}

pub fn human_bytes(n: u64) -> String {
    let mut v = n as f64;
    for unit in ["B", "KB", "MB", "GB"] {
        if v < 1024.0 {
            return if unit == "B" { format!("{n} B") } else { format!("{v:.1} {unit}") };
        }
        v /= 1024.0;
    }
    format!("{v:.1} TB")
}

/// "42 s", "5 min", "3 h", "2 d".
pub fn span(secs: i64) -> String {
    let s = secs.max(0);
    match s {
        0..=59 => format!("{s} s"),
        60..=3599 => format!("{} min", s / 60),
        3600..=86399 => format!("{} h", s / 3600),
        _ => format!("{} d", s / 86400),
    }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

pub fn clip(s: &str) -> String {
    let one_line = s.lines().next().unwrap_or("");
    if one_line.chars().count() > MAX_LINE {
        format!("{}...", one_line.chars().take(MAX_LINE - 3).collect::<String>())
    } else {
        one_line.to_string()
    }
}

/// The disabled information lines at the top of the menu.
#[derive(Debug, PartialEq)]
pub struct Info {
    pub title: String,
    pub server: String,
    pub device: String,
    pub now: String,
    pub last_sync: String,
    pub on_server: String,
    pub session: String,
    pub error: String,
}

pub fn describe(s: Option<&daemon::Status>, v: &View, now: i64) -> Info {
    let Some(s) = s.filter(|_| v.light != Light::Stopped) else {
        return Info {
            title: format!("fafnirx: {}", v.text),
            server: format!("Server: {}", s.map(|s| s.server.as_str()).filter(|x| !x.is_empty()).unwrap_or("-")),
            device: "Device: -".into(),
            now: "The client is not running".into(),
            last_sync: String::new(),
            on_server: String::new(),
            session: String::new(),
            error: String::new(),
        };
    };
    if !s.fatal.is_empty() {
        return Info {
            title: format!("fafnirx: {}", v.text),
            server: format!("Server: {}", if s.server.is_empty() { "-" } else { &s.server }),
            device: "Device: -".into(),
            now: clip(&format!("The client stopped: {}", s.fatal)),
            last_sync: String::new(),
            on_server: String::new(),
            session: String::new(),
            error: "Fix the cause, then choose Start in this menu".into(),
        };
    }
    let connection = if s.online { "connected" } else { "unreachable" };
    let a = &s.activity;
    let now_line = if v.light == Light::Paused {
        "Paused: nothing is uploaded".to_string()
    } else if !s.online {
        "Waiting for the master".to_string()
    } else if s.syncing && !a.phase.is_empty() && a.phase != "idle" {
        let mut t = if s.initial_sync { format!("First sync of all files: {}", a.phase) } else { cap(&a.phase) };
        if a.total > 0 {
            let position = (a.done + usize::from(a.phase == "uploading")).min(a.total);
            t.push_str(&format!(" {position}/{}", a.total));
        }
        // Progress goes before the file name: a long name is what gets clipped.
        if let Some(p) = progress(a) {
            t.push_str(&format!(", {p}"));
        }
        if !a.current.is_empty() {
            t.push_str(&format!(": {}", a.current));
        }
        t
    } else if s.initial_sync {
        "First sync of all files has not finished".to_string()
    } else if s.next_scan > now {
        format!("Idle, next scan in {}", span(s.next_scan - now))
    } else {
        "Idle".to_string()
    };
    let last_sync = if s.last_cycle == 0 {
        if s.initial_sync {
            "Last sync: the first sync is not finished".to_string()
        } else {
            "Last sync: not yet".to_string()
        }
    } else {
        format!("Last sync {} ago: {} ({})", span(now - s.last_cycle), s.last, human_bytes(s.last.bytes))
    };
    let error = if !s.online && !s.message.is_empty() {
        format!("Problem: {}", s.message)
    } else if !s.last_error.is_empty() {
        format!("Last error: {}", s.last_error)
    } else if s.failing > 0 {
        format!("{} files failed and will be retried", s.failing)
    } else {
        "No errors".to_string()
    };
    let plural = |n: usize| if n == 1 { "" } else { "s" };
    Info {
        title: format!("fafnirx {}: {}", s.version, v.text),
        server: format!("Server: {} ({connection})", if s.server.is_empty() { "-" } else { &s.server }),
        device: format!("Device: {}", if s.device.is_empty() { "-" } else { &s.device }),
        now: clip(&now_line),
        last_sync: clip(&last_sync),
        on_server: format!(
            "On the server: {} file{}, {} from {} folder{}{}",
            s.synced_files,
            plural(s.synced_files),
            human_bytes(s.synced_bytes),
            s.folders,
            plural(s.folders),
            if s.initial_sync { " (the first sync sends all of them)" } else { "" }
        ),
        session: format!(
            "Since start: {} file{} sent ({}), running {}",
            s.session_files,
            plural(s.session_files),
            human_bytes(s.session_bytes),
            span(now - s.started)
        ),
        error: clip(&error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(online: bool, updated: i64) -> daemon::Status {
        daemon::Status { online, updated, ..Default::default() }
    }

    #[test]
    fn a_client_that_could_not_start_says_why_for_as_long_as_it_is_down() {
        let stopped = daemon::Status {
            fatal: "no device token found; run `fafnirx enroll`".into(),
            updated: 1,
            ..Default::default()
        };
        // Long after it exited: still the reason, not a bare "not running".
        let v = view(Some(&stopped), false, 1_000_000);
        assert_eq!((v.light, v.text.as_str()), (Light::Error, "Stopped: no device token found; run `fafnirx enroll`"));
        let info = describe(Some(&stopped), &v, 1_000_000);
        assert!(info.now.starts_with("The client stopped: no device token"), "{}", info.now);
        assert!(!info.error.is_empty());
    }

    #[test]
    fn view_reflects_state() {
        let now = 1_000;
        assert_eq!(view(None, false, now).light, Light::Stopped);
        assert_eq!(view(Some(&status(true, now)), false, now).light, Light::Ok);
        assert_eq!(
            view(Some(&status(true, now - 500)), false, now).light,
            Light::Stopped,
            "stale status means not running"
        );
        let v = view(Some(&daemon::Status { message: "tls".into(), ..status(false, now) }), false, now);
        assert_eq!((v.light, v.text.as_str()), (Light::Error, "Error: tls"));
        assert_eq!(view(Some(&status(true, now)), true, now).light, Light::Paused);
    }

    #[test]
    fn only_a_running_online_client_blinks() {
        let now = 1_000;
        let syncing = daemon::Status { syncing: true, ..status(true, now) };
        let v = view(Some(&syncing), false, now);
        assert_eq!((v.light, v.syncing, v.text.as_str()), (Light::Ok, true, "Syncing"));
        assert!(!view(Some(&syncing), true, now).syncing, "paused does not blink");
        assert!(
            !view(Some(&daemon::Status { syncing: true, ..status(true, now - 500) }), false, now).syncing,
            "a dead daemon does not blink"
        );
        assert!(
            !view(Some(&daemon::Status { syncing: true, ..status(false, now) }), false, now).syncing,
            "offline does not blink"
        );
    }

    fn syncing_status() -> daemon::Status {
        daemon::Status {
            version: "0.1.0".into(),
            server: "https://nas:7443".into(),
            device: "laptop".into(),
            online: true,
            syncing: true,
            updated: 1_000,
            started: 1_000 - 4_000,
            last_cycle: 940,
            last: crate::sync::Report { uploaded: 3, bytes: 2048, ..Default::default() },
            folders: 2,
            synced_files: 1,
            synced_bytes: 6 << 20,
            session_files: 5,
            session_bytes: 3 << 30,
            activity: daemon::Activity {
                phase: "uploading".into(),
                current: "docs/big.bin".into(),
                done: 1,
                total: 4,
                bytes: 5 << 20,
                total_bytes: 20 << 20,
                done_bytes: 5 << 20,
                speed_bps: 2 << 20,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn menu_text_describes_a_running_sync() {
        let s = syncing_status();
        let i = describe(Some(&s), &view(Some(&s), false, 1_000), 1_000);
        assert_eq!(i.title, "fafnirx 0.1.0: Syncing 25%, 2.0 MB/s, 7 s left");
        assert_eq!(i.server, "Server: https://nas:7443 (connected)");
        assert_eq!(i.device, "Device: laptop");
        assert_eq!(i.now, "Uploading 2/4, 25%, 2.0 MB/s, 7 s left: docs/big.bin");
        assert_eq!(i.last_sync, "Last sync 1 min ago: 3 uploaded, 0 renamed, 0 deleted, 0 failed (2.0 KB)");
        assert_eq!(i.on_server, "On the server: 1 file, 6.0 MB from 2 folders");
        assert_eq!(i.session, "Since start: 5 files sent (3.0 GB), running 1 h");
        assert_eq!(i.error, "No errors");
    }

    #[test]
    fn a_new_install_says_that_the_first_sync_sends_all_files() {
        let mut s = syncing_status();
        s.initial_sync = true;
        let i = describe(Some(&s), &view(Some(&s), false, 1_000), 1_000);
        assert!(i.now.starts_with("First sync of all files: uploading 2/4"), "{}", i.now);
        assert!(i.on_server.ends_with("(the first sync sends all of them)"), "{}", i.on_server);
        s.syncing = false;
        s.activity = daemon::Activity::default();
        assert_eq!(
            describe(Some(&s), &view(Some(&s), false, 1_000), 1_000).now,
            "First sync of all files has not finished"
        );
    }

    #[test]
    fn menu_text_for_idle_paused_offline_and_stopped() {
        let mut s = syncing_status();
        s.syncing = false;
        s.activity = daemon::Activity::default();
        s.next_scan = 1_000 + 600;
        assert_eq!(describe(Some(&s), &view(Some(&s), false, 1_000), 1_000).now, "Idle, next scan in 10 min");
        assert_eq!(describe(Some(&s), &view(Some(&s), true, 1_000), 1_000).now, "Paused: nothing is uploaded");

        s.online = false;
        s.message = "cannot reach the master: tcp open error".into();
        let i = describe(Some(&s), &view(Some(&s), false, 1_000), 1_000);
        assert!(i.server.ends_with("(unreachable)") && i.error.starts_with("Problem: cannot reach"), "{i:?}");
        assert_eq!(i.now, "Waiting for the master");

        let i = describe(None, &view(None, false, 1_000), 1_000);
        assert_eq!((i.title.as_str(), i.now.as_str()), ("fafnirx: Not running", "The client is not running"));

        s.online = true;
        s.message.clear();
        s.last_error = "x".repeat(300);
        assert!(describe(Some(&s), &view(Some(&s), false, 1_000), 1_000).error.chars().count() <= MAX_LINE);
    }

    #[test]
    fn sizes_and_spans() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(5 << 20), "5.0 MB");
        assert_eq!(
            [span(5), span(59), span(60), span(7200), span(3 * 86400), span(-4)],
            ["5 s", "59 s", "1 min", "2 h", "3 d", "0 s"]
        );
    }
}
