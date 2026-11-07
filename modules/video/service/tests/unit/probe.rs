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
fn tags_are_found_whatever_their_case_and_wherever_the_file_keeps_them() {
    let p = parse(
        br#"{"streams":[{"codec_type":"video","width":4,"height":2,"tags":{"CREATION_TIME":"2020-01-02T03:04:05Z","Rotate":"0"}},
                           {"codec_type":"audio","tags":{"creation_time":"1999-01-01T00:00:00Z"}}],
                "format":{"tags":{"Location-eng":"+52.5200+013.4050/","com.apple.quicktime.creationdate":"2023-11-14T22:13:20+0100"}}}"#,
    )
    .unwrap();
    assert_eq!(p.tag(&["creation_time"]), Some("2020-01-02T03:04:05Z"), "a stream's tag, found in lower case");
    assert_eq!(p.tag_starting("location"), Some("+52.5200+013.4050/"));
    assert_eq!(p.tag(&["com.apple.quicktime.creationdate"]), Some("2023-11-14T22:13:20+0100"));
    let both = parse(br#"{"streams":[{"codec_type":"video","tags":{"creation_time":"2000-01-01T00:00:00Z"}}],"format":{"tags":{"creation_time":"2021-01-01T00:00:00Z"}}}"#).unwrap();
    assert_eq!(both.tag(&["creation_time"]), Some("2021-01-01T00:00:00Z"), "the file's own tag wins");
}

#[test]
fn a_video_on_its_side_is_measured_the_way_it_is_shown() {
    let newer = br#"{"streams":[{"codec_type":"video","width":1920,"height":1080,"side_data_list":[{"rotation":-90}]}]}"#;
    let older = br#"{"streams":[{"codec_type":"video","width":1920,"height":1080,"tags":{"rotate":"270"}}]}"#;
    let upside_down = br#"{"streams":[{"codec_type":"video","width":1920,"height":1080,"tags":{"rotate":"180"}}]}"#;
    for (json, size) in [(&newer[..], (1080, 1920)), (&older[..], (1080, 1920)), (&upside_down[..], (1920, 1080))] {
        let v = parse(json).unwrap().video.unwrap();
        assert_eq!((v.width, v.height), (Some(size.0), Some(size.1)));
    }
}

#[test]
fn cover_art_is_not_a_video_and_a_missing_length_falls_back_to_the_stream() {
    let song = parse(br#"{"streams":[{"codec_type":"audio"},{"codec_type":"video","width":5,"height":5,"disposition":{"attached_pic":1}}],"format":{"duration":"200"}}"#).unwrap();
    assert!(song.video.is_none() && song.has_audio && song.duration == Some(200.0));
    let clip = parse(br#"{"streams":[{"codec_type":"video","width":4,"height":2,"duration":"3.5"}],"format":{}}"#).unwrap();
    assert_eq!(clip.duration, Some(3.5));
    let none = parse(br#"{"streams":[{"codec_type":"video","width":4,"height":2}]}"#).unwrap();
    assert_eq!(none.duration, None);
    assert!(parse(b"not json").is_err());
}
