use super::{first_of, natural_cmp};
use uuid::Uuid;

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}
use std::cmp::Ordering;

#[test]
fn the_cover_of_a_folder_is_its_first_video() {
    let media = vec![
        (id(1), "Show/Season 1/other.jpg".to_string()),
        (id(2), "Show/Season 2/E01.mkv".to_string()),
        (id(3), "Show/Season 1/E10.mkv".to_string()),
        (id(4), "Show/Season 1/E2.mkv".to_string()),
    ];
    assert_eq!(first_of(media), Some(id(4)), "a video, and episode 2 before episode 10");
    assert_eq!(first_of(vec![(id(7), "b.jpg".into()), (id(8), "a.jpg".into())]), Some(id(8)));
    assert_eq!(first_of(Vec::new()), None);
}

#[test]
fn names_sort_the_way_people_read_them() {
    let mut names = ["Episode 10.mkv", "episode 2.mkv", "Episode 1.mkv", "Season 2/E01.mkv", "Season 10/E01.mkv"];
    names.sort_by(|a, b| natural_cmp(a, b));
    assert_eq!(names, ["Episode 1.mkv", "episode 2.mkv", "Episode 10.mkv", "Season 2/E01.mkv", "Season 10/E01.mkv"]);
    assert_eq!(natural_cmp("Episode 02", "episode 2"), Ordering::Equal, "leading zeros and case do not count");
    assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
    assert_eq!(natural_cmp("a1", "a"), Ordering::Greater);
    assert_eq!(natural_cmp("", "a"), Ordering::Less);
}
