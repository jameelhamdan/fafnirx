//! Pictures and descriptions that sit next to a video, named the way Jellyfin, Kodi and Plex name them, so that a library
//! organised for those shows up here with its own artwork and episode titles, and without them nothing changes: the
//! frame taken from the video is used.
//!
//! * An episode or a film: `<name>-thumb.jpg`, `<name>-landscape.jpg`, `<name>-poster.jpg` or `<name>.jpg` next to
//!   `<name>.mkv` (also `.jpeg`, `.png`, `.webp`).
//! * A video that is alone in its folder (a film in its own folder): `poster`, `folder`, `cover` or `thumb` there. In a folder
//!   with several videos those belong to the show, not to one episode, so they are not used for an episode.
//! * Descriptions: `<name>.nfo` (Kodi's XML: `title`, `plot`, `season`, `episode`, `aired`, `showtitle`), or `movie.nfo`
//!   for a video that is alone in its folder.
//!
//! Everything is looked up when asked for, among the files the folder has now, so a picture that arrives after the video
//! is used from then on and nothing has to be scanned again.

use crate::library::meta;
use serde::Serialize;

const IMAGE_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];
/// Per episode: the file name's own suffixes, best first.
const EPISODE_SUFFIXES: [&str; 4] = ["-thumb", "-landscape", "-poster", ""];
/// For a video that is alone in its folder.
const FOLDER_NAMES: [&str; 4] = ["poster", "folder", "cover", "thumb"];
/// An .nfo is a few kilobytes; a larger file is not one.
pub const NFO_MAX_BYTES: u64 = 256 * 1024;

type Neighbour = (String, Vec<u8>);

fn split(path: &str) -> (&str, &str) {
    path.rsplit_once('/').map_or(("", path), |(dir, name)| (dir, name))
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

fn is_alone(video: &str, neighbours: &[Neighbour]) -> bool {
    !neighbours.iter().any(|(p, _)| p != video && meta::kind(split(p).1) == Some("video"))
}

fn find<'a>(neighbours: &'a [Neighbour], dir: &str, wanted: &str, extensions: &[&str]) -> Option<&'a Neighbour> {
    extensions.iter().find_map(|ext| {
        let name = format!("{wanted}.{ext}");
        neighbours.iter().find(|(p, _)| {
            let (d, n) = split(p);
            d == dir && n.eq_ignore_ascii_case(&name)
        })
    })
}

/// The picture to show for a video instead of a frame of it: its path and content, out of the files next to it.
pub fn picture<'a>(video: &str, neighbours: &'a [Neighbour]) -> Option<&'a Neighbour> {
    let (dir, name) = split(video);
    let stem = stem(name);
    for suffix in EPISODE_SUFFIXES {
        if let Some(found) = find(neighbours, dir, &format!("{stem}{suffix}"), &IMAGE_EXTENSIONS) {
            return Some(found);
        }
    }
    if is_alone(video, neighbours) {
        return FOLDER_NAMES.iter().find_map(|n| find(neighbours, dir, n, &IMAGE_EXTENSIONS));
    }
    None
}

/// The .nfo that describes a video.
pub fn description<'a>(video: &str, neighbours: &'a [Neighbour]) -> Option<&'a Neighbour> {
    let (dir, name) = split(video);
    find(neighbours, dir, stem(name), &["nfo"]).or_else(|| {
        if is_alone(video, neighbours) {
            find(neighbours, dir, "movie", &["nfo"])
        } else {
            None
        }
    })
}

/// What an .nfo says about an episode or a film.
#[derive(Serialize, Default, Debug, PartialEq)]
pub struct Described {
    pub title: Option<String>,
    pub show: Option<String>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub plot: Option<String>,
    /// `2021-03-05`, or the year alone.
    pub aired: Option<String>,
}

impl Described {
    fn is_empty(&self) -> bool {
        *self == Described::default()
    }
}

