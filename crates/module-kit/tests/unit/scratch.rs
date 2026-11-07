use super::*;

#[test]
fn a_folder_lives_until_it_is_dropped_and_each_has_its_own_name() {
    let parent = tempfile::tempdir().unwrap();
    let (a, b) = (Scratch::new(parent.path()).unwrap(), Scratch::new(parent.path()).unwrap());
    assert_ne!(a.path(), b.path());
    std::fs::write(a.path().join("half"), "x").unwrap();
    let gone = a.path().to_path_buf();
    drop(a);
    assert!(!gone.exists() && b.path().is_dir());
}

#[test]
fn an_old_folder_at_the_same_place_is_emptied() {
    let parent = tempfile::tempdir().unwrap();
    let place = parent.path().join("work");
    std::fs::create_dir_all(&place).unwrap();
    std::fs::write(place.join("old"), "x").unwrap();
    assert!(!Scratch::new_at(place).unwrap().path().join("old").exists());
}
