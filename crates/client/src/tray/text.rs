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
    /// No client is running (not started, or it stopped with a fatal error): the menu offers to start it.
    pub stopped: bool,
}

/// Works out what the icon should say from the status file and the pause marker.
pub fn view(s: Option<&daemon::Status>, paused: bool, now: i64) -> View {
    let mk = |light, text: String, syncing| View { light, text, syncing, stopped: light == Light::Stopped };
    if paused {
        return mk(Light::Paused, "Paused".into(), false);
    }
    let Some(s) = s else {
        return mk(Light::Stopped, "Not running".into(), false);
    };
    if !s.fatal.is_empty() {
        return View { stopped: true, ..mk(Light::Error, format!("Stopped: {}", s.fatal), false) };
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
        if v < 1000.0 {
            return if unit == "B" { format!("{n} B") } else { format!("{v:.1} {unit}") };
        }
        v /= 1000.0;
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

/// The first line of a text, cut to fit a menu line.
/// The first line of a text, cut to fit a menu line.
pub fn one_line(s: &str) -> String {
    let first = s.lines().next().unwrap_or("");
    if first.chars().count() > MAX_LINE {
        format!("{}...", first.chars().take(MAX_LINE - 3).collect::<String>())
    } else {
        first.to_string()
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
            now: one_line(&format!("The client stopped: {}", s.fatal)),
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
        now: one_line(&now_line),
        last_sync: one_line(&last_sync),
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
        error: one_line(&error),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tray/text.rs"]
mod tests;
