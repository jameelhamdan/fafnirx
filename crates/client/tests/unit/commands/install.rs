use super::*;

#[test]
fn a_folder_is_added_to_the_path_once_and_removed_cleanly() {
    let dir = r"C:\Users\me\AppData\Local\Programs\fafnirx";
    assert_eq!(path_with(r"C:\Windows;C:\Tools", dir).as_deref(), Some(&*format!(r"C:\Windows;C:\Tools;{dir}")));
    assert_eq!(path_with("", dir).as_deref(), Some(dir), "an empty PATH");
    assert_eq!(
        path_with(&format!("C:\\Windows;{}\\;", dir.to_uppercase()), dir),
        None,
        "there already, in another case and with a final backslash"
    );
    assert_eq!(path_without(&format!(r"C:\Windows;{dir};C:\Tools"), dir).as_deref(), Some(r"C:\Windows;C:\Tools"));
    assert_eq!(path_without(r"C:\Windows", dir), None, "nothing to remove");
    assert_eq!(
        path_with(r"%USERPROFILE%\bin;C:\Windows", dir).as_deref(),
        Some(&*format!(r"%USERPROFILE%\bin;C:\Windows;{dir}")),
        "variables stay as they are"
    );
}
