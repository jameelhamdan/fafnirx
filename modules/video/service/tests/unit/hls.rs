use super::*;

fn source(width: u32, height: u32, duration: f64) -> Source {
    Source { width, height, duration, has_audio: true }
}

fn names(s: &Source) -> Vec<String> {
    variants(s).into_iter().map(|v| v.name).collect()
}

#[test]
fn a_video_is_offered_every_quality_up_to_its_own() {
    assert_eq!(names(&source(3840, 2160, 60.0)), ["1080p", "720p", "480p", "360p"], "never above the ladder");
    assert_eq!(names(&source(1280, 720, 60.0)), ["720p", "480p", "360p"]);
    assert_eq!(names(&source(640, 480, 60.0)), ["480p", "360p"]);
}

#[test]
fn a_video_below_the_ladder_is_offered_once_at_its_own_size() {
    let v = variants(&source(320, 240, 10.0));
    assert_eq!(v.len(), 1);
    assert_eq!((v[0].name.as_str(), v[0].width, v[0].height), ("240p", 320, 240));
    let odd = variants(&source(101, 51, 10.0));
    assert!(odd[0].width.is_multiple_of(2) && odd[0].height.is_multiple_of(2), "H.264 needs even sizes");
}

#[test]
fn widths_keep_the_picture_shape_and_are_even() {
    let v = variants(&source(1920, 1080, 10.0));
    assert_eq!((v[0].width, v[0].height), (1920, 1080));
    assert_eq!((v[1].width, v[1].height), (1280, 720));
    let vertical = variants(&source(1080, 1920, 10.0));
    assert_eq!(vertical[0].height, 1080, "a vertical video keeps its height as the ladder's");
    assert_eq!(vertical[0].width, 608, "and its shape: 1080 * 1080 / 1920 = 607.5, made even");
}

#[test]
fn segments_cover_the_whole_video_once() {
    assert_eq!(segment_count(0.0), 1);
    assert_eq!(segment_count(6.0), 1);
    assert_eq!(segment_count(6.1), 2);
    assert_eq!(segment_count(61.0), 11);
    assert_eq!(segment_span(0, 14.5), Some((0.0, 6.0)));
    assert_eq!(segment_span(2, 14.5), Some((12.0, 2.5)), "the last one is the rest");
    assert_eq!(segment_span(3, 14.5), None);
    let total: f64 = (0..segment_count(14.5)).map(|n| segment_span(n, 14.5).unwrap().1).sum();
    assert!((total - 14.5).abs() < 1e-9);
}

#[test]
fn playlists_list_the_qualities_and_the_segments() {
    let master = master_playlist(&variants(&source(1280, 720, 30.0)));
    assert!(master.starts_with("#EXTM3U\n"));
    assert!(master.contains("BANDWIDTH=2928000,RESOLUTION=1280x720"));
    assert!(master.contains("\n720p/index.m3u8\n") && master.contains("\n360p/index.m3u8\n"));
    assert!(!master.contains("1080p"));

    let media = media_playlist(14.5);
    assert!(media.contains("#EXT-X-TARGETDURATION:6\n") && media.contains("#EXT-X-PLAYLIST-TYPE:VOD\n"));
    assert!(media.contains("#EXTINF:6.000,\nseg0.ts\n#EXTINF:6.000,\nseg1.ts\n#EXTINF:2.500,\nseg2.ts\n"));
    assert!(media.ends_with("#EXT-X-ENDLIST\n"));
    assert!(!media.contains("seg3.ts"));
}

#[test]
fn segment_names_are_strict() {
    assert_eq!(parse_segment("seg0.ts"), Some(0));
    assert_eq!(parse_segment("seg123.ts"), Some(123));
    for bad in ["seg.ts", "seg-1.ts", "seg1.mp4", "seg1.ts/", "../seg1.ts", "segx.ts", "seg99999999.ts", "SEG1.ts"] {
        assert_eq!(parse_segment(bad), None, "{bad}");
    }
}
