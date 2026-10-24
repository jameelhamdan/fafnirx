//! Video support through `ffmpeg` and `ffprobe`: what a video is (length, size, codec, date, place), a poster
//! frame, and a copy that every browser can play. The programs come from `PATH`, or from `FAFNIRX_FFMPEG`
//! and `FAFNIRX_FFPROBE`. Without them videos still show up, just without details or pictures.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

fn program(var: &str, default: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| default.into())
}

fn ffmpeg() -> String {
    program("FAFNIRX_FFMPEG", "ffmpeg")
}

fn ffprobe() -> String {
    program("FAFNIRX_FFPROBE", "ffprobe")
}

/// Whether ffmpeg runs here. Looked up once.
pub fn available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let found = Command::new(ffmpeg())
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !found {
            tracing::warn!("ffmpeg was not found: videos get no details, posters or playable copies");
        }
        found
    })
}

#[derive(Debug, Default, PartialEq)]
pub struct Probe {
    pub duration_ms: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub codec: Option<String>,
    pub taken_at: Option<i64>,
    pub position: Option<(f64, f64)>,
}

pub fn probe(path: &Path) -> Result<Probe> {
    let out = Command::new(ffprobe())
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .context("run ffprobe")?;
    if !out.status.success() {
        bail!("ffprobe: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    parse_probe(&out.stdout)
}

fn parse_probe(json: &[u8]) -> Result<Probe> {
    let p = fafnirx_common::probe::parse(json)?;
    let (codec, width, height) = p.video.as_ref().map_or((None, None, None), |v| (v.codec.clone(), v.width, v.height));
    Ok(Probe {
        duration_ms: p.duration.map(|d| (d * 1000.0).round() as i64),
        width: width.map(i64::from),
        height: height.map(i64::from),
        codec,
        taken_at: p
            .tag(&["creation_time", "com.apple.quicktime.creationdate"])
            .and_then(crate::library::meta::parse_time)
            .filter(|t| *t > 0),
        position: p.tag(&["com.apple.quicktime.location.ISO6709", "location"]).and_then(parse_iso6709),
    })
}

/// "+52.5200+013.4050+034.000/" to (latitude, longitude).
fn parse_iso6709(text: &str) -> Option<(f64, f64)> {
    let text = text.trim().trim_end_matches('/');
    let mut parts: Vec<String> = Vec::new();
    for c in text.chars() {
        if c == '+' || c == '-' || parts.is_empty() {
            parts.push(String::new());
        }
        parts.last_mut()?.push(c);
    }
    let lat: f64 = parts.first()?.parse().ok()?;
    let lon: f64 = parts.get(1)?.parse().ok()?;
    crate::library::meta::valid_position(lat, lon)
}

/// Whether browsers play this file as it is (so that no second copy is needed).
pub fn plays_everywhere(extension: &str, codec: Option<&str>) -> bool {
    match extension {
        "webm" => true,
        "mp4" | "m4v" | "mov" => codec == Some("h264"),
        _ => false,
    }
}

fn run(command: &mut Command, what: &str) -> Result<()> {
    let out = command.stdin(Stdio::null()).output().with_context(|| format!("run {what}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{what} failed: {}", err.lines().last().unwrap_or("no message"));
    }
    Ok(())
}

/// Writes a JPEG of one frame no larger than `max` on its longest side. `dest` must end in `.jpg`.
pub fn poster(src: &Path, max: u32, dest: &Path) -> Result<()> {
    let scale = format!("scale=min({max}\\,iw):min({max}\\,ih):force_original_aspect_ratio=decrease");
    // A second in is usually past the black first frame; a clip shorter than that gets its first frame.
    for seek in ["1", "0"] {
        let _ = std::fs::remove_file(dest);
        let made = run(
            Command::new(ffmpeg())
                .args(["-v", "error", "-y", "-ss", seek, "-i"])
                .arg(src)
                .args(["-frames:v", "1", "-vf", &scale, "-q:v", "4"])
                .arg(dest),
            "ffmpeg poster",
        );
        if made.is_ok() && std::fs::metadata(dest).is_ok_and(|m| m.len() > 0) {
            return Ok(());
        }
    }
    let _ = std::fs::remove_file(dest);
    bail!("no frame could be taken from {}", src.display())
}

/// Writes a 720p H.264/AAC MP4 that starts playing before it is fully downloaded.
pub fn proxy(src: &Path, dest: &Path) -> Result<()> {
    run(
        Command::new(ffmpeg())
            .args(["-v", "error", "-y", "-i"])
            .arg(src)
            .args(["-map", "0:v:0", "-map", "0:a:0?", "-vf", "scale=-2:min(720\\,ih)"])
            .args(["-c:v", "libx264", "-preset", "veryfast", "-crf", "28", "-pix_fmt", "yuv420p"])
            .args(["-c:a", "aac", "-b:a", "96k", "-movflags", "+faststart"])
            .arg(dest),
        "ffmpeg proxy",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_probe_result_is_read() {
        let json = br#"{
            "streams": [
                {"codec_type": "audio", "codec_name": "aac"},
                {"codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080,
                 "side_data_list": [{"rotation": -90}]}
            ],
            "format": {"duration": "12.5", "tags": {
                "creation_time": "2023-11-14T22:13:20.000000Z",
                "com.apple.quicktime.location.ISO6709": "+52.5200+013.4050+034.000/"}}
        }"#;
        let p = parse_probe(json).unwrap();
        assert_eq!((p.width, p.height), (Some(1080), Some(1920)), "a sideways video is shown upright");
        assert_eq!(p.duration_ms, Some(12_500));
        assert_eq!(p.codec.as_deref(), Some("h264"));
        assert_eq!(p.taken_at, Some(1_700_000_000));
        assert_eq!(p.position, Some((52.52, 13.405)));
    }

    #[test]
    fn a_file_without_tags_has_only_what_it_says() {
        let p = parse_probe(
            br#"{"streams": [{"codec_type": "video", "codec_name": "vp9", "width": 4, "height": 2}], "format": {}}"#,
        )
        .unwrap();
        assert_eq!((p.width, p.height, p.duration_ms, p.taken_at, p.position), (Some(4), Some(2), None, None, None));
    }

    #[test]
    fn positions_in_the_southern_and_western_hemispheres_are_negative() {
        assert_eq!(parse_iso6709("-33.8688+151.2093/"), Some((-33.8688, 151.2093)));
        assert_eq!(parse_iso6709("+40.7128-074.0060+10.0/"), Some((40.7128, -74.006)));
        assert_eq!(parse_iso6709("+00.0000+000.0000/"), None, "0,0 is what broken cameras write");
        assert_eq!(parse_iso6709("nonsense"), None);
    }

    #[test]
    fn only_formats_every_browser_plays_skip_the_copy() {
        assert!(plays_everywhere("mp4", Some("h264")) && plays_everywhere("webm", None));
        assert!(!plays_everywhere("mp4", Some("hevc")) && !plays_everywhere("mkv", Some("h264")));
    }
}
