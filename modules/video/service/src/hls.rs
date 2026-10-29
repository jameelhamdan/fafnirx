//! HTTP Live Streaming, worked out from what a video is: which qualities it is offered in, how long its segments
//! are, and the playlists that list them. Nothing here touches a file or a program, so it can all be tested.
//!
//! The playlists are complete before anything is transcoded: every segment has a fixed length (the last one is
//! shorter), so segment `n` of a video is always the same stretch of it. A player asks for the segments it needs
//! and each is made when it is asked for (see `service`).

/// Seconds of video in a segment.
pub const SEGMENT_SECS: f64 = 6.0;

/// A quality the ladder offers: the picture height and the bit rates it is encoded at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rung {
    pub height: u32,
    pub video_kbps: u32,
    pub audio_kbps: u32,
}

/// From the best to the worst.
pub const LADDER: [Rung; 4] = [
    Rung { height: 1080, video_kbps: 5000, audio_kbps: 160 },
    Rung { height: 720, video_kbps: 2800, audio_kbps: 128 },
    Rung { height: 480, video_kbps: 1400, audio_kbps: 128 },
    Rung { height: 360, video_kbps: 800, audio_kbps: 96 },
];

/// What is known about a video before it is streamed.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub width: u32,
    pub height: u32,
    /// Seconds.
    pub duration: f64,
    pub has_audio: bool,
}

/// One quality of one video.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    /// How it is called in addresses: `720p`.
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub video_kbps: u32,
    pub audio_kbps: u32,
}

impl Variant {
    /// Bits per second, as a playlist announces it.
    pub fn bandwidth(&self) -> u64 {
        u64::from(self.video_kbps + self.audio_kbps) * 1000
    }
}

fn even(n: f64) -> u32 {
    ((n / 2.0).round() as u32).max(1) * 2
}

/// The qualities offered for `source`: every rung of the ladder that is not taller than the video, so that a
/// small video is never blown up. A video smaller than the lowest rung is offered once, at its own size.
pub fn variants(source: &Source) -> Vec<Variant> {
    let width_for = |height: u32| even(f64::from(source.width) * f64::from(height) / f64::from(source.height.max(1)));
    let mut out: Vec<Variant> = LADDER
        .iter()
        .filter(|r| r.height <= source.height)
        .map(|r| Variant {
            name: format!("{}p", r.height),
            width: width_for(r.height),
            height: r.height,
            video_kbps: r.video_kbps,
            audio_kbps: r.audio_kbps,
        })
        .collect();
    if out.is_empty() {
        let lowest = LADDER[LADDER.len() - 1];
        let height = even(f64::from(source.height));
        out.push(Variant {
            name: format!("{height}p"),
            width: even(f64::from(source.width)),
            height,
            video_kbps: lowest.video_kbps,
            audio_kbps: lowest.audio_kbps,
        });
    }
    out
}

/// How many segments a video of `duration` seconds is cut into. Never none.
pub fn segment_count(duration: f64) -> usize {
    ((duration / SEGMENT_SECS).ceil() as usize).max(1)
}

/// Where segment `n` starts and how long it is, in seconds. `None` past the end.
pub fn segment_span(n: usize, duration: f64) -> Option<(f64, f64)> {
    if n >= segment_count(duration) {
        return None;
    }
    let start = n as f64 * SEGMENT_SECS;
    Some((start, (duration - start).clamp(0.001, SEGMENT_SECS)))
}

/// The playlist that lists the qualities.
pub fn master_playlist(variants: &[Variant]) -> String {
    let mut out = String::from("#EXTM3U\n#EXT-X-VERSION:3\n");
    for v in variants {
        out += &format!(
            "#EXT-X-STREAM-INF:BANDWIDTH={},RESOLUTION={}x{},CODECS=\"avc1.640028,mp4a.40.2\"\n{}/index.m3u8\n",
            v.bandwidth(),
            v.width,
            v.height,
            v.name
        );
    }
    out
}

