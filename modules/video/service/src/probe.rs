//! What `ffprobe -print_format json -show_format -show_streams` says about a video. Both the details the master asks for
//! (`derive`) and the segments of a stream (`ffmpeg`) read it, so the rules (which stream is the
//! picture, how a sideways video is measured, which length counts) are written once, here.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

/// A video file as `ffprobe` describes it. Everything is optional: files differ in what they say.
#[derive(Debug, Default, PartialEq)]
pub struct Probe {
    /// The first stream that is a picture of its own (cover art in a song is not one).
    pub video: Option<Video>,
    pub has_audio: bool,
    /// Seconds: the length of the whole file, else of the video stream.
    pub duration: Option<f64>,
    /// The tags of the file and of its streams (`creation_time`, `location`, ...) by name in lower case: files differ in
    /// how they write them, and some keep the date and the place only on a stream. The file's own tag wins.
    pub tags: HashMap<String, String>,
}

/// The video stream, with the size it is shown in.
#[derive(Debug, Default, PartialEq)]
pub struct Video {
    pub codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl Probe {
    /// The value of the first of `names` (in lower case) the file has a tag for.
    pub fn tag(&self, names: &[&str]) -> Option<&str> {
        names.iter().find_map(|n| self.tags.get(*n).map(String::as_str))
    }

    /// The value of a tag whose name starts with `prefix`, such as the `location-eng` a phone writes. When there are
    /// several, the one that sorts first, so that the answer does not change from run to run.
    pub fn tag_starting(&self, prefix: &str) -> Option<&str> {
        self.tags
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .min_by_key(|(k, _)| *k)
            .map(|(_, v)| v.as_str())
    }
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    streams: Vec<Stream>,
    format: Option<Format>,
}

#[derive(Deserialize, Default)]
struct Format {
    duration: Option<String>,
    #[serde(default)]
    tags: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Stream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    duration: Option<String>,
    #[serde(default)]
    disposition: Disposition,
    #[serde(default)]
    side_data_list: Vec<SideData>,
    #[serde(default)]
    tags: HashMap<String, String>,
}

#[derive(Deserialize, Default)]
struct Disposition {
    #[serde(default)]
    attached_pic: u8,
}

#[derive(Deserialize)]
struct SideData {
    rotation: Option<f64>,
}

fn seconds(text: Option<&String>) -> Option<f64> {
    text.and_then(|t| t.parse::<f64>().ok()).filter(|d| d.is_finite() && *d > 0.0)
}

/// Reads `ffprobe`'s JSON. A file with no video stream is not an error here (a song has none): `video` is `None`.
pub fn parse(json: &[u8]) -> Result<Probe> {
    let raw: Raw = serde_json::from_slice(json).context("ffprobe's answer is not JSON")?;
    let format = raw.format.unwrap_or_default();
    let has_audio = raw.streams.iter().any(|s| s.codec_type.as_deref() == Some("audio"));
    let stream = raw
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video") && s.disposition.attached_pic == 0);
    let video = stream.map(|s| {
        let (mut width, mut height) = (s.width, s.height);
        // A phone films sideways and says so in a rotation (the newer way: side data; the older one: a `rotate` tag).
        // People see the rotated picture, so that is the size that counts.
        let rotation = s
            .side_data_list
            .iter()
            .find_map(|d| d.rotation)
            .or_else(|| s.tags.get("rotate").and_then(|r| r.parse().ok()))
            .unwrap_or(0.0);
        if (rotation.abs() as i64) % 180 == 90 {
            std::mem::swap(&mut width, &mut height);
        }
        Video { codec: s.codec_name.clone(), width, height }
    });
    let duration = seconds(format.duration.as_ref()).or_else(|| stream.and_then(|s| seconds(s.duration.as_ref())));
    let mut tags: HashMap<String, String> = format.tags.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect();
    for stream in &raw.streams {
        for (name, value) in &stream.tags {
            tags.entry(name.to_lowercase()).or_insert_with(|| value.clone());
        }
    }
    Ok(Probe { video, has_audio, duration, tags })
}

#[cfg(test)]
#[path = "../tests/unit/probe.rs"]
mod tests;
