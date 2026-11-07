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
fn black_white_and_flat_frames_are_not_pictures() {
    let flat = |r: u8, g: u8, b: u8| [r, g, b].repeat(LOOK_W * LOOK_H);
    assert!(!shows_a_picture(&flat(0, 0, 0)), "black");
    assert!(!shows_a_picture(&flat(12, 12, 12)), "almost black");
    assert!(!shows_a_picture(&flat(250, 250, 250)), "white");
    assert!(!shows_a_picture(&flat(120, 120, 120)), "one flat grey");
    assert!(shows_a_picture(&flat(200, 40, 40)), "a flat colour is coloured");
    let mut half = flat(30, 30, 30);
    let middle = half.len() / 2;
    half[..middle].fill(200);
    assert!(shows_a_picture(&half), "light and dark parts are contrast");
    assert!(!shows_a_picture(&[]), "no frame");
}

#[test]
fn the_places_to_look_are_every_tenth_of_the_length() {
    assert_eq!(poster_candidates(Some(100.0)), [10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0]);
    assert_eq!(poster_candidates(None), [1.0, 3.0, 10.0, 30.0], "no length: a few fixed places");
    assert_eq!(poster_candidates(Some(0.0)), poster_candidates(None));
    let args = look_args(std::path::Path::new("in.mp4"), 12.5);
    assert!(args.windows(2).any(|w| w == ["-ss", "12.500"]) && args.iter().any(|a| a == "rawvideo"));
}

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
    let args = segment_args(
        &Segment {
            source: Path::new("/d/files/pc/a.mkv"),
            out: Path::new("/c/seg2.tmp"),
            start: 12.0,
            len: 6.0,
            has_audio: true,
            vcodec: "libx264",
        },
        v,
    );
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
    let args = segment_args(
        &Segment {
            source: Path::new("a"),
            out: Path::new("b"),
            start: 0.0,
            len: 6.0,
            has_audio: false,
            vcodec: "h264_nvenc",
        },
        v,
    );
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
