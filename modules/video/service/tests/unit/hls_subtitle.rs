use super::*;

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn subtitle_files_are_found_by_the_name_of_the_video() {
    let found = subtitles_in(
        "Movie.mp4",
        &names(&[
            "Movie.srt",
            "Movie.en.srt",
            "Movie.AR.vtt",
            "Movie.es.forced.srt",
            "Other.srt",
            "Movie.txt",
            "Movie2.srt",
            "Movie.en.srt.bak",
        ]),
        false,
    );
    let seen: Vec<(&str, &str, bool)> = found.iter().map(|s| (s.file.as_str(), s.language.as_str(), s.forced)).collect();
    assert_eq!(
        seen,
        [
            ("Movie.AR.vtt", "ar", false),
            ("Movie.en.srt", "en", false),
            ("Movie.es.forced.srt", "es", true),
            ("Movie.srt", "", false)
        ],
    );
    let shown: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(shown, ["العربية", "English", "Español (forced)", "Subtitles"]);
    assert_eq!(found.iter().map(|s| s.index).collect::<Vec<_>>(), [0, 1, 2, 3]);
}

#[test]
fn languages_are_known_by_code_or_name_and_unknown_codes_are_kept() {
    let found = subtitles_in(
        "a.b.mkv",
        &names(&["a.b.jpn.ass", "a.b.Russian.srt", "a.b.zh-cn.srt", "a.b.xx.srt", "a.b.eng.sdh.srt"]),
        false,
    );
    let seen: Vec<(&str, &str)> = found.iter().map(|s| (s.language.as_str(), s.name.as_str())).collect();
    assert_eq!(
        seen,
        [("ru", "Русский"), ("en", "English (SDH)"), ("ja", "日本語"), ("xx", "XX"), ("zh", "中文")],
        "ordered by file name"
    );
}

#[test]
fn tracks_with_one_name_are_numbered() {
    let found = subtitles_in("m.mp4", &names(&["m.en.srt", "m.eng.srt", "m.ar.srt"]), false);
    let shown: Vec<&str> = found.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(shown, ["العربية", "English 1", "English 2"]);
    assert!(subtitles_in("m.mp4", &names(&["n.srt"]), false).is_empty());
}

#[test]
fn subtitles_in_a_subs_folder_are_found_too() {
    let found = subtitles_in(
        "Show - 01.mkv",
        &names(&[
            "Subs/Show - 01.eng.ass",
            "Subs/Show - 01/3_Spanish.srt",
            "Subs/Show - 02.eng.ass",
            "Subs/Other/2_English.srt",
        ]),
        false,
    );
    let seen: Vec<(&str, &str)> = found.iter().map(|s| (s.file.as_str(), s.language.as_str())).collect();
    assert_eq!(seen, [("Subs/Show - 01.eng.ass", "en"), ("Subs/Show - 01/3_Spanish.srt", "es")]);
}

#[test]
fn the_only_video_in_a_folder_gets_subtitles_named_otherwise() {
    let files = names(&["[Grp] Show - 01 [ABCD1234].jpn.srt", "[Grp] Show - 01 [1080p].eng.ass", "Subs/the.srt"]);
    assert!(subtitles_in("Show.mkv", &files, false).is_empty());
    let found = subtitles_in("Show.mkv", &files, true);
    let seen: Vec<(&str, &str)> = found.iter().map(|s| (s.name.as_str(), s.language.as_str())).collect();
    // Only languages that are known are read out of a name that was not made for this video.
    assert_eq!(seen, [("Subtitles", ""), ("English", "en"), ("日本語", "ja")]);
}

#[test]
fn embedded_tracks_are_named_by_title_then_language() {
    let a = embedded_subtitle(0, "eng", "English [Full]", false);
    assert_eq!((a.name.as_str(), a.language.as_str(), a.stream), ("English [Full]", "en", Some(0)));
    let b = embedded_subtitle(1, "jpn", "", true);
    assert_eq!((b.name.as_str(), b.language.as_str(), b.forced), ("日本語 (forced)", "ja", true));
    let c = embedded_subtitle(2, "und", "", false);
    assert_eq!((c.name.as_str(), c.language.as_str()), ("Subtitles", ""));
    let d = embedded_subtitle(3, "xxx", "", false);
    assert_eq!((d.name.as_str(), d.language.as_str()), ("XXX", ""));
}

#[test]
fn old_files_are_read_in_the_code_page_of_their_language() {
    assert_eq!(legacy_charset("ar"), "CP1256");
    assert_eq!(legacy_charset("ru"), "CP1251");
    assert_eq!(legacy_charset(""), "CP1252");
}