/// The text between `<tag>` and `</tag>` (the first one), with the five XML entities and CDATA undone.
fn element(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find(&format!("</{tag}>"))?;
    let raw = xml[start..end].trim();
    let text = match raw.strip_prefix("<![CDATA[").and_then(|t| t.strip_suffix("]]>")) {
        Some(inner) => inner.to_string(),
        None => raw
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&"),
    };
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Reads a Kodi/Jellyfin .nfo. `None` when it says nothing this app shows.
pub fn parse_nfo(xml: &str) -> Option<Described> {
    let number = |tag: &str| element(xml, tag).and_then(|t| t.parse::<u32>().ok());
    let described = Described {
        title: element(xml, "title"),
        show: element(xml, "showtitle"),
        season: number("season"),
        episode: number("episode"),
        plot: element(xml, "plot"),
        aired: element(xml, "aired").or_else(|| element(xml, "premiered")).or_else(|| element(xml, "year")),
    };
    (!described.is_empty()).then_some(described)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(paths: &[&str]) -> Vec<Neighbour> {
        paths.iter().map(|p| (p.to_string(), vec![1])).collect()
    }

    #[test]
    fn an_episode_uses_its_own_picture_before_a_shared_one() {
        let n = files(&[
            "Show/S01E01.mkv",
            "Show/S01E02.mkv",
            "Show/poster.jpg",
            "Show/S01E01-thumb.jpg",
            "Show/S01E02.png",
        ]);
        assert_eq!(picture("Show/S01E01.mkv", &n).unwrap().0, "Show/S01E01-thumb.jpg");
        assert_eq!(picture("Show/S01E02.mkv", &n).unwrap().0, "Show/S01E02.png");
    }

    #[test]
    fn the_poster_of_a_show_is_not_the_picture_of_an_episode() {
        let n = files(&["Show/S01E01.mkv", "Show/S01E02.mkv", "Show/poster.jpg"]);
        assert!(picture("Show/S01E01.mkv", &n).is_none(), "the frame of the video is used");
    }

    #[test]
    fn a_film_alone_in_its_folder_uses_the_folder_pictures() {
        let n = files(&["Film (2020)/Film.mkv", "Film (2020)/Folder.JPG", "Film (2020)/fanart.jpg"]);
        assert_eq!(picture("Film (2020)/Film.mkv", &n).unwrap().0, "Film (2020)/Folder.JPG");
        assert!(picture("Film (2020)/Film.mkv", &files(&["Film (2020)/Film.mkv"])).is_none());
    }

    #[test]
    fn the_picture_must_be_in_the_same_folder() {
        let n = files(&["a/x.mkv", "b/x.jpg"]);
        assert!(picture("a/x.mkv", &n).is_none());
        assert_eq!(picture("x.mkv", &files(&["x.mkv", "x-thumb.webp"])).unwrap().0, "x-thumb.webp");
    }

    #[test]
    fn descriptions_come_from_the_nfo_of_the_video_or_of_the_lone_film() {
        let n = files(&["Show/E1.mkv", "Show/E1.nfo", "Show/movie.nfo", "Show/E2.mkv"]);
        assert_eq!(description("Show/E1.mkv", &n).unwrap().0, "Show/E1.nfo");
        assert!(description("Show/E2.mkv", &n).is_none(), "movie.nfo is for a lone film");
        let film = files(&["F/f.mkv", "F/movie.nfo"]);
        assert_eq!(description("F/f.mkv", &film).unwrap().0, "F/movie.nfo");
    }

    #[test]
    fn an_nfo_is_read() {
        let xml = r#"<?xml version="1.0"?><episodedetails>
            <title>Pilot &amp; Beyond</title><showtitle>The Show</showtitle><season>1</season><episode>02</episode>
            <plot><![CDATA[It <begins>.]]></plot><aired>2021-03-05</aired></episodedetails>"#;
        let d = parse_nfo(xml).unwrap();
        assert_eq!(d.title.as_deref(), Some("Pilot & Beyond"));
        assert_eq!((d.show.as_deref(), d.season, d.episode), (Some("The Show"), Some(1), Some(2)));
        assert_eq!(d.plot.as_deref(), Some("It <begins>."));
        assert_eq!(d.aired.as_deref(), Some("2021-03-05"));
        assert!(parse_nfo("<movie></movie>").is_none());
    }
}
