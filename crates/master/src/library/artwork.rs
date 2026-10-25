//! Pictures and descriptions that sit next to a video, named the way Jellyfin, Kodi and Plex name them, so that a library
//! organised for those shows up here with its own artwork and episode titles, and without them nothing changes: the
//! frame taken from the video is used.
//!
//! * An episode or a film: `<name>-thumb.jpg`, `<name>-landscape.jpg`, `<name>-poster.jpg` or `<name>.jpg` next to
//!   `<name>.mkv` (also `.jpeg`, `.png`, `.webp`).
//! * A video without a picture of its own takes the folder's: `poster`, `folder`, `cover`, `thumb` or `thumbnail` in its folder, else in
//!   the folder above (a season under a show), up to [`FOLDERS_UP`] levels. A film alone in its folder is the same case.
//! * Descriptions: `<name>.nfo` (Kodi's XML: `title`, `plot`, `season`, `episode`, `aired`, `showtitle`), or `movie.nfo`
//!   for a video that is alone in its folder.
//!
//! Everything is looked up when asked for, among the files the folder has now, so a picture that arrives after the video
//! is used from then on and nothing has to be scanned again.

use crate::library::meta;
use crate::platform::store::Store;
use crate::repo::files;
use serde::Serialize;
use std::path::PathBuf;

const IMAGE_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];
/// Per episode: the file name's own suffixes, best first.
const EPISODE_SUFFIXES: [&str; 4] = ["-thumb", "-landscape", "-poster", ""];
/// A picture for a whole folder.
const FOLDER_NAMES: [&str; 5] = ["poster", "folder", "cover", "thumb", "thumbnail"];
/// How many folders above a video are looked in for a picture that stands for the folder.
pub const FOLDERS_UP: usize = 2;
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

/// The picture of a video itself, out of the files next to it: `<name>-thumb.jpg`, `<name>.png` ...
pub fn picture<'a>(video: &str, neighbours: &'a [Neighbour]) -> Option<&'a Neighbour> {
    let (dir, name) = split(video);
    let stem = stem(name);
    EPISODE_SUFFIXES.iter().find_map(|suffix| find(neighbours, dir, &format!("{stem}{suffix}"), &IMAGE_EXTENSIONS))
}

/// The picture that stands for a folder (`poster.jpg`, `folder.png` ...) among the files that are directly in it.
pub fn folder_picture<'a>(dir: &str, neighbours: &'a [Neighbour]) -> Option<&'a Neighbour> {
    FOLDER_NAMES.iter().find_map(|n| find(neighbours, dir, n, &IMAGE_EXTENSIONS))
}

/// The folders above a path, nearest first, as the paths the pictures in them would have (`a/b/c.mkv` gives `a/b`, `a`, ``).
pub fn folders_above(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut dir = split(path).0;
    loop {
        out.push(dir.to_string());
        if dir.is_empty() {
            return out;
        }
        dir = split(dir).0;
    }
}

/// Where a video's picture comes from.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Source {
    /// A picture named after the video.
    Own,
    /// The poster of its folder or of a folder above it.
    Folder,
}

/// The picture that stands for a video, if there is one: its own (`<name>-thumb.jpg`), else the poster of its folder or of a
/// folder above it (`poster.jpg`, `cover.png` ...). Returns its path in the device, its content and which of the two it is.
/// Not finding one is the usual case, and a failed lookup only means the frame of the video is used.
pub fn find_picture(c: &mut postgres::Client, device: &str, path: &str) -> Option<(String, Vec<u8>, Source)> {
    if meta::kind(path) != Some("video") {
        return None;
    }
    let near = files::neighbours(c, device, path).ok()?;
    if let Some((art, root)) = picture(path, &near) {
        return Some((art.clone(), root.clone(), Source::Own));
    }
    for (level, dir) in folders_above(path).into_iter().take(FOLDERS_UP + 1).enumerate() {
        let found = if level == 0 {
            folder_picture(&dir, &near).cloned()
        } else {
            // The neighbours of a file named in that folder are the files directly in it.
            let probe = if dir.is_empty() { "x".to_string() } else { format!("{dir}/x") };
            let up = files::neighbours(c, device, &probe).ok()?;
            folder_picture(&dir, &up).cloned()
        };
        if let Some((art, root)) = found {
            return Some((art, root, Source::Folder));
        }
    }
    None
}

/// [`find_picture`] with the picture's place in the file store.
pub fn lookup(c: &mut postgres::Client, store: &Store, device: &str, path: &str) -> Option<(PathBuf, Vec<u8>)> {
    let (art, root, _) = find_picture(c, device, path)?;
    Some((store.live_path(device, &art).ok()?, root))
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
    fn a_folder_picture_stands_for_a_video_that_has_none_of_its_own() {
        let n = files(&["Show/S01E01.mkv", "Show/S01E02.mkv", "Show/poster.jpg"]);
        assert!(picture("Show/S01E01.mkv", &n).is_none(), "no picture of its own");
        assert_eq!(folder_picture("Show", &n).unwrap().0, "Show/poster.jpg");
        let film = files(&["Film (2020)/Film.mkv", "Film (2020)/Folder.JPG", "Film (2020)/fanart.jpg"]);
        assert_eq!(folder_picture("Film (2020)", &film).unwrap().0, "Film (2020)/Folder.JPG");
        assert!(folder_picture("Film (2020)", &files(&["Film (2020)/Film.mkv"])).is_none());
        assert_eq!(folder_picture("", &files(&["cover.PNG"])).unwrap().0, "cover.PNG");
    }

    #[test]
    fn the_folders_above_a_path_are_nearest_first() {
        assert_eq!(folders_above("a/b/c.mkv"), ["a/b", "a", ""]);
        assert_eq!(folders_above("c.mkv"), [""]);
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
