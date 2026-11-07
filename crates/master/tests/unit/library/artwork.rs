use super::*;

fn files(paths: &[&str]) -> Vec<Neighbour> {
    paths.iter().map(|p| (p.to_string(), vec![1])).collect()
}

#[test]
fn an_episode_uses_its_own_picture_before_a_shared_one() {
    let n = files(&["Show/S01E01.mkv", "Show/S01E02.mkv", "Show/poster.jpg", "Show/S01E01-thumb.jpg", "Show/S01E02.png"]);
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
