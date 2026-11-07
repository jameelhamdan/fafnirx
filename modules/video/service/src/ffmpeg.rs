//! `ffprobe` and `ffmpeg`: reading what a video is (the size, length and sound come from `crate::probe`), listing its
//! subtitle tracks, making one segment of one quality of it, and writing subtitles as WebVTT.

use crate::hls::{Source, Variant};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// What `ffprobe` said, as the size, length and sound of the video. A file with no picture (a song with cover art counts
/// as none), no size or no length is not a video.
pub fn parse_probe(json: &str) -> Result<Source> {
    let probe = crate::probe::parse(json.as_bytes())?;
    let video = probe.video.context("the file has no video")?;
    let (Some(width), Some(height)) = (video.width.filter(|w| *w > 0), video.height.filter(|h| *h > 0)) else {
        bail!("the video has no size");
    };
    let duration = probe.duration.context("the video has no length")?;
    Ok(Source { width, height, duration, has_audio: probe.has_audio })
}

/// Runs `ffprobe` with `args` on `file` and returns what it printed.
async fn ffprobe_output(ffprobe: &str, args: &[&str], file: &Path) -> Result<String> {
    let out = Command::new(ffprobe)
        .args(args)
        .arg(file)
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .with_context(|| format!("run {ffprobe} (is ffmpeg installed?)"))?;
    if !out.status.success() {
        bail!("ffprobe could not read the file: {}", tail(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What `ffprobe` says about `file`, as JSON.
async fn probe_json(ffprobe: &str, file: &Path) -> Result<String> {
    ffprobe_output(ffprobe, &["-v", "error", "-print_format", "json", "-show_format", "-show_streams"], file).await
}

/// Runs `ffprobe` on `file`, for the size, length and sound a stream is made from.
pub async fn probe(ffprobe: &str, file: &Path) -> Result<Source> {
    parse_probe(&probe_json(ffprobe, file).await?)
}

/// Runs `ffprobe` on `file`, for everything it says (tags included).
pub async fn details(ffprobe: &str, file: &Path) -> Result<crate::probe::Probe> {
    crate::probe::parse(probe_json(ffprobe, file).await?.as_bytes())
}

/// The arguments that take one frame of `source`, `seek` seconds in, to the JPEG `out`, no larger than `max` pixels on its
/// longest side.
pub fn poster_args(source: &Path, out: &Path, max: u32, seek: &str) -> Vec<String> {
    let scale = format!("scale=min({max}\\,iw):min({max}\\,ih):force_original_aspect_ratio=decrease");
    let mut a: Vec<String> = ["-v", "error", "-y", "-ss", seek, "-i"].iter().map(|s| s.to_string()).collect();
    a.push(source.to_string_lossy().into_owned());
    a.extend(["-frames:v", "1", "-vf", &scale, "-q:v", "4"].iter().map(|s| s.to_string()));
    a.push(out.to_string_lossy().into_owned());
    a
}

/// Size of the small frame a poster is chosen from (in pixels): enough to tell a black or blank frame from a picture.
const LOOK_W: usize = 32;
const LOOK_H: usize = 18;

/// The arguments that take one tiny frame of `source`, `seek` seconds in, as raw RGB on standard output.
fn look_args(source: &Path, seek: f64) -> Vec<String> {
    let mut a: Vec<String> = ["-v", "error", "-nostdin", "-ss", &format!("{seek:.3}"), "-i"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    a.push(source.to_string_lossy().into_owned());
    let filter = format!("scale={LOOK_W}:{LOOK_H},format=rgb24");
    a.extend(["-frames:v", "1", "-vf", &filter, "-f", "rawvideo", "-"].iter().map(|s| s.to_string()));
    a
}

/// Whether a tiny RGB frame shows something: not black, not blown out to white, and not one flat colour.
pub fn shows_a_picture(rgb: &[u8]) -> bool {
    let pixels: Vec<[f64; 3]> = rgb.as_chunks::<3>().0.iter().map(|p| p.map(f64::from)).collect();
    if pixels.is_empty() {
        return false;
    }
    let luma: Vec<f64> = pixels.iter().map(|p| 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]).collect();
    let n = luma.len() as f64;
    let mean = luma.iter().sum::<f64>() / n;
    let spread = (luma.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / n).sqrt();
    // How far apart the colour channels are: a grey frame has none, a coloured one has some.
    let colour = pixels.iter().map(|p| p[0].max(p[1]).max(p[2]) - p[0].min(p[1]).min(p[2])).sum::<f64>() / n;
    (24.0..235.0).contains(&mean) && (spread >= 10.0 || colour >= 20.0)
}

/// Where to look, in seconds, for a video of `duration` seconds (none when its length is not known): 10%, 20%, ... 90%.
pub fn poster_candidates(duration: Option<f64>) -> Vec<f64> {
    match duration.filter(|d| d.is_finite() && *d > 0.0) {
        Some(d) => (1..=9).map(|i| d * i as f64 / 10.0).collect(),
        None => vec![1.0, 3.0, 10.0, 30.0],
    }
}

/// The place in the video (milliseconds) for its poster: the first frame at 10%, 20%, ... of its length that shows a
/// picture. When none does, 10% (a video that is dark throughout has no better one); none when no frame can be taken.
pub async fn pick_poster(ffmpeg: &str, source: &Path, duration: Option<f64>) -> Option<i64> {
    let mut first = None;
    for at in poster_candidates(duration) {
        let out = Command::new(ffmpeg)
            .args(look_args(source, at))
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .ok()
            .filter(|o| o.status.success() && o.stdout.len() == LOOK_W * LOOK_H * 3);
        let Some(out) = out else { continue };
        let ms = (at * 1000.0).round() as i64;
        if shows_a_picture(&out.stdout) {
            return Some(ms);
        }
        first.get_or_insert(ms);
    }
    first
}

/// The part of a video to make: the source file, where to write, the span in seconds, whether the source has sound and the video codec.
#[derive(Clone, Copy)]
pub struct Segment<'a> {
    pub source: &'a Path,
    pub out: &'a Path,
    pub start: f64,
    pub len: f64,
    pub has_audio: bool,
    pub vcodec: &'a str,
}

/// The arguments that make segment `start`..`start + len` (seconds) of `source` in quality `v`, as MPEG-TS with
/// timestamps that continue from the segment before it, so that a player joins them without a gap.
pub fn segment_args(seg: &Segment, v: &Variant) -> Vec<String> {
    let Segment { source, out, start, len, has_audio, vcodec } = *seg;
    let mut a: Vec<String> = Vec::new();
    let mut add = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
    add(&["-nostdin", "-hide_banner", "-loglevel", "error"]);
    // Seeking before the input is quick; the length goes after it, so that it counts what comes out.
    add(&["-ss", &format!("{start:.3}"), "-i"]);
    a.push(source.to_string_lossy().into_owned());
    let mut add = |items: &[&str]| a.extend(items.iter().map(|s| s.to_string()));
    add(&["-t", &format!("{len:.3}"), "-map", "0:v:0"]);
    if has_audio {
        add(&["-map", "0:a:0"]);
    }
    add(&["-vf", &format!("scale={}:{},format=yuv420p", v.width, v.height), "-c:v", vcodec]);
    if vcodec == "libx264" {
        add(&["-preset", "veryfast", "-profile:v", "high"]);
    }
    let kbps = v.video_kbps;
    add(&[
        "-b:v",
        &format!("{kbps}k"),
        "-maxrate",
        &format!("{}k", kbps * 107 / 100),
        "-bufsize",
        &format!("{}k", kbps * 3 / 2),
    ]);
    if has_audio {
        add(&["-c:a", "aac", "-b:a", &format!("{}k", v.audio_kbps), "-ac", "2"]);
    }
    add(&["-output_ts_offset", &format!("{start:.3}"), "-f", "mpegts", "-y"]);
    a.push(out.to_string_lossy().into_owned());
    a
}

/// The arguments that turn a subtitle file (SubRip, WebVTT, SSA/ASS) into WebVTT. `charset` is the character set it is
/// written in when that is not UTF-8.
pub fn subtitle_args(source: &Path, out: &Path, charset: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = ["-nostdin", "-hide_banner", "-loglevel", "error"].iter().map(|s| s.to_string()).collect();
    if let Some(charset) = charset {
        a.extend(["-sub_charenc".to_string(), charset.to_string()]);
    }
    a.push("-i".into());
    a.push(source.to_string_lossy().into_owned());
    a.extend(["-f", "webvtt", "-y"].iter().map(|s| s.to_string()));
    a.push(out.to_string_lossy().into_owned());
    a
}

/// A subtitle track inside a video file, as `ffprobe` lists it.
#[derive(Clone, Debug, PartialEq, Deserialize, serde::Serialize)]
pub struct Embedded {
    /// Its place among the file's subtitle streams (`-map 0:s:N`).
    pub stream: usize,
    pub language: String,
    pub title: String,
    pub forced: bool,
}

/// Codecs of subtitles that are text, so that they can be written as WebVTT. Pictures (PGS, VobSub, DVB) cannot.
const TEXT_SUBTITLES: [&str; 7] = ["subrip", "srt", "ass", "ssa", "webvtt", "mov_text", "text"];

/// The text subtitle tracks in what `ffprobe -select_streams s -show_entries ...` said, in the order of the file.
pub fn parse_embedded(json: &str) -> Result<Vec<Embedded>> {
    #[derive(Deserialize)]
    struct Out {
        #[serde(default)]
        streams: Vec<S>,
    }
    #[derive(Deserialize)]
    struct S {
        codec_name: Option<String>,
        #[serde(default)]
        tags: std::collections::HashMap<String, String>,
        #[serde(default)]
        disposition: Forced,
    }
    #[derive(Deserialize, Default)]
    struct Forced {
        #[serde(default)]
        forced: u8,
    }
    let out: Out = serde_json::from_str(json).context("ffprobe's answer is not JSON")?;
    let tag = |s: &S, name: &str| {
        s.tags
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    Ok(out
        .streams
        .iter()
        .enumerate()
        .filter(|(_, s)| s.codec_name.as_deref().is_some_and(|c| TEXT_SUBTITLES.contains(&c)))
        .map(|(stream, s)| Embedded { stream, language: tag(s, "language"), title: tag(s, "title"), forced: s.disposition.forced != 0 })
        .collect())
}

/// Runs `ffprobe` on `file` for its subtitle tracks.
pub async fn embedded_subtitles(ffprobe: &str, file: &Path) -> Result<Vec<Embedded>> {
    let args = [
        "-v",
        "error",
        "-select_streams",
        "s",
        "-print_format",
        "json",
        "-show_entries",
        "stream=codec_name:stream_tags=language,title:stream_disposition=forced",
    ];
    parse_embedded(&ffprobe_output(ffprobe, &args, file).await?)
}

/// The arguments that write subtitle stream number `stream` of a video as WebVTT.
pub fn embedded_subtitle_args(video: &Path, stream: usize, out: &Path) -> Vec<String> {
    let mut a: Vec<String> = ["-nostdin", "-hide_banner", "-loglevel", "error"].iter().map(|s| s.to_string()).collect();
    a.push("-i".into());
    a.push(video.to_string_lossy().into_owned());
    a.extend(["-map".to_string(), format!("0:s:{stream}")]);
    a.extend(["-f", "webvtt", "-y"].iter().map(|s| s.to_string()));
    a.push(out.to_string_lossy().into_owned());
    a
}

/// Runs `ffmpeg` with `args`. The program is stopped if the caller gives up (the request was dropped).
pub async fn run(ffmpeg: &str, args: &[String]) -> Result<()> {
    let out = Command::new(ffmpeg)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .with_context(|| format!("run {ffmpeg} (is ffmpeg installed?)"))?;
    if !out.status.success() {
        bail!("ffmpeg failed: {}", tail(&out.stderr));
    }
    Ok(())
}

fn tail(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let chars: Vec<char> = text.trim().chars().collect();
    chars[chars.len().saturating_sub(400)..].iter().collect()
}

#[cfg(test)]
#[path = "../tests/unit/ffmpeg.rs"]
mod tests;