/// The playlist of one quality: all the segments of the video, in order.
pub fn media_playlist(duration: f64) -> String {
    let mut out = format!(
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:{}\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n",
        SEGMENT_SECS as u32
    );
    for n in 0..segment_count(duration) {
        let (_, len) = segment_span(n, duration).expect("n is below the count");
        out += &format!("#EXTINF:{len:.3},\nseg{n}.ts\n");
    }
    out += "#EXT-X-ENDLIST\n";
    out
}

/// `seg12.ts` -> 12.
pub fn parse_segment(name: &str) -> Option<usize> {
    let digits = name.strip_prefix("seg")?.strip_suffix(".ts")?;
    (!digits.is_empty() && digits.len() <= 7 && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| digits.parse().ok())?
}

/// A subtitle track of a video: a file next to it (or in its `Subs` folder), or a track inside the video file.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Subtitle {
    /// Its place in the list; the address of its track.
    pub index: usize,
    /// What a player calls it: the language in its own script, or "Subtitles".
    pub name: String,
    /// The two-letter code, empty when the file name does not say.
    pub language: String,
    pub forced: bool,
    /// The file next to the video, as a path from the video's folder (not sent to players). Empty for an embedded track.
    #[serde(skip)]
    pub file: String,
    /// For a track inside the video file, its place among the video's subtitle streams (`-map 0:s:N`).
    #[serde(skip)]
    pub stream: Option<usize>,
}

/// Names a subtitle file may carry for a language: the codes and the name, and what to show.
const LANGUAGES: &[(&[&str], &str, &str)] = &[
    (&["en", "eng", "english"], "en", "English"),
    (&["es", "spa", "spanish", "espanol", "español"], "es", "Español"),
    (&["zh", "zho", "chi", "cmn", "chinese", "zh-cn", "zh-tw", "chs", "cht"], "zh", "中文"),
    (&["ar", "ara", "arabic"], "ar", "العربية"),
    (&["ru", "rus", "russian"], "ru", "Русский"),
    (&["ja", "jpn", "jp", "japanese"], "ja", "日本語"),
    (&["fr", "fra", "fre", "french"], "fr", "Français"),
    (&["de", "deu", "ger", "german"], "de", "Deutsch"),
    (&["it", "ita", "italian"], "it", "Italiano"),
    (&["pt", "por", "portuguese"], "pt", "Português"),
    (&["ko", "kor", "korean"], "ko", "한국어"),
    (&["tr", "tur", "turkish"], "tr", "Türkçe"),
    (&["hi", "hin", "hindi"], "hi", "हिन्दी"),
    (&["nl", "nld", "dut", "dutch"], "nl", "Nederlands"),
    (&["pl", "pol", "polish"], "pl", "Polski"),
    (&["uk", "ukr", "ukrainian"], "uk", "Українська"),
    (&["he", "heb", "hebrew"], "he", "עברית"),
    (&["fa", "fas", "per", "persian"], "fa", "فارسی"),
];

/// Extensions of the subtitle formats that ffmpeg turns into WebVTT.
pub const SUBTITLE_EXTENSIONS: [&str; 4] = ["srt", "vtt", "ass", "ssa"];

