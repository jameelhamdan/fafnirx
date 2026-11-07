use super::*;

#[test]
fn names_become_names_a_file_system_accepts() {
    assert_eq!(sanitize_name("a/b:c*?.txt"), "a_b_c__.txt");
    assert_eq!(sanitize_name("  name. . "), "name");
    assert_eq!(sanitize_name("..."), "_");
    let long = sanitize_name(&"é".repeat(300));
    assert!(long.len() <= NAME_BYTES && long.chars().all(|c| c == 'é'), "cut on a character");
}
