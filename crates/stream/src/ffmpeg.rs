//! `ffprobe` and `ffmpeg`: reading what a video is (the size, length and sound come from `fafnirx_common::probe`), listing its
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
    let probe = fafnirx_common::probe::parse(json.as_bytes())?;
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

/// Runs `ffprobe` on `file`.
pub async fn probe(ffprobe: &str, file: &Path) -> Result<Source> {
    let json =
        ffprobe_output(ffprobe, &["-v", "error", "-print_format", "json", "-show_format", "-show_streams"], file)
            .await?;
    parse_probe(&json)
}

/// The arguments that make segment `start`..`start + len` (seconds) of `source` in quality `v`, as MPEG-TS with
/// timestamps that continue from the segment before it, so that a player joins them without a gap.
pub fn segment_args(
    source: &Path,
    out: &Path,
    v: &Variant,
    start: f64,
    len: f64,
    has_audio: bool,
    vcodec: &str,
) -> Vec<String> {
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
        s.tags.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone()).unwrap_or_default()
    };
    Ok(out
        .streams
        .iter()
        .enumerate()
        .filter(|(_, s)| s.codec_name.as_deref().is_some_and(|c| TEXT_SUBTITLES.contains(&c)))
        .map(|(stream, s)| Embedded {
            stream,
            language: tag(s, "language"),
            title: tag(s, "title"),
            forced: s.disposition.forced != 0,
        })
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
mod tests {
    use super::*;
    use crate::hls::variants;

    const PROBE: &str = r#"{
        "streams": [
            { "codec_type": "video", "width": 1920, "height": 1080, "duration": "61.5" },
            { "codec_type": "audio" }
        ],
        "format": { "duration": "61.480000" }
    }"#;

    #[test]
    fn a_probe_gives_size_length_and_sound() {
        let s = parse_probe(PROBE).unwrap();
        assert_eq!((s.width, s.height, s.has_audio), (1920, 1080, true));
        assert_eq!(s.duration, 61.48, "the length of the whole file, not of one stream");
    }

    #[test]
    fn a_video_recorded_on_its_side_is_measured_the_way_it_is_shown() {
        let json = r#"{"streams":[{"codec_type":"video","width":1920,"height":1080,"side_data_list":[{"rotation":-90}]}],"format":{"duration":"5"}}"#;
        let s = parse_probe(json).unwrap();
        assert_eq!((s.width, s.height, s.has_audio), (1080, 1920, false));
    }

    #[test]
    fn what_is_not_a_video_is_refused() {
        let song = r#"{"streams":[{"codec_type":"audio"},{"codec_type":"video","width":500,"height":500,"disposition":{"attached_pic":1}}],"format":{"duration":"200"}}"#;
        assert!(format!("{:#}", parse_probe(song).unwrap_err()).contains("no video"), "cover art is not a video");
        assert!(
            parse_probe(r#"{"streams":[{"codec_type":"video","width":10,"height":10}],"format":{}}"#).is_err(),
            "no length"
        );
        assert!(
            parse_probe(r#"{"streams":[{"codec_type":"video","height":10}],"format":{"duration":"1"}}"#).is_err(),
            "no size"
        );
        assert!(parse_probe("not json").is_err());
    }

    #[test]
    fn a_segment_is_cut_at_its_place_and_continues_the_clock() {
        let v = &variants(&parse_probe(PROBE).unwrap())[1];
        let args =
            segment_args(Path::new("/d/files/pc/a.mkv"), Path::new("/c/seg2.tmp"), v, 12.0, 6.0, true, "libx264");
        let at = |flag: &str| args.iter().position(|a| a == flag).map(|i| args[i + 1].as_str());
        assert_eq!(at("-ss"), Some("12.000"));
        assert_eq!(at("-t"), Some("6.000"));
        assert_eq!(at("-output_ts_offset"), Some("12.000"));
        assert_eq!(at("-vf"), Some("scale=1280:720,format=yuv420p"));
        assert_eq!(at("-b:v"), Some("2800k"));
        assert_eq!(at("-c:a"), Some("aac"));
        assert_eq!(at("-f"), Some("mpegts"));
        assert!(args.iter().position(|a| a == "-ss") < args.iter().position(|a| a == "-i"), "seek before the input");
        assert_eq!(args.last().map(String::as_str), Some("/c/seg2.tmp"));
        assert!(!args.iter().any(|a| a.contains("&&") || a.contains(';')), "nothing is run through a shell");
    }

    #[test]
    fn a_video_without_sound_is_made_without_it() {
        let v = &variants(&parse_probe(PROBE).unwrap())[0];
        let args = segment_args(Path::new("a"), Path::new("b"), v, 0.0, 6.0, false, "h264_nvenc");
        assert!(!args.contains(&"-c:a".to_string()) && !args.contains(&"0:a:0".to_string()));
        assert!(
            args.contains(&"h264_nvenc".to_string()) && !args.contains(&"-preset".to_string()),
            "x264 options only for x264"
        );
    }

    #[test]
    fn embedded_subtitles_are_the_text_ones_with_their_place_among_all() {
        let json = r#"{"streams":[
            {"codec_name":"hdmv_pgs_subtitle","tags":{"language":"eng"},"disposition":{"forced":0}},
            {"codec_name":"ass","tags":{"language":"jpn","TITLE":"Signs"},"disposition":{"forced":1}},
            {"codec_name":"subrip","disposition":{}}]}"#;
        let found = parse_embedded(json).unwrap();
        assert_eq!(
            found,
            [
                Embedded { stream: 1, language: "jpn".into(), title: "Signs".into(), forced: true },
                Embedded { stream: 2, language: String::new(), title: String::new(), forced: false },
            ]
        );
        assert!(parse_embedded("{}").unwrap().is_empty());
        let args = embedded_subtitle_args(Path::new("v.mkv"), 1, Path::new("o.vtt"));
        assert!(args.windows(2).any(|w| w == ["-map", "0:s:1"]));
    }
}