/// The subtitle files among `names` (paths from the video's folder: `Movie.en.srt`, `Subs/Movie.en.srt`,
/// `Subs/Movie/2_English.srt`) that belong to the video called `video`: `Movie.srt`, `Movie.en.srt`, `Movie.ar.forced.vtt`,
/// for `Movie.mp4`. Case does not matter. The order is the order of the names, so a track keeps its place while the files
/// stay. When `lone` (the video is the only one in its folder) a subtitle file that is named differently still belongs to it.
pub fn subtitles_in(video: &str, names: &[String], lone: bool) -> Vec<Subtitle> {
    let stem = video.rsplit_once('.').map_or(video, |(s, _)| s).to_lowercase();
    let mut files: Vec<&String> = names.iter().collect();
    files.sort();
    let mut found: Vec<Subtitle> = Vec::new();
    for file in files {
        let lower = file.to_lowercase();
        let (folder, name) = lower.rsplit_once('/').unwrap_or(("", &lower));
        let Some((base, ext)) = name.rsplit_once('.') else { continue };
        if !SUBTITLE_EXTENSIONS.contains(&ext) {
            continue;
        }
        let in_own_folder = folder.rsplit('/').next() == Some(stem.as_str());
        // `exact` is false for a file that only belongs here because nothing else could: then only a language that is
        // known is read from its name, as the rest of the name is anything.
        let (rest, exact) = if base == stem {
            ("", true)
        } else if let Some(rest) = base.strip_prefix(&format!("{stem}.")) {
            (rest, true)
        } else if in_own_folder {
            (base, true)
        } else if lone {
            (base, false)
        } else {
            continue;
        };
        let (mut language, mut label, mut forced, mut sdh) = (String::new(), String::new(), false, false);
        for token in rest.split(['.', '_', ' ']).filter(|t| !t.is_empty()) {
            match token {
                "forced" => forced = true,
                "sdh" | "cc" | "hi" => sdh = true,
                _ => {
                    if let Some((_, code, name)) = LANGUAGES.iter().find(|(names, _, _)| names.contains(&token)) {
                        (language, label) = ((*code).into(), (*name).into());
                    } else if exact
                        && language.is_empty()
                        && (2..=3).contains(&token.len())
                        && token.chars().all(|c| c.is_ascii_alphabetic())
                    {
                        (language, label) = (token.into(), token.to_uppercase());
                    }
                }
            }
        }
        let mut name = if label.is_empty() { "Subtitles".to_string() } else { label };
        if sdh {
            name += " (SDH)";
        }
        if forced {
            name += " (forced)";
        }
        found.push(Subtitle { index: found.len(), name, language, forced, file: file.clone(), stream: None });
    }
    number_duplicates(&mut found);
    found
}

/// A track inside the video file. `language` is what the file says (`jpn`, `en`), `title` what whoever made it called it.
pub fn embedded_subtitle(stream: usize, language: &str, title: &str, forced: bool) -> Subtitle {
    let language = language.trim().to_lowercase();
    let known = LANGUAGES.iter().find(|(names, _, _)| names.contains(&language.as_str()));
    let (code, label) = known.map_or((String::new(), String::new()), |(_, c, n)| ((*c).into(), (*n).into()));
    let title = title.trim();
    let mut name = if !title.is_empty() {
        title.to_string()
    } else if !label.is_empty() {
        label
    } else if (2..=3).contains(&language.len()) && language != "und" {
        language.to_uppercase()
    } else {
        "Subtitles".to_string()
    };
    if forced && !name.to_lowercase().contains("forced") {
        name += " (forced)";
    }
    Subtitle { index: 0, name, language: code, forced, file: String::new(), stream: Some(stream) }
}

/// Numbers the tracks and tells two with one name apart by a number.
pub fn number_duplicates(found: &mut [Subtitle]) {
    for (i, s) in found.iter_mut().enumerate() {
        s.index = i;
    }
    let counts: Vec<usize> = found.iter().map(|s| found.iter().filter(|o| o.name == s.name).count()).collect();
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (s, count) in found.iter_mut().zip(counts) {
        if count > 1 {
            let n = seen.entry(s.name.clone()).or_insert(0);
            *n += 1;
            s.name = format!("{} {}", s.name, n);
        }
    }
}

/// The character set to read a subtitle file in when it is not UTF-8, guessed from its language: files of the
/// Windows days are in the code page of their language. Western Europe's is the guess for the rest.
pub fn legacy_charset(language: &str) -> &'static str {
    match language {
        "ar" | "fa" => "CP1256",
        "he" => "CP1255",
        "ru" | "uk" => "CP1251",
        "ja" => "CP932",
        "zh" => "GB18030",
        "ko" => "CP949",
        "tr" => "CP1254",
        "pl" => "CP1250",
        "el" => "CP1253",
        _ => "CP1252",
    }
}

#[cfg(test)]
mod tests {
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
        for bad in ["seg.ts", "seg-1.ts", "seg1.mp4", "seg1.ts/", "../seg1.ts", "segx.ts", "seg99999999.ts", "SEG1.ts"]
        {
            assert_eq!(parse_segment(bad), None, "{bad}");
        }
    }
}

#[cfg(test)]
mod subtitle_tests {
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
        let seen: Vec<(&str, &str, bool)> =
            found.iter().map(|s| (s.file.as_str(), s.language.as_str(), s.forced)).collect();
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
}
