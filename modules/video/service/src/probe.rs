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
    /// The tags of the file (`creation_time`, `location`, ...), by the names the file gives them.
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
    /// The value of the first of `names` the file has a tag for.
    pub fn tag(&self, names: &[&str]) -> Option<&str> {
        names.iter().find_map(|n| self.tags.get(*n).map(String::as_str))
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
    let stream =
        raw.streams.iter().find(|s| s.codec_type.as_deref() == Some("video") && s.disposition.attached_pic == 0);
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
    Ok(Probe { video, has_audio, duration, tags: format.tags })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_its_size_its_length_and_its_tags_are_read() {
        let p = parse(
            br#"{"streams":[{"codec_type":"audio"},{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"duration":"9"}],
                "format":{"duration":"12.5","tags":{"creation_time":"2023-11-14T22:13:20.000000Z"}}}"#,
        )
        .unwrap();
        let video = p.video.as_ref().unwrap();
        assert_eq!((video.codec.as_deref(), video.width, video.height), (Some("h264"), Some(1920), Some(1080)));
        assert!(p.has_audio);
        assert_eq!(p.duration, Some(12.5), "the length of the whole file, not of one stream");
        assert_eq!(p.tag(&["nope", "creation_time"]), Some("2023-11-14T22:13:20.000000Z"));
        assert_eq!(p.tag(&["nope"]), None);
    }

    #[test]
    fn a_video_on_its_side_is_measured_the_way_it_is_shown() {
        let newer =
            br#"{"streams":[{"codec_type":"video","width":1920,"height":1080,"side_data_list":[{"rotation":-90}]}]}"#;
        let older = br#"{"streams":[{"codec_type":"video","width":1920,"height":1080,"tags":{"rotate":"270"}}]}"#;
        let upside_down = br#"{"streams":[{"codec_type":"video","width":1920,"height":1080,"tags":{"rotate":"180"}}]}"#;
        for (json, size) in [(&newer[..], (1080, 1920)), (&older[..], (1080, 1920)), (&upside_down[..], (1920, 1080))] {
            let v = parse(json).unwrap().video.unwrap();
            assert_eq!((v.width, v.height), (Some(size.0), Some(size.1)));
        }
    }

    #[test]
    fn cover_art_is_not_a_video_and_a_missing_length_falls_back_to_the_stream() {
        let song = parse(
            br#"{"streams":[{"codec_type":"audio"},{"codec_type":"video","width":5,"height":5,"disposition":{"attached_pic":1}}],"format":{"duration":"200"}}"#,
        )
        .unwrap();
        assert!(song.video.is_none() && song.has_audio && song.duration == Some(200.0));
        let clip = parse(br#"{"streams":[{"codec_type":"video","width":4,"height":2,"duration":"3.5"}],"format":{}}"#)
            .unwrap();
        assert_eq!(clip.duration, Some(3.5));
        let none = parse(br#"{"streams":[{"codec_type":"video","width":4,"height":2}]}"#).unwrap();
        assert_eq!(none.duration, None);
        assert!(parse(b"not json").is_err());
    }
}
