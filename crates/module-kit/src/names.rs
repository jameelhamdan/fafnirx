//! Names of files made from names a source gives.

/// The longest name of a file or a folder kept (bytes): file systems stop at 255, and a module may add to it.
const NAME_BYTES: usize = 150;

/// A name from a source as a name of a file any file system accepts: no characters a file system or a person's other computer
/// refuses, no dots or spaces at the end, not longer than a name may be (cut on a character).
pub fn sanitize_name(name: &str) -> String {
    let replaced: String = name.chars().map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let mut cut = String::new();
    for c in replaced.trim().chars() {
        if cut.len() + c.len_utf8() > NAME_BYTES {
            break;
        }
        cut.push(c);
    }
    let cut = cut.trim_matches(|c: char| c == '.' || c.is_whitespace()).to_string();
    if cut.is_empty() {
        "_".into()
    } else {
        cut
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_names_a_file_system_accepts() {
        assert_eq!(sanitize_name("a/b:c*?.txt"), "a_b_c__.txt");
        assert_eq!(sanitize_name("  name. . "), "name");
        assert_eq!(sanitize_name("..."), "_");
        let long = sanitize_name(&"é".repeat(300));
        assert!(long.len() <= NAME_BYTES && long.chars().all(|c| c == 'é'), "cut on a character");
    }
}
