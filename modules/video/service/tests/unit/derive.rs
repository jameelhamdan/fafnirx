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
    let p = describe(&crate::probe::parse(json).unwrap());
    assert_eq!((p.width, p.height), (Some(1080), Some(1920)), "a sideways video is shown upright");
    assert_eq!(p.duration_ms, Some(12_500));
    assert_eq!(p.codec.as_deref(), Some("h264"));
    assert_eq!(p.taken_at, Some(1_700_000_000));
    assert_eq!(p.position, Some((52.52, 13.405)));
}

#[test]
fn a_phones_local_date_and_an_androids_language_tagged_place_are_used() {
    let json = br#"{"streams": [{"codec_type": "video", "width": 4, "height": 2}],
            "format": {"tags": {"creation_time": "2023-11-14T21:13:20.000000Z",
                                "com.apple.quicktime.creationdate": "2023-11-14T22:13:20+0100",
                                "location-eng": "+52.5200+013.4050/"}}}"#;
    let p = describe(&crate::probe::parse(json).unwrap());
    assert_eq!(p.taken_at, Some(1_700_000_000), "the wall clock of the phone, the same as its photos");
    assert_eq!(p.position, Some((52.52, 13.405)));
    let zero = br#"{"streams": [{"codec_type": "video"}], "format": {"tags": {"creation_time": "1904-01-01T00:00:00Z"}}}"#;
    assert_eq!(describe(&crate::probe::parse(zero).unwrap()).taken_at, None, "the QuickTime epoch is no date");
}

#[test]
fn a_file_without_tags_has_only_what_it_says() {
    let json = br#"{"streams": [{"codec_type": "video", "codec_name": "vp9", "width": 4, "height": 2}], "format": {}}"#;
    let p = describe(&crate::probe::parse(json).unwrap());
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
fn a_poster_is_one_frame_scaled_down() {
    let args = crate::ffmpeg::poster_args(std::path::Path::new("in.mp4"), std::path::Path::new("out.jpg"), 320, "1");
    assert!(args.windows(2).any(|w| w == ["-ss", "1"]) && args.windows(2).any(|w| w == ["-frames:v", "1"]));
    assert!(args.iter().any(|a| a.starts_with("scale=min(320")));
}
