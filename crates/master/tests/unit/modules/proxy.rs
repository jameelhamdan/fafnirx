use super::safe_rest;

#[test]
fn only_short_plain_names_are_passed_on() {
    for ok in ["hls/master.m3u8", "hls/720p/index.m3u8", "hls/480p/seg0.ts", "hls/subtitles/12.vtt", "a-b_c.d"] {
        assert!(safe_rest(ok), "{ok}");
    }
    for bad in [
        "",
        "/",
        "..",
        "../probe",
        "hls/../master.m3u8",
        ".hidden",
        "hls//index.m3u8",
        "hls/index.m3u8/",
        "a/b/c/d/e",
        "a b",
        "seg1.ts?x=1",
        "hls\\index.m3u8",
        "hls/%2e%2e",
        "é",
    ] {
        assert!(!safe_rest(bad), "{bad:?}");
    }
}
